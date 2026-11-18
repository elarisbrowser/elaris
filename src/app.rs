// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`Browser`].

use rune::{Key, KeyEvent, PointerPress, Tx};
use std::path::PathBuf;
use std::sync::Arc;

use crate::address::{self, Destination, SearchEngine};
use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::config::ConfigLoad;
use crate::engine::Engine;
use crate::internal::{InternalPage, InternalRoute};
use crate::omnibox::{self, Action, Suggestion};
use crate::settings::{DownloadDirectory, Settings, SettingsChange, StartupMode};
use crate::storage;
use crate::storage::{Session, SessionTab, Storage};
use crate::tab::{Tab, TabId};
use crate::ui::{SidebarState, VIEWPORT};

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

pub(crate) struct AppState {
    pub(crate) omnibox: omnibox::State,
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
    profile_dir: Option<PathBuf>,
    config_file: Option<PathBuf>,
    default_download_directory: PathBuf,
    download_directory_override: Option<PathBuf>,
    pub(crate) configuration_issue: Option<String>,
    storage_error: Option<String>,
    last_saved_session: Option<Session>,
    history_revision: u64,
    /// The page title the window shows.
    window_title: Option<String>,
    context_menu: Option<(Option<TabId>, PointerPress)>,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    /// Opens the supplied page, or shows the omnibox in an empty window.
    fn new(start: Option<&str>) -> Self {
        #[cfg(test)]
        let (storage, profile_dir, config_file, config_load, session, storage_error) = (
            None,
            None,
            None,
            None::<Result<ConfigLoad, String>>,
            None,
            None,
        );
        #[cfg(not(test))]
        let (storage, profile_dir, config_file, config_load, session, storage_error) =
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
            profile_dir,
            config_file,
            default_download_directory,
            download_directory_override,
            configuration_issue,
            storage_error,
            last_saved_session: session.clone(),
            history_revision: 0,
            window_title: None,
            context_menu: None,
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
            // Keep startup visually blank without creating a web tab; the sidebar launcher
            // opens a tab when the user submits an address.
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
        self.address_error = None;
    }

    /// Closes the tab and opens the omnibox when the window becomes empty.
    pub(crate) fn close(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        self.page_menu = Default::default();
        self.browser.close_tab(tab);
        if self.browser.tabs().is_empty() {
            self.open_omnibox(omnibox::Mode::New);
        }
    }

    pub(crate) fn duplicate(&mut self, tab: TabId) {
        if let Some(tab) = self.browser.duplicate_tab(tab) {
            self.activate(tab);
        }
    }

    pub(crate) fn close_others(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        self.browser.close_other_tabs(tab);
    }

    pub(crate) fn cancel_tab_drag(&mut self) -> bool {
        self.tab_drag.cancel()
    }

    pub(crate) fn commit_tab_reorder(
        &mut self,
        tab: TabId,
        target_index: usize,
    ) -> Result<(), crate::browser::Error> {
        self.browser.move_tab(tab, target_index)?;
        self.persist_session_if_changed();
        Ok(())
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
                    .restore_tab(&tab.url, &tab.title, index != session.active_tab)
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
        let tabs = self
            .browser
            .tabs()
            .iter()
            .map(|tab| SessionTab {
                url: tab.url.clone(),
                title: tab.title.clone(),
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
        Session::new(tabs, active_tab)
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

/// Starts the browser engine. Native windows do this after Rune's first frame; Ladybird starts on
/// the engine's thread, and the waker schedules `poll` when it has news.
pub(crate) fn start_engine(state: &mut AppState, waker: rune::Waker) {
    let Some(profile_dir) = state.profile_dir.clone() else {
        state.browser.fail(
            state
                .storage_error
                .clone()
                .unwrap_or_else(|| "Cannot locate the browser profile directory".to_owned()),
        );
        return;
    };
    if let Some(storage) = &state.storage {
        storage.set_notify(Arc::clone(&waker));
    }
    match Engine::start(waker, profile_dir, state.engine_download_directory()) {
        Ok(engine) => state.browser.connect(engine),
        Err(error) => state.browser.fail(error),
    }
}

/// Headless tooling has no native `on_ready`, so start its engine during Rune's setup callback.
pub(crate) fn setup_headless(state: &mut AppState, waker: rune::Waker) {
    if rune::is_headless() {
        start_engine(state, waker);
    }
}

/// Rune's per-frame hook: forwards UI changes to the browser and shows what it reported.
pub(crate) fn poll(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    let focus_changed = state
        .browser
        .set_viewport_focus(tx.focused() == Some(VIEWPORT));
    let settling = crate::ui::settle_sidebar(state, tx);
    let polled = state.browser.poll();
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
    state.persist_session_if_changed();
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
        || settling
        || focus_changed
        || configuration_changed
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
    state.omnibox.close();
    state.address_error = None;
    let tab = state.settings_tab();
    state.activate(tab);
    tx.focus(VIEWPORT, false);
}

/// Keyboard input the omnibox uses before the text editor handles it.
pub(crate) fn omnibox_input_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
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
    if !focused && state.cancel_tab_drag() {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
}

pub(crate) fn shut_down(state: &mut AppState, _: &mut Tx) {
    state.persist_session();
    state.browser.shut_down();
}

#[cfg(test)]
mod tests {
    use super::*;
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

        state.commit_tab_reorder(first, 2).unwrap();

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
        assert!(runtime.state.tab_drag.visual().is_none());
    }
}
