// SPDX-License-Identifier: GPL-3.0-only

//! The Ladybird engine behind the page view, on a thread of its own so that nothing it does
//! (starting, running page work, copying frames, shutting down) blocks the UI thread. The page
//! sends it [`Command`]s and reads back [`Update`]s; frames go straight into the shared `Pixels`,
//! which Rune draws from any thread. Without the `ladybird-bridge` feature, starting it reports
//! why, so `cargo run` works without Ladybird.
//!
//! - `thread`: starting the thread, its loop, and finishing it as the process exits.
//! - `runtime`: Ladybird and its view, which only the engine thread touches.
//! - `frames`: which of Ladybird's frames are worth showing.

mod frames;
#[cfg(feature = "ladybird-bridge")]
mod runtime;
#[cfg(feature = "ladybird-bridge")]
mod thread;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};

use elaris::ladybird::{KeyEvent, MouseEvent};
use rune::Pixels;

/// Wakes the UI so it reads new updates.
pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

/// Wakes the engine thread for new commands. Set once Ladybird has started; until then commands
/// wait in the queue, and the engine reads them as soon as it starts.
type Wake = Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>;

/// What the page asks of the engine.
#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Load(String),
    /// The viewport in device pixels and the display scale it is laid out at.
    Resize {
        width: i32,
        height: i32,
        scale: f64,
    },
    Mouse(MouseEvent),
    Key(KeyEvent),
    Focus(bool),
    /// Whether pages see a dark colour scheme.
    ColorScheme(bool),
    /// Closes the page and shuts the engine down.
    Close,
}

/// What the engine tells the page.
#[derive(Debug)]
// Only the engine thread makes updates, which a build without the bridge lacks.
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

/// The page's handle on the engine thread. Nothing it does blocks.
// A build without the bridge never starts an engine.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) struct Engine {
    commands: Sender<Command>,
    updates: Receiver<Update>,
    /// Frames shown so far, counted on the engine thread.
    frames: Arc<AtomicU64>,
    wake: Wake,
}

#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
impl Engine {
    /// Starts the engine thread, which shows its frames in `pixels` and calls `notify` when
    /// there are updates to read. Returns at once; starting Ladybird happens on the thread.
    pub(crate) fn start(pixels: Pixels, notify: Notify) -> Result<Self, String> {
        #[cfg(feature = "ladybird-bridge")]
        return thread::start(pixels, notify);
        #[cfg(not(feature = "ladybird-bridge"))]
        {
            let _ = (pixels, notify);
            Err("This build has no web engine. Rebuild with --features ladybird-bridge.".to_owned())
        }
    }

    /// Queues `command` and wakes the engine for it.
    pub(crate) fn send(&self, command: Command) {
        if self.commands.send(command).is_ok()
            && let Some(wake) = self.wake.get()
        {
            wake();
        }
    }

    /// How many frames the engine has shown, which rises while the page animates.
    pub(crate) fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// The updates that arrived since the last call.
    pub(crate) fn updates(&self) -> impl Iterator<Item = Update> + '_ {
        self.updates.try_iter()
    }

    /// Asks the engine to close the page and shut down, without waiting for it; the process
    /// waits for it as it exits.
    pub(crate) fn close(&self) {
        self.send(Command::Close);
    }
}

/// Applies every queued command in order, except resizes: only the newest is applied, after the
/// rest. A live resize sends a size every UI frame, and laying out only the newest keeps Ladybird
/// on the size the window has now rather than working through old ones. Returns false once the
/// engine should close, without applying anything queued after the close.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
fn drain(commands: &Receiver<Command>, mut apply: impl FnMut(Command)) -> bool {
    let mut resize = None;
    loop {
        match commands.try_recv() {
            Ok(Command::Close) | Err(TryRecvError::Disconnected) => return false,
            Ok(command @ Command::Resize { .. }) => resize = Some(command),
            Ok(command) => apply(command),
            Err(TryRecvError::Empty) => break,
        }
    }
    if let Some(resize) = resize {
        apply(resize);
    }
    true
}

/// The engine thread's side of an [`Engine`] with no thread behind it, for tests to play.
#[cfg(test)]
pub(crate) struct FakeEngine {
    pub(crate) commands: Receiver<Command>,
    pub(crate) updates: Sender<Update>,
    pub(crate) frames: Arc<AtomicU64>,
}

#[cfg(test)]
impl Engine {
    pub(crate) fn fake() -> (Self, FakeEngine) {
        let (commands, received) = std::sync::mpsc::channel();
        let (sender, updates) = std::sync::mpsc::channel();
        let frames = Arc::new(AtomicU64::new(0));
        let engine = Self {
            commands,
            updates,
            frames: Arc::clone(&frames),
            wake: Wake::default(),
        };
        let fake = FakeEngine {
            commands: received,
            updates: sender,
            frames,
        };
        (engine, fake)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    fn resize(width: i32) -> Command {
        Command::Resize {
            width,
            height: 10,
            scale: 2.0,
        }
    }

    fn drained(queue: impl IntoIterator<Item = Command>) -> (Vec<Command>, bool) {
        let (sender, receiver) = channel();
        queue.into_iter().for_each(|c| sender.send(c).unwrap());
        let mut applied = Vec::new();
        let open = drain(&receiver, |c| applied.push(c));
        (applied, open)
    }

    #[test]
    fn applies_only_the_newest_resize_after_the_rest() {
        let (applied, open) = drained([
            resize(1),
            Command::Focus(true),
            resize(2),
            Command::Load("about:blank".into()),
            resize(3),
        ]);
        assert!(open);
        assert_eq!(
            applied,
            [
                Command::Focus(true),
                Command::Load("about:blank".into()),
                resize(3),
            ]
        );
    }

    #[test]
    fn stops_at_close_without_applying_what_follows() {
        let (applied, open) = drained([
            Command::Focus(true),
            resize(1),
            Command::Close,
            Command::Focus(false),
        ]);
        assert!(!open);
        assert_eq!(applied, [Command::Focus(true)]);
    }

    #[test]
    fn closes_once_the_page_drops_its_engine() {
        let (sender, receiver) = channel::<Command>();
        drop(sender);
        assert!(!drain(&receiver, |_| unreachable!()));
    }

    #[test]
    fn sending_wakes_the_engine_once_it_can_be_woken() {
        let (engine, fake) = Engine::fake();
        engine.send(Command::Focus(true));
        let woken = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&woken);
        let _ = engine.wake.set(Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        engine.close();
        assert_eq!(woken.load(Ordering::Relaxed), 1);
        let queued: Vec<_> = fake.commands.try_iter().collect();
        assert_eq!(queued, [Command::Focus(true), Command::Close]);
    }
}
