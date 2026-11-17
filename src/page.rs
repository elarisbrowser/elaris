// SPDX-License-Identifier: GPL-3.0-only

//! The page shown in the viewport: its engine, its latest frame and what the UI shows about it.
//! Rune draws the frame from `pixels`; this module never touches rendering.

use std::cell::Cell;
use std::time::{Duration, Instant};

use elaris::ladybird::{Event, MouseEvent, MouseKind, button};
use rune::{KeyEvent, Mods, Pixels};

use crate::engine::Engine;
use crate::input;

/// How often Ladybird is pumped while the page is busy, and while it is quiet.
const ACTIVE_PUMP_INTERVAL: Duration = Duration::from_millis(4);
const IDLE_PUMP_INTERVAL: Duration = Duration::from_millis(50);
/// How long the page counts as busy after its last event or input.
const ACTIVE_WINDOW: Duration = Duration::from_millis(750);
/// Frames laid out at a different display scale than the viewport's are stale.
const SCALE_TOLERANCE: f64 = 1e-3;

/// What one poll found that the rest of the UI shows.
#[derive(Default)]
pub(crate) struct Polled {
    pub(crate) changed: bool,
    pub(crate) url: Option<String>,
    pub(crate) title: Option<String>,
}

pub(crate) struct Page {
    /// The latest frame, which the viewport draws at `frame_size`.
    pub(crate) pixels: Pixels,
    /// Points the latest frame covers.
    pub(crate) frame_size: [f32; 2],
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    engine: Option<Engine>,
    started: bool,
    pending_load: Option<String>,
    /// The display scale, as the view last saw it.
    scale: Cell<f32>,
    /// The viewport's size in points, from layout.
    viewport: [f32; 2],
    /// The device-pixel size and scale the engine was last given.
    applied: Option<(i32, i32, f64)>,
    focus: Focus,
    frame_serial: Option<u64>,
    last_activity: Instant,
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
            started: false,
            pending_load: initial_url,
            scale: Cell::new(1.0),
            viewport: [0.0; 2],
            applied: None,
            focus: Focus {
                window: true,
                ..Focus::default()
            },
            frame_serial: None,
            last_activity: Instant::now(),
        }
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    pub(crate) fn set_viewport(&mut self, size: [f32; 2]) {
        self.viewport = size;
    }

    pub(crate) fn load(&mut self, url: String) {
        self.error = None;
        match self.engine.as_mut() {
            Some(engine) => {
                if let Err(error) = engine.load(&url) {
                    self.error = Some(error);
                }
                self.last_activity = Instant::now();
            }
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

    pub(crate) fn wheel(&mut self, point: [f32; 2], delta: [f32; 2], precise: bool) {
        let event = input::wheel(point, delta, precise, self.scale.get());
        self.send_mouse(event);
    }

    /// Sends a key press to the page; false when the page takes no such key.
    pub(crate) fn key(&mut self, event: &KeyEvent) -> bool {
        let keys = input::keys(event);
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        for key in &keys {
            if let Err(error) = engine.key(*key) {
                self.error = Some(error);
            }
        }
        self.last_activity = Instant::now();
        !keys.is_empty()
    }

    fn send_mouse(&mut self, event: MouseEvent) {
        if let Some(engine) = self.engine.as_mut() {
            if let Err(error) = engine.mouse(event) {
                self.error = Some(error);
            }
            self.last_activity = Instant::now();
        }
    }

    /// Starts the engine on first use, then forwards viewport and focus changes, runs ready
    /// engine work and takes the newest frame. Call it on the UI thread every pump interval.
    pub(crate) fn poll(&mut self) -> Polled {
        let mut polled = Polled::default();
        if !self.started {
            self.started = true;
            polled.changed = true;
            match Engine::start() {
                Ok(engine) => self.engine = Some(engine),
                Err(error) => self.error = Some(error),
            }
        }
        if self.engine.is_none() {
            return polled;
        }

        if let Err(error) = self.sync_engine() {
            self.error = Some(error);
            polled.changed = true;
        }

        let Some(engine) = self.engine.as_mut() else {
            return polled;
        };
        let events = match engine.pump() {
            Ok(events) => events,
            Err(error) => {
                self.error = Some(error);
                polled.changed = true;
                return polled;
            }
        };
        let mut frame_ready = false;
        for event in &events {
            match event {
                Event::Navigation(url) => {
                    polled.url = Some(url.clone());
                    self.error = None;
                }
                Event::Title(title) => polled.title = Some(title.clone()),
                Event::Loading(loading) => self.loading = *loading,
                Event::ReadyToPaint => frame_ready = true,
            }
        }
        if !events.is_empty() {
            self.last_activity = Instant::now();
            polled.changed |= events.iter().any(|e| *e != Event::ReadyToPaint);
        }
        if frame_ready {
            polled.changed |= self.take_frame();
        }
        polled
    }

    /// How long until the next poll: brief while the page is busy, longer while it is quiet.
    /// `None` once there is no engine to pump.
    pub(crate) fn next_poll(&self) -> Option<Duration> {
        if !self.started {
            return Some(Duration::ZERO);
        }
        self.engine.as_ref()?;
        let busy = self.loading || self.last_activity.elapsed() < ACTIVE_WINDOW;
        Some(if busy {
            ACTIVE_PUMP_INTERVAL
        } else {
            IDLE_PUMP_INTERVAL
        })
    }

    /// Stops the engine and its helper processes.
    pub(crate) fn shut_down(&mut self) {
        self.engine = None;
    }

    fn sync_engine(&mut self) -> Result<(), String> {
        let Some(engine) = self.engine.as_mut() else {
            return Ok(());
        };
        let scale = self.scale.get();
        let size = (
            input::device(self.viewport[0], scale),
            input::device(self.viewport[1], scale),
        );
        if size.0 > 0 && size.1 > 0 && self.applied != Some((size.0, size.1, f64::from(scale))) {
            engine.resize(size.0, size.1, f64::from(scale))?;
            self.applied = Some((size.0, size.1, f64::from(scale)));
            self.last_activity = Instant::now();
        }
        // The first load waits for the first viewport, so the page lays out at its real size.
        if self.applied.is_some()
            && let Some(url) = self.pending_load.take()
        {
            engine.load(&url)?;
            self.last_activity = Instant::now();
        }
        let wanted = self.focus.window && self.focus.viewport;
        if wanted != self.focus.applied {
            engine.set_focus(wanted)?;
            self.focus.applied = wanted;
        }
        Ok(())
    }

    /// Shows the engine's newest frame unless it is one already shown or one laid out at another
    /// display scale. True when the frame's size changed, so the layout must too.
    fn take_frame(&mut self) -> bool {
        let Some(engine) = self.engine.as_mut() else {
            return false;
        };
        let frame = match engine.frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => return false,
            Err(error) => {
                self.error = Some(error);
                return true;
            }
        };
        let current_scale = f64::from(self.scale.get());
        if self.frame_serial == Some(frame.serial)
            || (frame.scale - current_scale).abs() > SCALE_TOLERANCE
        {
            return false;
        }
        let rgba = match engine.copy_frame(&frame) {
            Ok(rgba) => rgba,
            Err(error) => {
                self.error = Some(error);
                return true;
            }
        };
        self.frame_serial = Some(frame.serial);
        self.pixels.set(frame.width, frame.height, rgba);
        let size = [
            (f64::from(frame.width) / frame.scale) as f32,
            (f64::from(frame.height) / frame.scale) as f32,
        ];
        std::mem::replace(&mut self.frame_size, size) != size
    }
}
