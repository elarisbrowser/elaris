// SPDX-License-Identifier: GPL-3.0-only

//! Building an `AppState`: where storage, config and saved sessions come from, and which pages open
//! first.

use super::AppState;
#[cfg(elaris_ui = "linux")]
use super::LinuxTitlebar;
use super::history::HistoryPage;
use super::registry::mark_private_window;
#[cfg(not(test))]
use super::shared::{shared_downloads, shared_permission_store, shared_settings};
use super::window_sessions::seed_window_session;
use crate::browser::Browser;
use crate::omnibox;
use crate::storage;
use crate::storage::Session;
#[cfg(not(test))]
use crate::storage::Storage;
use crate::ui::SidebarState;
#[cfg(test)]
use elaris_core::config::ConfigLoad;
use elaris_core::permissions::PermissionStore;
use elaris_core::settings::{Settings, StartupMode};
use rune::TextEdit;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

impl AppState {
    /// Opens the supplied page, or shows the omnibox in an empty window.
    pub(crate) fn new(start: Option<&str>) -> Self {
        Self::build(start, true)
    }

    pub(super) fn build(start: Option<&str>, restore: bool) -> Self {
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
        let keybinding_issues = crate::shortcuts::apply_keybindings(&settings.keybindings);
        if !keybinding_issues.is_empty() {
            configuration_issue = Some(
                configuration_issue
                    .into_iter()
                    .chain(keybinding_issues)
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
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
            display: 60.0.into(),
            omnibox: omnibox::State::default(),
            page_guards: Default::default(),
            clear_data: None,
            clear_data_status: None,
            clear_data_pending: false,
            find_open: false,
            tab_switcher: None,
            find_input: TextEdit::default(),
            find_tab: None,
            find_selection_request: 0,
            settings,
            browsing_data_size_bytes: None,
            browsing_data_refresh_at: 0.0,
            address_error: None,
            browser: Browser::default(),
            downloads_open: false,
            site_permissions_open: false,
            site_controls_anchor: None,
            site_controls_page: crate::site_controls::SiteControlsPage::Main,
            permission_row_anchors: [None; 4],
            permission_menu: None,
            autoplay_menu_open: false,
            autoplay_row_anchor: None,
            site_overflow_open: false,
            site_action_done: None,
            site_overflow_anchor: None,
            permission_store,
            page_menu: Default::default(),
            form_picker: None,
            sidebar: SidebarState::default(),
            hover_highlight: None,
            animated_icon_hover: None,
            context_menu_highlight: None,
            context_menu_focus: None,
            context_menu_hover: None,
            split_rect: [0.0; 4],
            peek_presentation: Default::default(),
            split_preview: None,
            split_selection: 0,
            tab_drag: crate::ui::TabDragState::default(),
            page_category: None,
            page_nav_hover: None,
            settings_picker_open: false,
            startup_picker_open: false,
            #[cfg(elaris_ui = "linux")]
            decorations_picker_open: false,
            #[cfg(elaris_ui = "linux")]
            restart_notice: false,
            #[cfg(elaris_ui = "linux")]
            titlebar: LinuxTitlebar::default(),
            storage,
            workers_started: false,
            pending_detach: false,
            merged_away: false,
            mini: false,
            mini_start: None,
            #[cfg(feature = "ladybird-bridge")]
            profile_dir: _profile_dir,
            config_file,
            default_download_directory,
            download_directory_override,
            configuration_issue,
            default_browser: None,
            default_browser_error: None,
            #[cfg(feature = "ladybird-bridge")]
            storage_error: _storage_error,
            last_saved_session: initial_session.clone(),
            restored_windows,
            history_revision: 0,
            history_page: HistoryPage::default(),
            task_manager: Default::default(),
            memory_saver: Default::default(),
            idle: Default::default(),
            search_engine_form: Default::default(),
            shortcut_recorder: Default::default(),
            window_title: None,
            zoom_menu: None,
            translate_menu: None,
            bookmarks: Default::default(),
            zoom_chip: None,
            toasts: Default::default(),
            navigation_menu: None,
            context_menu: None,
            recently_closed: Vec::new(),
            closing_tabs: None,
        };
        state.browser.split_view_limit = state.settings.split_view_limit;
        state.browser.set_default_zoom(state.settings.page_zoom);
        #[cfg(not(test))]
        state
            .browser
            .share_downloads(shared_downloads(state.storage.as_deref()));
        #[cfg(not(test))]
        {
            state.bookmarks = super::bookmarks::BookmarksState::with_list(
                super::bookmarks::shared(state.storage.as_deref()),
            );
        }
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

    /// A full browser window that opens `url` in its first tab.
    pub(crate) fn new_window_with_url(url: &str) -> Self {
        let mut state = Self::build(None, false);
        state.omnibox.close();
        if let Err(error) = state.browser.open_tab(url) {
            state.open_omnibox(omnibox::Mode::New);
            state.address_error = Some(error.to_string());
        }
        state
    }

    /// A window whose pages share the private session and leave no history or saved session.
    pub(crate) fn new_private_window() -> Self {
        let mut state = Self::build(None, false);
        state.make_private();
        state.open_omnibox(omnibox::Mode::New);
        state
    }

    /// Marks an empty window as private before any page opens in it.
    pub(crate) fn make_private(&mut self) {
        debug_assert!(self.browser.tabs().is_empty());
        self.browser.private = true;
        mark_private_window(self.window_id);
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

    pub(super) fn open_start_page(&mut self, start: Option<&str>, session: Option<&Session>) {
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
        if start.is_none() && self.horizontal_tabs() && self.settings.open_tab_on_startup {
            if let Ok(tab) = self.browser.open_tab("about:blank") {
                self.browser.activate_tab(tab);
            }
            self.open_omnibox(omnibox::Mode::New);
        }
    }
}
