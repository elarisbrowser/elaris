//! Ladybird integration. The plain event, frame and input types are always available, so input
//! translation can be tested without the engine; the owner-thread FFI wrappers need the
//! `ladybird-bridge` feature.

#[cfg(feature = "ladybird-bridge")]
mod bridge;
#[cfg(feature = "ladybird-bridge")]
pub use bridge::{Ladybird, Waker, WebView};

/// Ladybird's hit-tested target. Opening it uses the retained navigation descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageMenu {
    pub request: u64,
    pub url: String,
    pub image: bool,
    pub editable: bool,
    pub selection: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Copy,
    Cut,
    Paste(String),
    SelectAll,
    Mark(String),
    Commit(String),
}

/// A page-owned dialog awaiting a response from the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDialog {
    pub request_id: u64,
    pub kind: PageDialogKind,
    pub message: String,
    pub default_text: String,
}

/// The kind of user response the page expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

/// What a view raised, delivered with the id it was created with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Navigation(String),
    /// The active top-level document committed this URL, or a history traversal completed.
    NavigationCommitted(String),
    NewTab {
        url: String,
        request: u64,
        activate: bool,
    },
    /// A Shift-clicked hyperlink whose prepared navigation is still owned by its source view.
    Peek {
        url: String,
        request: u64,
    },
    /// The active document is an engine error page or its renderer crashed.
    LoadError(String),
    /// The absolute destination of the hovered link, or an empty string when it is cleared.
    HoveredLink(String),
    /// The hovered element's tooltip, or an empty string when it is cleared.
    Tooltip(String),
    /// Ladybird's standard cursor name.
    Cursor(String),
    PageDialog(Option<PageDialog>),
    CloseDecision(bool),
    ContextMenu(PageMenu),
    Clipboard(String),
    ImeCaret(Option<[i32; 4]>),
    Title(String),
    Loading(bool),
    ReadyToPaint,
    /// Whether the view can go back or forward through its session history now.
    History {
        back: bool,
        forward: bool,
    },
    /// The page's favicon changed, or was cleared when its active document changed.
    Favicon(Option<Favicon>),
    /// Ladybird completed a find request for a view.
    FindResult {
        request_id: u64,
        current_match_index: u64,
        total_match_count: Option<u64>,
    },
    /// The selected webpage text returned for a find bar opened on that view.
    SelectedText {
        request_id: u64,
        text: String,
    },
    /// A copied download snapshot raised by Ladybird's application-level downloader.
    Download(crate::downloads::Update),
}

/// A favicon copied from Ladybird as tightly packed, straight-alpha RGBA8 pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    /// The favicon's pixel width.
    pub width: u32,
    /// The favicon's pixel height.
    pub height: u32,
    /// `width * height * 4` RGBA8 bytes, with rows ordered top to bottom.
    pub rgba: Vec<u8>,
}

/// The frame a view displays, in device pixels. [`WebView::copy_frame`] returns its pixels as
/// tightly packed, opaque RGBA8 rows of `stride` bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub scale: f64,
    /// `[x, y, width, height]` changed since the last copy; empty when nothing did.
    pub damage: [i32; 4],
    pub serial: u64,
}

/// Mouse button bit flags, as the C ABI defines them.
pub mod button {
    pub const PRIMARY: i32 = 1;
    pub const SECONDARY: i32 = 2;
    pub const MIDDLE: i32 = 4;
}

/// Modifier bit flags, as the C ABI defines them. `SUPER` is Command on macOS.
pub mod modifier {
    pub const ALT: i32 = 1;
    pub const CTRL: i32 = 2;
    pub const SHIFT: i32 = 4;
    pub const SUPER: i32 = 8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MouseKind {
    Down = 1,
    Up = 2,
    Move = 3,
    Leave = 4,
    Wheel = 5,
    Cancel = 6,
}

/// A pointer event at a device-pixel position in the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub x: i32,
    pub y: i32,
    /// The button that changed, for `Down` and `Up`.
    pub button: i32,
    /// Every button held after the event.
    pub buttons: i32,
    pub modifiers: i32,
    /// `Wheel` only: logical pixels to scroll by, positive toward the page's end.
    pub wheel: (f64, f64),
    /// `Wheel` only: touchpad-like pixel deltas rather than wheel notches.
    pub precise: bool,
    /// `Wheel` only: where a touchpad scroll is in its gesture.
    pub phase: WheelPhase,
    pub clicks: i32,
}

/// Where a touchpad scroll is in its gesture, as the C ABI numbers it. Wheel notches have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum WheelPhase {
    #[default]
    None = 0,
    /// Fingers on the touchpad.
    Ongoing = 1,
    /// Coasting after the fingers lifted.
    Momentum = 2,
    /// The fingers lifted, or the momentum stopped.
    Ended = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Return,
    Backspace,
    Delete,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Insert,
    PrintScreen,
    ScrollLock,
    Pause,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub pressed: bool,
    pub modifiers: i32,
    pub repeat: bool,
}
