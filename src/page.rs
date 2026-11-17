// SPDX-License-Identifier: GPL-3.0-only

//! The page shown in the viewport: its engine, its latest frame and what the UI shows about it.
//! Rune draws the frame from `pixels`; this module never touches rendering, and never waits on
//! the engine, which runs on a thread of its own.

use std::cell::Cell;
use std::time::Instant;

use elaris::ladybird::{MouseEvent, MouseKind, WheelPhase, button};
use rune::{KeyEvent, Mods, Pixels};

use crate::engine::{Command, Engine, Notify, Update};
use crate::input;
use crate::perf::{self, Stage};

/// What one poll found that the rest of the UI shows.
#[derive(Default)]
pub(crate) struct Polled {
    pub(crate) changed: bool,
    pub(crate) url: Option<String>,
    pub(crate) title: Option<String>,
    /// The engine showed a frame since the last poll, so the page is likely animating.
    pub(crate) animating: bool,
}

pub(crate) struct Page {
    /// The latest frame, which the viewport draws at `frame_size`. The engine thread replaces it.
    pub(crate) pixels: Pixels,
    /// Points the latest frame covers.
    pub(crate) frame_size: [f32; 2],
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    engine: Option<Engine>,
    /// Wakes the UI when the engine has updates; Rune hands it over before the first poll.
    notify: Option<Notify>,
    started: bool,
    pending_load: Option<String>,
    /// The display scale, as the view last saw it.
    scale: Cell<f32>,
    /// Whether the interface is dark, as the view last drew it, and what the engine was told.
    dark: Cell<bool>,
    applied_dark: Cell<Option<bool>>,
    /// The viewport's size in points, from layout.
    viewport: [f32; 2],
    /// The device-pixel size and scale the engine was last given.
    applied: Option<(i32, i32, f64)>,
    focus: Focus,
    /// The engine's frame count at the last poll.
    frames: u64,
    /// Development timing: the last poll.
    last_poll: Option<Instant>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Focus {
    window: bool,
    viewport: bool,
    applied: bool,
}

impl Page {
    pub(crate) fn new(initial_url: Option<String>) -> Self {
        Self {
            pixels: Pixels::new(),
            frame_size: [0.0; 2],
            loading: false,
            error: None,
            engine: None,
            notify: None,
            started: false,
            pending_load: initial_url,
            scale: Cell::new(1.0),
            dark: Cell::new(false),
            applied_dark: Cell::new(None),
            viewport: [0.0; 2],
            applied: None,
            focus: Focus {
                window: true,
                ..Focus::default()
            },
            frames: 0,
            last_poll: None,
        }
    }

    /// Takes the waker that lets the engine thread run the UI's poll.
    pub(crate) fn set_notify(&mut self, notify: Notify) {
        self.notify = Some(notify);
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    /// Records whether the interface is dark, so pages follow it. The view calls it as it
    /// builds, because only the view knows the system's appearance; a change goes to the engine
    /// at once, since no poll may follow this build.
    pub(crate) fn note_dark(&self, dark: bool) {
        self.dark.set(dark);
        self.send_color_scheme();
    }

    fn send_color_scheme(&self) {
        let dark = self.dark.get();
        if let Some(engine) = &self.engine
            && self.applied_dark.replace(Some(dark)) != Some(dark)
        {
            engine.send(Command::ColorScheme(dark));
        }
    }

    pub(crate) fn set_viewport(&mut self, size: [f32; 2]) {
        self.viewport = size;
    }

    pub(crate) fn load(&mut self, url: String) {
        self.error = None;
        match &self.engine {
            Some(engine) => engine.send(Command::Load(url)),
            None => self.pending_load = Some(url),
        }
    }

    pub(crate) fn set_window_focus(&mut self, focused: bool) {
        self.focus.window = focused;
    }

    pub(crate) fn set_viewport_focus(&mut self, focused: bool) {
        self.focus.viewport = focused;
    }

    pub(crate) fn pointer(&mut self, kind: MouseKind, point: [f32; 2], held: bool, clicks: u32) {
        let mut event = input::mouse(kind, point, self.scale.get(), Mods::default());
        if matches!(kind, MouseKind::Down | MouseKind::Up) {
            event.button = button::PRIMARY;
        }
        if held {
            event.buttons = button::PRIMARY;
        }
        event.clicks = clicks as i32;
        self.send_mouse(event);
    }

    pub(crate) fn wheel(
        &mut self,
        point: [f32; 2],
        delta: [f32; 2],
        precise: bool,
        phase: WheelPhase,
    ) {
        let event = input::wheel(point, delta, precise, phase, self.scale.get());
        self.send_mouse(event);
    }

    /// Sends a key press to the page; false when the page takes no such key.
    pub(crate) fn key(&mut self, event: &KeyEvent) -> bool {
        let Some(engine) = &self.engine else {
            return false;
        };
        let keys = input::keys(event);
        for key in &keys {
            engine.send(Command::Key(*key));
        }
        !keys.is_empty()
    }

    fn send_mouse(&mut self, event: MouseEvent) {
        if let Some(engine) = &self.engine {
            engine.send(Command::Mouse(event));
        }
    }

    /// Starts the engine on first use, then forwards viewport and focus changes and reads what
    /// the engine reported. Rune calls it at the start of every frame; it never blocks.
    pub(crate) fn poll(&mut self) -> Polled {
        if perf::enabled() {
            let now = Instant::now();
            if let Some(last) = self.last_poll.replace(now) {
                perf::record(Stage::Poll, now - last);
            }
        }
        let mut polled = Polled::default();
        if !self.started {
            self.started = true;
            polled.changed = true;
            let notify = self
                .notify
                .clone()
                .unwrap_or_else(|| std::sync::Arc::new(|| {}));
            match Engine::start(self.pixels.clone(), notify) {
                Ok(engine) => self.engine = Some(engine),
                Err(error) => self.error = Some(error),
            }
        }
        self.sync_engine();
        let Some(engine) = &self.engine else {
            return polled;
        };
        let frames = engine.frames();
        polled.animating = std::mem::replace(&mut self.frames, frames) != frames;
        for update in engine.updates() {
            polled.changed = true;
            match update {
                Update::Navigation(url) => {
                    polled.url = Some(url);
                    self.error = None;
                }
                Update::Title(title) => polled.title = Some(title),
                Update::Loading(loading) => self.loading = loading,
                Update::Error(error) => self.error = Some(error),
                Update::Resized {
                    width,
                    height,
                    size,
                    rgba,
                } => {
                    self.pixels.set(width, height, rgba);
                    self.frame_size = size;
                }
            }
        }
        polled
    }

    /// Asks the engine to stop with its helper processes, without waiting for it.
    pub(crate) fn shut_down(&mut self) {
        if let Some(engine) = &self.engine {
            engine.close();
        }
    }

    fn sync_engine(&mut self) {
        let Some(engine) = &self.engine else {
            return;
        };
        let scale = self.scale.get();
        let size = (
            input::device(self.viewport[0], scale),
            input::device(self.viewport[1], scale),
        );
        let wanted = (size.0, size.1, f64::from(scale));
        if size.0 > 0 && size.1 > 0 && self.applied != Some(wanted) {
            engine.send(Command::Resize(wanted.0, wanted.1, wanted.2));
            self.applied = Some(wanted);
        }
        // Before the first load, so the first page lays out in the right scheme.
        self.send_color_scheme();
        // The first load waits for the first viewport, so the page lays out at its real size.
        if self.applied.is_some()
            && let Some(url) = self.pending_load.take()
        {
            engine.send(Command::Load(url));
        }
        let focused = self.focus.window && self.focus.viewport;
        if focused != self.focus.applied {
            engine.send(Command::Focus(focused));
            self.focus.applied = focused;
        }
    }
}
