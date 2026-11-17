// SPDX-License-Identifier: GPL-3.0-only

//! The page shown in the viewport: its engine, its latest frame and what the UI shows about it.
//! Rune draws the frame from `pixels`; this module never touches rendering, and never waits on
//! the engine, which runs on a thread of its own.

use std::cell::Cell;
use std::time::Instant;

use elaris::ladybird::{MouseEvent, MouseKind, WheelPhase};
use rune::{KeyEvent, Pixels};

use crate::engine::{Command, Engine, Update};
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
    /// None until the app connects one, and in a build without an engine.
    engine: Option<Engine>,
    /// The first page, held until the engine can lay it out.
    pending_load: Option<String>,
    /// The display scale, as the view last saw it.
    scale: Cell<f32>,
    /// Whether the interface is dark, as the view last drew it, and what the engine was told.
    /// Unknown until the view first builds, and nothing is sent until then.
    dark: Cell<Option<bool>>,
    applied_dark: Cell<Option<bool>>,
    /// The viewport's size in points, from layout.
    viewport: [f32; 2],
    /// The device-pixel size and scale the engine was last given.
    applied_viewport: Option<(i32, i32, f64)>,
    focus: Focus,
    /// The engine's frame count at the last poll.
    frames: u64,
    /// Development timing: the last poll.
    last_poll: Option<Instant>,
}

/// Whether the window and the viewport have focus, and what the engine was last told.
#[derive(Default)]
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
            pending_load: initial_url,
            scale: Cell::new(1.0),
            dark: Cell::new(None),
            applied_dark: Cell::new(None),
            viewport: [0.0; 2],
            applied_viewport: None,
            focus: Focus {
                window: true,
                ..Focus::default()
            },
            frames: 0,
            last_poll: None,
        }
    }

    /// Gives the page the engine that shows it.
    pub(crate) fn connect(&mut self, engine: Engine) {
        self.engine = Some(engine);
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    /// Records whether the interface is dark, so pages follow it. The view calls it as it
    /// builds, because only the view knows the system's appearance; a change goes to the engine
    /// at once, since no poll may follow this build.
    pub(crate) fn note_dark(&self, dark: bool) {
        self.dark.set(Some(dark));
        self.send_color_scheme();
    }

    fn send_color_scheme(&self) {
        if let Some(engine) = &self.engine
            && let Some(dark) = self.dark.get()
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
        let event = input::pointer(kind, point, self.scale.get(), held, clicks);
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

    /// Forwards viewport and focus changes and reads what the engine reported. Rune calls it at
    /// the start of every frame; it never blocks.
    pub(crate) fn poll(&mut self) -> Polled {
        if perf::enabled() {
            let now = Instant::now();
            if let Some(last) = self.last_poll.replace(now) {
                perf::record(Stage::Poll, now - last);
            }
        }
        let mut polled = Polled::default();
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
        let [width, height] = self.viewport.map(|points| input::device(points, scale));
        let wanted = (width, height, f64::from(scale));
        if width > 0 && height > 0 && self.applied_viewport != Some(wanted) {
            engine.send(Command::Resize {
                width,
                height,
                scale: wanted.2,
            });
            self.applied_viewport = Some(wanted);
        }
        // Before the first load, so the first page lays out in the right scheme.
        self.send_color_scheme();
        // The first load waits for the first viewport and the colour scheme, so the page lays out
        // at its real size and never starts in the wrong scheme.
        if self.applied_viewport.is_some()
            && self.applied_dark.get().is_some()
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use rune::{Key as RuneKey, Mods};

    use super::*;
    use crate::engine::FakeEngine;

    fn connected(url: Option<&str>) -> (Page, FakeEngine) {
        let mut page = Page::new(url.map(str::to_owned));
        let (engine, fake) = Engine::fake();
        page.connect(engine);
        (page, fake)
    }

    fn sent(fake: &FakeEngine) -> Vec<Command> {
        fake.commands.try_iter().collect()
    }

    /// What the view does as it builds and lays out.
    fn lay_out(page: &mut Page, size: [f32; 2], scale: f32, dark: bool) {
        page.note_scale(scale);
        page.note_dark(dark);
        page.set_viewport(size);
    }

    #[test]
    fn the_first_page_waits_for_its_viewport_and_colour_scheme() {
        let (mut page, fake) = connected(Some("about:blank"));
        page.poll();
        assert_eq!(sent(&fake), []);

        lay_out(&mut page, [100.0, 50.0], 2.0, true);
        page.poll();
        assert_eq!(
            sent(&fake),
            [
                // Building sends the scheme at once, ahead of the poll's commands.
                Command::ColorScheme(true),
                Command::Resize {
                    width: 200,
                    height: 100,
                    scale: 2.0,
                },
                Command::Load("about:blank".into()),
            ]
        );
    }

    #[test]
    fn sends_only_what_changed() {
        let (mut page, fake) = connected(None);
        lay_out(&mut page, [100.0, 50.0], 1.0, false);
        page.poll();
        sent(&fake);

        lay_out(&mut page, [100.0, 50.0], 1.0, false);
        page.poll();
        assert_eq!(sent(&fake), []);

        lay_out(&mut page, [100.0, 50.0], 2.0, true);
        page.poll();
        assert_eq!(
            sent(&fake),
            [
                Command::ColorScheme(true),
                Command::Resize {
                    width: 200,
                    height: 100,
                    scale: 2.0,
                },
            ]
        );
    }

    #[test]
    fn the_page_has_focus_only_while_the_window_and_viewport_both_do() {
        let (mut page, fake) = connected(None);
        page.set_viewport_focus(true);
        page.poll();
        assert_eq!(sent(&fake), [Command::Focus(true)]);

        page.set_window_focus(false);
        page.poll();
        assert_eq!(sent(&fake), [Command::Focus(false)]);

        page.set_viewport_focus(false);
        page.set_window_focus(true);
        page.poll();
        assert_eq!(sent(&fake), []);
    }

    #[test]
    fn shows_what_the_engine_reports() {
        let (mut page, fake) = connected(None);
        page.error = Some("old".into());
        for update in [
            Update::Loading(true),
            Update::Navigation("https://example.com/".into()),
            Update::Title("Example".into()),
            Update::Resized {
                width: 4,
                height: 2,
                size: [2.0, 1.0],
                rgba: vec![0; 32],
            },
        ] {
            fake.updates.send(update).unwrap();
        }
        let polled = page.poll();
        assert!(polled.changed);
        assert_eq!(polled.url.as_deref(), Some("https://example.com/"));
        assert_eq!(polled.title.as_deref(), Some("Example"));
        assert!(page.loading);
        assert_eq!(page.error, None);
        assert_eq!(page.frame_size, [2.0, 1.0]);

        fake.updates.send(Update::Error("failed".into())).unwrap();
        assert_eq!(page.poll().url, None);
        assert_eq!(page.error.as_deref(), Some("failed"));
        assert!(!page.poll().changed);
    }

    #[test]
    fn a_page_animates_while_the_engine_shows_frames() {
        let (mut page, fake) = connected(None);
        assert!(!page.poll().animating);
        fake.frames.fetch_add(3, Ordering::Relaxed);
        assert!(page.poll().animating);
        assert!(!page.poll().animating);
    }

    #[test]
    fn loading_clears_the_error_and_goes_to_the_engine() {
        let (mut page, fake) = connected(None);
        page.error = Some("old".into());
        page.load("https://example.com/".into());
        assert_eq!(page.error, None);
        assert_eq!(sent(&fake), [Command::Load("https://example.com/".into())]);
    }

    #[test]
    fn input_goes_to_the_engine_in_device_pixels() {
        let (mut page, fake) = connected(None);
        page.note_scale(2.0);
        page.pointer(MouseKind::Down, [1.0, 2.0], true, 1);
        let enter = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(page.key(&enter));
        let shortcut = KeyEvent {
            key: RuneKey::Command('q'),
            mods: Mods::default(),
        };
        assert!(!page.key(&shortcut));
        let sent = sent(&fake);
        assert!(matches!(
            sent[0],
            Command::Mouse(MouseEvent { x: 2, y: 4, .. })
        ));
        assert_eq!(sent.len(), 3);
    }

    #[test]
    fn without_an_engine_the_page_keeps_its_first_load_and_takes_no_keys() {
        let mut page = Page::new(None);
        page.load("https://example.com/".into());
        assert_eq!(page.pending_load.as_deref(), Some("https://example.com/"));
        let key = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(!page.key(&key));
        assert!(!page.poll().changed);
    }

    #[test]
    fn shutting_down_asks_the_engine_to_close() {
        let (mut page, fake) = connected(None);
        page.shut_down();
        assert_eq!(sent(&fake), [Command::Close]);
    }
}
