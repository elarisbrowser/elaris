// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`Browser`].

mod history;

use history::HistoryPage;
use rune::{Key, KeyEvent, PointerPress, TextEdit, Tx};
use std::path::PathBuf;

use crate::address::{self, Destination, SearchEngine};
use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::config::ConfigLoad;
#[cfg(feature = "ladybird-bridge")]
use crate::engine::Engine;
use crate::internal::{InternalPage, InternalRoute};
use crate::omnibox::{self, Action, Suggestion};
use crate::settings::{DownloadDirectory, Settings, SettingsChange, StartupMode};
use crate::storage;
use crate::storage::{Session, SessionTab, Storage};
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::{SidebarState, TabDropTarget, VIEWPORT};

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

pub(crate) struct AppState {
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
    pub(crate) page_menu: crate::ui::PageMenuState,
    pub(crate) sidebar: SidebarState,
    pub(crate) tab_drag: crate::ui::TabDragState,
    pub(crate) settings_picker_open: bool,
    pub(crate) startup_picker_open: bool,
    #[cfg(target_os = "linux")]
    pub(crate) titlebar: LinuxTitlebar,
    storage: Option<Storage>,
    workers_started: bool,
    #[cfg(feature = "ladybird-bridge")]
    profile_dir: Option<PathBuf>,
    config_file: Option<PathBuf>,
    default_download_directory: PathBuf,
    download_directory_override: Option<PathBuf>,
    pub(crate) configuration_issue: Option<String>,
    #[cfg(feature = "ladybird-bridge")]
    storage_error: Option<String>,
    last_saved_session: Option<Session>,
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
    /// Opens the supplied page, or shows the omnibox in an empty window.
    pub(crate) fn new(start: Option<&str>) -> Self {
        #[cfg(test)]
        let (storage, _profile_dir, config_file, config_load, session, _storage_error) = (
            None,
            None::<PathBuf>,
            None,
            None::<Result<ConfigLoad, String>>,
            None,
            None::<String>,
        );
        #[cfg(not(test))]
        let (storage, _profile_dir, config_file, config_load, session, _storage_error) =
            match storage::Paths::resolve() {
                Ok(paths) => {
                    let profile_dir = Some(paths.profile_dir.clone());
                    let config_file = Some(paths.config_file.clone());
                    match Storage::open(paths) {
                        Ok(mut storage) => {
                            let session = storage.take_session();
                            let config_load = storage.take_config_load();
                            (
                                Some(storage),
                                profile_dir,
                                config_file,
                                config_load,
                                session,
                                None,
                            )
                        }
                        Err(error) => {
                            let message = format!("Cannot open browser storage: {error}");
                            eprintln!("Elaris: {message}");
                            (None, profile_dir, config_file, None, None, Some(message))
                        }
                    }
                }
                Err(error) => {
                    let message = format!("Cannot locate browser storage: {error}");
                    eprintln!("Elaris: {message}");
                    (None, None, None, None, None, Some(message))
                }
            };
        #[cfg_attr(test, allow(unused_mut))]
        let (settings, mut configuration_issue) = match config_load {
            Some(Ok(load)) => (
                load.settings,
                (!load.issues.is_empty()).then(|| load.issues.join("\n")),
            ),
            Some(Err(error)) => (Settings::default(), Some(error)),
            None => (Settings::default(), None),
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
            page_menu: Default::default(),
            sidebar: SidebarState::default(),
            tab_drag: crate::ui::TabDragState::default(),
            settings_picker_open: false,
            startup_picker_open: false,
            #[cfg(target_os = "linux")]
            titlebar: LinuxTitlebar::default(),
            storage,
            workers_started: false,
            #[cfg(feature = "ladybird-bridge")]
            profile_dir: _profile_dir,
            config_file,
            default_download_directory,
            download_directory_override,
            configuration_issue,
            #[cfg(feature = "ladybird-bridge")]
            storage_error: _storage_error,
            last_saved_session: session.clone(),
            history_revision: 0,
            history_page: HistoryPage::default(),
            window_title: None,
            context_menu: None,
            recently_closed: Vec::new(),
        };
        state.open_start_page(start, session.as_ref());
        state
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

    /// Loads what the omnibox holds in the active tab.
    pub(crate) fn submit_omnibox(&mut self) {
        let address = self.omnibox.input.text.clone();
        let result = match address::destination_for_input(&address) {
            Ok(Destination::Url(url)) => self.navigate_destination(&url),
            Ok(Destination::Search(query)) => {
                self.navigate_destination(&address::search_url(self.settings.search_engine, &query))
            }
            Err(error) => Err(crate::browser::Error::Address(error)),
        };
        self.address_error = result.err().map(|error| error.to_string());
    }

    pub(crate) fn open_omnibox(&mut self, mode: omnibox::Mode) {
        self.dismiss_find();
        self.page_menu = Default::default();
        self.omnibox.mode = mode;
        let use_active_url = mode == omnibox::Mode::Current;
        let query = if use_active_url {
            self.browser
                .active_tab()
                .map(|tab| tab.url.clone())
                .filter(|url| url != "about:blank")
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.omnibox
            .open(&query, use_active_url && !query.is_empty());
        self.request_history_search();
        self.address_error = None;
    }

    pub(crate) fn suggestions(&self) -> Vec<Suggestion> {
        let query = self.omnibox.input.text.trim();
        let history = self.storage.as_ref().map_or_else(Vec::new, |storage| {
            let history = storage.history();
            if query.is_empty() {
                let mut entries = history.recent(6);
                if entries.len() < 3 {
                    for entry in history.frequent(6) {
                        if !entries.iter().any(|recent| recent.url == entry.url) {
                            entries.push(entry);
                        }
                        if entries.len() >= 6 {
                            break;
                        }
                    }
                }
                entries
            } else {
                history.search(query, 100)
            }
        });
        omnibox::suggestions(
            query,
            self.browser.tabs(),
            &history,
            self.sidebar.collapsed,
            self.settings.search_engine,
        )
    }

    pub(crate) fn request_history_search(&self) {
        if let Some(storage) = &self.storage
            && let Err(error) = storage.history().request_search(&self.omnibox.input.text)
        {
            eprintln!("Elaris: {error}");
        }
    }

    pub(crate) fn choose_suggestion(&mut self, suggestion: Suggestion, tx: &mut Tx) {
        let result = match suggestion.action {
            Action::Navigate(url) => self.navigate_destination(&url),
            Action::Search(query) => {
                self.navigate_destination(&address::search_url(self.settings.search_engine, &query))
            }
            Action::VisitRecent(url) => self.navigate_destination(&url),
            Action::SwitchTab(tab) => {
                self.activate(tab);
                Ok(())
            }
            Action::NewTab => {
                new_tab(self, tx);
                return;
            }
            Action::ReloadPage => {
                self.with_active_tab(Browser::reload);
                Ok(())
            }
            Action::CloseTab => {
                self.close_active();
                if self.browser.tabs().is_empty() {
                    tx.focus("omnibox-input", false);
                    return;
                }
                Ok(())
            }
            Action::ToggleSidebar => {
                crate::ui::toggle_sidebar(self, tx);
                Ok(())
            }
        };
        self.address_error = result.err().map(|error| error.to_string());
        if self.address_error.is_none() {
            self.omnibox.close();
            tx.focus(VIEWPORT, false);
        }
    }

    fn navigate_destination(&mut self, url: &str) -> Result<(), crate::browser::Error> {
        let route = InternalRoute::parse(url);
        match (
            self.omnibox.mode,
            self.browser.active_tab().map(Tab::id),
            route,
        ) {
            (omnibox::Mode::Current, Some(tab), Some(route)) => {
                self.browser.navigate_internal(tab, route)?;
            }
            (omnibox::Mode::Current, Some(tab), None) => self.browser.navigate(tab, url)?,
            (_, _, _) => {
                let tab = self.browser.open_tab(url)?;
                self.browser.activate_tab(tab);
            }
        }
        self.address_error = None;
        Ok(())
    }

    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        if self.settings.appearance == appearance {
            return;
        }
        self.settings.set_appearance(appearance);
        self.save_setting(SettingsChange::Appearance(appearance));
    }

    pub(crate) fn settings_tab(&mut self) -> TabId {
        if let Some(tab) = self.browser.internal_tab(InternalPage::Settings) {
            return tab;
        }
        self.browser
            .open_tab(InternalPage::Settings.address())
            .expect("the built-in settings route is valid")
    }

    pub(crate) fn set_search_engine(&mut self, search_engine: SearchEngine) {
        if self.settings.search_engine == search_engine {
            return;
        }
        self.settings.set_search_engine(search_engine);
        self.save_setting(SettingsChange::SearchEngine(search_engine));
    }

    pub(crate) fn set_download_directory(&mut self, directory: DownloadDirectory) {
        if let Err(error) = self.settings.set_download_directory(directory.clone()) {
            self.configuration_issue = Some(error);
            return;
        }
        self.browser
            .set_download_directory(self.engine_download_directory());
        self.save_setting(SettingsChange::DownloadDirectory(directory));
    }

    pub(crate) fn download_directory(&self) -> PathBuf {
        self.engine_download_directory()
            .unwrap_or_else(|| self.default_download_directory.clone())
    }

    fn engine_download_directory(&self) -> Option<PathBuf> {
        self.download_directory_override.clone().or_else(|| {
            match &self.settings.download_directory {
                DownloadDirectory::Default => None,
                DownloadDirectory::Custom(path) => Some(path.clone()),
            }
        })
    }

    pub(crate) fn set_startup(&mut self, startup: StartupMode) {
        if self.settings.startup == startup {
            return;
        }
        self.settings.set_startup(startup);
        self.save_setting(SettingsChange::Startup(startup));
    }

    pub(crate) fn set_glass_enabled(&mut self, enabled: bool) {
        if self.settings.glass_enabled == enabled {
            return;
        }
        self.settings.set_glass_enabled(enabled);
        self.save_setting(SettingsChange::GlassEnabled(enabled));
    }

    pub(crate) fn set_tint_opacity(&mut self, opacity: u8) {
        let opacity = opacity.min(100);
        if self.settings.tint_opacity == opacity {
            return;
        }
        self.settings.set_tint_opacity(opacity);
        self.save_setting(SettingsChange::TintOpacity(opacity));
    }

    fn save_setting(&mut self, change: SettingsChange) {
        if let Some(storage) = &self.storage
            && let Err(error) = storage.save_setting(change)
        {
            self.configuration_issue = Some(error);
        }
    }

    pub(crate) fn reload_configuration(&mut self) {
        let result = self
            .storage
            .as_ref()
            .ok_or_else(|| "Configuration storage is unavailable".to_owned())
            .and_then(Storage::reload_configuration);
        if let Err(error) = result {
            self.configuration_issue = Some(error);
        }
    }

    pub(crate) fn open_config_file(&mut self) {
        let Some(path) = &self.config_file else {
            self.configuration_issue = Some("Configuration file path is unavailable".into());
            return;
        };
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(path).spawn();
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open").arg(path).spawn();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let result: std::io::Result<std::process::Child> = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "opening config.toml is not supported on this platform",
        ));
        if let Err(error) = result {
            self.configuration_issue = Some(format!("Cannot open config.toml: {error}"));
        }
    }

    fn apply_config_load(&mut self, load: ConfigLoad) {
        let previous_download_directory = self.engine_download_directory();
        self.settings = load.settings;
        let download_directory = self.engine_download_directory();
        if previous_download_directory != download_directory {
            self.browser.set_download_directory(download_directory);
        }
        self.configuration_issue = (!load.issues.is_empty()).then(|| load.issues.join("\n"));
    }

    pub(crate) fn activate(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        self.page_menu = Default::default();
        self.browser.activate_tab(tab);
        self.sync_find_tab();
        self.address_error = None;
    }

    /// Closes the tab and opens the omnibox when the window becomes empty.
    pub(crate) fn close(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        self.page_menu = Default::default();
        if let Some(tab) = self.browser.tab(tab) {
            self.remember_closed(RecentlyClosedTab::from_tab(tab));
        }
        self.browser.close_tab(tab);
        self.sync_find_tab();
        if self.browser.tabs().is_empty() {
            self.open_omnibox(omnibox::Mode::New);
        }
    }

    /// Reopens a recently closed tab and returns its new identity.
    pub(crate) fn reopen_recently_closed(&mut self, index: usize) -> Option<TabId> {
        let closed = self.recently_closed.get(index)?.clone();
        let tab = self
            .browser
            .restore_tab_in_section(&closed.url, &closed.title, false, closed.section)
            .ok()?;
        self.recently_closed.remove(index);
        self.omnibox.close();
        self.activate(tab);
        Some(tab)
    }

    pub(crate) fn duplicate(&mut self, tab: TabId) {
        if let Some(tab) = self.browser.duplicate_tab(tab) {
            self.activate(tab);
        }
    }

    pub(crate) fn close_others(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        if self.browser.tab(tab).is_none() {
            return;
        }
        let closed: Vec<RecentlyClosedTab> = self
            .browser
            .tabs()
            .iter()
            .filter(|candidate| candidate.id() != tab)
            .map(RecentlyClosedTab::from_tab)
            .collect();
        for closed_tab in closed.into_iter().rev() {
            self.remember_closed(closed_tab);
        }
        self.browser.close_other_tabs(tab);
    }

    #[cfg(test)]
    pub(crate) fn recently_closed(&self) -> &[RecentlyClosedTab] {
        &self.recently_closed
    }

    fn remember_closed(&mut self, tab: RecentlyClosedTab) {
        self.recently_closed.insert(0, tab);
        self.recently_closed.truncate(RECENTLY_CLOSED_LIMIT);
    }

    pub(crate) fn cancel_tab_drag(&mut self) -> bool {
        self.tab_drag.cancel()
    }

    pub(crate) fn commit_tab_move(
        &mut self,
        tab: TabId,
        section: TabSection,
        target_index: usize,
    ) -> Result<(), crate::browser::Error> {
        self.browser.move_tab_to(tab, section, target_index)?;
        self.persist_session_if_changed();
        Ok(())
    }

    pub(crate) fn move_tab_to_section_animated(
        &mut self,
        tab: TabId,
        section: TabSection,
        tx: &mut Tx,
    ) {
        let Some(source_section) = self.browser.tab(tab).map(Tab::section) else {
            return;
        };
        let target_index = self
            .browser
            .section_tabs(section)
            .count()
            .saturating_sub(usize::from(source_section == section));
        let target = TabDropTarget {
            section,
            index: target_index,
        };
        if let (Some(from), Some(to)) = (
            self.tab_drag.tab_rect(tab),
            Some(
                self.tab_drag
                    .target_rect(self.browser.tabs(), target, source_section),
            ),
        ) {
            self.tab_drag
                .transition(tab, source_section, section, target_index, from, to);
        }
        if let Err(error) = self.browser.move_tab_to(tab, section, target_index) {
            self.tab_drag.cancel();
            eprintln!("Elaris: {error}");
            return;
        }
        self.persist_session_if_changed();
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        tx.wake_after(std::time::Duration::ZERO);
    }

    pub(crate) fn reload(&mut self, tab: TabId) {
        self.browser.reload(tab);
    }

    pub(crate) fn open_context_menu(&mut self, tab: Option<TabId>, point: PointerPress) {
        self.page_menu = Default::default();
        self.context_menu = Some((tab, point));
    }

    pub(crate) fn context_menu(&self) -> Option<(Option<TabId>, PointerPress)> {
        self.context_menu
    }

    pub(crate) fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
    }

    pub(crate) fn close_active(&mut self) {
        if let Some(tab) = self.browser.active_tab().map(Tab::id) {
            self.close(tab);
        }
    }

    /// Opens Find in Page for the active tab and selects its retained query.
    pub(crate) fn open_find(&mut self, tx: &mut Tx) {
        let Some(tab) = self.browser.active_tab() else {
            return;
        };
        if self.active_history_tab().is_some() {
            self.dismiss_find();
            self.omnibox.close();
            self.history_page.input.anchor = 0;
            self.history_page.input.focus = self.history_page.input.text.len();
            tx.focus(crate::ui::HISTORY_SEARCH, false);
            return;
        }
        let id = tab.id();
        let internal = tab.is_internal();
        let query = if internal {
            String::new()
        } else {
            tab.find.query.clone()
        };
        self.page_menu = Default::default();
        self.omnibox.close();
        self.find_open = true;
        self.find_tab = Some(id);
        self.replace_find_input(query, true);
        if !internal && !self.find_input.text.is_empty() {
            let query = self.find_input.text.clone();
            let _ = self.browser.find(id, &query);
        } else if !internal {
            self.request_find_selection(id);
        }
        tx.animated();
        tx.motion(crate::ui::FIND_MOTION);
        tx.focus(crate::ui::FIND_INPUT, false);
    }

    /// Closes Find, clears highlights, and restores focus to the page.
    pub(crate) fn close_find(&mut self, tx: &mut Tx) {
        self.dismiss_find();
        tx.animated();
        tx.motion(crate::ui::FIND_MOTION);
        tx.focus(VIEWPORT, false);
    }

    /// Ends the visible search without changing the active keyboard target.
    pub(crate) fn dismiss_find(&mut self) {
        if !self.find_open {
            return;
        }
        self.find_open = false;
        self.find_selection_request = self.find_selection_request.wrapping_add(1);
        if let Some(tab) = self.find_tab.take() {
            let _ = self.browser.clear_find(tab);
        }
    }

    /// Routes the editor's latest value to the tab it was built for.
    pub(crate) fn find_input_changed(&mut self) {
        self.find_selection_request = self.find_selection_request.wrapping_add(1);
        let Some(id) = self.find_tab else {
            return;
        };
        if self.browser.active_tab().map(Tab::id) != Some(id) {
            return;
        }
        let query = self.find_input.text.clone();
        let _ = self.browser.find(id, &query);
    }

    /// Rebinds the open bar after a tab switch without carrying another tab's query over.
    fn sync_find_tab(&mut self) {
        if !self.find_open {
            return;
        }
        let active = self.browser.active_tab().map(Tab::id);
        if self.find_tab == active {
            return;
        }
        if let Some(previous) = self.find_tab {
            let _ = self.browser.clear_find(previous);
        }
        self.find_tab = active;
        if self.active_history_tab().is_some() {
            self.dismiss_find();
            return;
        }
        let query = active
            .and_then(|id| self.browser.tab(id))
            .filter(|tab| !tab.is_internal())
            .map_or_else(String::new, |tab| tab.find.query.clone());
        self.replace_find_input(query, true);
        if let Some(id) = active
            && !self.find_input.text.is_empty()
            && self.browser.tab(id).is_some_and(|tab| !tab.is_internal())
        {
            let query = self.find_input.text.clone();
            let _ = self.browser.find(id, &query);
        } else if let Some(id) = active
            && self.browser.tab(id).is_some_and(|tab| !tab.is_internal())
        {
            self.request_find_selection(id);
        }
    }

    fn request_find_selection(&mut self, id: TabId) {
        if !cfg!(feature = "ladybird-bridge") {
            return;
        }
        self.find_selection_request = self.find_selection_request.wrapping_add(1);
        let _ = self
            .browser
            .request_selected_text(id, self.find_selection_request);
    }

    fn replace_find_input(&mut self, query: String, select_all: bool) {
        self.find_input.replace_text(query);
        if select_all {
            self.find_input.anchor = 0;
            self.find_input.focus = self.find_input.text.len();
        }
    }

    /// Runs `operation` on the active tab, if there is one.
    pub(crate) fn with_active_tab(&mut self, operation: impl FnOnce(&mut Browser, TabId)) {
        if let Some(tab) = self.browser.active_tab().map(|tab| tab.id()) {
            operation(&mut self.browser, tab);
        }
    }

    /// Why the address or active page failed.
    pub(crate) fn status(&self) -> Option<&str> {
        self.address_error
            .as_deref()
            .or_else(|| self.browser.active_tab()?.error.as_deref())
    }

    fn restore_session(&mut self, session: &Session) {
        let mut restored = Vec::with_capacity(session.tabs.len());
        for (index, tab) in session.tabs.iter().enumerate() {
            restored.push(
                self.browser
                    .restore_tab_in_section(
                        &tab.url,
                        &tab.title,
                        index != session.active_tab,
                        session.section_at(index),
                    )
                    .ok(),
            );
        }
        let active = restored
            .get(session.active_tab)
            .copied()
            .flatten()
            .or_else(|| restored.iter().find_map(|tab| *tab));
        if let Some(active) = active {
            self.browser.activate_tab(active);
        }
    }

    fn restore_persistent_tabs(&mut self, session: &Session) {
        for (index, tab) in session.tabs.iter().enumerate() {
            let section = session.section_at(index);
            if section != TabSection::Regular {
                let _ = self
                    .browser
                    .restore_tab_in_section(&tab.url, &tab.title, true, section);
            }
        }
        self.browser.clear_active_tab();
    }

    pub(crate) fn persist_session_if_changed(&mut self) {
        let session = self.session_snapshot();
        if self.last_saved_session.as_ref() != Some(&session) {
            self.queue_session(session);
        }
    }

    fn persist_session(&mut self) {
        let session = self.session_snapshot();
        self.queue_session(session);
    }

    fn session_snapshot(&self) -> Session {
        let mut favourites = Vec::new();
        let mut pinned = Vec::new();
        let tabs = self
            .browser
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                match tab.section() {
                    TabSection::Favourite => favourites.push(index),
                    TabSection::Pinned => pinned.push(index),
                    TabSection::Regular => {}
                }
                SessionTab {
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                }
            })
            .collect();
        let active_tab = self
            .browser
            .active_tab()
            .and_then(|active| {
                self.browser
                    .tabs()
                    .iter()
                    .position(|tab| tab.id() == active.id())
            })
            .unwrap_or_default();
        Session::new_with_sections(tabs, active_tab, favourites, pinned)
    }

    fn queue_session(&mut self, session: Session) {
        if let Some(storage) = &self.storage {
            match storage.save_session(session.clone()) {
                Ok(()) => self.last_saved_session = Some(session),
                Err(error) => eprintln!("Elaris: {error}"),
            }
        }
    }

    /// The active tab's title, when it differs from what the window shows.
    fn new_window_title(&mut self) -> Option<String> {
        let title = self
            .browser
            .active_tab()
            .map_or("", |tab| tab.title.as_str());
        (self.window_title.as_deref() != Some(title)).then(|| {
            self.window_title = Some(title.to_owned());
            title.to_owned()
        })
    }
}

/// Connect worker notifications once Rune can schedule a poll.
pub(crate) fn start_workers(state: &mut AppState, waker: rune::Waker) {
    // Rune's real-time replays call both the headless setup and the native ready hook.
    if std::mem::replace(&mut state.workers_started, true) {
        return;
    }
    if let Some(storage) = &state.storage {
        storage.set_notify(waker.clone());
    }
    #[cfg(feature = "ladybird-bridge")]
    start_engine(state, waker);
}

/// Ladybird starts on its own thread after Rune's first frame.
#[cfg(feature = "ladybird-bridge")]
fn start_engine(state: &mut AppState, waker: rune::Waker) {
    let Some(profile_dir) = state.profile_dir.clone() else {
        state.browser.fail(
            state
                .storage_error
                .clone()
                .unwrap_or_else(|| "Cannot locate the browser profile directory".to_owned()),
        );
        return;
    };
    match Engine::start(waker, profile_dir, state.engine_download_directory()) {
        Ok(engine) => state.browser.connect(engine),
        Err(error) => state.browser.fail(error),
    }
}

/// Headless tooling has no native `on_ready`, so connect workers during Rune's setup callback.
pub(crate) fn setup_headless(state: &mut AppState, waker: rune::Waker) {
    if rune::is_headless() {
        start_workers(state, waker);
    }
}

/// Rune's per-frame hook: forwards UI changes to the browser and shows what it reported.
pub(crate) fn poll(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    let focus_changed = state
        .browser
        .set_viewport_focus(tx.focused() == Some(VIEWPORT));
    let settling = crate::ui::settle_sidebar(state, tx);
    let polled = state.browser.poll();
    let switcher_changed = crate::ui::tab_switcher::reconcile(state, tx);
    state.sync_find_tab();
    for (tab, request_id, text) in &polled.find_selections {
        if state.find_open
            && state.find_tab == Some(*tab)
            && state.find_selection_request == *request_id
            && state.find_input.text.is_empty()
            && !text.is_empty()
        {
            state.replace_find_input(text.clone(), true);
            state.find_input_changed();
        }
    }
    let mut tab_drag_changed = false;
    if state.tab_drag.is_dragging() {
        match state.tab_drag.refresh_target(state.browser.tabs()) {
            crate::ui::TabDragMoveResult::PreviewChanged => {
                tx.animated();
                tx.motion(crate::ui::TAB_REORDER_MOTION);
                tab_drag_changed = true;
            }
            crate::ui::TabDragMoveResult::Cancelled => tab_drag_changed = true,
            crate::ui::TabDragMoveResult::NoChange => {}
        }
        if let Some(y) = state.tab_drag.scroll_target(clock) {
            tx.scroll_to_instant("tabs-scroll", 0.0, y);
            tx.wake_after(std::time::Duration::ZERO);
        }
    }
    let tab_drag_settled = state.tab_drag.settle_tick(clock);
    if tab_drag_settled {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        tab_drag_changed = true;
    } else if let Some(wait) = state.tab_drag.settle_wake() {
        tx.wake_after(wait);
    }
    if let Some(error) = &polled.download_directory_error {
        state.configuration_issue = Some(error.clone());
    }
    let config_events = state
        .storage
        .as_ref()
        .map_or_else(Vec::new, Storage::take_config_events);
    let mut configuration_changed = polled.download_directory_error.is_some();
    for event in config_events {
        match event {
            storage::ConfigEvent::Saved(Ok(())) => {}
            storage::ConfigEvent::Saved(Err(error)) => {
                state.configuration_issue = Some(error);
                configuration_changed = true;
            }
            storage::ConfigEvent::Reloaded(Ok(load)) => {
                state.apply_config_load(load);
                configuration_changed = true;
            }
            storage::ConfigEvent::Reloaded(Err(error)) => {
                state.configuration_issue = Some(error);
                configuration_changed = true;
            }
        }
    }
    if let Some(storage) = &state.storage {
        for (_, url, title) in &polled.committed {
            if let Err(error) = storage.history().record_visit(url, title) {
                eprintln!("Elaris: {error}");
            }
        }
        for (url, title) in &polled.titles {
            if let Err(error) = storage.history().update_title(url, title) {
                eprintln!("Elaris: {error}");
            }
        }
    }
    if state.find_open
        && let Some(id) = state.find_tab
        && polled.committed.iter().any(|(tab, _, _)| *tab == id)
        && !state.find_input.text.is_empty()
    {
        let query = state.find_input.text.clone();
        let _ = state.browser.find(id, &query);
    }
    state.persist_session_if_changed();
    let history_page_changed = state.poll_history_page(
        tx,
        !polled.committed.is_empty() || !polled.titles.is_empty(),
    );
    let history_changed = if let Some(storage) = &state.storage {
        let revision = storage.history().revision();
        let changed = state.history_revision != revision;
        state.history_revision = revision;
        changed
    } else {
        false
    };
    if let Some(text) = polled.clipboard {
        tx.copy(text);
    }
    if let Some((id, menu)) = polled.menu
        && !state.omnibox.open
    {
        state.page_menu.receive(id, menu);
    }
    // While the page animates, Rune keeps drawing at the display's rate; a frame's pixels alone
    // would only wake it, which can miss every other refresh.
    if polled.animating {
        tx.wake_after(std::time::Duration::ZERO);
    }
    if let Some(title) = state.new_window_title() {
        tx.set_title(if title.is_empty() {
            APP_NAME.to_owned()
        } else {
            title
        });
    }
    polled.changed
        || history_changed
        || history_page_changed
        || settling
        || focus_changed
        || configuration_changed
        || switcher_changed
        || tab_drag_changed
}

/// A query was submitted from the focused omnibox.
pub(crate) fn submit_omnibox(state: &mut AppState, tx: &mut Tx) {
    if !state.omnibox.open
        || (state.omnibox.input.text.trim().is_empty() && !state.omnibox.selection_moved)
    {
        return;
    }
    let suggestions = state.suggestions();
    if let Some(suggestion) = suggestions.get(state.omnibox.selected).cloned() {
        state.choose_suggestion(suggestion, tx);
    } else {
        state.submit_omnibox();
        if state.address_error.is_none() {
            state.omnibox.close();
            tx.focus(VIEWPORT, false);
        }
    }
}

/// The sidebar launcher edits the current address.
pub(crate) fn open_from_launcher(state: &mut AppState, tx: &mut Tx) {
    state.open_omnibox(omnibox::Mode::Current);
    tx.focus("omnibox-input", false);
}

/// A new tab starts in the omnibox, ready to type.
pub(crate) fn new_tab(state: &mut AppState, tx: &mut Tx) {
    if state.cancel_tab_drag() {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
    state.open_omnibox(omnibox::Mode::New);
    tx.focus("omnibox-input", false);
}

/// Opens the existing settings tab, or creates it once, and returns focus to the native page.
pub(crate) fn open_settings(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    state.address_error = None;
    let tab = state.settings_tab();
    state.activate(tab);
    tx.focus(VIEWPORT, false);
}

pub(crate) fn open_history(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    let tab = state
        .active_history_tab()
        .or_else(|| state.browser.internal_tab(InternalPage::History))
        .unwrap_or_else(|| {
            state
                .browser
                .open_tab(InternalPage::History.address())
                .expect("valid built-in History route")
        });
    state.activate(tab);
    tx.focus(crate::ui::HISTORY_SEARCH, false);
}

/// Keyboard input the omnibox uses before the text editor handles it.
pub(crate) fn omnibox_input_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if crate::ui::tab_switcher::cycle_key(state, event, tx) {
        return true;
    }
    match &event.key {
        Key::Escape => {
            state.omnibox.close();
            tx.focus(VIEWPORT, false);
            true
        }
        Key::Up | Key::Down => {
            let count = state.suggestions().len();
            let direction = if event.key == Key::Up { -1 } else { 1 };
            state.omnibox.move_selection(count, direction);
            if count > 0 {
                tx.scroll_into_view(format!("omnibox-result-{}", state.omnibox.selected));
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn on_window_focus(state: &mut AppState, focused: bool, tx: &mut Tx) {
    state.browser.set_window_focus(focused);
    if !focused {
        crate::ui::tab_switcher::finish(state, None, tx);
    }
    if !focused && state.cancel_tab_drag() {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
}

pub(crate) fn shut_down(state: &mut AppState, _: &mut Tx) {
    state.persist_session();
    if let Some(storage) = &state.storage
        && let Err(error) = storage.flush_on_close()
    {
        eprintln!("Elaris: {error}");
    }
    state.browser.shut_down();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::HistoryMutation;
    use rune::Mods;

    fn active_url(state: &AppState) -> &str {
        &state.browser.active_tab().unwrap().url
    }

    #[test]
    fn starts_with_an_empty_window_without_an_about_blank_tab() {
        let state = AppState::new(None);
        assert!(state.browser.tabs().is_empty());
        assert!(!state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn blank_window_startup_does_not_create_a_web_tab() {
        let mut state = AppState::new(None);
        state.settings.set_startup(StartupMode::BlankWindow);
        state.open_start_page(None, None);
        assert!(state.browser.tabs().is_empty());
        assert!(!state.omnibox.open);
    }

    #[test]
    fn reduced_motion_centers_the_resized_omnibox_immediately() {
        let mut state = AppState::new(None);
        state.open_omnibox(omnibox::Mode::New);
        let mut runtime = rune::Runtime::new(
            state,
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.set_system_reduced_motion(true);
        runtime.frame(0.0);
        runtime.key(KeyEvent {
            key: Key::Text("x".into()),
            mods: Mods::default(),
        });
        runtime.frame(1.0 / 60.0);
        let panel = runtime
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("omnibox-panel"))
            .unwrap();
        assert_eq!(panel.rect, panel.target);
        let [x, y, width, height] = panel.rect;
        assert_eq!((x + width / 2.0, y + height / 2.0), (480.0, 320.0));
    }

    #[test]
    fn find_bar_tracks_a_narrow_page_and_uses_reversible_motion() {
        use rune::{Env, Runtime};

        let mut state = AppState::new(Some("example.com"));
        state.sidebar.collapsed = true;
        let mut runtime =
            Runtime::new(state, crate::ui::view, (420.0, 360.0), Env::new(false, 1.0));
        runtime.frame(0.0);
        runtime.update(|state, tx| state.open_find(tx));
        runtime.frame(1.0 / 120.0);

        let elements = runtime.elements();
        let page = elements
            .iter()
            .find(|element| element.key.as_deref() == Some(crate::ui::VIEWPORT))
            .expect("page viewport is rendered");
        let bar = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("find-bar"))
            .expect("find bar is rendered");
        assert!(bar.rect[0] >= page.rect[0]);
        assert!(bar.rect[0] + bar.rect[2] <= page.rect[0] + page.rect[2]);
        assert!(bar.opacity < 1.0, "opening should fade in");
        #[cfg(feature = "ladybird-bridge")]
        assert!(elements.iter().any(|element| {
            element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
        }));

        runtime.update(|state, tx| state.close_find(tx));
        runtime.frame(1.0 / 60.0);
        let closing = runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("find-bar"))
            .expect("closing bar remains for its exit transition");
        assert!(closing.exiting);
        assert!(closing.opacity < 1.0, "closing should fade out");
        runtime.frame(1.0);
        assert!(
            !runtime
                .elements()
                .iter()
                .any(|element| element.key.as_deref() == Some("find-bar") && !element.exiting)
        );
    }

    #[test]
    fn find_bar_reduced_motion_lands_and_closes_without_transition() {
        use rune::{Env, Runtime};

        let mut runtime = Runtime::new(
            AppState::new(Some("example.com")),
            crate::ui::view,
            (960.0, 640.0),
            Env::new(false, 1.0),
        );
        runtime.set_system_reduced_motion(true);
        runtime.frame(0.0);
        runtime.update(|state, tx| state.open_find(tx));
        runtime.frame(1.0 / 60.0);
        let bar = runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("find-bar"))
            .expect("find bar is rendered");
        assert_eq!(bar.rect, bar.target);
        assert_eq!(bar.opacity, 1.0);

        runtime.update(|state, tx| state.close_find(tx));
        runtime.frame(1.0);
        assert!(
            !runtime
                .elements()
                .iter()
                .any(|element| element.key.as_deref() == Some("find-bar") && !element.exiting)
        );
    }

    #[test]
    fn find_queries_follow_tabs_and_a_closed_tab_cannot_seed_another() {
        let mut state = AppState::new(Some("first.example"));
        let first = state.browser.active_tab().unwrap().id();
        let (engine, _fake) = crate::engine::Engine::fake();
        state.browser.connect(engine);
        let second = state.browser.open_tab("second.example").unwrap();
        state.open_find(&mut Tx::default());
        state.find_input.replace_text("alpha");
        state.find_input_changed();
        assert_eq!(state.browser.tab(first).unwrap().find.query, "alpha");

        state.activate(second);
        assert!(state.find_input.text.is_empty());
        state.find_input.replace_text("beta");
        state.find_input_changed();
        assert_eq!(state.browser.tab(second).unwrap().find.query, "beta");

        state.activate(first);
        assert_eq!(state.find_input.text, "alpha");
        state.close(first);
        assert_eq!(state.browser.active_tab().unwrap().id(), second);
        assert_eq!(state.find_input.text, "beta");
        assert_eq!(state.browser.tab(second).unwrap().find.query, "beta");
    }

    #[cfg(feature = "ladybird-bridge")]
    #[test]
    fn find_shortcuts_buttons_and_escape_keep_input_focus_and_route_to_the_active_tab() {
        use crate::engine::{Command, ViewCommand};
        use rune::{Key, KeyEvent, Runtime};

        let state = AppState::new(Some("example.com"));
        let tab = state.browser.active_tab().unwrap().id();
        let (engine, fake) = crate::engine::Engine::fake();
        let mut state = state;
        state.browser.connect(engine);
        let mut runtime = Runtime::new(
            state,
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.frame(0.0);

        runtime.update(|state, tx| {
            crate::shortcuts::on_key(
                state,
                &KeyEvent {
                    key: Key::Command('f'),
                    mods: Mods {
                        cmd: true,
                        ..Mods::default()
                    },
                },
                tx,
            );
        });
        runtime.frame(0.3);
        assert!(runtime.state.find_open);
        assert!(runtime.elements().iter().any(|element| {
            element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
        }));
        let _ = fake.commands.try_iter().collect::<Vec<_>>();

        runtime.key(KeyEvent {
            key: Key::Text("match".into()),
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.find_input.text, "match");
        let commands = fake.commands.try_iter().collect::<Vec<_>>();
        assert!(commands.iter().any(|command| matches!(command,
            Command::View(id, ViewCommand::Find { query, .. }) if *id == tab && query == "match"
        )));

        for (key, shift, expected_previous) in [
            (Key::Enter, false, false),
            (Key::Enter, true, true),
            (Key::Command('g'), false, false),
            (Key::Command('G'), true, true),
        ] {
            let command = matches!(&key, Key::Command(_));
            runtime.key(KeyEvent {
                key,
                mods: Mods {
                    cmd: command,
                    shift,
                    ..Mods::default()
                },
            });
            runtime.frame(0.0);
            let commands = fake.commands.try_iter().collect::<Vec<_>>();
            let has_next = commands.iter().any(|command| {
                matches!(command,
                    Command::View(id, ViewCommand::FindNext { .. }) if *id == tab
                )
            });
            let has_previous = commands.iter().any(|command| {
                matches!(command,
                    Command::View(id, ViewCommand::FindPrevious { .. }) if *id == tab
                )
            });
            assert_eq!(has_next, !expected_previous);
            assert_eq!(has_previous, expected_previous);
        }

        for (button_key, expected_previous) in
            [("find-previous-button", true), ("find-next-button", false)]
        {
            let button = runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(button_key))
                .expect("find navigation button is rendered");
            let point = [
                button.rect[0] + button.rect[2] / 2.0,
                button.rect[1] + button.rect[3] / 2.0,
            ];
            runtime.pointer_down(point[0], point[1]);
            runtime.frame(0.0);
            runtime.pointer_up(point[0], point[1]);
            runtime.frame(0.0);
            let focused = runtime
                .elements()
                .into_iter()
                .filter(|element| element.focused)
                .map(|element| (element.key, element.label))
                .collect::<Vec<_>>();
            assert!(
                runtime.elements().iter().any(|element| {
                    element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
                }),
                "focused elements after {button_key}: {focused:?}"
            );
            let commands = fake.commands.try_iter().collect::<Vec<_>>();
            let has_next = commands.iter().any(|command| {
                matches!(command,
                    Command::View(id, ViewCommand::FindNext { .. }) if *id == tab
                )
            });
            let has_previous = commands.iter().any(|command| {
                matches!(command,
                    Command::View(id, ViewCommand::FindPrevious { .. }) if *id == tab
                )
            });
            assert_eq!(has_next, !expected_previous);
            assert_eq!(has_previous, expected_previous);
        }

        runtime.state.find_input.anchor = 2;
        runtime.state.find_input.focus = 2;
        runtime.key(KeyEvent {
            key: Key::Command('f'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.find_input.anchor, 0);
        assert_eq!(runtime.state.find_input.focus, "match".len());

        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(!runtime.state.find_open);
        assert!(runtime.elements().iter().any(|element| {
            element.focused && element.key.as_deref() == Some(crate::ui::VIEWPORT)
        }));
        assert!(fake.commands.try_iter().any(|command| matches!(command,
            Command::View(id, ViewCommand::FindEnd) if id == tab
        )));
    }

    #[test]
    fn two_suggestions_fit_a_compact_panel() {
        let mut state = AppState::new(None);
        for address in ["example.org", "example.com"] {
            state.browser.open_tab(address).unwrap();
        }
        state.open_omnibox(omnibox::Mode::New);
        assert_eq!(state.suggestions().len(), 2);
        let mut runtime = rune::Runtime::new(
            state,
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        let elements = runtime.elements();
        let panel = elements
            .iter()
            .find(|el| el.key.as_deref() == Some("omnibox-panel"))
            .unwrap();
        assert!((190.0..=240.0).contains(&panel.target[3]));
    }

    #[test]
    fn startup_omnibox_accepts_typing_and_opens_the_first_tab_on_enter() {
        let mut state = AppState::new(None);
        state.open_omnibox(omnibox::Mode::New);
        let mut runtime = rune::Runtime::new(
            state,
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        runtime.key(KeyEvent {
            key: Key::Text("example.com".into()),
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(runtime.state.browser.tabs().is_empty());
        runtime.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.browser.tabs().len(), 1);
        assert_eq!(active_url(&runtime.state), "https://example.com");
        assert!(!runtime.state.omnibox.open);
    }

    #[test]
    fn opens_a_domain_it_was_given() {
        let state = AppState::new(Some("example.com"));
        assert_eq!(active_url(&state), "https://example.com");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn cmd_shift_t_reopens_the_last_closed_tab_in_its_original_section() {
        let mut state = AppState::new(Some("example.com"));
        let original = state.browser.active_tab().unwrap().id();
        state
            .browser
            .move_tab_to(original, TabSection::Favourite, 0)
            .unwrap();
        state.close(original);
        assert!(state.omnibox.open);

        let event = KeyEvent {
            key: Key::Command('t'),
            mods: Mods {
                cmd: true,
                shift: true,
                ..Mods::default()
            },
        };
        assert!(crate::shortcuts::on_key(
            &mut state,
            &event,
            &mut Tx::default()
        ));

        let reopened = state.browser.active_tab().unwrap();
        assert_ne!(reopened.id(), original);
        assert_eq!(reopened.url, "https://example.com");
        assert_eq!(reopened.section(), TabSection::Favourite);
        assert!(!state.omnibox.open);
        assert!(state.recently_closed().is_empty());
    }

    #[test]
    fn close_others_keeps_recently_closed_pinned_tabs_reopenable() {
        let mut state = AppState::new(Some("pinned.example"));
        let pinned = state.browser.active_tab().unwrap().id();
        state
            .browser
            .move_tab_to(pinned, TabSection::Pinned, 0)
            .unwrap();
        let kept = state.browser.open_tab("kept.example").unwrap();
        state.activate(kept);

        state.close_others(kept);
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(state.recently_closed().len(), 1);

        let reopened = state.reopen_recently_closed(0).unwrap();
        let tab = state.browser.tab(reopened).unwrap();
        assert_eq!(tab.url, "https://pinned.example");
        assert_eq!(tab.section(), TabSection::Pinned);
        assert_eq!(state.browser.active_tab().map(Tab::id), Some(reopened));
    }

    #[test]
    fn the_settings_shortcut_reuses_its_existing_native_tab() {
        let mut state = AppState::new(Some("example.com"));
        let website = state.browser.active_tab().unwrap().id();
        let mut tx = Tx::default();
        let event = KeyEvent {
            key: Key::Command(','),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        };
        assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
        let settings = state.browser.active_tab().unwrap().id();
        assert!(state.browser.active_tab().unwrap().is_internal());
        assert_eq!(state.browser.tabs().len(), 2);

        state.activate(website);
        assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
        assert_eq!(state.browser.tabs().len(), 2);
        assert_eq!(state.browser.active_tab().unwrap().id(), settings);
    }

    #[test]
    fn omnibox_routes_internal_pages_and_keeps_ordinary_web_navigation() {
        let mut state = AppState::new(Some("example.com"));
        let tab = state.browser.active_tab().unwrap().id();
        state.open_omnibox(omnibox::Mode::Current);
        state
            .omnibox
            .input
            .replace_text("elaris://settings".to_owned());
        state.submit_omnibox();
        assert_eq!(state.browser.active_tab().unwrap().id(), tab);
        assert!(state.browser.active_tab().unwrap().is_internal());
        assert_eq!(active_url(&state), "elaris://settings");

        state.open_omnibox(omnibox::Mode::Current);
        state.omnibox.input.replace_text("example.org".to_owned());
        state.submit_omnibox();
        assert!(!state.browser.active_tab().unwrap().is_internal());
        assert_eq!(active_url(&state), "https://example.org");
    }

    #[test]
    fn settings_updates_drive_search_download_and_startup_preferences() {
        let mut state = AppState::new(Some("example.com"));
        state.set_appearance(Appearance::Dark);
        state.set_search_engine(SearchEngine::Brave);
        state.set_startup(StartupMode::BlankWindow);
        let download_path = PathBuf::from("/tmp/elaris-test-downloads");
        state.set_download_directory(DownloadDirectory::Custom(download_path.clone()));

        assert_eq!(state.settings.appearance, Appearance::Dark);
        assert_eq!(state.settings.search_engine, SearchEngine::Brave);
        assert_eq!(state.settings.startup, StartupMode::BlankWindow);
        assert_eq!(state.download_directory(), download_path);

        state.open_omnibox(omnibox::Mode::Current);
        state
            .omnibox
            .input
            .replace_text("browser settings".to_owned());
        state.submit_omnibox();
        assert_eq!(
            active_url(&state),
            "https://search.brave.com/search?q=browser%20settings"
        );
    }

    #[test]
    fn restored_sessions_keep_internal_pages_in_their_tab_positions() {
        let mut state = AppState::new(None);
        let session = Session::new(
            vec![
                SessionTab {
                    url: "https://one.example/".into(),
                    title: "One".into(),
                },
                SessionTab {
                    url: "elaris://settings".into(),
                    title: "Settings".into(),
                },
                SessionTab {
                    url: "https://three.example/".into(),
                    title: "Three".into(),
                },
            ],
            1,
        );
        state.restore_session(&session);
        assert_eq!(state.browser.tabs().len(), 3);
        assert_eq!(state.browser.active_tab().unwrap().url, "elaris://settings");
        assert!(state.browser.active_tab().unwrap().is_internal());
        assert!(state.browser.tabs()[0].engine.deferred_load);
        assert!(state.browser.tabs()[2].engine.deferred_load);
    }

    #[test]
    fn history_shortcut_restores_native_tab_and_keyboard_opens_a_result() {
        use crate::engine::Command;

        let url = "https://example.org/guide";
        let db = elaris::history::History::open(std::path::Path::new(":memory:")).unwrap();
        db.record_visit(url, "Example guide").unwrap();
        let mut state = AppState::new(Some("elaris://history"));
        let restored = state.session_snapshot();
        state = AppState::new(None);
        state.restore_session(&restored);
        let tab = state.active_history_tab().unwrap();
        let (engine, fake) = crate::engine::Engine::fake();
        state.browser.connect(engine);
        assert!(
            !fake
                .commands
                .try_iter()
                .any(|command| matches!(command, Command::Open(..)))
        );

        let mut runtime =
            rune::Runtime::new(state, crate::ui::view, (880.0, 600.0), rune::Env::default());
        runtime.on_key = Some(crate::shortcuts::on_key);
        runtime.frame(0.0);
        let key = if cfg!(target_os = "macos") { 'y' } else { 'h' };
        for key in [key, key, 'f'] {
            runtime.key(KeyEvent {
                key: Key::Command(key),
                mods: Mods {
                    cmd: true,
                    ..Mods::default()
                },
            });
            runtime.frame(0.0);
            assert_eq!(runtime.state.browser.tabs().len(), 1);
            assert_eq!(runtime.state.active_history_tab(), Some(tab));
            assert!(!runtime.state.find_open);
            assert!(runtime.elements().iter().any(|element| {
                element.focused && element.key.as_deref() == Some(crate::ui::HISTORY_SEARCH)
            }));
        }
        runtime.update(move |state, _| {
            state.history_page.page = Some(db.page("guide", None).unwrap());
        });
        runtime.frame(0.0);
        runtime.key(KeyEvent {
            key: Key::Down,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(runtime.elements().iter().any(|element| {
            element.focused
                && element
                    .label
                    .as_deref()
                    .is_some_and(|label| label.contains("Example guide"))
        }));
        runtime.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), tab);
        assert_eq!(active_url(&runtime.state), url);
        assert!(runtime.state.active_history_tab().is_none());
        assert_eq!(
            fake.commands
                .try_iter()
                .filter(|command| matches!(command, Command::Open(id, _) if *id == tab))
                .count(),
            1
        );
    }

    #[test]
    fn removing_a_history_row_collapses_with_motion_and_snaps_with_reduced_motion() {
        for reduced_motion in [false, true] {
            let db = elaris::history::History::open(std::path::Path::new(":memory:")).unwrap();
            db.record_visit("https://older.example/", "Older").unwrap();
            db.record_visit("https://recent.example/", "Recent")
                .unwrap();
            let mut state = AppState::new(Some("elaris://history"));
            state.history_page.page = Some(db.page("", None).unwrap());
            let mut runtime =
                rune::Runtime::new(state, crate::ui::view, (880.0, 600.0), rune::Env::default());
            runtime.set_system_reduced_motion(reduced_motion);
            runtime.frame(0.0);
            let y = |runtime: &rune::Runtime<AppState>| {
                runtime
                    .elements()
                    .into_iter()
                    .find(|node| {
                        node.key.as_deref() == Some("history-entry:https://older.example/")
                    })
                    .unwrap()
                    .rect[1]
            };
            let before = y(&runtime);
            runtime.update(|state, tx| {
                state.history_page.page.as_mut().unwrap().entries.remove(0);
                tx.animated();
            });
            runtime.frame(1.0 / 60.0);
            let mid = y(&runtime);
            let exiting = runtime.elements().iter().any(|node| {
                node.key.as_deref() == Some("history-entry:https://recent.example/") && node.exiting
            });
            runtime.frame(1.0);
            let after = y(&runtime);
            assert!(before > after);
            assert_eq!(exiting, !reduced_motion);
            if reduced_motion {
                assert_eq!(mid, after);
            } else {
                assert!(mid <= before && mid > after, "{before}, {mid}, {after}");
            }
        }
    }

    #[test]
    fn history_mutation_preserves_tabs_and_query_on_success_or_failure() {
        let data = std::env::temp_dir().join(format!("elaris-app-history-{}", std::process::id()));
        let paths = storage::Paths {
            application_dir: data.clone(),
            profile_dir: data.join("profile"),
            history_file: data.join("history.sqlite"),
            session_file: data.join("session.json"),
            config_file: data.join("config.toml"),
        };
        let mut state = AppState::new(Some("elaris://history"));
        state.storage = Some(Storage::open(paths.clone()).unwrap());
        for (url, section) in [
            ("https://favourite.example/", TabSection::Favourite),
            ("https://pinned.example/", TabSection::Pinned),
        ] {
            state
                .browser
                .restore_tab_in_section(url, "", true, section)
                .unwrap();
        }
        let session = state.session_snapshot();

        let record = |state: &AppState, url: &str| {
            state
                .storage
                .as_ref()
                .unwrap()
                .history()
                .record_visit(url, "Example")
                .unwrap();
        };
        let pump = |state: &mut AppState| {
            state.storage.as_ref().unwrap().flush_on_close().unwrap();
            state.poll_history_page(&mut Tx::default(), false);
        };

        let url = "https://example.com/";
        record(&state, url);
        record(&state, "https://example.org/");
        state.history_page.input.replace_text("example");
        state.poll_history_page(&mut Tx::default(), false);
        pump(&mut state);
        assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 2);

        let db = rusqlite::Connection::open(&paths.history_file).unwrap();
        db.execute_batch("CREATE TRIGGER prevent_delete BEFORE DELETE ON history BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        state.mutate_history(HistoryMutation::Remove(url.into()));
        pump(&mut state);
        assert!(state.history_page.action_error.is_some());
        assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 2);
        db.execute_batch("DROP TRIGGER prevent_delete").unwrap();

        state.mutate_history(HistoryMutation::Remove(url.into()));
        pump(&mut state);
        pump(&mut state);
        assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 1);
        assert!(state.history_page.action_error.is_none());

        state.history_page.confirming = true;
        state.mutate_history(HistoryMutation::Clear);
        pump(&mut state);
        pump(&mut state);
        assert!(state.history_page.page.as_ref().unwrap().entries.is_empty());
        assert_eq!(state.history_page.input.text, "example");
        assert_eq!(state.session_snapshot(), session);
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn treats_an_ordinary_start_argument_as_a_search() {
        let state = AppState::new(Some("not an address"));
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(
            active_url(&state),
            "https://duckduckgo.com/?q=not%20an%20address"
        );
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn submitting_navigates_the_current_tab_for_urls_and_searches() {
        let mut state = AppState::new(Some("example.org"));
        state.omnibox.input.replace_text("example.com/a".to_owned());
        state.submit_omnibox();
        assert_eq!(active_url(&state), "https://example.com/a");
        assert_eq!(state.address_error, None);

        state
            .omnibox
            .input
            .replace_text("best Rust GUI framework".to_owned());
        state.submit_omnibox();
        assert_eq!(
            active_url(&state),
            "https://duckduckgo.com/?q=best%20Rust%20GUI%20framework"
        );
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn cmd_l_opens_with_the_current_url_selected_even_when_sidebar_is_collapsed() {
        let mut state = AppState::new(Some("example.com"));
        state.sidebar.collapsed = true;
        let mut tx = Tx::default();
        let event = KeyEvent {
            key: Key::Command('l'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        };
        assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
        assert!(state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "https://example.com");
        assert_eq!(
            (state.omnibox.input.anchor, state.omnibox.input.focus),
            (0, state.omnibox.input.text.len())
        );
        assert_eq!(active_url(&state), "https://example.com");
    }

    #[test]
    fn escape_closes_without_changing_the_page() {
        let mut state = AppState::new(Some("example.com"));
        state.open_omnibox(omnibox::Mode::Current);
        let mut tx = Tx::default();
        assert!(omnibox_input_key(
            &mut state,
            &KeyEvent {
                key: Key::Escape,
                mods: Mods::default(),
            },
            &mut tx,
        ));
        assert!(!state.omnibox.open);
        assert_eq!(active_url(&state), "https://example.com");
    }

    #[test]
    fn choosing_a_navigation_result_reuses_the_active_tab() {
        let mut state = AppState::new(Some("example.com"));
        let active = state.browser.active_tab().unwrap().id();
        state.open_omnibox(omnibox::Mode::Current);
        state.omnibox.input.replace_text("example.org".to_owned());
        let suggestion = state.suggestions().remove(0);
        state.choose_suggestion(suggestion, &mut Tx::default());
        assert_eq!(active_url(&state), "https://example.org");
        assert_eq!(state.browser.active_tab().unwrap().id(), active);
        assert_eq!(state.browser.tabs().len(), 1);
        assert!(!state.omnibox.open);
    }

    #[test]
    fn choosing_a_history_result_uses_the_omnibox_tab_mode() {
        let mut state = AppState::new(Some("example.org"));
        let current = state.browser.active_tab().unwrap().id();
        let recent = Suggestion {
            section: omnibox::Section::Recent,
            title: "Example Domain".to_owned(),
            detail: "example.com".to_owned(),
            glyph: omnibox::Glyph::History,
            action: Action::VisitRecent("https://example.com/".to_owned()),
        };

        state.open_omnibox(omnibox::Mode::Current);
        state.choose_suggestion(recent.clone(), &mut Tx::default());
        assert_eq!(state.browser.active_tab().unwrap().id(), current);
        assert_eq!(active_url(&state), "https://example.com/");
        assert_eq!(state.browser.tabs().len(), 1);

        state.open_omnibox(omnibox::Mode::New);
        state.choose_suggestion(recent, &mut Tx::default());
        assert_eq!(state.browser.tabs().len(), 2);
        assert_eq!(active_url(&state), "https://example.com/");
        assert_ne!(state.browser.active_tab().unwrap().id(), current);
    }

    #[test]
    fn new_tab_opens_empty_and_dismisses_without_creating_tabs() {
        let mut state = AppState::new(Some("example.com"));
        let mut tx = Tx::default();
        assert!(crate::shortcuts::on_key(
            &mut state,
            &KeyEvent {
                key: Key::Command('t'),
                mods: Mods {
                    cmd: true,
                    ..Mods::default()
                },
            },
            &mut tx,
        ));
        for _ in 0..3 {
            assert!(state.omnibox.open);
            submit_omnibox(&mut state, &mut tx);
            state.omnibox.close();
            assert_eq!(state.browser.tabs().len(), 1);
            assert_eq!(active_url(&state), "https://example.com");
            new_tab(&mut state, &mut tx);
        }
        for (query, expected, count) in [
            ("example.org", "https://example.org", 2),
            ("two words", "https://duckduckgo.com/?q=two%20words", 3),
        ] {
            new_tab(&mut state, &mut tx);
            state.omnibox.input.replace_text(query);
            submit_omnibox(&mut state, &mut tx);
            assert_eq!(state.browser.tabs().len(), count);
            assert_eq!(active_url(&state), expected);
        }
    }

    #[test]
    fn the_status_reports_unsupported_schemes_and_new_tabs_clear_them() {
        let mut state = AppState::new(None);
        assert_eq!(state.status(), None);
        state
            .omnibox
            .input
            .replace_text("ftp://example.com".to_owned());
        state.submit_omnibox();
        assert!(state.status().is_some());
        new_tab(&mut state, &mut Tx::default());
        assert_eq!(state.status(), None);
    }

    #[test]
    fn launcher_selects_the_current_address() {
        let mut state = AppState::new(Some("example.com"));
        open_from_launcher(&mut state, &mut Tx::default());
        assert!(state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "https://example.com");
    }

    #[test]
    fn selecting_an_open_tab_activates_it_without_opening_a_duplicate() {
        let mut state = AppState::new(Some("example.com"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("github.com").unwrap();
        state.activate(second);
        state.open_omnibox(omnibox::Mode::New);
        state.omnibox.input.replace_text("example.com");
        let tab_suggestion = state
            .suggestions()
            .into_iter()
            .find(|suggestion| suggestion.action == Action::SwitchTab(first))
            .unwrap();
        state.choose_suggestion(tab_suggestion, &mut Tx::default());
        assert_eq!(state.browser.active_tab().unwrap().id(), first);
        assert_eq!(state.browser.tabs().len(), 2);
        assert_eq!(active_url(&state), "https://example.com");
        assert!(!state.omnibox.open);
    }

    #[test]
    fn omnibox_keys_stay_in_the_input_and_escape_restores_page_focus() {
        use rune::{Env, Runtime};

        let mut runtime = Runtime::new(
            AppState::new(Some("example.com")),
            crate::ui::view,
            (960.0, 640.0),
            Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        runtime.update(|state, tx| {
            state.open_omnibox(omnibox::Mode::New);
            tx.focus("omnibox-input", false);
        });
        runtime.frame(0.0);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|element| element.focused && element.key.as_deref() == Some("omnibox-input"))
        );

        runtime.key(KeyEvent {
            key: Key::Text("x".into()),
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.omnibox.input.text, "x");
        assert_eq!(active_url(&runtime.state), "https://example.com");

        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(!runtime.state.omnibox.open);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|element| element.focused && element.key.as_deref() == Some(VIEWPORT))
        );
    }

    #[test]
    fn closing_the_last_tab_returns_to_the_empty_omnibox() {
        let mut state = AppState::new(Some("example.com"));
        let only = state.browser.active_tab().unwrap().id();
        state.close(only);
        assert!(state.browser.tabs().is_empty());
        assert!(state.omnibox.open);
        assert!(state.omnibox.input.text.is_empty());
    }

    #[test]
    fn session_restoration_keeps_tab_order_and_active_tab() {
        let mut state = AppState::new(None);
        let session = Session::new(
            vec![
                SessionTab {
                    url: "https://one.example/".into(),
                    title: "One".into(),
                },
                SessionTab {
                    url: "https://two.example/".into(),
                    title: "Two".into(),
                },
                SessionTab {
                    url: "https://three.example/".into(),
                    title: "Three".into(),
                },
            ],
            1,
        );

        state.restore_session(&session);

        let tabs = state.browser.tabs();
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[0].url, "https://one.example/");
        assert_eq!(tabs[1].url, "https://two.example/");
        assert_eq!(tabs[2].url, "https://three.example/");
        assert_eq!(
            state.browser.active_tab().unwrap().url,
            "https://two.example/"
        );
        assert_eq!(
            state.browser.recent_tabs(),
            [tabs[1].id(), tabs[0].id(), tabs[2].id()]
        );
        state.browser.activate_tab(tabs[1].id());
        let tabs = state.browser.tabs();
        assert_eq!(
            state.browser.recent_tabs(),
            [tabs[1].id(), tabs[0].id(), tabs[2].id()]
        );
        assert!(tabs[0].engine.deferred_load);
        assert!(!tabs[1].engine.deferred_load);
        assert!(tabs[2].engine.deferred_load);
    }

    #[test]
    fn restoring_an_empty_session_does_not_create_a_blank_tab() {
        let mut state = AppState::new(None);
        state.restore_session(&Session::new(Vec::new(), 0));
        assert!(state.browser.tabs().is_empty());
        assert!(!state.omnibox.open);
    }

    #[test]
    fn unsent_omnibox_text_is_not_part_of_the_session() {
        let mut state = AppState::new(None);
        state.open_omnibox(omnibox::Mode::New);
        state
            .omnibox
            .input
            .replace_text("typed but not submitted".to_owned());
        assert!(state.session_snapshot().tabs.is_empty());
    }

    #[test]
    fn session_snapshot_keeps_order_and_active_index() {
        let mut state = AppState::new(Some("one.example"));
        state.browser.open_tab("two.example").unwrap();
        let third = state.browser.open_tab("three.example").unwrap();
        state.activate(third);

        let session = state.session_snapshot();

        assert_eq!(session.tabs[0].url, "https://one.example");
        assert_eq!(session.tabs[1].url, "https://two.example");
        assert_eq!(session.tabs[2].url, "https://three.example");
        assert_eq!(session.active_tab, 2);
    }

    #[test]
    fn a_committed_reorder_is_saved_in_order_without_changing_the_active_tab() {
        let mut state = AppState::new(Some("one.example"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("two.example").unwrap();
        let third = state.browser.open_tab("three.example").unwrap();
        state.activate(second);

        state
            .commit_tab_move(first, TabSection::Regular, 2)
            .unwrap();

        assert_eq!(
            state.browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
            [second, third, first]
        );
        assert_eq!(state.browser.active_tab().map(Tab::id), Some(second));
        let session = state.session_snapshot();
        assert_eq!(
            session
                .tabs
                .iter()
                .map(|tab| tab.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://two.example",
                "https://three.example",
                "https://one.example"
            ]
        );
        assert_eq!(session.active_tab, 0);
    }

    #[test]
    fn favourite_tiles_fill_each_responsive_row() {
        use rune::{Env, Runtime};

        for favourite_count in 0..=9 {
            let mut candidate = AppState::new(Some("one.example"));
            let mut candidate_ids = vec![candidate.browser.active_tab().unwrap().id()];
            for index in 1..9 {
                candidate_ids.push(
                    candidate
                        .browser
                        .open_tab(&format!("tab-{index}.example"))
                        .unwrap(),
                );
            }
            for (index, id) in candidate_ids
                .iter()
                .take(favourite_count)
                .copied()
                .enumerate()
            {
                candidate
                    .browser
                    .move_tab_to(id, TabSection::Favourite, index)
                    .unwrap();
            }
            let mut runtime = Runtime::new(
                candidate,
                crate::ui::view,
                (960.0, 640.0),
                Env::new(false, 1.0),
            );
            runtime.frame(0.0);

            let elements = runtime.elements();
            if favourite_count == 0 {
                assert!(
                    !elements
                        .iter()
                        .any(|element| element.key.as_deref() == Some("favourites-grid"))
                );
                assert!(
                    !elements
                        .iter()
                        .any(|element| element.key.as_deref() == Some("pinned-section"))
                );
                continue;
            }
            let grid = elements
                .iter()
                .find(|element| element.key.as_deref() == Some("favourites-grid"))
                .expect("the favourites grid is visible")
                .rect;
            let tiles: Vec<_> = candidate_ids
                .iter()
                .take(favourite_count)
                .map(|id| {
                    elements
                        .iter()
                        .find(|element| {
                            element.key.as_deref() == Some(&format!("favourite-{}", id.raw()))
                        })
                        .expect("each favourite has a tile")
                        .rect
                })
                .collect();

            for (row_index, row) in tiles.chunks(4).enumerate() {
                let row_count = row.len();
                let gap = if row_count > 1 {
                    row[1][0] - row[0][0] - row[0][2]
                } else {
                    0.0
                };
                let expected_width = (grid[2] - gap * (row_count - 1) as f32) / row_count as f32;
                assert!((row[0][0] - grid[0]).abs() < 0.1);
                assert!(
                    (row[0][1]
                        - (grid[1]
                            + row_index as f32 * (48.0 + if row_count > 1 { gap } else { 8.0 })))
                    .abs()
                        < 0.1
                );
                for (column, tile) in row.iter().enumerate() {
                    assert!((tile[2] - expected_width).abs() < 0.1);
                    assert_eq!(tile[3], 48.0);
                    if column > 0 {
                        assert!((tile[0] - row[column - 1][0] - expected_width - gap).abs() < 0.1);
                    }
                }
                let last = row.last().unwrap();
                assert!((last[0] + last[2] - (grid[0] + grid[2])).abs() < 0.1);
            }
        }

        // The same count reflows against the narrower sidebar's actual content width.
        let mut narrow = AppState::new(Some("one.example"));
        narrow.sidebar.width = 220.0;
        let mut narrow_ids = vec![narrow.browser.active_tab().unwrap().id()];
        for index in 1..4 {
            narrow_ids.push(
                narrow
                    .browser
                    .open_tab(&format!("narrow-{index}.example"))
                    .unwrap(),
            );
        }
        for (index, id) in narrow_ids.iter().copied().enumerate() {
            narrow
                .browser
                .move_tab_to(id, TabSection::Favourite, index)
                .unwrap();
        }
        let mut runtime = Runtime::new(
            narrow,
            crate::ui::view,
            (960.0, 640.0),
            Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        let elements = runtime.elements();
        let grid = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("favourites-grid"))
            .unwrap()
            .rect;
        let tile = elements
            .iter()
            .find(|element| {
                element.key.as_deref() == Some(&format!("favourite-{}", narrow_ids[0].raw()))
            })
            .unwrap()
            .rect;
        let next = elements
            .iter()
            .find(|element| {
                element.key.as_deref() == Some(&format!("favourite-{}", narrow_ids[1].raw()))
            })
            .unwrap()
            .rect;
        let gap = next[0] - tile[0] - tile[2];
        assert!((tile[2] - (grid[2] - 3.0 * gap) / 4.0).abs() < 0.1);
    }

    #[test]
    fn adding_a_favourite_animates_existing_tile_and_favicon_during_preview() {
        use rune::{Env, Runtime};

        let mut state = AppState::new(Some("one.example"));
        let favourite = state.browser.active_tab().unwrap().id();
        let regular = state.browser.open_tab("two.example").unwrap();
        state
            .browser
            .move_tab_to(favourite, TabSection::Favourite, 0)
            .unwrap();
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);

        let element = |runtime: &Runtime<AppState>, key: &str| {
            runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(key))
                .unwrap()
        };
        let favourite_tile = format!("favourite-{}", favourite.raw());
        let initial_tile = element(&runtime, &favourite_tile);
        let initial_icon = element(&runtime, &format!("favourite-icon-{}", favourite.raw()));
        let regular_row = element(&runtime, &format!("tab-{}", regular.raw()));
        let grid = element(&runtime, "favourites-grid").rect;
        let pointer = [
            regular_row.rect[0] + 24.0,
            regular_row.rect[1] + regular_row.rect[3] / 2.0,
        ];
        runtime.pointer_move(pointer[0], pointer[1]);
        runtime.pointer_down(pointer[0], pointer[1]);
        runtime.frame(0.0);

        let destination = [grid[0] + grid[2] * 0.75, grid[1] + grid[3] / 2.0];
        runtime.pointer_move(destination[0], destination[1]);
        runtime.frame(1.0 / 60.0);

        assert!(runtime.state.tab_drag.is_dragging());
        assert_eq!(
            runtime.state.browser.tab(regular).unwrap().section(),
            TabSection::Regular
        );
        let preview = element(&runtime, &favourite_tile);
        let preview_icon = element(&runtime, &format!("favourite-icon-{}", favourite.raw()));
        assert!(preview.target[2] < initial_tile.rect[2] * 0.75);
        assert!(
            preview.rect[2] < initial_tile.rect[2] && preview.rect[2] > preview.target[2],
            "the existing tile eases to its new width during the drag: {:?} -> {:?}",
            preview.rect,
            preview.target
        );
        assert!(
            preview_icon.target[0] < initial_icon.rect[0]
                && preview_icon.rect[0] < initial_icon.rect[0]
                && preview_icon.rect[0] > preview_icon.target[0],
            "the favicon follows the tile's live horizontal layout motion: {:?} -> {:?}",
            preview_icon.rect,
            preview_icon.target
        );
        assert_eq!(
            runtime.state.browser.active_tab().map(Tab::id),
            Some(favourite),
            "dragging a background tab must not activate it"
        );
    }

    #[test]
    fn favourite_morph_keeps_the_dragged_favicon_moving_inside_the_tile() {
        use rune::{Env, Runtime};

        let state = AppState::new(Some("one.example"));
        let tab = state.browser.active_tab().unwrap().id();
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);
        let rect = |runtime: &Runtime<AppState>, key: &str| {
            runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(key))
                .unwrap()
                .rect
        };
        let row = rect(&runtime, &format!("tab-{}", tab.raw()));
        let scroll = rect(&runtime, "tabs-scroll");
        let (x, y) = (row[0] + 24.0, row[1] + row[3] / 2.0);
        let destination_y = scroll[1] - 20.0;
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.frame(0.0);
        runtime.pointer_move(x, destination_y);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.tab_drag.is_dragging());

        let tracking_point = [x + 40.0, destination_y + 4.0];
        runtime.pointer_move(tracking_point[0], tracking_point[1]);
        runtime.frame(1.0 / 60.0);
        let live_visual = rect(&runtime, "tab-drag-visual");
        assert!((live_visual[0] - (tracking_point[0] - 24.0)).abs() < 0.1);
        assert!((live_visual[1] - (tracking_point[1] - 17.0)).abs() < 0.1);

        runtime.pointer_up(tracking_point[0], tracking_point[1]);
        for _ in 0..5 {
            runtime.frame(1.0 / 60.0);
        }
        assert_eq!(
            runtime.state.browser.tab(tab).map(Tab::section),
            Some(TabSection::Favourite)
        );

        let visual = rect(&runtime, "tab-drag-visual");
        let icon = rect(&runtime, "drag-tab-icon");
        assert!(
            icon[0] + icon[2] / 2.0 > visual[0] + visual[2] / 3.0,
            "the favicon moves toward the tile center during the morph: icon {icon:?}, visual {visual:?}"
        );
    }

    #[test]
    fn dragging_a_background_tab_reorders_live_without_activating_it() {
        use rune::{Env, Runtime};

        let mut state = AppState::new(Some("one.example"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("two.example").unwrap();
        let third = state.browser.open_tab("three.example").unwrap();
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);

        let rect = |runtime: &Runtime<AppState>, id: TabId| {
            runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(&format!("tab-{}", id.raw())))
                .unwrap()
                .rect
        };
        let second_rect = rect(&runtime, second);
        let third_rect = rect(&runtime, third);
        let before_order = [first, second, third];
        let pointer_x = second_rect[0] + 24.0;
        let pointer_y = second_rect[1] + second_rect[3] / 2.0;
        runtime.pointer_move(pointer_x, pointer_y);
        runtime.pointer_down(pointer_x, pointer_y);
        runtime.frame(0.0);
        runtime.pointer_move(pointer_x, third_rect[1] + third_rect[3] / 2.0 + 5.0);
        runtime.frame(1.0 / 60.0);

        assert!(runtime.state.tab_drag.is_dragging());
        assert_eq!(
            runtime
                .state
                .browser
                .tabs()
                .iter()
                .map(Tab::id)
                .collect::<Vec<_>>(),
            before_order,
            "the browser order stays committed until release"
        );
        let third_mid_y = rect(&runtime, third)[1];
        assert!(
            third_mid_y < third_rect[1] && third_mid_y > second_rect[1],
            "the neighbouring row starts moving into the preview slot"
        );

        let release_y = third_rect[1] + third_rect[3] / 2.0 + 5.0;
        let dragged_visual = runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("tab-drag-visual"))
            .expect("drag overlay is visible");
        assert!(
            (dragged_visual.rect[0] - second_rect[0]).abs() < 0.1,
            "drag visual x {} differs from row x {}",
            dragged_visual.rect[0],
            second_rect[0]
        );
        assert!(
            (dragged_visual.rect[1] - (release_y - second_rect[3] / 2.0)).abs() < 0.1,
            "drag visual y {} differs from expected row y {}",
            dragged_visual.rect[1],
            release_y - second_rect[3] / 2.0
        );
        runtime.pointer_up(pointer_x, release_y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(
            runtime
                .state
                .browser
                .tabs()
                .iter()
                .map(Tab::id)
                .collect::<Vec<_>>(),
            [first, third, second]
        );
        assert_eq!(runtime.state.browser.active_tab().map(Tab::id), Some(first));
        assert!(
            runtime.state.tab_drag.visual().is_some(),
            "the drop is still settling"
        );
    }

    #[test]
    fn a_tab_click_without_drag_still_selects_the_tab() {
        use rune::{Env, Runtime};

        let mut state = AppState::new(Some("one.example"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("two.example").unwrap();
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);

        let row = runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(&format!("tab-{}", second.raw())))
            .unwrap()
            .rect;
        let (x, y) = (row[0] + 24.0, row[1] + row[3] / 2.0);
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.frame(0.0);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(
            runtime.state.browser.active_tab().map(Tab::id),
            Some(second)
        );
        assert_eq!(
            runtime
                .state
                .browser
                .tabs()
                .iter()
                .map(Tab::id)
                .collect::<Vec<_>>(),
            [first, second],
            "clicking leaves tab order unchanged"
        );
        assert!(!runtime.state.tab_drag.is_dragging());
    }

    #[test]
    fn escape_cancels_a_live_tab_preview_without_persisting_an_order_change() {
        use rune::{Env, Runtime};

        let mut state = AppState::new(Some("one.example"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("two.example").unwrap();
        let third = state.browser.open_tab("three.example").unwrap();
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);
        let row = |runtime: &Runtime<AppState>, id: TabId| {
            runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(&format!("tab-{}", id.raw())))
                .unwrap()
                .rect
        };
        let second_rect = row(&runtime, second);
        let third_rect = row(&runtime, third);
        let (x, y) = (second_rect[0] + 24.0, second_rect[1] + second_rect[3] / 2.0);
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.frame(0.0);
        runtime.pointer_move(x, third_rect[1] + third_rect[3] / 2.0 + 5.0);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.tab_drag.is_dragging());

        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(1.0 / 60.0);

        assert_eq!(
            runtime
                .state
                .browser
                .tabs()
                .iter()
                .map(Tab::id)
                .collect::<Vec<_>>(),
            [first, second, third]
        );
        assert!(!runtime.state.tab_drag.is_dragging());
        assert!(
            runtime.state.tab_drag.visual().is_some(),
            "cancellation settles the shared visual back to its source"
        );
    }
}
