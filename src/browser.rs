// SPDX-License-Identifier: GPL-3.0-only

//! The browser: its tabs, which one is active, and the one engine behind them all.
//!
//! ```ignore
//! let tab = browser.open_tab("https://example.com")?;
//! browser.activate_tab(tab);
//! browser.navigate(tab, "https://ladybird.org")?;
//! browser.reload(tab);
//! browser.close_tab(tab);
//! ```
//!
//! Every operation returns at once. What it asks of the engine happens on the engine's thread,
//! and its outcome (the committed address, title, loading state, errors) arrives in the tab's
//! state through [`Browser::poll`]. Tabs are named by [`TabId`]s, which are never reused: an
//! operation on a tab that has closed does nothing, and nothing the engine reports about a closed
//! tab reaches another one. Only the active tab is visible, laid out at the viewport's size and
//! given focus and input; the others keep their pages but stop painting.

use std::cell::Cell;
use std::fmt;
use std::time::Instant;

mod input_routing;

use crate::address::url_for_address;
use crate::engine::{Command, DownloadAction, Engine, Surface, Update, ViewCommand};
use crate::input;
use crate::internal::{InternalPage, InternalRoute};
use crate::perf::{self, Stage};
use crate::tab::{Tab, TabId, TabSection};
use elaris::downloads::{
    self, Download, DownloadId, Request as DownloadRequest, Status as DownloadStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
    /// The address cannot be loaded, for the reason given.
    Address(&'static str),
    /// No open tab has this id.
    NoSuchTab(TabId),
    /// The requested position is outside the current tab order.
    #[cfg(test)]
    InvalidTabIndex(usize),
    /// The requested position is outside the destination section.
    InvalidSectionIndex(usize),
    /// The requested download operation could not be queued.
    Download(downloads::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(reason) => f.write_str(reason),
            Self::NoSuchTab(_) => f.write_str("The tab has closed"),
            #[cfg(test)]
            Self::InvalidTabIndex(_) => f.write_str("The tab position is out of range"),
            Self::InvalidSectionIndex(_) => f.write_str("The section position is out of range"),
            Self::Download(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// What one poll found.
#[derive(Default)]
pub(crate) struct Polled {
    /// Some tab's state changed, so the interface needs building again.
    pub(crate) changed: bool,
    pub(crate) clipboard: Option<String>,
    pub(crate) menu: Option<(TabId, elaris::ladybird::PageMenu)>,
    /// The active tab showed a frame since the last poll, so it is likely animating.
    pub(crate) animating: bool,
    /// URLs Ladybird confirmed as committed, with the title currently known for each tab.
    pub(crate) committed: Vec<(TabId, String, String)>,
    /// Titles reported for a tab's last committed URL.
    pub(crate) titles: Vec<(String, String)>,
    /// The engine could not apply the selected download directory.
    pub(crate) download_directory_error: Option<String>,
}

pub(crate) struct Browser {
    /// In the order the sidebar shows them.
    tabs: Vec<Tab>,
    active: Option<TabId>,
    next_id: u64,
    input: input_routing::Held,
    /// None until the app connects one, and in a build without an engine.
    engine: Option<Engine>,
    /// Why there is no engine, which every tab shows.
    engine_error: Option<String>,
    /// Downloads belong to the browser process, not to the tab that requested them.
    downloads: Vec<Download>,
    window: Window,
    /// Development timing: the last poll.
    last_poll: Option<Instant>,
}

/// What the window tells the browser about the page viewport, which every tab shares.
struct Window {
    /// The display scale, as the view last saw it.
    scale: Cell<f32>,
    /// Whether the interface is dark, as the view last drew it, and what the engine was told.
    /// Unknown until the view first builds, and nothing is sent until then.
    dark: Cell<Option<bool>>,
    applied_dark: Cell<Option<bool>>,
    /// The viewport's size in points, from layout.
    viewport: [f32; 2],
    /// Whether the window, and the viewport within it, have focus.
    focused: bool,
    viewport_focused: bool,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
            input: Default::default(),
            engine: None,
            engine_error: None,
            downloads: Vec::new(),
            window: Window {
                scale: Cell::new(1.0),
                dark: Cell::new(None),
                applied_dark: Cell::new(None),
                viewport: [0.0; 2],
                focused: true,
                viewport_focused: false,
            },
            last_poll: None,
        }
    }
}

impl Browser {
    /// Gives the browser the engine that shows its tabs, opening a view for each.
    #[cfg(any(feature = "ladybird-bridge", test))]
    pub(crate) fn connect(&mut self, engine: Engine) {
        for tab in &self.tabs {
            if !tab.is_internal() {
                engine.send(Command::Open(tab.id(), tab.surface.clone()));
            }
        }
        self.engine = Some(engine);
    }

    /// Records that the engine cannot run, which every tab shows.
    pub(crate) fn fail(&mut self, error: String) {
        for tab in &mut self.tabs {
            tab.error = Some(error.clone());
        }
        self.engine_error = Some(error);
    }

    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(crate) fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    /// The browser's downloads, including work initiated by a webpage.
    pub(crate) fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub(crate) fn set_download_directory(&self, directory: Option<std::path::PathBuf>) {
        if let Some(engine) = &self.engine {
            engine.send(Command::SetDownloadDirectory(directory));
        }
    }

    /// Starts a Ladybird transfer and returns its stable browser identifier immediately.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "The direct Rust start path complements page initiated downloads from Ladybird."
        )
    )]
    pub(crate) fn start_download(
        &mut self,
        mut request: DownloadRequest,
    ) -> Result<DownloadId, Error> {
        let Some(engine) = &self.engine else {
            return Err(Error::Download(downloads::Error::EngineUnavailable));
        };
        let is_http_url = request
            .url
            .get(..7)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
            || request
                .url
                .get(..8)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
        if !is_http_url {
            return Err(Error::Download(downloads::Error::InvalidAddress));
        }
        let url = url_for_address(&request.url)
            .map_err(|_| Error::Download(downloads::Error::InvalidAddress))?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(Error::Download(downloads::Error::InvalidAddress));
        }
        request.url.clone_from(&url);
        let id = DownloadId::next().map_err(Error::Download)?;
        self.downloads.push(Download::pending(id, &request, url));
        engine.send(Command::StartDownload(id, request));
        Ok(id)
    }

    /// Cancels an active download without waiting for Ladybird's cancellation callback.
    pub(crate) fn cancel_download(&self, id: DownloadId) -> Result<(), Error> {
        let download = self.download(id)?;
        if !matches!(
            download.status,
            DownloadStatus::Pending | DownloadStatus::Downloading
        ) {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Cancel)
    }

    /// Opens a completed file through the operating system's default handler.
    pub(crate) fn open_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status != DownloadStatus::Completed {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Open)
    }

    /// Opens the completed file's containing folder in the system file manager.
    pub(crate) fn reveal_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status != DownloadStatus::Completed {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Reveal)
    }

    fn download(&self, id: DownloadId) -> Result<&Download, Error> {
        self.downloads
            .iter()
            .find(|download| download.id == id)
            .ok_or(Error::Download(downloads::Error::NoSuchDownload))
    }

    fn send_download_action(&self, id: DownloadId, action: DownloadAction) -> Result<(), Error> {
        let Some(engine) = &self.engine else {
            return Err(Error::Download(downloads::Error::EngineUnavailable));
        };
        engine.send(Command::DownloadAction(id, action));
        Ok(())
    }

    fn apply_download_update(&mut self, id: DownloadId, update: downloads::Update) {
        let Some(download) = self.downloads.iter_mut().find(|download| download.id == id) else {
            self.downloads.push(update.into_download(id));
            return;
        };
        *download = update.into_download(id);
    }

    /// The tab `offset` places from `id` in the list, wrapping around its ends.
    pub(crate) fn tab_beside(&self, id: TabId, offset: isize) -> Option<TabId> {
        let index = self.tabs.iter().position(|tab| tab.id() == id)?;
        let len = self.tabs.len() as isize;
        let beside = (index as isize + offset).rem_euclid(len) as usize;
        Some(self.tabs[beside].id())
    }

    /// Moves an open tab to its zero-based position without touching its page or active state.
    #[cfg(test)]
    pub(crate) fn move_tab(&mut self, id: TabId, target_index: usize) -> Result<(), Error> {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return Err(Error::NoSuchTab(id));
        };
        let section = self.tabs[source_index].section();
        let section_index = self.tabs[..source_index]
            .iter()
            .filter(|tab| tab.section() == section)
            .count();
        let section_len = self
            .tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .count();
        if target_index >= section_len {
            return Err(Error::InvalidTabIndex(target_index));
        }
        if target_index == section_index {
            return Ok(());
        }
        self.move_tab_to(id, section, target_index)
    }

    /// Moves a live tab into `section` at its zero-based position without changing its page or
    /// active state. The single tab vector stays grouped in keyboard order: favourites, pinned,
    /// then regular tabs.
    pub(crate) fn move_tab_to(
        &mut self,
        id: TabId,
        section: TabSection,
        target_index: usize,
    ) -> Result<(), Error> {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return Err(Error::NoSuchTab(id));
        };
        let source_section = self.tabs[source_index].section();
        let source_section_index = self.tabs[..source_index]
            .iter()
            .filter(|tab| tab.section() == source_section)
            .count();
        let target_len = self
            .tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .count()
            - usize::from(source_section == section);
        if target_index > target_len {
            return Err(Error::InvalidSectionIndex(target_index));
        }
        if source_section == section && source_section_index == target_index {
            return Ok(());
        }

        let mut tab = self.tabs.remove(source_index);
        tab.section = section;
        let section_start = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) >= section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(section_start + target_index, tab);
        Ok(())
    }

    pub(crate) fn section_tabs(&self, section: TabSection) -> impl Iterator<Item = &Tab> {
        self.tabs.iter().filter(move |tab| tab.section() == section)
    }

    pub(crate) fn clear_active_tab(&mut self) {
        self.reset_input();
        self.active = None;
    }

    /// Opens a tab at the end of the list loading `address`, and makes it active if no tab is.
    pub(crate) fn open_tab(&mut self, address: &str) -> Result<TabId, Error> {
        if let Some(route) = InternalRoute::parse(address) {
            return Ok(self.insert_internal_tab(route));
        }
        let url = url_for_address(address).map_err(Error::Address)?;
        Ok(self.insert_tab(url))
    }

    pub(crate) fn internal_tab(&self, page: InternalPage) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| tab.internal == Some(InternalRoute::Page(page)))
            .map(Tab::id)
    }

    fn insert_tab(&mut self, url: String) -> TabId {
        self.insert_tab_with_state(url, String::new(), false, TabSection::Regular)
    }

    /// Restores a tab without starting a background page until it becomes active.
    #[cfg(test)]
    pub(crate) fn restore_tab(
        &mut self,
        address: &str,
        title: &str,
        deferred_load: bool,
    ) -> Result<TabId, Error> {
        self.restore_tab_in_section(address, title, deferred_load, TabSection::Regular)
    }

    pub(crate) fn restore_tab_in_section(
        &mut self,
        address: &str,
        title: &str,
        deferred_load: bool,
        section: TabSection,
    ) -> Result<TabId, Error> {
        if let Some(route) = InternalRoute::parse(address) {
            return Ok(self.insert_internal_tab_in_section(route, section));
        }
        let url = url_for_address(address).map_err(Error::Address)?;
        Ok(self.insert_tab_with_state(url, title.to_owned(), deferred_load, section))
    }

    fn insert_internal_tab(&mut self, route: InternalRoute) -> TabId {
        self.insert_internal_tab_in_section(route, TabSection::Regular)
    }

    fn insert_internal_tab_in_section(
        &mut self,
        route: InternalRoute,
        section: TabSection,
    ) -> TabId {
        let id = TabId::from_raw(self.next_id);
        self.next_id += 1;
        let mut tab = Tab::new_internal(id, route);
        tab.section = section;
        let index = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) > section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active.get_or_insert(id);
        id
    }

    fn insert_tab_with_state(
        &mut self,
        url: String,
        title: String,
        deferred_load: bool,
        section: TabSection,
    ) -> TabId {
        let id = TabId::from_raw(self.next_id);
        self.next_id += 1;
        let mut tab = Tab::new(id, url);
        tab.section = section;
        tab.title = title;
        tab.engine.deferred_load = deferred_load;
        tab.error.clone_from(&self.engine_error);
        if !tab.is_internal()
            && let Some(engine) = &self.engine
        {
            engine.send(Command::Open(id, tab.surface.clone()));
        }
        let index = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) > section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(index, tab);
        self.active.get_or_insert(id);
        id
    }

    /// Shows the tab in the viewport and gives it the page's input.
    pub(crate) fn activate_tab(&mut self, id: TabId) {
        if self.tab(id).is_none() {
            return;
        }
        if self.active != Some(id) {
            self.reset_input();
            self.active = Some(id);
        }
        if let Some(tab) = self.tab_mut(id) {
            tab.engine.deferred_load = false;
        }
    }

    /// Loads `address` in the tab.
    pub(crate) fn navigate(&mut self, id: TabId, address: &str) -> Result<(), Error> {
        if InternalRoute::parse(address).is_some() {
            return Err(Error::Address(
                "Internal pages must be opened through Elaris navigation",
            ));
        }
        let url = url_for_address(address).map_err(Error::Address)?;
        let connected = self.engine.is_some();
        let open_view = self.tab(id).ok_or(Error::NoSuchTab(id))?.is_internal();
        let mut load_now = false;
        let mut surface = None;
        {
            let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
            if open_view {
                tab.internal = None;
                tab.surface = Surface::default();
                tab.engine = Default::default();
                tab.committed_url.clear();
                surface = Some(tab.surface.clone());
            }
            tab.url.clone_from(&url);
            tab.error = None;
            // A tab the engine has not laid out yet loads once it has.
            if open_view || !connected || tab.engine.pending_load.is_some() {
                tab.engine.pending_load = Some(url.clone());
            } else {
                load_now = true;
            }
        }
        if let Some(surface) = surface
            && let Some(engine) = &self.engine
        {
            engine.send(Command::Open(id, surface));
        }
        if load_now {
            self.send(id, ViewCommand::Load(url));
        }
        Ok(())
    }

    /// Replaces a tab with an Elaris-owned page without asking Ladybird to load its address.
    pub(crate) fn navigate_internal(
        &mut self,
        id: TabId,
        route: InternalRoute,
    ) -> Result<(), Error> {
        let is_internal = self.tab(id).ok_or(Error::NoSuchTab(id))?.is_internal();
        if !is_internal && let Some(engine) = &self.engine {
            engine.send(Command::Close(id));
        }
        let internal = Tab::new_internal(id, route);
        *self.tab_mut(id).ok_or(Error::NoSuchTab(id))? = internal;
        Ok(())
    }

    pub(crate) fn reload(&mut self, id: TabId) {
        if self.tab(id).is_some_and(|tab| !tab.is_internal()) {
            self.send(id, ViewCommand::Reload);
        }
    }

    pub(crate) fn stop_loading(&mut self, id: TabId) {
        self.send(id, ViewCommand::StopLoading);
    }

    /// Opens another tab at the same address as `id`.
    pub(crate) fn duplicate_tab(&mut self, id: TabId) -> Option<TabId> {
        let url = self.tab(id)?.url.clone();
        self.open_tab(&url).ok()
    }

    pub(crate) fn go_back(&mut self, id: TabId) {
        self.send(id, ViewCommand::Traverse(-1));
    }

    pub(crate) fn go_forward(&mut self, id: TabId) {
        self.send(id, ViewCommand::Traverse(1));
    }

    /// Closes the tab and its page. Closing the active tab activates the one after it, or the
    /// one before when it was last.
    pub(crate) fn close_tab(&mut self, id: TabId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return;
        };
        if self.active == Some(id) {
            self.reset_input();
        }
        let is_internal = self.tabs[index].is_internal();
        self.tabs.remove(index);
        if !is_internal && let Some(engine) = &self.engine {
            engine.send(Command::Close(id));
        }
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(Tab::id);
        }
    }

    /// Closes every tab except `id` and leaves it active.
    pub(crate) fn close_other_tabs(&mut self, id: TabId) {
        if self.tab(id).is_none() {
            return;
        }
        let closed: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| tab.id() != id && !tab.is_internal())
            .map(Tab::id)
            .collect();
        if self.active != Some(id) {
            self.reset_input();
        }
        self.tabs.retain(|tab| tab.id() == id);
        if let Some(engine) = &self.engine {
            for tab in closed {
                engine.send(Command::Close(tab));
            }
        }
        self.active = Some(id);
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.window.scale.set(scale);
    }

    /// Records whether the interface is dark, so pages follow it. The view calls it as it
    /// builds, because only the view knows the system's appearance; a change goes to the engine
    /// at once, since no poll may follow this build.
    pub(crate) fn note_dark(&self, dark: bool) {
        self.window.dark.set(Some(dark));
        self.send_color_scheme();
    }

    pub(crate) fn set_viewport(&mut self, size: [f32; 2]) {
        self.window.viewport = size;
    }

    pub(crate) fn viewport_width(&self) -> f32 {
        self.window.viewport[0]
    }

    pub(crate) fn set_window_focus(&mut self, focused: bool) {
        if !focused {
            self.reset_input();
        }
        self.window.focused = focused;
    }

    pub(crate) fn set_viewport_focus(&mut self, focused: bool) -> bool {
        let changed = self.window.viewport_focused != focused;
        if self.window.viewport_focused && !focused {
            self.reset_input();
        }
        self.window.viewport_focused = focused;
        changed
    }
    pub(crate) fn page_focused(&self) -> bool {
        self.window.focused && self.window.viewport_focused
    }
    pub(crate) fn scale(&self) -> f32 {
        self.window.scale.get()
    }

    /// Forwards what changed to the engine and takes in what it reported. Rune calls it at the
    /// start of every frame; it never blocks.
    pub(crate) fn poll(&mut self) -> Polled {
        if perf::enabled() {
            let now = Instant::now();
            if let Some(last) = self.last_poll.replace(now) {
                perf::record(Stage::Poll, now - last);
            }
        }
        let mut polled = Polled::default();
        self.sync_engine();
        // Collected first: taking them in needs the browser, which the engine is part of.
        // Empty, as most polls are, it allocates nothing.
        let updates: Vec<Update> = self.engine.iter().flat_map(Engine::updates).collect();
        for update in updates {
            match update {
                // A tab closed since the engine reported this has nothing to show it.
                Update::Tab(id, update) => {
                    if self.tab(id).is_some_and(Tab::is_internal) {
                        continue;
                    }
                    let internal_navigation = match &update {
                        crate::engine::TabUpdate::Navigation(url)
                        | crate::engine::TabUpdate::NavigationCommitted(url) => {
                            InternalRoute::is_internal_address(url)
                        }
                        _ => false,
                    };
                    if internal_navigation {
                        if let Some(tab) = self.tab_mut(id) {
                            tab.error = Some(
                                "Internal pages can only be opened from Elaris navigation".into(),
                            );
                            polled.changed = true;
                        }
                        continue;
                    }
                    if let Some(tab) = self.tab_mut(id) {
                        let committed = match &update {
                            crate::engine::TabUpdate::NavigationCommitted(url) => Some(url.clone()),
                            _ => None,
                        };
                        let title_changed = matches!(&update, crate::engine::TabUpdate::Title(_));
                        tab.update(update);
                        if let Some(url) = committed {
                            polled.committed.push((id, url, tab.title.clone()));
                        }
                        if title_changed
                            && !tab.committed_url.is_empty()
                            && tab.url == tab.committed_url
                        {
                            polled
                                .titles
                                .push((tab.committed_url.clone(), tab.title.clone()));
                        }
                        polled.changed = true;
                    }
                }
                Update::NewTab(source, url, request, activate) => {
                    if self.tab(source).is_some_and(|tab| !tab.is_internal())
                        && !InternalRoute::is_internal_address(&url)
                    {
                        // Ladybird already parsed and authorized this navigation. Keep its referrer
                        // and origin by transferring the descriptor instead of re-loading the URL.
                        let tab = self.insert_tab(url);
                        self.tab_mut(tab).unwrap().engine.pending_load = None;
                        self.send(tab, ViewCommand::LoadPrepared { source, request });
                        if activate {
                            self.activate_tab(tab);
                        }
                        polled.changed = true;
                    }
                }
                Update::ContextMenu(id, menu) if self.active == Some(id) => {
                    polled.menu = Some((id, menu));
                    polled.changed = true;
                }
                Update::Clipboard(id, text) if self.active == Some(id) => {
                    polled.clipboard = Some(text)
                }
                Update::ContextMenu(..) | Update::Clipboard(..) => {}
                Update::Failed(error) => {
                    self.fail(error);
                    polled.changed = true;
                }
                Update::Download(id, update) => {
                    self.apply_download_update(id, update);
                    polled.changed = true;
                }
                Update::DownloadStartFailed(id, error) => {
                    if let Some(download) = self.downloads.iter_mut().find(|item| item.id == id) {
                        download.status = DownloadStatus::Failed;
                        download.error = Some(error);
                        polled.changed = true;
                    }
                }
                Update::DownloadActionFailed(id, error) => {
                    if let Some(download) = self.downloads.iter_mut().find(|item| item.id == id) {
                        download.error = Some(error);
                        polled.changed = true;
                    }
                }
                Update::DownloadDirectoryFailed(error) => {
                    polled.download_directory_error = Some(error);
                    polled.changed = true;
                }
            }
        }
        if let Some(id) = self.active
            && let Some(tab) = self.tab_mut(id)
        {
            let frames = tab.surface.frames();
            polled.animating = std::mem::replace(&mut tab.engine.frames, frames) != frames;
        }
        polled
    }

    /// Asks the engine to close every page and stop, without waiting for it.
    pub(crate) fn shut_down(&mut self) {
        if let Some(engine) = &self.engine {
            engine.shut_down();
        }
    }

    fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }

    /// Sends `command` to the tab's view, if the tab is open and there is an engine.
    pub(crate) fn send(&self, id: TabId, command: ViewCommand) {
        if let Some(engine) = &self.engine
            && self.tab(id).is_some()
        {
            engine.send(Command::View(id, command));
        }
    }

    fn send_color_scheme(&self) {
        if let Some(engine) = &self.engine
            && let Some(dark) = self.window.dark.get()
            && self.window.applied_dark.replace(Some(dark)) != Some(dark)
        {
            engine.send(Command::ColorScheme(dark));
        }
    }

    /// Tells the engine what changed about each tab: whether it is shown and focused, the
    /// viewport it lays out in, and its first page.
    fn sync_engine(&mut self) {
        // Before any first load, so the first page lays out in the right scheme.
        self.send_color_scheme();
        let Some(engine) = &self.engine else {
            return;
        };
        let window = &self.window;
        let scale = window.scale.get();
        let [width, height] = window.viewport.map(|points| input::device(points, scale));
        let viewport = (width > 0 && height > 0).then_some((width, height, f64::from(scale)));
        let scheme_known = window.applied_dark.get().is_some();
        for tab in &mut self.tabs {
            if tab.is_internal() {
                continue;
            }
            let id = tab.id();
            let applied = &mut tab.engine;
            let active = self.active == Some(id);
            let send = |command| engine.send(Command::View(id, command));
            if applied.hidden == active {
                applied.hidden = !active;
                send(ViewCommand::Visible(active));
            }
            // Hidden tabs keep their last layout, unless they have yet to load their first page.
            if (active || (applied.pending_load.is_some() && !applied.deferred_load))
                && let Some(wanted) = viewport
                && applied.viewport != Some(wanted)
            {
                let (width, height, scale) = wanted;
                send(ViewCommand::Resize {
                    width,
                    height,
                    scale,
                });
                applied.viewport = Some(wanted);
            }
            // The first load waits for a viewport and the colour scheme, so the page lays out
            // at its real size and never starts in the wrong scheme.
            if applied.viewport.is_some()
                && scheme_known
                && (active || !applied.deferred_load)
                && let Some(url) = applied.pending_load.take()
            {
                send(ViewCommand::Load(url));
            }
            let focused = active && window.focused && window.viewport_focused;
            if applied.focused != focused {
                applied.focused = focused;
                send(ViewCommand::Focus(focused));
            }
        }
    }
}

fn section_rank(section: TabSection) -> usize {
    match section {
        TabSection::Favourite => 0,
        TabSection::Pinned => 1,
        TabSection::Regular => 2,
    }
}

#[cfg(test)]
mod tests {
    use elaris::downloads::{Request as DownloadRequest, Status as DownloadStatus};
    use elaris::ladybird::{MouseEvent, MouseKind};
    use rune::{Key as RuneKey, KeyEvent, Mods, PointerButton};

    use super::*;
    use crate::engine::{FakeEngine, Surface, TabUpdate};

    fn connected() -> (Browser, FakeEngine) {
        let mut browser = Browser::default();
        let (engine, fake) = Engine::fake();
        browser.connect(engine);
        (browser, fake)
    }

    #[test]
    fn downloads_are_queued_with_a_stable_id_and_can_be_cancelled() {
        let (mut browser, fake) = connected();
        let id = browser
            .start_download(DownloadRequest::new("https://example.com/archive.zip"))
            .expect("an HTTP download can be queued");

        let download = browser
            .downloads()
            .iter()
            .find(|download| download.id == id)
            .expect("queued download is visible immediately");
        assert_eq!(download.status, DownloadStatus::Pending);
        assert_eq!(download.source_url, "https://example.com/archive.zip");
        assert_eq!(download.filename, "archive.zip");
        assert_eq!(
            fake.commands.recv().expect("start command is queued"),
            Command::StartDownload(id, DownloadRequest::new("https://example.com/archive.zip"))
        );

        browser
            .cancel_download(id)
            .expect("pending download can be cancelled");
        assert_eq!(
            fake.commands.recv().expect("cancel command is queued"),
            Command::DownloadAction(id, DownloadAction::Cancel)
        );
    }

    #[test]
    fn downloads_reject_non_http_addresses_and_unavailable_engines() {
        let mut disconnected = Browser::default();
        assert_eq!(
            disconnected.start_download(DownloadRequest::new("https://example.com")),
            Err(Error::Download(downloads::Error::EngineUnavailable))
        );

        let (mut browser, _) = connected();
        assert_eq!(
            browser.start_download(DownloadRequest::new("file:///etc/passwd")),
            Err(Error::Download(downloads::Error::InvalidAddress))
        );
    }

    fn sent(fake: &FakeEngine) -> Vec<Command> {
        fake.commands.try_iter().collect()
    }

    /// What the view does as it builds and lays out.
    fn lay_out(browser: &mut Browser, size: [f32; 2], scale: f32, dark: bool) {
        browser.note_scale(scale);
        browser.note_dark(dark);
        browser.set_viewport(size);
    }

    /// A browser laid out at 100x50 points, scale 1, with its commands so far read.
    fn ready() -> (Browser, FakeEngine) {
        let (mut browser, fake) = connected();
        lay_out(&mut browser, [100.0, 50.0], 1.0, false);
        browser.poll();
        sent(&fake);
        (browser, fake)
    }

    fn view(id: TabId, command: ViewCommand) -> Command {
        Command::View(id, command)
    }

    fn resize(width: i32, height: i32, scale: f64) -> ViewCommand {
        ViewCommand::Resize {
            width,
            height,
            scale,
        }
    }

    fn surface(browser: &Browser, id: TabId) -> Surface {
        browser.tab(id).unwrap().surface.clone()
    }

    #[test]
    fn a_tab_opened_before_the_engine_opens_once_it_connects() {
        let mut browser = Browser::default();
        let tab = browser.open_tab("example.com").unwrap();
        let (engine, fake) = Engine::fake();
        browser.connect(engine);
        assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);
        assert_eq!(browser.active_tab().map(Tab::id), Some(tab));
    }

    #[test]
    fn internal_tabs_never_open_ladybird_views_and_close_like_other_tabs() {
        let (mut browser, fake) = connected();
        let settings = browser.open_tab("elaris://settings").unwrap();
        let tab = browser
            .tab(settings)
            .expect("native settings tab is present");
        assert!(tab.is_internal());
        assert_eq!(tab.title, "Settings");
        assert_eq!(tab.engine.pending_load, None);
        browser.poll();
        assert!(
            sent(&fake).is_empty(),
            "native pages never create engine views"
        );

        let website = browser.open_tab("example.com").unwrap();
        assert!(matches!(sent(&fake).as_slice(), [Command::Open(id, _)] if *id == website));
        browser.close_tab(settings);
        assert!(browser.tab(settings).is_none());
        assert_eq!(browser.active_tab().map(Tab::id), Some(website));
        assert!(
            sent(&fake).is_empty(),
            "closing a native page sends no engine command"
        );
    }

    #[test]
    fn download_directory_updates_are_engine_commands_and_report_without_failing_the_browser() {
        let (mut browser, fake) = connected();
        let path = std::path::PathBuf::from("/tmp/elaris-downloads");
        browser.set_download_directory(Some(path.clone()));
        assert_eq!(sent(&fake), [Command::SetDownloadDirectory(Some(path))]);

        fake.updates
            .send(Update::DownloadDirectoryFailed(
                "directory is read-only".into(),
            ))
            .unwrap();
        let polled = browser.poll();
        assert!(polled.changed);
        assert_eq!(
            polled.download_directory_error.as_deref(),
            Some("directory is read-only")
        );
        assert!(browser.engine_error.is_none());

        browser.set_download_directory(None);
        assert_eq!(sent(&fake), [Command::SetDownloadDirectory(None)]);
    }

    #[test]
    fn restored_internal_tabs_stay_native_and_do_not_record_history() {
        let (mut browser, fake) = connected();
        let settings = browser
            .restore_tab("elaris://settings", "Old title", true)
            .unwrap();
        let website = browser
            .restore_tab("https://example.com/", "Example", true)
            .unwrap();
        browser.activate_tab(settings);
        assert_eq!(browser.tab(settings).unwrap().title, "Settings");
        assert_eq!(
            browser.tab(settings).unwrap().internal,
            Some(InternalRoute::Page(InternalPage::Settings))
        );

        fake.updates
            .send(Update::Tab(
                settings,
                crate::engine::TabUpdate::NavigationCommitted("elaris://settings".into()),
            ))
            .unwrap();
        assert!(browser.poll().committed.is_empty());
        assert_eq!(
            sent(&fake),
            [
                Command::Open(website, surface(&browser, website)),
                view(website, ViewCommand::Visible(false)),
            ]
        );
    }

    #[test]
    fn website_navigation_cannot_open_privileged_internal_pages() {
        let (mut browser, fake) = ready();
        let website = browser.open_tab("example.com").unwrap();
        browser.poll();
        sent(&fake);

        assert!(browser.navigate(website, "elaris://settings").is_err());
        fake.updates
            .send(Update::Tab(
                website,
                TabUpdate::NavigationCommitted("elaris://settings".into()),
            ))
            .unwrap();
        let result = browser.poll();
        assert!(result.committed.is_empty());
        assert_eq!(browser.tab(website).unwrap().url, "https://example.com");
        assert!(!browser.tab(website).unwrap().is_internal());
    }

    #[test]
    fn navigation_between_websites_and_native_pages_opens_and_closes_views() {
        let (mut browser, fake) = ready();
        let tab = browser.open_tab("example.com").unwrap();
        browser.poll();
        sent(&fake);

        browser
            .navigate_internal(tab, InternalRoute::Page(InternalPage::Settings))
            .unwrap();
        assert!(browser.tab(tab).unwrap().is_internal());
        assert_eq!(sent(&fake), [Command::Close(tab)]);

        browser.navigate(tab, "example.org").unwrap();
        assert!(!browser.tab(tab).unwrap().is_internal());
        assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);
        browser.poll();
        assert!(sent(&fake).contains(&view(tab, ViewCommand::Load("https://example.org".into()))));
    }

    #[test]
    fn the_first_page_waits_for_its_viewport_and_colour_scheme() {
        let (mut browser, fake) = connected();
        let tab = browser.open_tab("about:blank").unwrap();
        browser.poll();
        assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);

        lay_out(&mut browser, [100.0, 50.0], 2.0, true);
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                // Building sends the scheme at once, ahead of the poll's commands.
                Command::ColorScheme(true),
                view(tab, resize(200, 100, 2.0)),
                view(tab, ViewCommand::Load("about:blank".into())),
            ]
        );
        browser.poll();
        assert_eq!(sent(&fake), []);
    }

    #[test]
    fn only_the_active_tab_is_shown_resized_and_focused() {
        let (mut browser, fake) = ready();
        browser.set_viewport_focus(true);
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                Command::Open(first, surface(&browser, first)),
                Command::Open(second, surface(&browser, second)),
                view(first, resize(100, 50, 1.0)),
                view(first, ViewCommand::Load("https://example.com".into())),
                view(first, ViewCommand::Focus(true)),
                // A background tab lays out once, to load its first page, then hides.
                view(second, ViewCommand::Visible(false)),
                view(second, resize(100, 50, 1.0)),
                view(second, ViewCommand::Load("https://example.org".into())),
            ]
        );

        // A window resize reaches only the tab on show.
        browser.set_viewport([120.0, 50.0]);
        browser.poll();
        assert_eq!(sent(&fake), [view(first, resize(120, 50, 1.0))]);

        browser.activate_tab(second);
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                view(first, ViewCommand::Visible(false)),
                view(first, ViewCommand::Focus(false)),
                view(second, ViewCommand::Visible(true)),
                view(second, resize(120, 50, 1.0)),
                view(second, ViewCommand::Focus(true)),
            ]
        );
    }

    #[test]
    fn restored_background_tabs_load_only_when_selected() {
        let (mut browser, fake) = connected();
        let first = browser
            .restore_tab("https://one.example/", "One", true)
            .unwrap();
        let second = browser
            .restore_tab("https://two.example/", "Two", false)
            .unwrap();
        browser.activate_tab(second);
        lay_out(&mut browser, [100.0, 50.0], 1.0, false);

        browser.poll();
        let commands = sent(&fake);
        assert!(commands.contains(&Command::Open(first, surface(&browser, first))));
        assert!(commands.contains(&Command::Open(second, surface(&browser, second))));
        assert!(!commands.iter().any(|command| {
            matches!(command, Command::View(id, ViewCommand::Load(_)) if *id == first)
        }));
        assert!(commands.contains(&view(
            second,
            ViewCommand::Load("https://two.example/".into())
        )));

        browser.activate_tab(first);
        browser.poll();
        assert!(sent(&fake).contains(&view(
            first,
            ViewCommand::Load("https://one.example/".into())
        )));
    }

    #[test]
    fn input_goes_only_to_the_active_tab_in_device_pixels() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        browser.open_tab("example.org").unwrap();
        browser.poll();
        sent(&fake);

        browser.note_scale(2.0);
        browser.drag_pointer(
            MouseKind::Down,
            [1.0, 2.0],
            PointerButton::Primary,
            Mods::default(),
            1,
        );
        let enter = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(browser.key(&enter, true, false, None));
        assert!(browser.key(&enter, false, false, None));
        let shortcut = KeyEvent {
            key: RuneKey::Command('q'),
            mods: Mods::default(),
        };
        assert!(!browser.key(&shortcut, true, false, None));
        let sent = sent(&fake);
        assert_eq!(sent.len(), 3);
        assert!(matches!(
            sent[0],
            Command::View(id, ViewCommand::Mouse(MouseEvent { x: 2, y: 4, .. })) if id == first
        ));
        assert!(
            sent[1..]
                .iter()
                .all(|c| matches!(c, Command::View(id, ViewCommand::Key(_)) if *id == first))
        );
    }

    #[test]
    fn navigation_and_history_go_to_the_tab_named() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        browser.poll();
        sent(&fake);

        browser.navigate(second, "ladybird.org").unwrap();
        browser.reload(first);
        browser.go_back(second);
        browser.go_forward(first);
        assert_eq!(
            sent(&fake),
            [
                view(second, ViewCommand::Load("https://ladybird.org".into())),
                view(first, ViewCommand::Reload),
                view(second, ViewCommand::Traverse(-1)),
                view(first, ViewCommand::Traverse(1)),
            ]
        );
        assert_eq!(browser.tab(second).unwrap().url, "https://ladybird.org");
        assert_eq!(browser.tab(first).unwrap().url, "https://example.com");
    }

    #[test]
    fn searches_ordinary_text_and_rejects_unsupported_schemes() {
        let (mut browser, fake) = ready();
        let tab = browser.open_tab("example.com").unwrap();
        assert!(matches!(
            browser.open_tab("ftp://example.com"),
            Err(Error::Address(_))
        ));
        browser.navigate(tab, "two words").unwrap();
        assert_eq!(
            browser.tab(tab).unwrap().url,
            "https://duckduckgo.com/?q=two%20words"
        );
        assert!(matches!(
            browser.navigate(tab, "ftp://example.com"),
            Err(Error::Address(_))
        ));
        browser.close_tab(tab);
        assert_eq!(
            browser.navigate(tab, "example.org"),
            Err(Error::NoSuchTab(tab))
        );
        assert_eq!(browser.tabs().len(), 0);
        // Only the one tab opened and closed; nothing else reached the engine.
        assert_eq!(
            sent(&fake).last(),
            Some(&Command::Close(tab)),
            "a closed tab takes no commands"
        );
    }

    #[test]
    fn closing_the_active_tab_activates_its_neighbour() {
        let (mut browser, _fake) = ready();
        let [a, b, c] = ["a.com", "b.com", "c.com"].map(|a| browser.open_tab(a).unwrap());
        browser.activate_tab(b);

        // An inactive tab closes without changing which one is active.
        browser.close_tab(a);
        assert_eq!(browser.active_tab().map(Tab::id), Some(b));

        // The active one hands over to the tab after it, or before it when it was last.
        browser.close_tab(b);
        assert_eq!(browser.active_tab().map(Tab::id), Some(c));
        let d = browser.open_tab("d.com").unwrap();
        browser.activate_tab(d);
        browser.close_tab(d);
        assert_eq!(browser.active_tab().map(Tab::id), Some(c));
        browser.close_tab(c);
        assert!(browser.active_tab().is_none());
    }

    #[test]
    fn finds_the_tab_beside_another_wrapping_around() {
        let (mut browser, _fake) = ready();
        let [a, b, c] = ["a.com", "b.com", "c.com"].map(|a| browser.open_tab(a).unwrap());
        assert_eq!(browser.tab_beside(a, 1), Some(b));
        assert_eq!(browser.tab_beside(c, 1), Some(a));
        assert_eq!(browser.tab_beside(a, -1), Some(c));
        browser.close_tab(b);
        assert_eq!(browser.tab_beside(b, 1), None);
    }

    #[test]
    fn moves_tabs_in_both_directions_without_replacing_their_pages() {
        let (mut browser, fake) = connected();
        let [a, b, c, d] = ["a.example", "b.example", "c.example", "d.example"]
            .map(|address| browser.open_tab(address).unwrap());
        browser.activate_tab(c);
        let surfaces = [a, b, c, d].map(|id| (id, surface(&browser, id)));
        sent(&fake);

        browser.move_tab(d, 0).unwrap();
        assert_eq!(
            browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
            [d, a, b, c]
        );
        browser.move_tab(d, 3).unwrap();
        assert_eq!(
            browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
            [a, b, c, d]
        );
        browser.move_tab(a, 3).unwrap();
        assert_eq!(
            browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
            [b, c, d, a]
        );
        browser.move_tab(a, 3).unwrap();

        assert_eq!(browser.active_tab().map(Tab::id), Some(c));
        for (id, old_surface) in surfaces {
            assert_eq!(surface(&browser, id), old_surface);
        }
        assert!(
            sent(&fake).is_empty(),
            "reordering sends no engine commands"
        );
        assert_eq!(browser.tab_beside(c, 1), Some(d));
        assert_eq!(browser.tab_beside(c, -1), Some(b));
    }

    #[test]
    fn rejects_missing_tabs_and_out_of_range_positions() {
        let mut browser = Browser::default();
        let first = browser.open_tab("a.example").unwrap();
        let second = browser.open_tab("b.example").unwrap();
        let missing = TabId::from_raw(99);

        assert_eq!(browser.move_tab(missing, 0), Err(Error::NoSuchTab(missing)));
        assert_eq!(browser.move_tab(first, 2), Err(Error::InvalidTabIndex(2)));
        assert_eq!(
            browser.move_tab(first, usize::MAX),
            Err(Error::InvalidTabIndex(usize::MAX))
        );
        assert_eq!(
            browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
            [first, second]
        );
    }

    #[test]
    fn tab_ids_are_never_reused() {
        let (mut browser, _fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        browser.close_tab(first);
        let second = browser.open_tab("example.com").unwrap();
        assert_ne!(first, second);
        assert!(browser.tab(first).is_none());
        browser.activate_tab(first);
        assert_eq!(browser.active_tab().map(Tab::id), Some(second));
    }

    #[test]
    fn updates_reach_only_their_own_tab_and_never_a_closed_one() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        let send = |id, update| fake.updates.send(Update::Tab(id, update)).unwrap();
        send(first, TabUpdate::Title("First".into()));
        send(second, TabUpdate::Title("Second".into()));
        send(second, TabUpdate::Loading(true));
        assert!(browser.poll().changed);
        assert_eq!(browser.tab(first).unwrap().title, "First");
        assert!(!browser.tab(first).unwrap().loading);
        assert_eq!(browser.tab(second).unwrap().title, "Second");
        assert!(browser.tab(second).unwrap().loading);

        browser.close_tab(second);
        send(second, TabUpdate::Title("Late".into()));
        assert!(!browser.poll().changed);
        assert_eq!(browser.tab(first).unwrap().title, "First");
    }

    #[test]
    fn only_committed_navigation_updates_are_reported_for_history() {
        let (mut browser, fake) = connected();
        let tab = browser.open_tab("example.com").unwrap();
        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::Navigation("https://provisional.example/".into()),
            ))
            .unwrap();
        assert!(browser.poll().committed.is_empty());

        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::NavigationCommitted("https://committed.example/".into()),
            ))
            .unwrap();
        let polled = browser.poll();
        assert_eq!(
            polled.committed,
            [(tab, "https://committed.example/".into(), String::new())]
        );
        assert_eq!(
            browser.tab(tab).unwrap().committed_url,
            "https://committed.example/"
        );
    }

    #[test]
    fn provisional_titles_do_not_update_the_previous_history_entry() {
        let (mut browser, fake) = connected();
        let tab = browser.open_tab("example.com").unwrap();
        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::NavigationCommitted("https://example.com/".into()),
            ))
            .unwrap();
        browser.poll();

        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::Navigation("https://iana.org/domains/reserved".into()),
            ))
            .unwrap();
        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::Title("IANA-managed Reserved Domains".into()),
            ))
            .unwrap();
        assert!(browser.poll().titles.is_empty());

        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::NavigationCommitted("https://iana.org/domains/reserved".into()),
            ))
            .unwrap();
        browser.poll();
        fake.updates
            .send(Update::Tab(
                tab,
                TabUpdate::Title("IANA-managed Reserved Domains".into()),
            ))
            .unwrap();
        assert_eq!(
            browser.poll().titles,
            [(
                "https://iana.org/domains/reserved".into(),
                "IANA-managed Reserved Domains".into()
            )]
        );
    }

    #[test]
    fn only_the_active_tabs_frames_mean_animation() {
        let (mut browser, _fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        assert!(!browser.poll().animating);
        surface(&browser, second).fake_frame();
        assert!(!browser.poll().animating);
        surface(&browser, first).fake_frame();
        assert!(browser.poll().animating);
        assert!(!browser.poll().animating);
    }

    #[test]
    fn an_engine_failure_shows_in_every_tab_and_new_ones() {
        let (mut browser, fake) = ready();
        let tab = browser.open_tab("example.com").unwrap();
        fake.updates.send(Update::Failed("gone".into())).unwrap();
        assert!(browser.poll().changed);
        assert_eq!(browser.tab(tab).unwrap().error.as_deref(), Some("gone"));
        let later = browser.open_tab("example.org").unwrap();
        assert_eq!(browser.tab(later).unwrap().error.as_deref(), Some("gone"));
    }

    #[test]
    fn without_an_engine_tabs_keep_their_page_and_take_no_keys() {
        let mut browser = Browser::default();
        let tab = browser.open_tab("example.com").unwrap();
        browser.navigate(tab, "example.org").unwrap();
        assert_eq!(
            browser.tab(tab).unwrap().engine.pending_load.as_deref(),
            Some("https://example.org")
        );
        let key = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(!browser.key(&key, true, false, None));
        assert!(!browser.poll().changed);
    }

    #[test]
    fn shutting_down_asks_the_engine_to_stop() {
        let (mut browser, fake) = connected();
        browser.shut_down();
        assert_eq!(sent(&fake), [Command::Shutdown]);
    }
}
