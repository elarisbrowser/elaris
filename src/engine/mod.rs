// SPDX-License-Identifier: GPL-3.0-only

//! The Ladybird engine behind every tab, on a thread of its own so that nothing it does
//! (starting, running page work, copying frames, shutting down) blocks the UI thread. One engine
//! runs one Ladybird application with a view per tab. The browser sends it [`Command`]s and reads
//! back [`Update`]s, each naming its tab; frames go straight into the tab's [`Surface`], which Rune
//! draws from any thread. Without the `ladybird-bridge` feature, starting it reports why, so
//! `cargo run` works without Ladybird.
//!
//! - `thread`: starting the thread, its loop, and finishing it as the process exits.
//! - `runtime`: Ladybird and its views, which only the engine thread touches.
//! - `frames`: which of Ladybird's frames are worth showing.

mod frames;
#[cfg(feature = "ladybird-bridge")]
mod runtime;
#[cfg(feature = "ladybird-bridge")]
mod thread;

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock};

use elaris::ladybird::{Favicon, KeyEvent, MouseEvent};
use rune::Pixels;

use crate::tab::TabId;

/// Wakes the UI so it reads new updates.
pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

/// Wakes the engine thread for new commands. Set once Ladybird has started; until then commands
/// wait in the queue, and the engine reads them as soon as it starts.
type Wake = Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>;

/// Where the engine shows a tab's frames, shared between the two threads.
#[derive(Clone, Default)]
pub(crate) struct Surface {
    /// The latest frame. The engine thread replaces it; Rune draws it.
    pub(crate) pixels: Pixels,
    /// Frames shown so far, which rises while the page animates.
    frames: Arc<AtomicU64>,
}

impl Surface {
    pub(crate) fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    #[cfg_attr(not(any(test, feature = "ladybird-bridge")), allow(dead_code))]
    fn count_frame(&self) {
        self.frames.fetch_add(1, Ordering::Relaxed);
    }
}

/// Two surfaces are equal when they are the same surface.
impl PartialEq for Surface {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frames, &other.frames)
    }
}

impl fmt::Debug for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Surface")
    }
}

/// What the browser asks of the engine.
#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    /// Creates the tab's view, which shows its frames in the surface.
    Open(TabId, Surface),
    View(TabId, ViewCommand),
    /// Destroys the tab's view. Nothing more is reported for it.
    Close(TabId),
    /// Whether pages see a dark colour scheme.
    ColorScheme(bool),
    /// Closes every view and stops the engine.
    Shutdown,
}

/// What one tab's view is asked to do.
#[derive(Debug, PartialEq)]
pub(crate) enum ViewCommand {
    Load(String),
    Reload,
    /// Moves through the session history: -1 goes back, 1 forward.
    Traverse(i32),
    /// The viewport in device pixels and the display scale it is laid out at.
    Resize {
        width: i32,
        height: i32,
        scale: f64,
    },
    Mouse(MouseEvent),
    Key(KeyEvent),
    Focus(bool),
    /// Whether the view is on screen; hidden ones stop painting.
    Visible(bool),
}

/// What the engine tells the browser.
#[derive(Debug)]
// Only the engine thread makes updates, which a build without the bridge lacks.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum Update {
    Tab(TabId, TabUpdate),
    /// The engine could not start or stopped; no tab will hear from it again.
    Failed(String),
}

/// What one tab's view reported.
#[derive(Debug)]
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum TabUpdate {
    Navigation(String),
    Title(String),
    Loading(bool),
    Favicon(Option<Favicon>),
    History {
        back: bool,
        forward: bool,
    },
    Error(String),
    /// A frame of a size the tab has not shown yet, in device pixels. The tab shows it itself,
    /// together with the layout for its size in points, so the two never disagree.
    Resized {
        width: u32,
        height: u32,
        size: [f32; 2],
        rgba: Vec<u8>,
    },
}

/// The browser's handle on the engine thread. Nothing it does blocks.
// A build without the bridge never starts an engine.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) struct Engine {
    commands: Sender<Command>,
    updates: Receiver<Update>,
    wake: Wake,
}

#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
impl Engine {
    /// Starts the engine thread, which calls `notify` when there are updates to read. Returns at
    /// once; starting Ladybird happens on the thread.
    pub(crate) fn start(notify: Notify) -> Result<Self, String> {
        #[cfg(feature = "ladybird-bridge")]
        return thread::start(notify);
        #[cfg(not(feature = "ladybird-bridge"))]
        {
            let _ = notify;
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

    /// The updates that arrived since the last call.
    pub(crate) fn updates(&self) -> impl Iterator<Item = Update> + '_ {
        self.updates.try_iter()
    }

    /// Asks the engine to close every view and stop, without waiting for it; the process waits
    /// for it as it exits.
    pub(crate) fn shut_down(&self) {
        self.send(Command::Shutdown);
    }
}

/// Applies every queued command in order, except resizes: only each tab's newest is applied,
/// after the rest. A live resize sends a size every UI frame, and laying out only the newest keeps
/// Ladybird on the size the window has now rather than working through old ones. Returns false
/// once the engine should stop, without applying anything queued after that.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
fn drain(commands: &Receiver<Command>, mut apply: impl FnMut(Command)) -> bool {
    let mut resizes: Vec<(TabId, ViewCommand)> = Vec::new();
    loop {
        match commands.try_recv() {
            Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return false,
            Ok(Command::View(tab, resize @ ViewCommand::Resize { .. })) => {
                resizes.retain(|(t, _)| *t != tab);
                resizes.push((tab, resize));
            }
            Ok(command) => apply(command),
            Err(TryRecvError::Empty) => break,
        }
    }
    for (tab, resize) in resizes {
        apply(Command::View(tab, resize));
    }
    true
}

/// The engine thread's side of an [`Engine`] with no thread behind it, for tests to play.
#[cfg(test)]
pub(crate) struct FakeEngine {
    pub(crate) commands: Receiver<Command>,
    pub(crate) updates: Sender<Update>,
}

#[cfg(test)]
impl Engine {
    pub(crate) fn fake() -> (Self, FakeEngine) {
        let (commands, received) = std::sync::mpsc::channel();
        let (sender, updates) = std::sync::mpsc::channel();
        let engine = Self {
            commands,
            updates,
            wake: Wake::default(),
        };
        let fake = FakeEngine {
            commands: received,
            updates: sender,
        };
        (engine, fake)
    }
}

#[cfg(test)]
impl Surface {
    /// What the engine thread does as it shows a frame.
    pub(crate) fn fake_frame(&self) {
        self.count_frame();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    fn resize(tab: u64, width: i32) -> Command {
        Command::View(
            TabId::from_raw(tab),
            ViewCommand::Resize {
                width,
                height: 10,
                scale: 2.0,
            },
        )
    }

    fn focus(tab: u64) -> Command {
        Command::View(TabId::from_raw(tab), ViewCommand::Focus(true))
    }

    fn drained(queue: impl IntoIterator<Item = Command>) -> (Vec<Command>, bool) {
        let (sender, receiver) = channel();
        queue.into_iter().for_each(|c| sender.send(c).unwrap());
        let mut applied = Vec::new();
        let open = drain(&receiver, |c| applied.push(c));
        (applied, open)
    }

    #[test]
    fn applies_only_each_tabs_newest_resize_after_the_rest() {
        let (applied, open) = drained([
            resize(1, 1),
            focus(1),
            resize(2, 5),
            resize(1, 2),
            Command::ColorScheme(true),
            resize(1, 3),
        ]);
        assert!(open);
        assert_eq!(
            applied,
            [
                focus(1),
                Command::ColorScheme(true),
                resize(2, 5),
                resize(1, 3)
            ]
        );
    }

    #[test]
    fn stops_at_shutdown_without_applying_what_follows() {
        let (applied, open) = drained([focus(1), resize(1, 1), Command::Shutdown, focus(2)]);
        assert!(!open);
        assert_eq!(applied, [focus(1)]);
    }

    #[test]
    fn stops_once_the_browser_drops_its_engine() {
        let (sender, receiver) = channel::<Command>();
        drop(sender);
        assert!(!drain(&receiver, |_| unreachable!()));
    }

    #[test]
    fn sending_wakes_the_engine_once_it_can_be_woken() {
        let (engine, fake) = Engine::fake();
        engine.send(focus(1));
        let woken = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&woken);
        let _ = engine.wake.set(Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        engine.shut_down();
        assert_eq!(woken.load(Ordering::Relaxed), 1);
        let queued: Vec<_> = fake.commands.try_iter().collect();
        assert_eq!(queued, [focus(1), Command::Shutdown]);
    }

    #[test]
    fn surfaces_are_equal_only_to_themselves() {
        let surface = Surface::default();
        assert_eq!(surface, surface.clone());
        assert_ne!(surface, Surface::default());
    }
}
