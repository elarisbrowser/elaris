// SPDX-License-Identifier: GPL-3.0-only

//! The Ladybird engine behind the page view, on a thread of its own so that nothing it does
//! (starting, running page work, copying frames, shutting down) blocks the UI thread. The page
//! sends it [`Command`]s and reads back [`Update`]s; frames go straight into the shared `Pixels`,
//! which Rune draws from any thread. Without the `ladybird-bridge` feature, starting it reports
//! why, so `cargo run` works without Ladybird.

use std::sync::Arc;

use elaris::ladybird::{KeyEvent, MouseEvent};

#[cfg(feature = "ladybird-bridge")]
pub(crate) use bridged::Engine;
#[cfg(not(feature = "ladybird-bridge"))]
pub(crate) use unavailable::Engine;

/// Wakes the UI so it reads new updates.
pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

/// What the page asks of the engine.
// Only an engine reads commands or makes updates; a build without one still names them.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum Command {
    Load(String),
    /// The viewport in device pixels and the display scale it is laid out at.
    Resize(i32, i32, f64),
    Mouse(MouseEvent),
    Key(KeyEvent),
    Focus(bool),
    /// Closes the page and shuts the engine down.
    Close,
}

/// What the engine tells the page.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum Update {
    Navigation(String),
    Title(String),
    Loading(bool),
    Error(String),
    /// A frame of a size the page has not shown yet, in device pixels. The page shows it itself,
    /// together with the layout for its size in points, so the two never disagree.
    Resized {
        width: u32,
        height: u32,
        size: [f32; 2],
        rgba: Vec<u8>,
    },
}

#[cfg(feature = "ladybird-bridge")]
mod bridged {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use elaris::ladybird::{Event, Ladybird, Waker, WebView};
    use rune::Pixels;

    use super::{Command, Notify, Update};
    use crate::perf::{self, Count, Stage};

    /// How long shutdown keeps running Ladybird so helper processes see the view close.
    const CLOSE_GRACE: Duration = Duration::from_millis(100);
    /// Frames laid out at a different display scale than the viewport's are stale.
    const SCALE_TOLERANCE: f64 = 1e-3;

    /// The page's handle on the engine thread. Dropping it shuts the engine down.
    pub(crate) struct Engine {
        /// Frames shown so far, counted on the engine thread.
        frames: Arc<AtomicU64>,
        commands: Sender<Command>,
        updates: Receiver<Update>,
        waker: Arc<OnceLock<Waker>>,
    }

    /// What the process needs to finish the engine as it exits. Rune ends the process from
    /// within its event loop once the window has gone, so the only place left to wait for the
    /// engine without the UI waiting too is an exit handler.
    struct Running {
        thread: JoinHandle<()>,
        commands: Sender<Command>,
        waker: Arc<OnceLock<Waker>>,
    }

    static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> i32;
    }

    /// Closes the engine if the page has not, and waits for it, so its helper processes exit
    /// and no Ladybird code is running when the process tears down Ladybird's statics. Exit
    /// handlers run in reverse order of registration, so this runs before those, which
    /// Ladybird registered as it started, earlier than this.
    extern "C" fn finish_at_exit() {
        let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(running) = running {
            let _ = running.commands.send(Command::Close);
            if let Some(waker) = running.waker.get() {
                waker.wake();
            }
            let _ = running.thread.join();
        }
    }

    impl Engine {
        /// Starts the engine thread, which shows its frames in `pixels` and calls `notify` when
        /// there are updates to read. Returns at once; starting Ladybird happens on the thread.
        pub(crate) fn start(pixels: Pixels, notify: Notify) -> Result<Self, String> {
            let (commands, received) = channel();
            let (sender, updates) = channel();
            let waker = Arc::new(OnceLock::new());
            let engine_waker = Arc::clone(&waker);
            let frames = Arc::new(AtomicU64::new(0));
            let shown = Arc::clone(&frames);
            let thread = std::thread::Builder::new()
                .name("elaris-engine".to_owned())
                .spawn(move || run(pixels, shown, &received, &sender, &engine_waker, &*notify))
                .map_err(|e| format!("cannot start the engine thread: {e}"))?;
            *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
                thread,
                commands: commands.clone(),
                waker: Arc::clone(&waker),
            });
            // SAFETY: `finish_at_exit` is a plain function that stays valid for the process.
            unsafe { atexit(finish_at_exit) };
            Ok(Self {
                frames,
                commands,
                updates,
                waker,
            })
        }

        /// Queues `command` and wakes the engine for it. Never blocks.
        pub(crate) fn send(&self, command: Command) {
            if self.commands.send(command).is_ok() {
                self.wake();
            }
        }

        /// How many frames the engine has shown, which rises while the page animates.
        pub(crate) fn frames(&self) -> u64 {
            self.frames.load(Ordering::Relaxed)
        }

        /// The updates that arrived since the last call. Never blocks.
        pub(crate) fn updates(&self) -> impl Iterator<Item = Update> + '_ {
            self.updates.try_iter()
        }

        /// Asks the engine to close the page and shut down, without waiting for it; the
        /// process waits for it as it exits.
        pub(crate) fn close(&self) {
            self.send(Command::Close);
        }

        fn wake(&self) {
            // Before Ladybird has started there is no waker; commands wait in the queue, and the
            // engine reads them once it starts.
            if let Some(waker) = self.waker.get() {
                waker.wake();
            }
        }
    }

    /// The engine thread: starts Ladybird, then runs its work as it comes, between commands.
    fn run(
        pixels: Pixels,
        frames: Arc<AtomicU64>,
        commands: &Receiver<Command>,
        updates: &Sender<Update>,
        waker: &OnceLock<Waker>,
        notify: &(dyn Fn() + Send + Sync),
    ) {
        let report = |update| {
            if updates.send(update).is_ok() {
                notify();
            }
        };
        let mut engine = match Thread::start(pixels, frames) {
            Ok(engine) => engine,
            Err(error) => return report(Update::Error(error)),
        };
        match engine.ladybird.waker() {
            Ok(engine_waker) => drop(waker.set(engine_waker)),
            Err(error) => return report(Update::Error(error)),
        }
        loop {
            loop {
                match commands.try_recv() {
                    Ok(Command::Close) | Err(TryRecvError::Disconnected) => return engine.close(),
                    Ok(command) => {
                        if let Err(error) = engine.apply(command) {
                            report(Update::Error(error));
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Err(error) = engine.ladybird.wait(None) {
                return report(Update::Error(error));
            }
            for update in perf::time(Stage::Pump, || engine.handle_events()) {
                report(update);
            }
        }
    }

    /// Ladybird and its one view, owned by the engine thread.
    struct Thread {
        view: Option<WebView>,
        ladybird: Ladybird,
        events: Rc<RefCell<Vec<Event>>>,
        pixels: Pixels,
        frames: Arc<AtomicU64>,
        /// The display scale the view was last laid out at.
        scale: Option<f64>,
        /// The serial and size of the frame on show.
        serial: Option<u64>,
        shown_size: Option<(u32, u32)>,
        /// Development timing: the last frame shown and the oldest input not yet on screen.
        last_frame: Option<Instant>,
        pending_input: Option<Instant>,
    }

    impl Thread {
        fn start(pixels: Pixels, frames: Arc<AtomicU64>) -> Result<Self, String> {
            let resources = directory("ELARIS_LADYBIRD_RESOURCE_DIR", "vendor/ladybird/Base/res");
            let profile = directory("ELARIS_LADYBIRD_PROFILE_DIR", "build/elaris-profile");
            std::fs::create_dir_all(&profile)
                .map_err(|e| format!("cannot create profile {}: {e}", profile.display()))?;
            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&events);
            let mut ladybird =
                Ladybird::new(&resources, &profile, move |e| sink.borrow_mut().push(e))?;
            let view = ladybird.create_view()?;
            Ok(Self {
                view: Some(view),
                ladybird,
                events,
                pixels,
                frames,
                scale: None,
                serial: None,
                shown_size: None,
                last_frame: None,
                pending_input: None,
            })
        }

        fn view(&mut self) -> &mut WebView {
            self.view
                .as_mut()
                .expect("the view lives until the engine closes")
        }

        fn apply(&mut self, command: Command) -> Result<(), String> {
            match command {
                Command::Load(url) => self.view().load(&url),
                Command::Resize(width, height, scale) => {
                    self.scale = Some(scale);
                    self.view().resize(width, height, scale)
                }
                Command::Mouse(event) => {
                    self.note_input();
                    self.view().mouse_event(event)
                }
                Command::Key(event) => {
                    self.note_input();
                    self.view().key_event(event)
                }
                Command::Focus(focused) => self.view().set_focus(focused),
                Command::Close => Ok(()),
            }
        }

        /// Turns the events Ladybird raised into updates, showing a new frame if there is one.
        fn handle_events(&mut self) -> Vec<Update> {
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

        /// Shows Ladybird's newest frame unless it is one already shown or one laid out at
        /// another display scale. A frame of a new size goes to the page as an update instead.
        fn show_frame(&mut self) -> Result<Option<Update>, String> {
            let Some(frame) = self.view().frame()? else {
                return Ok(None);
            };
            if self.serial == Some(frame.serial) {
                perf::count(Count::Duplicate, 1);
                return Ok(None);
            }
            if self
                .scale
                .is_some_and(|scale| (frame.scale - scale).abs() > SCALE_TOLERANCE)
            {
                perf::count(Count::Stale, 1);
                return Ok(None);
            }
            let rgba = perf::time(Stage::Copy, || self.view().copy_frame(&frame))?;
            let skipped = self
                .serial
                .map_or(0, |s| frame.serial.saturating_sub(s + 1));
            self.serial = Some(frame.serial);
            self.frames.fetch_add(1, Ordering::Relaxed);
            self.note_frame(skipped);
            let dimensions = (frame.width, frame.height);
            if self.shown_size.replace(dimensions) == Some(dimensions) {
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

        fn note_input(&mut self) {
            if perf::enabled() {
                self.pending_input.get_or_insert_with(Instant::now);
            }
        }

        fn note_frame(&mut self, skipped: u64) {
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
        }

        /// Closes the view, then keeps Ladybird running briefly so helper processes see it go.
        fn close(mut self) {
            drop(self.view.take());
            let deadline = Instant::now() + CLOSE_GRACE;
            while let Some(left) = deadline.checked_duration_since(Instant::now())
                && self.ladybird.wait(Some(left)).is_ok()
            {}
        }
    }

    /// A directory from `variable`, or `fallback` within the source tree for development builds.
    fn directory(variable: &str, fallback: &str) -> PathBuf {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback))
    }
}

#[cfg(not(feature = "ladybird-bridge"))]
mod unavailable {
    use rune::Pixels;

    use super::{Command, Notify, Update};

    /// Never constructed: this build has no engine.
    pub(crate) enum Engine {}

    impl Engine {
        pub(crate) fn start(_: Pixels, _: Notify) -> Result<Self, String> {
            Err("This build has no web engine. Rebuild with --features ladybird-bridge.".to_owned())
        }

        pub(crate) fn send(&self, _: Command) {
            match *self {}
        }

        pub(crate) fn updates(&self) -> std::iter::Empty<Update> {
            match *self {}
        }

        pub(crate) fn frames(&self) -> u64 {
            match *self {}
        }

        pub(crate) fn close(&self) {
            match *self {}
        }
    }
}
