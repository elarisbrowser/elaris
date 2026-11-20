// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`Browser`].

mod history;

#[cfg(test)]
use crate::config::ConfigLoad;
use history::HistoryPage;
#[cfg(test)]
use rune::{Key, KeyEvent, Tx};
use rune::{PointerPress, TextEdit};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

use crate::browser::Browser;
use crate::omnibox;
use crate::permissions::{PermissionKind, PermissionStore};
use crate::settings::{Settings, StartupMode};
use crate::storage;
use crate::storage::{Session, Storage};
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::SidebarState;
#[cfg(test)]
use crate::ui::VIEWPORT;

/// How the Linux toolbar stays visible while browsing.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LinuxTitlebarMode {
    /// Reveal the toolbar while the pointer is in the window's top band.
    #[default]
    Hover,
    /// Keep the toolbar visible regardless of pointer position.
    Always,
}

#[cfg(target_os = "linux")]
impl LinuxTitlebarMode {
    fn from_environment() -> Self {
        match std::env::var("ELARIS_LINUX_TITLEBAR") {
            Ok(mode) if mode.eq_ignore_ascii_case("always") => Self::Always,
            _ => Self::Hover,
        }
    }
}

/// Linux titlebar visibility and its current pointer state.
#[cfg(target_os = "linux")]
pub(crate) struct LinuxTitlebar {
    mode: LinuxTitlebarMode,
    top_hovered: bool,
    toolbar_hovered: bool,
    x11_window: Option<std::ffi::c_ulong>,
}

#[cfg(target_os = "linux")]
impl Default for LinuxTitlebar {
    fn default() -> Self {
        Self {
            mode: LinuxTitlebarMode::from_environment(),
            top_hovered: false,
            toolbar_hovered: false,
            x11_window: None,
        }
    }
}

#[cfg(target_os = "linux")]
impl LinuxTitlebar {
    /// Whether the toolbar should be drawn now.
    pub(crate) fn is_visible(&self) -> bool {
        self.mode == LinuxTitlebarMode::Always || self.top_hovered || self.toolbar_hovered
    }

    /// Updates hover visibility and reports whether the view needs an animated commit.
    pub(crate) fn set_top_hovered(&mut self, hovered: bool) -> bool {
        if self.mode == LinuxTitlebarMode::Always || self.top_hovered == hovered {
            return false;
        }
        self.top_hovered = hovered;
        true
    }

    /// Keeps the toolbar open while the pointer moves from the top reveal strip into the row.
    pub(crate) fn set_toolbar_hovered(&mut self, hovered: bool) -> bool {
        if self.mode == LinuxTitlebarMode::Always || self.toolbar_hovered == hovered {
            return false;
        }
        self.toolbar_hovered = hovered;
        true
    }

    /// Records the X11 window ID for the custom minimize control. Wayland has no minimize
    /// request in Rune for borderless windows, so the control is omitted there.
    pub(crate) fn set_x11_window(&mut self, window: Option<std::ffi::c_ulong>) {
        self.x11_window = window;
    }

    pub(crate) fn can_minimize(&self) -> bool {
        self.x11_window.is_some()
    }

    /// Minimizes an X11 window using a separate, short-lived Xlib connection.
    pub(crate) fn minimize(&self) {
        let Some(window) = self.x11_window else {
            return;
        };
        let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
            return;
        };
        // SAFETY: the Xlib display is opened here and used only for these calls, then closed.
        unsafe {
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return;
            }
            let screen = (xlib.XDefaultScreen)(display);
            (xlib.XIconifyWindow)(display, window, screen);
            (xlib.XFlush)(display);
            (xlib.XCloseDisplay)(display);
        }
    }
}

/// The window's title when the page has none.
pub(crate) const APP_NAME: &str = "Elaris";
const RECENTLY_CLOSED_LIMIT: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecentlyClosedTab {
    url: String,
    title: String,
    section: TabSection,
}

impl RecentlyClosedTab {
    fn from_tab(tab: &Tab) -> Self {
        Self {
            url: tab.url.clone(),
            title: tab.title.clone(),
            section: tab.section(),
        }
    }
}

/// State for one Rune window. Its Browser owns its tabs; process services are shared separately.
pub(crate) struct AppState {
    pub(crate) window_id: u64,
    /// Maximum page frame rate reported by the window's current display.
    #[cfg(any(target_os = "macos", feature = "ladybird-bridge"))]
    display_refresh_rate: f64,
    pub(crate) omnibox: omnibox::State,
    /// Whether the floating Find in Page bar is mounted.
    pub(crate) find_open: bool,
    pub(crate) tab_switcher: Option<crate::ui::tab_switcher::Switcher>,
    /// The active tab's find input. Each tab separately remembers its last query.
    pub(crate) find_input: TextEdit,
    /// The tab whose query currently backs the floating bar.
    pub(crate) find_tab: Option<TabId>,
    /// Identifies asynchronous selected-text reads so previous tabs cannot seed this query.
    find_selection_request: u64,
    pub(crate) settings: Settings,
    /// Why the last submitted query could not be opened, until the next is.
    pub(crate) address_error: Option<String>,
    pub(crate) browser: Browser,
    /// Whether the downloads popover is open.
    pub(crate) downloads_open: bool,
    /// Whether the site controls popover is mounted.
    pub(crate) site_permissions_open: bool,
    /// Rune-space bounds of the URL-bar control, used as AppKit's popover anchor.
    pub(crate) site_controls_anchor: Option<[f32; 4]>,
    /// Layout bounds for each Rune permission row, in `PermissionKind::index()` order.
    #[cfg(target_os = "linux")]
    pub(crate) permission_row_anchors: [Option<[f32; 4]>; 4],
    /// The open permission choice menu, if any.
    pub(crate) permission_menu: Option<PermissionKind>,
    /// Whether the compact site overflow menu is open.
    pub(crate) site_overflow_open: bool,
    /// The site footer's overflow-button position for the Rune menu host.
    #[cfg(target_os = "linux")]
    pub(crate) site_overflow_anchor: Option<PointerPress>,
    /// Per-origin permission choices loaded from config.toml.
    pub(crate) permission_store: PermissionStore,
    pub(crate) page_menu: crate::ui::PageMenuState,
    pub(crate) sidebar: SidebarState,
    pub(crate) hover_highlight: Option<crate::ui::hover_highlight::Target>,
    pub(crate) context_menu_highlight: Option<String>,
    pub(crate) split_rect: [f32; 4],
    pub(crate) peek_presentation: crate::ui::peek::Presentation,
    pub(crate) split_preview: Option<usize>,
    pub(crate) split_selection: usize,
    pub(crate) tab_drag: crate::ui::TabDragState,
    pub(crate) settings_picker_open: bool,
    pub(crate) startup_picker_open: bool,
    #[cfg(target_os = "linux")]
    pub(crate) titlebar: LinuxTitlebar,
    storage: Option<std::sync::Arc<Storage>>,
    workers_started: bool,
    pending_detach: bool,
    merged_away: bool,
    #[cfg(feature = "ladybird-bridge")]
    profile_dir: Option<PathBuf>,
    config_file: Option<PathBuf>,
    default_download_directory: PathBuf,
    download_directory_override: Option<PathBuf>,
    pub(crate) configuration_issue: Option<String>,
    #[cfg(feature = "ladybird-bridge")]
    storage_error: Option<String>,
    last_saved_session: Option<Session>,
    restored_windows: Vec<Session>,
    history_revision: u64,
    pub(crate) history_page: HistoryPage,
    /// The page title the window shows.
    window_title: Option<String>,
    context_menu: Option<(Option<TabId>, PointerPress)>,
    recently_closed: Vec<RecentlyClosedTab>,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    #[cfg(target_os = "macos")]
    pub(crate) fn set_display_refresh_rate(&mut self, refresh_rate: f64) {
        if refresh_rate.is_finite() && refresh_rate > 0.0 {
            self.display_refresh_rate = refresh_rate;
        }
    }

    /// Opens the supplied page, or shows the omnibox in an empty window.
    pub(crate) fn new(start: Option<&str>) -> Self {
        Self::build(start, true)
    }

    fn build(start: Option<&str>, restore: bool) -> Self {
        #[cfg(test)]
        let (storage, _profile_dir, config_file, config_load, sessions, _storage_error) = (
            None,
            None::<PathBuf>,
            None,
            None::<Result<ConfigLoad, String>>,
            Vec::new(),
            None::<String>,
        );
        #[cfg(not(test))]
        let (storage, _profile_dir, config_file, config_load, sessions, _storage_error) =
            match storage::Paths::resolve() {
                Ok(paths) => {
                    let profile_dir = Some(paths.profile_dir.clone());
                    let config_file = Some(paths.config_file.clone());
                    match Storage::open_shared(paths) {
                        Ok(storage) => {
                            let sessions = if restore {
                                storage.take_sessions().unwrap_or_default()
                            } else {
                                Vec::new()
                            };
                            let config_load = storage.take_config_load();
                            (
                                Some(storage),
                                profile_dir,
                                config_file,
                                config_load,
                                sessions,
                                None,
                            )
                        }
                        Err(error) => {
                            let message = format!("Cannot open browser storage: {error}");
                            eprintln!("Elaris: {message}");
                            (
                                None,
                                profile_dir,
                                config_file,
                                None,
                                Vec::new(),
                                Some(message),
                            )
                        }
                    }
                }
                Err(error) => {
                    let message = format!("Cannot locate browser storage: {error}");
                    eprintln!("Elaris: {message}");
                    (None, None, None, None, Vec::new(), Some(message))
                }
            };
        let permission_store = config_load
            .as_ref()
            .and_then(|load| load.as_ref().ok())
            .map_or_else(PermissionStore::default, |load| load.permissions.clone());
        #[cfg(not(test))]
        let permission_store = shared_permission_store(permission_store);
        #[cfg_attr(test, allow(unused_mut))]
        let (settings, mut configuration_issue) = match config_load.as_ref() {
            Some(Ok(load)) => (
                load.settings.clone(),
                (!load.issues.is_empty()).then(|| load.issues.join("\n")),
            ),
            Some(Err(error)) => (Settings::default(), Some(error.clone())),
            None => (Settings::default(), None),
        };
        #[cfg(not(test))]
        let settings = shared_settings(settings);
        let initial_session = sessions.first().cloned();
        let restored_windows =
            if restore && start.is_none() && settings.startup != StartupMode::BlankWindow {
                sessions.into_iter().skip(1).collect()
            } else {
                Vec::new()
            };
        let default_download_directory = storage::default_download_directory();
        #[cfg_attr(test, allow(unused_mut))]
        let mut download_directory_override = None;
        #[cfg(not(test))]
        if let Some(path) = std::env::var_os("ELARIS_DOWNLOAD_DIR").map(PathBuf::from) {
            if path.is_absolute() {
                download_directory_override = Some(path);
            } else {
                configuration_issue = Some("ELARIS_DOWNLOAD_DIR must be an absolute path".into());
            }
        }
        let mut state = Self {
            window_id: NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed),
            #[cfg(any(target_os = "macos", feature = "ladybird-bridge"))]
            display_refresh_rate: 60.0,
            omnibox: omnibox::State::default(),
            find_open: false,
            tab_switcher: None,
            find_input: TextEdit::default(),
            find_tab: None,
            find_selection_request: 0,
            settings,
            address_error: None,
            browser: Browser::default(),
            downloads_open: false,
            site_permissions_open: false,
            site_controls_anchor: None,
            #[cfg(target_os = "linux")]
            permission_row_anchors: [None; 4],
            permission_menu: None,
            site_overflow_open: false,
            #[cfg(target_os = "linux")]
            site_overflow_anchor: None,
            permission_store,
            page_menu: Default::default(),
            sidebar: SidebarState::default(),
            hover_highlight: None,
            context_menu_highlight: None,
            split_rect: [0.0; 4],
            peek_presentation: Default::default(),
            split_preview: None,
            split_selection: 0,
            tab_drag: crate::ui::TabDragState::default(),
            settings_picker_open: false,
            startup_picker_open: false,
            #[cfg(target_os = "linux")]
            titlebar: LinuxTitlebar::default(),
            storage,
            workers_started: false,
            pending_detach: false,
            merged_away: false,
            #[cfg(feature = "ladybird-bridge")]
            profile_dir: _profile_dir,
            config_file,
            default_download_directory,
            download_directory_override,
            configuration_issue,
            #[cfg(feature = "ladybird-bridge")]
            storage_error: _storage_error,
            last_saved_session: initial_session.clone(),
            restored_windows,
            history_revision: 0,
            history_page: HistoryPage::default(),
            window_title: None,
            context_menu: None,
            recently_closed: Vec::new(),
        };
        #[cfg(not(test))]
        state.browser.share_downloads(shared_downloads());
        state.open_start_page(start, initial_session.as_ref());
        if start.is_none()
            && let Some(session) = initial_session
        {
            seed_window_session(state.window_id, session);
        }
        state
    }

    pub(crate) fn new_window() -> Self {
        let mut state = Self::build(None, false);
        state.open_omnibox(omnibox::Mode::New);
        state
    }

    pub(crate) fn new_window_from_session(session: Session) -> Self {
        let mut state = Self::build(None, false);
        state.omnibox.close();
        state.restore_session(&session);
        state.last_saved_session = Some(session.clone());
        seed_window_session(state.window_id, session);
        state
    }

    pub(crate) fn take_restored_windows(&mut self) -> Vec<Session> {
        std::mem::take(&mut self.restored_windows)
    }

    pub(crate) fn sync_shared_settings(&mut self) -> bool {
        #[cfg(not(test))]
        {
            let settings = shared_settings_snapshot();
            if self.settings == settings {
                false
            } else {
                let previous = self.engine_download_directory();
                self.prepare_tab_layout(settings.tab_layout);
                self.settings = settings;
                let current = self.engine_download_directory();
                if previous != current {
                    self.browser.set_download_directory(current);
                }
                true
            }
        }
        #[cfg(test)]
        false
    }

    pub(crate) fn sync_shared_permissions(&mut self) -> bool {
        #[cfg(not(test))]
        {
            let permissions = shared_permission_snapshot();
            if self.permission_store == permissions {
                false
            } else {
                self.permission_store = permissions;
                true
            }
        }
        #[cfg(test)]
        false
    }

    fn open_start_page(&mut self, start: Option<&str>, session: Option<&Session>) {
        if let Some(start) = start {
            match self.browser.open_tab(start) {
                Ok(_) => {}
                Err(error) => {
                    if let Some(session) = session {
                        self.restore_session(session);
                    }
                    self.open_omnibox(omnibox::Mode::New);
                    self.address_error = Some(error.to_string());
                }
            }
        } else if self.settings.startup == StartupMode::BlankWindow {
            // Keep the viewport blank while restoring live persistent tabs in the sidebar.
            if let Some(session) = session {
                self.restore_persistent_tabs(session);
            }
        } else if let Some(session) = session {
            self.restore_session(session);
        }
    }
}

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

static WINDOW_REGISTRY: OnceLock<Mutex<HashMap<u64, rune::WindowHandle>>> = OnceLock::new();
static SESSION_REGISTRY: OnceLock<Mutex<BTreeMap<u64, Session>>> = OnceLock::new();
static SHARED_SETTINGS: OnceLock<RwLock<Settings>> = OnceLock::new();
static SHARED_PERMISSIONS: OnceLock<RwLock<PermissionStore>> = OnceLock::new();
#[cfg(not(test))]
static SHARED_DOWNLOADS: OnceLock<Arc<Mutex<Vec<elaris::downloads::Download>>>> = OnceLock::new();
static ACTIVE_WINDOW: AtomicU64 = AtomicU64::new(0);

#[cfg(not(test))]
fn shared_settings(settings: Settings) -> Settings {
    SHARED_SETTINGS
        .get_or_init(|| RwLock::new(settings))
        .read()
        .expect("shared settings poisoned")
        .clone()
}

#[cfg(not(test))]
fn shared_settings_snapshot() -> Settings {
    SHARED_SETTINGS
        .get()
        .expect("shared settings initialized before windows")
        .read()
        .expect("shared settings poisoned")
        .clone()
}

pub(crate) fn publish_settings(settings: &Settings) {
    *SHARED_SETTINGS
        .get_or_init(|| RwLock::new(settings.clone()))
        .write()
        .expect("shared settings poisoned") = settings.clone();
    crate::engine::wake_all_windows();
}

#[cfg(not(test))]
fn shared_downloads() -> Arc<Mutex<Vec<elaris::downloads::Download>>> {
    Arc::clone(SHARED_DOWNLOADS.get_or_init(|| Arc::new(Mutex::new(Vec::new()))))
}

#[cfg(not(test))]
fn shared_permission_store(permissions: PermissionStore) -> PermissionStore {
    SHARED_PERMISSIONS
        .get_or_init(|| RwLock::new(permissions))
        .read()
        .expect("shared permissions poisoned")
        .clone()
}

#[cfg(not(test))]
fn shared_permission_snapshot() -> PermissionStore {
    SHARED_PERMISSIONS
        .get()
        .expect("shared permissions initialized before windows")
        .read()
        .expect("shared permissions poisoned")
        .clone()
}

pub(crate) fn publish_permissions(permissions: &PermissionStore) {
    *SHARED_PERMISSIONS
        .get_or_init(|| RwLock::new(permissions.clone()))
        .write()
        .expect("shared permissions poisoned") = permissions.clone();
    crate::engine::wake_all_windows();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn native_window(id: u64) -> Option<rune::WindowHandle> {
    WINDOW_REGISTRY.get()?.lock().ok()?.get(&id).cloned()
}

pub(crate) fn register_window(id: u64, handle: rune::WindowHandle) {
    WINDOW_REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("window registry poisoned")
        .insert(id, handle);
}

pub(crate) fn unregister_window(id: u64) {
    if let Some(registry) = WINDOW_REGISTRY.get() {
        registry
            .lock()
            .expect("window registry poisoned")
            .remove(&id);
    }
    let _ = ACTIVE_WINDOW.compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed);
}

pub(crate) fn activate_window(id: u64) {
    ACTIVE_WINDOW.store(id, Ordering::Relaxed);
}

pub(crate) fn open_window_count() -> usize {
    WINDOW_REGISTRY.get().map_or(0, |registry| {
        registry.lock().expect("window registry poisoned").len()
    })
}

pub(crate) fn reopen_or_create_window() {
    let existing = WINDOW_REGISTRY.get().and_then(|registry| {
        registry
            .lock()
            .expect("window registry poisoned")
            .values()
            .next()
            .cloned()
    });
    if let Some(handle) = existing {
        handle.open();
    } else {
        crate::open_browser_window();
    }
}

fn save_registered_sessions(storage: &Storage, restore_on_launch: bool) {
    let registry = SESSION_REGISTRY
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("session registry poisoned");
    send_session_registry_snapshot(&registry, storage, restore_on_launch);
}

fn send_session_registry_snapshot(
    registry: &BTreeMap<u64, Session>,
    storage: &Storage,
    restore_on_launch: bool,
) {
    let sessions = registry.values().cloned().collect();
    if let Err(error) = storage.save_sessions_for_launch(sessions, restore_on_launch) {
        eprintln!("Elaris: {error}");
    }
}

pub(crate) fn store_window_session(id: u64, session: Session, storage: &Storage) {
    store_window_session_in(
        SESSION_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new())),
        id,
        session,
        storage,
    );
}

fn store_window_session_in(
    registry: &Mutex<BTreeMap<u64, Session>>,
    id: u64,
    session: Session,
    storage: &Storage,
) {
    let mut registry = registry.lock().expect("session registry poisoned");
    registry.insert(id, session);
    send_session_registry_snapshot(&registry, storage, true);
}

fn seed_window_session(id: u64, session: Session) {
    SESSION_REGISTRY
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("session registry poisoned")
        .insert(id, session);
}

pub(crate) fn remove_window_session(id: u64, storage: &Storage) {
    remove_window_session_from(
        SESSION_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new())),
        id,
        storage,
    );
}

fn remove_window_session_from(
    registry: &Mutex<BTreeMap<u64, Session>>,
    id: u64,
    storage: &Storage,
) {
    let mut registry = registry.lock().expect("session registry poisoned");
    registry.remove(&id);
    send_session_registry_snapshot(&registry, storage, true);
}

pub(crate) fn save_closed_final_session(storage: &Storage) {
    save_registered_sessions(storage, false);
}

pub(crate) mod cross_window_drag;
pub(crate) mod detach;
mod find;
mod lifecycle;
mod omnibox_actions;
mod session;
mod settings_actions;
mod tabs;
mod transfer;

#[cfg(test)]
mod tests;

pub(crate) use lifecycle::{
    new_tab, omnibox_input_key, on_reopen, on_window_focus, open_from_launcher, open_history,
    open_settings, poll, setup_headless, shut_down, start_workers, submit_omnibox, window_closed,
};
