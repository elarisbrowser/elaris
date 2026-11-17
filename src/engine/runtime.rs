// SPDX-License-Identifier: GPL-3.0-only

//! Ladybird and its one view, owned by the engine thread: applying commands, turning Ladybird's
//! events into updates, and copying its frames out.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use elaris::ladybird::{Event, Frame, Ladybird, Waker, WebView};
use rune::Pixels;

use super::frames::{FrameGate, Verdict};
use super::{Command, Update};
use crate::perf::{self, Count, Stage};

/// How long closing keeps running Ladybird so helper processes see the view close.
const CLOSE_GRACE: Duration = Duration::from_millis(100);

pub(super) struct Runtime {
    view: WebView,
    ladybird: Ladybird,
    /// What Ladybird's callback raised during the last wait, handled after it returns.
    events: Rc<RefCell<Vec<Event>>>,
    /// Where frames go. Rune draws from it on the UI thread.
    pixels: Pixels,
    /// Frames shown so far; the page reads it to tell when the page animates.
    frames: Arc<AtomicU64>,
    gate: FrameGate,
    timing: Timing,
}

impl Runtime {
    pub(super) fn start(pixels: Pixels, frames: Arc<AtomicU64>) -> Result<Self, String> {
        let resources = directory("ELARIS_LADYBIRD_RESOURCE_DIR", "vendor/ladybird/Base/res");
        let profile = directory("ELARIS_LADYBIRD_PROFILE_DIR", "build/elaris-profile");
        std::fs::create_dir_all(&profile)
            .map_err(|e| format!("cannot create profile {}: {e}", profile.display()))?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let mut ladybird = Ladybird::new(&resources, &profile, move |e| sink.borrow_mut().push(e))?;
        let view = ladybird.create_view()?;
        Ok(Self {
            view,
            ladybird,
            events,
            pixels,
            frames,
            gate: FrameGate::default(),
            timing: Timing::default(),
        })
    }

    /// A handle any thread can use to end a [`Self::wait`].
    pub(super) fn waker(&mut self) -> Result<Waker, String> {
        self.ladybird.waker()
    }

    /// Runs Ladybird work, blocking until there is some or a waker wakes it.
    pub(super) fn wait(&mut self) -> Result<(), String> {
        self.ladybird.wait(None)
    }

    pub(super) fn apply(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Load(url) => self.view.load(&url),
            Command::Resize {
                width,
                height,
                scale,
            } => {
                self.gate.laid_out_at(scale);
                self.timing.resized((width as u32, height as u32));
                self.view.resize(width, height, scale)
            }
            Command::Mouse(event) => {
                self.timing.input();
                self.view.mouse_event(event)
            }
            Command::Key(event) => {
                self.timing.input();
                self.view.key_event(event)
            }
            Command::Focus(focused) => self.view.set_focus(focused),
            Command::ColorScheme(dark) => self.ladybird.set_color_scheme(dark),
            Command::Close => Ok(()),
        }
    }

    /// Turns the events Ladybird raised into updates, showing a new frame if there is one.
    pub(super) fn handle_events(&mut self) -> Vec<Update> {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        let mut updates = Vec::new();
        let mut frame_ready = false;
        for event in events {
            match event {
                Event::Navigation(url) => updates.push(Update::Navigation(url)),
                Event::Title(title) => updates.push(Update::Title(title)),
                Event::Loading(loading) => updates.push(Update::Loading(loading)),
                Event::ReadyToPaint => frame_ready = true,
            }
        }
        if frame_ready {
            match self.show_frame() {
                Ok(update) => updates.extend(update),
                Err(error) => updates.push(Update::Error(error)),
            }
        }
        updates
    }

    /// Shows Ladybird's newest frame if the gate lets it through. A frame of the size on show
    /// goes straight into `pixels`; one of a new size goes to the page as an update instead.
    fn show_frame(&mut self) -> Result<Option<Update>, String> {
        let Some(frame) = self.view.frame()? else {
            return Ok(None);
        };
        let skipped = match self.gate.check(frame.serial, frame.scale) {
            Verdict::Show { skipped } => skipped,
            Verdict::Duplicate => {
                perf::count(Count::Duplicate, 1);
                return Ok(None);
            }
            Verdict::Stale => {
                perf::count(Count::Stale, 1);
                return Ok(None);
            }
        };
        let rgba = perf::time(Stage::Copy, || self.view.copy_frame(&frame))?;
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.timing.shown(&frame, skipped);
        if !self.gate.shown(frame.serial, (frame.width, frame.height)) {
            perf::time(Stage::Set, || {
                self.pixels.set(frame.width, frame.height, rgba)
            });
            return Ok(None);
        }
        Ok(Some(Update::Resized {
            width: frame.width,
            height: frame.height,
            size: [
                (f64::from(frame.width) / frame.scale) as f32,
                (f64::from(frame.height) / frame.scale) as f32,
            ],
            rgba,
        }))
    }

    /// Closes the view, then keeps Ladybird running briefly so helper processes see it go.
    pub(super) fn close(self) {
        let Self {
            view, mut ladybird, ..
        } = self;
        drop(view);
        let deadline = Instant::now() + CLOSE_GRACE;
        while let Some(left) = deadline.checked_duration_since(Instant::now())
            && ladybird.wait(Some(left)).is_ok()
        {}
    }
}

/// Development timing (`ELARIS_PERF`): the last frame shown, the oldest input not yet on screen,
/// and the newest size not yet on screen.
#[derive(Default)]
struct Timing {
    last_frame: Option<Instant>,
    pending_input: Option<Instant>,
    pending_resize: Option<((u32, u32), Instant)>,
}

impl Timing {
    fn input(&mut self) {
        if perf::enabled() {
            self.pending_input.get_or_insert_with(Instant::now);
        }
    }

    fn resized(&mut self, size: (u32, u32)) {
        if perf::enabled() {
            self.pending_resize = Some((size, Instant::now()));
        }
    }

    fn shown(&mut self, frame: &Frame, skipped: u64) {
        if !perf::enabled() {
            return;
        }
        let now = Instant::now();
        perf::count(Count::Shown, 1);
        perf::count(Count::Skipped, skipped);
        if let Some(last) = self.last_frame.replace(now) {
            perf::record(Stage::FrameInterval, now - last);
        }
        if let Some(input) = self.pending_input.take() {
            perf::record(Stage::InputToFrame, now - input);
        }
        if let Some((size, since)) = self.pending_resize
            && size == (frame.width, frame.height)
        {
            perf::record(Stage::ResizeToFrame, now - since);
            self.pending_resize = None;
        }
    }
}

/// A directory from `variable`, or `fallback` within the source tree for development builds.
fn directory(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback))
}
