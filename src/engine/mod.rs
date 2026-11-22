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
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod gpu;
#[cfg(feature = "ladybird-bridge")]
mod runtime;
#[cfg(feature = "ladybird-bridge")]
mod thread;
#[cfg(any(test, feature = "ladybird-bridge"))]
mod thumbnails;

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};

use elaris::downloads::{DownloadId, Request as DownloadRequest, Update as DownloadUpdate};
use elaris::ladybird::{Favicon, KeyEvent, MediaAction, MouseEvent};
use rune::Pixels;

use crate::tab::TabId;

pub(crate) const THUMBNAIL_ASPECT: (u32, u32) = (16, 9);

/// Wakes the UI so it reads new updates.
pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

static WINDOW_WAKERS: Mutex<Option<HashMap<u64, Notify>>> = Mutex::new(None);

pub(crate) fn register_window_waker(window: u64, notify: Notify) {
    let mut slot = WINDOW_WAKERS
        .lock()
        .expect("window waker registry poisoned");
    slot.get_or_insert_with(HashMap::new).insert(window, notify);
}

pub(crate) fn unregister_window_waker(window: u64) {
    if let Some(wakers) = WINDOW_WAKERS
        .lock()
        .expect("window waker registry poisoned")
        .as_mut()
    {
        wakers.remove(&window);
    }
}

pub(crate) fn wake_all_windows() {
    let wakers = WINDOW_WAKERS
        .lock()
        .expect("window waker registry poisoned");
    if let Some(wakers) = wakers.as_ref() {
        for notify in wakers.values() {
            notify();
        }
    }
}

/// Wakes the engine thread for new commands. Set once Ladybird has started; until then commands
/// wait in the queue, and the engine reads them as soon as it starts.
type Wake = Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>;

/// Where the engine shows a tab's frames, shared between the two threads.
#[derive(Clone)]
pub(crate) struct Surface {
    /// The latest frame. The engine thread replaces it; Rune draws it.
    pub(crate) pixels: Pixels,
    /// Where GPU frames go instead of `pixels`, on platforms that share them with Rune's GPU.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) gpu: Option<gpu::GpuSurface>,
    /// Frames shown so far, which rises while the page animates.
    frames: Arc<AtomicU64>,
    pub(crate) preview_source: PreviewSource,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            pixels: Pixels::default(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            gpu: gpu::enabled().then(gpu::GpuSurface::new),
            frames: Arc::default(),
            preview_source: PreviewSource::default(),
        }
    }
}

impl Surface {
    /// Whether the surface can show its frames in a layer the window server composites (macOS,
    /// unless `ELARIS_REMOTE_LAYERS=0`): each frame reaches the screen on the next refresh, with no
    /// copy and no redraw of the window's.
    pub(crate) fn hosts_layer(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self.gpu.is_some() && gpu::remote_layers();
        #[cfg(not(target_os = "macos"))]
        false
    }

    pub(crate) fn request_preview_invalidation(&self) {
        self.preview_source.0.fetch_add(1, Ordering::Relaxed);
    }

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
    /// The flag opens it in the shared private session.
    Open(TabId, Surface, bool),
    AdoptPopup {
        source: TabId,
        request: u64,
        tab: TabId,
        surface: Surface,
    },
    View(TabId, ViewCommand),
    /// Destroys the tab's view. Nothing more is reported for it.
    Close(TabId),
    PreflightClose(TabId),
    /// Starts a browser-owned transfer through Ladybird (a retry, Save Link As).
    StartDownload(DownloadId, DownloadRequest),
    /// Applies a browser-owned download action on the engine thread.
    DownloadAction(DownloadId, DownloadAction),
    /// Whether pages see a dark colour scheme.
    ColorScheme(bool),
    /// Changes the native destination for subsequent downloads.
    SetDownloadDirectory(Option<std::path::PathBuf>),
    /// Replaces the autoplay default and its site exceptions.
    SetAutoplay(
        crate::permissions::AutoplayPolicy,
        Vec<(String, crate::permissions::AutoplayPolicy)>,
    ),
    /// Turns the content blocker on or off, and enables the built-in filter lists named by identifier.
    SetContentBlocking(bool, Vec<&'static str>),
    /// Clears engine data last used at or after the Unix millisecond time, or for all time.
    ClearBrowsingData(Option<i64>, elaris::ladybird::BrowsingData),
    #[cfg(feature = "ladybird-bridge")]
    SetPermissions(crate::permissions::PermissionStore),
    SetPermissionDefault(crate::permissions::PermissionKind, bool),
    SetPermission(
        String,
        crate::permissions::PermissionKind,
        crate::permissions::PermissionState,
    ),
    /// Closes every view and stops the engine.
    Shutdown,
}

/// An operation applied to a download on the engine thread.
#[derive(Debug, PartialEq)]
pub(crate) enum DownloadAction {
    Cancel,
    Pause,
    Resume,
    /// Opens the completed file at this path with the system's default handler.
    Open(std::path::PathBuf),
    /// Shows the completed file at this path in the system file manager.
    Reveal(std::path::PathBuf),
}

/// What one tab's view is asked to do.
#[derive(Debug, PartialEq)]
pub(crate) enum ViewCommand {
    Load(String),
    Reload,
    ReloadBypassingCache,
    SetZoom(u16),
    TraverseToStep(i32),
    NavigationItems(i32),
    StopLoading,
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
    Edit(elaris::ladybird::Edit),
    LoadPrepared {
        source: TabId,
        request: u64,
    },
    DiscardPrepared(u64),
    OpenContext {
        request: u64,
        new_tab: bool,
    },
    MediaAction(MediaAction),
    /// Searches the page asynchronously and highlights every result.
    Find {
        query: String,
        request_id: u64,
    },
    /// Selects the next highlighted match and reports the resulting position.
    FindNext {
        request_id: u64,
    },
    /// Selects the previous highlighted match and reports the resulting position.
    FindPrevious {
        request_id: u64,
    },
    /// Clears Ladybird's active match and highlights.
    FindEnd,
    ExitWebFullscreen,
    /// Starts or stops reporting the page's accessibility tree; starting asks for it whole.
    SetAccessibility(bool),
    /// Asks the page to `"press"`, `"focus"` or `"scroll_into_view"` an accessibility node.
    AccessibilityAction {
        node: i64,
        action: &'static str,
    },
    /// Sets the caret or selection in a text node, in character offsets.
    AccessibilitySelect {
        node: i64,
        start: i32,
        end: i32,
    },
    ReplyToExternalUrl {
        request_id: u64,
        open: bool,
    },
    ReplyToPermission {
        request_id: u64,
        choice: Option<bool>,
    },
    ReplyToDialog {
        request_id: u64,
        response: Option<String>,
    },
    /// Reads the current webpage selection without blocking the UI thread.
    SelectedText {
        request_id: u64,
    },
    ReplyToPicker {
        request_id: u64,
        response: Option<Vec<u8>>,
    },
    Focus(bool),
    /// Whether the view is on screen; hidden ones stop painting.
    Visible(bool),
    /// Whether the Compositor shows the view's frames itself, in a layer under the window's UI
    /// (`Surface::hosts_layer`), rather than handing them to the engine to show.
    HostLayer(bool),
}

impl ViewCommand {
    pub(crate) fn invalidates_preview(&self) -> bool {
        matches!(
            self,
            Self::Load(_)
                | Self::LoadPrepared { .. }
                | Self::ReloadBypassingCache
                | Self::TraverseToStep(_)
                | Self::Reload
                | Self::Traverse(_)
                | Self::OpenContext { new_tab: false, .. }
        )
    }
}

/// What the engine tells the browser.
#[derive(Debug)]
// Only the engine thread makes updates, which a build without the bridge lacks.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum Update {
    Tab(TabId, TabUpdate),
    Thumbnail(TabId, Thumbnail),
    /// Selected page text for the find bar's guarded initial query.
    FindSelection(TabId, u64, String),
    NewTab(TabId, String, u64, bool),
    Popup(TabId, u64, bool),
    PopupClosed(TabId),
    Peek(TabId, String, u64),
    ContextMenu(TabId, elaris::ladybird::PageMenu),
    Clipboard(TabId, String),
    CloseDecision(TabId, bool),
    FormPicker(TabId, Option<(u64, elaris::ladybird::FormPicker)>),
    Download(DownloadId, DownloadUpdate),
    DownloadStartFailed(DownloadId, String),
    DownloadActionFailed(DownloadId, String),
    DownloadDirectoryFailed(String),
    PermissionSaved(
        String,
        crate::permissions::PermissionKind,
        crate::permissions::PermissionState,
    ),
    /// The engine could not start or stopped; no tab will hear from it again.
    Failed(String),
}

/// Identifies both an engine surface and the UI navigation requests sent to it.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreviewSource(Arc<AtomicU64>);

impl PreviewSource {
    pub(super) fn revision(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    fn matches(&self, other: &Self, revision: u64) -> bool {
        Arc::ptr_eq(&self.0, &other.0) && self.revision() == revision
    }
}

#[derive(Debug)]
pub(crate) struct Thumbnail {
    pub(crate) source: PreviewSource,
    pub(crate) revision: u64,
    pub(crate) pixels: std::sync::Weak<Pixels>,
}

impl Thumbnail {
    pub(crate) fn get(&self, surface: &Surface) -> Option<Arc<Pixels>> {
        if self.source.matches(&surface.preview_source, self.revision) {
            self.pixels.upgrade()
        } else {
            None
        }
    }
}

/// What one tab's view reported.
#[derive(Debug)]
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
pub(crate) enum TabUpdate {
    FindResult {
        request_id: u64,
        result: FindResult,
    },
    Navigation(String),
    NavigationCommitted(String),
    ImeCaret(Option<[i32; 4]>),
    Title(String),
    Loading(bool),
    Favicon(Option<Favicon>),
    History {
        back: bool,
        forward: bool,
    },
    NavigationItems(Vec<elaris::ladybird::NavigationItem>),
    LoadError(String),
    HoveredLink(String),
    Tooltip(String),
    Cursor(String),
    PageDialog(Option<elaris::ladybird::PageDialog>),
    PermissionPrompt(Option<elaris::ladybird::PermissionPrompt>),
    ExternalUrl(Option<elaris::ladybird::ExternalUrl>),
    Accessibility(elaris::ladybird::AccessibilityEvent),
    WebFullscreen(bool),
    /// Whether the page cancelled a wheel event; a cancelled wheel keeps its gesture from becoming a swipe.
    WheelResult(bool),
    /// The Compositor shows the page's frames in this CoreAnimation context (0: no longer).
    RemoteLayer(u64),
    PermissionUse(crate::permissions::PermissionKind, bool),
    Error(String),
}

/// Ladybird's current match and optional total, reported asynchronously.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FindResult {
    /// Zero-based position of the current match.
    pub(crate) current_match_index: u64,
    /// Number of matches, when Ladybird reports it.
    pub(crate) total_match_count: Option<u64>,
}

/// The browser's handle on the engine thread. Nothing it does blocks.
// A build without the bridge never starts an engine.
#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct Engine {
    commands: Sender<Command>,
    updates: Arc<SharedUpdates>,
    wake: Wake,
}

struct SharedUpdates {
    receiver: Mutex<Receiver<Update>>,
    pending: Mutex<VecDeque<Update>>,
}

#[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
impl Engine {
    /// Starts the engine thread, which calls `notify` when there are updates to read. Returns at
    /// once; starting Ladybird happens on the thread.
    pub(crate) fn start(
        notify: Notify,
        profile: std::path::PathBuf,
        download_directory: Option<std::path::PathBuf>,
        display: elaris::ladybird::DisplayMetadata,
    ) -> Result<Self, String> {
        #[cfg(feature = "ladybird-bridge")]
        return thread::start(notify, profile, download_directory, display);
        #[cfg(not(feature = "ladybird-bridge"))]
        {
            let _ = notify;
            let _ = profile;
            let _ = download_directory;
            let _ = display;
            Err("This build has no web engine. Rebuild with --features ladybird-bridge.".to_owned())
        }
    }

    /// Starts Ladybird once for the process. Every browser window gets a command handle to the
    /// same runtime; updates are demultiplexed by globally unique tab IDs when each window polls.
    pub(crate) fn start_shared(
        notify: Notify,
        profile: std::path::PathBuf,
        download_directory: Option<std::path::PathBuf>,
        display: elaris::ladybird::DisplayMetadata,
    ) -> Result<Self, String> {
        static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
        ENGINE
            .get_or_init(|| Self::start(notify, profile, download_directory, display))
            .clone()
    }

    pub(crate) fn same_runtime(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.updates, &other.updates)
    }

    /// Queues `command` and wakes the engine for it.
    pub(crate) fn send(&self, command: Command) {
        if self.commands.send(command).is_ok()
            && let Some(wake) = self.wake.get()
        {
            wake();
        }
    }

    /// Takes only updates owned by this window's tabs. Other windows' notifications remain queued.
    pub(crate) fn updates_for(&self, tabs: &[TabId]) -> Vec<Update> {
        let mut pending = self
            .updates
            .pending
            .lock()
            .expect("engine update queue poisoned");
        let receiver = self
            .updates
            .receiver
            .lock()
            .expect("engine update receiver poisoned");
        pending.extend(receiver.try_iter());
        let mut owned = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(update) = pending.pop_front() {
            match update_tab(&update) {
                Some(id) if !id.is_live() => {}
                Some(id) if tabs.contains(&id) => owned.push(update),
                Some(_) => retained.push_back(update),
                None => owned.push(update),
            }
        }
        *pending = retained;
        owned
    }

    /// Drops queued notifications for a view that its owning window has destroyed.
    pub(crate) fn discard_tab_updates(&self, tab: TabId) {
        let mut pending = self
            .updates
            .pending
            .lock()
            .expect("engine update queue poisoned");
        pending.retain(|update| update_tab(update) != Some(tab));
    }

    /// Asks the engine to close every view and stop, without waiting for it; the process waits
    /// for it as it exits.
    pub(crate) fn shut_down(&self) {
        self.send(Command::Shutdown);
    }
}

fn update_tab(update: &Update) -> Option<TabId> {
    match update {
        Update::Tab(id, _)
        | Update::Thumbnail(id, _)
        | Update::FindSelection(id, _, _)
        | Update::NewTab(id, _, _, _)
        | Update::Popup(id, _, _)
        | Update::PopupClosed(id)
        | Update::Peek(id, _, _)
        | Update::ContextMenu(id, _)
        | Update::Clipboard(id, _)
        | Update::CloseDecision(id, _)
        | Update::FormPicker(id, _) => Some(*id),
        _ => None,
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
            updates: Arc::new(SharedUpdates {
                receiver: Mutex::new(updates),
                pending: Mutex::new(VecDeque::new()),
            }),
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
