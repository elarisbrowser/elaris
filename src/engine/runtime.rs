// SPDX-License-Identifier: GPL-3.0-only

//! Ladybird and its views, owned by the engine thread: applying commands, turning Ladybird's
//! events into updates for the tab that raised them, and copying frames out.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use elaris::ladybird::{Event, Frame, Ladybird, Waker, WebView};

use super::frames::{FrameGate, Verdict};
use super::{Command, Surface, TabUpdate, Update, ViewCommand};
use crate::perf::{self, Count, Stage};
use crate::tab::TabId;

/// How long closing keeps running Ladybird so helper processes see the views close.
const CLOSE_GRACE: Duration = Duration::from_millis(100);

/// One tab's view and where its frames go.
struct View {
    view: WebView,
    surface: Surface,
    gate: FrameGate,
}

pub(super) struct Runtime {
    /// Declared before `ladybird`, so views are destroyed before the application.
    views: HashMap<TabId, View>,
    ladybird: Ladybird,
    /// What Ladybird's callback raised during the last wait, by view id, handled after it
    /// returns.
    events: Rc<RefCell<Vec<(u64, Event)>>>,
    timing: Timing,
}

impl Runtime {
    pub(super) fn start() -> Result<Self, String> {
        let resources = directory("ELARIS_LADYBIRD_RESOURCE_DIR", "vendor/ladybird/Base/res");
        let profile = directory("ELARIS_LADYBIRD_PROFILE_DIR", "build/elaris-profile");
        std::fs::create_dir_all(&profile)
            .map_err(|e| format!("cannot create profile {}: {e}", profile.display()))?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let ladybird = Ladybird::new(&resources, &profile, move |view, event| {
            sink.borrow_mut().push((view, event));
        })?;
        Ok(Self {
            views: HashMap::new(),
            ladybird,
            events,
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

    /// Applies `command`, returning the error to report if it failed. Commands for a tab that
    /// has closed are dropped: the browser may have sent them before it closed the tab.
    pub(super) fn apply(&mut self, command: Command) -> Option<Update> {
        let (tab, result) = match command {
            Command::Open(tab, surface) => (tab, self.open(tab, surface)),
            Command::Close(tab) => {
                self.views.remove(&tab);
                return None;
            }
            Command::View(tab, command) => {
                let view = self.views.get_mut(&tab)?;
                (tab, apply_to_view(view, command, &mut self.timing))
            }
            Command::ColorScheme(dark) => {
                return self
                    .ladybird
                    .set_color_scheme(dark)
                    .err()
                    .map(Update::Failed);
            }
            Command::Shutdown => return None,
        };
        result
            .err()
            .map(|error| Update::Tab(tab, TabUpdate::Error(error)))
    }

    fn open(&mut self, tab: TabId, surface: Surface) -> Result<(), String> {
        let view = self.ladybird.create_view(tab.raw())?;
        self.views.insert(
            tab,
            View {
                view,
                surface,
                gate: FrameGate::default(),
            },
        );
        Ok(())
    }

    /// Turns the events Ladybird raised into updates, showing each tab's new frame if it has
    /// one. Events from views that have closed since are dropped.
    pub(super) fn handle_events(&mut self) -> Vec<Update> {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        let mut updates = Vec::new();
        let mut painted: Vec<TabId> = Vec::new();
        for (id, event) in events {
            let tab = TabId::from_raw(id);
            if !self.views.contains_key(&tab) {
                continue;
            }
            let update = match event {
                Event::Navigation(url) => TabUpdate::Navigation(url),
                Event::Title(title) => TabUpdate::Title(title),
                Event::Loading(loading) => TabUpdate::Loading(loading),
                Event::History { back, forward } => TabUpdate::History { back, forward },
                Event::ReadyToPaint => {
                    if !painted.contains(&tab) {
                        painted.push(tab);
                    }
                    continue;
                }
            };
            updates.push(Update::Tab(tab, update));
        }
        for tab in painted {
            let Some(view) = self.views.get_mut(&tab) else {
                continue;
            };
            match show_frame(view, &mut self.timing) {
                Ok(update) => updates.extend(update.map(|u| Update::Tab(tab, u))),
                Err(error) => updates.push(Update::Tab(tab, TabUpdate::Error(error))),
            }
        }
        updates
    }

    /// Closes every view, then keeps Ladybird running briefly so helper processes see them go.
    pub(super) fn close(self) {
        let Self {
            views,
            mut ladybird,
            ..
        } = self;
        drop(views);
        let deadline = Instant::now() + CLOSE_GRACE;
        while let Some(left) = deadline.checked_duration_since(Instant::now())
            && ladybird.wait(Some(left)).is_ok()
        {}
    }
}

fn apply_to_view(view: &mut View, command: ViewCommand, timing: &mut Timing) -> Result<(), String> {
    let web = &mut view.view;
    match command {
        ViewCommand::Load(url) => web.load(&url),
        ViewCommand::Reload => web.reload(),
        ViewCommand::Traverse(delta) => web.traverse_history(delta),
        ViewCommand::Resize {
            width,
            height,
            scale,
        } => {
            view.gate.laid_out_at(scale);
            timing.resized((width as u32, height as u32));
            web.resize(width, height, scale)
        }
        ViewCommand::Mouse(event) => {
            timing.input();
            web.mouse_event(event)
        }
        ViewCommand::Key(event) => {
            timing.input();
            web.key_event(event)
        }
        ViewCommand::Focus(focused) => web.set_focus(focused),
        ViewCommand::Visible(visible) => web.set_visible(visible),
    }
}

/// Shows the view's newest frame if the gate lets it through. A frame of the size on show goes
/// straight into the surface; one of a new size goes to the tab as an update instead.
fn show_frame(view: &mut View, timing: &mut Timing) -> Result<Option<TabUpdate>, String> {
    let Some(frame) = view.view.frame()? else {
        return Ok(None);
    };
    let skipped = match view.gate.check(frame.serial, frame.scale) {
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
    let rgba = perf::time(Stage::Copy, || view.view.copy_frame(&frame))?;
    view.surface.count_frame();
    timing.shown(&frame, skipped);
    if !view.gate.shown(frame.serial, (frame.width, frame.height)) {
        perf::time(Stage::Set, || {
            view.surface.pixels.set(frame.width, frame.height, rgba)
        });
        return Ok(None);
    }
    Ok(Some(TabUpdate::Resized {
        width: frame.width,
        height: frame.height,
        size: [
            (f64::from(frame.width) / frame.scale) as f32,
            (f64::from(frame.height) / frame.scale) as f32,
        ],
        rgba,
    }))
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
