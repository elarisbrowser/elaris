// SPDX-License-Identifier: GPL-3.0-only

//! The browser's state: what it owns, and how it starts.

use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{input_routing, peek, split};
use crate::engine::Engine;
use crate::tab::{Tab, TabId};
use elaris_core::downloads::Download;

/// What one poll found.
#[derive(Default)]
pub(crate) struct Polled {
    /// Some tab's state changed, so the interface needs building again.
    pub(crate) changed: bool,
    /// This window's download list changed, so the kept history may need saving.
    pub(crate) downloads_changed: bool,
    pub(crate) permission_choices: Vec<(
        String,
        elaris_core::permissions::PermissionKind,
        elaris_core::permissions::PermissionState,
    )>,
    pub(crate) clipboard: Option<String>,
    pub(crate) close_decisions: Vec<(TabId, bool)>,
    pub(crate) form_pickers: Vec<(TabId, Option<(u64, elaris::ladybird::FormPicker)>)>,
    pub(crate) menu: Option<(TabId, elaris::ladybird::PageMenu)>,
    /// A visible tab showed a frame since the last poll, so it is likely animating.
    pub(crate) animating: bool,
    /// New preview handles. Pixel updates to existing handles wake Rune directly.
    pub(crate) thumbnails_ready: Vec<TabId>,
    /// URLs Ladybird confirmed as committed, with the title currently known for each tab.
    pub(crate) committed: Vec<(TabId, String, String)>,
    /// Titles reported for a tab's last committed URL.
    pub(crate) titles: Vec<(String, String)>,
    /// Asynchronously returned webpage selections requested by the open find bar.
    pub(crate) find_selections: Vec<(TabId, u64, String)>,
    /// The engine could not apply the selected download directory.
    pub(crate) download_directory_error: Option<String>,
    /// Popups pages opened, as (opener, request, activate), for the app to give a window.
    pub(crate) popups: Vec<(TabId, u64, bool)>,
    /// What pages loaded in hidden views reported.
    pub(crate) fetched: Vec<super::Fetched>,
    /// Tabs that fell asleep, for Memory Saver to count the memory they freed.
    pub(crate) slept: Vec<TabId>,
    /// Screenshots this window's tabs asked for: where each was saved, or why it could not be.
    pub(crate) screenshots: Vec<(TabId, Result<std::path::PathBuf, String>)>,
    /// The engine's processes, for the Task Manager in this tab.
    pub(crate) processes: Option<(TabId, Result<Vec<elaris_core::processes::Process>, String>)>,
    /// Why a process the Task Manager in this tab asked to end could not be ended.
    pub(crate) processes_error: Option<(TabId, String)>,
}

pub(crate) struct Browser {
    /// In the order the sidebar shows them.
    pub(super) tabs: Vec<Tab>,
    pub(super) active: Option<TabId>,
    /// Opens every page in the shared private session.
    pub(crate) private: bool,
    /// An assistive client reads this window, so pages report their accessibility trees.
    pub(super) accessibility: bool,
    /// The page trees last handed to Rune, to forget those of closed tabs.
    pub(super) accessibility_trees: std::collections::HashSet<rune::accesskit::TreeId>,
    pub(crate) peek: Option<peek::PeekState>,
    pub(super) peek_origin: Option<(TabId, [f32; 2])>,
    pub(super) promoted_visit: Option<(TabId, String, String)>,
    pub(crate) split: Option<split::Split>,
    pub(crate) split_view_limit: usize,
    /// The window's current page shape and chrome can cover the edges of a hosted surface.
    pub(super) hosted_pages_allowed: bool,
    pub(super) default_zoom: u16,
    /// The tab whose zoom changed since the window last polled.
    pub(super) zoom_changed: Option<TabId>,
    pub(super) recent: Vec<TabId>,
    pub(super) input: input_routing::Held,
    /// None until the app connects one, and in a build without an engine.
    pub(super) engine: Option<Engine>,
    /// Why there is no engine, which every tab shows.
    pub(super) engine_error: Option<String>,
    /// Downloads belong to the browser process, not to the tab that requested them.
    pub(super) downloads: Arc<Mutex<Vec<Download>>>,
    pub(super) last_download_snapshot: Vec<Download>,
    pub(super) window: Window,
    /// Development timing: the last poll.
    pub(super) last_poll: Option<Instant>,
    /// Pages loading in hidden views for their title and icon.
    pub(super) fetches: Vec<super::fetch::Fetch>,
}

/// Display state and the available area for all visible panes.
pub(super) struct Window {
    /// The display scale, as the view last saw it.
    pub(super) scale: Cell<f32>,
    /// Whether the interface is dark, as the view last drew it, and what the engine was told.
    /// Unknown until the view first builds, and nothing is sent until then.
    pub(super) dark: Cell<Option<bool>>,
    pub(super) applied_dark: Cell<Option<bool>>,
    /// The viewport's size in points, from layout.
    pub(super) viewport: [f32; 2],
    /// Whether the window, and the viewport within it, have focus.
    pub(super) focused: bool,
    pub(super) viewport_focused: bool,
    /// The native window is off-screen or the app has explicitly put it to rest.
    pub(super) suspended: bool,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            private: false,
            accessibility: false,
            accessibility_trees: Default::default(),
            peek: None,
            peek_origin: None,
            promoted_visit: None,
            split: None,
            split_view_limit: 2,
            hosted_pages_allowed: cfg!(target_os = "macos"),
            default_zoom: 100,
            zoom_changed: None,
            recent: Vec::new(),
            input: Default::default(),
            engine: None,
            engine_error: None,
            downloads: Arc::new(Mutex::new(Vec::new())),
            last_download_snapshot: Vec::new(),
            window: Window {
                scale: Cell::new(1.0),
                dark: Cell::new(None),
                applied_dark: Cell::new(None),
                viewport: [0.0; 2],
                focused: true,
                viewport_focused: false,
                suspended: false,
            },
            last_poll: None,
            fetches: Vec::new(),
        }
    }
}

impl Browser {
    pub(crate) fn transfer_buffer(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            ..Self::default()
        }
    }
}
