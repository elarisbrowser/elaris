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
//! tab reaches another one. Visible tabs each have a viewport. The active tab receives keyboard
//! input, and hidden tabs keep their pages but stop painting.

use std::cell::Cell;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod accessibility;
mod downloads;
mod engine_sync;
mod input_routing;
pub(crate) mod peek;
pub(crate) mod split;
pub(crate) mod swipe;

use crate::address::url_for_address;
use crate::engine::{Command, Engine, Surface, ViewCommand};
use crate::internal::{InternalPage, InternalRoute};
use crate::tab::{Tab, TabId, TabSection};
use elaris::downloads as download_types;
use elaris::downloads::Download;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
    /// The address cannot be loaded, for the reason given.
    Address(&'static str),
    /// No open tab has this id.
    NoSuchTab(TabId),
    /// Find in Page is not available on Elaris-owned internal pages.
    InternalPage(TabId),
    /// The requested position is outside the current tab order.
    #[cfg(test)]
    InvalidTabIndex(usize),
    /// The requested position is outside the destination section.
    InvalidSectionIndex(usize),
    /// The requested download operation could not be queued.
    Download(download_types::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(reason) => f.write_str(reason),
            Self::NoSuchTab(_) => f.write_str("The tab has closed"),
            Self::InternalPage(_) => f.write_str("Find in Page is unavailable on internal pages"),
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
    /// This window's download list changed, so the kept history may need saving.
    pub(crate) downloads_changed: bool,
    pub(crate) permission_choices: Vec<(
        String,
        crate::permissions::PermissionKind,
        crate::permissions::PermissionState,
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
}

pub(crate) struct Browser {
    /// In the order the sidebar shows them.
    tabs: Vec<Tab>,
    active: Option<TabId>,
    /// Opens every page in the shared private session.
    pub(crate) private: bool,
    /// An assistive client reads this window, so pages report their accessibility trees.
    accessibility: bool,
    /// The page trees last handed to Rune, to forget those of closed tabs.
    accessibility_trees: std::collections::HashSet<rune::accesskit::TreeId>,
    pub(crate) peek: Option<peek::PeekState>,
    peek_origin: Option<(TabId, [f32; 2])>,
    promoted_visit: Option<(TabId, String, String)>,
    pub(crate) split: Option<split::Split>,
    pub(crate) split_view_limit: usize,
    default_zoom: u16,
    /// The tab whose zoom changed since the window last polled.
    zoom_changed: Option<TabId>,
    recent: Vec<TabId>,
    input: input_routing::Held,
    /// None until the app connects one, and in a build without an engine.
    engine: Option<Engine>,
    /// Why there is no engine, which every tab shows.
    engine_error: Option<String>,
    /// Downloads belong to the browser process, not to the tab that requested them.
    downloads: Arc<Mutex<Vec<Download>>>,
    last_download_snapshot: Vec<Download>,
    window: Window,
    /// Development timing: the last poll.
    last_poll: Option<Instant>,
}

/// Display state and the available area for all visible panes.
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
            private: false,
            accessibility: false,
            accessibility_trees: Default::default(),
            peek: None,
            peek_origin: None,
            promoted_visit: None,
            split: None,
            split_view_limit: 2,
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
            },
            last_poll: None,
        }
    }
}

impl Browser {
    pub(crate) fn send_engine(&self, command: Command) {
        if let Some(engine) = &self.engine {
            engine.send(command);
        }
    }

    /// Gives the browser the engine that shows its tabs, opening a view for each.
    #[cfg(any(feature = "ladybird-bridge", test))]
    pub(crate) fn connect(&mut self, engine: Engine) {
        for tab in &self.tabs {
            if !tab.is_internal() {
                engine.send(Command::Open(tab.id(), tab.surface.clone(), self.private));
                let zoom = tab.zoom_override.unwrap_or(self.default_zoom);
                if zoom != 100 {
                    engine.send(Command::View(tab.id(), ViewCommand::SetZoom(zoom)));
                }
            }
        }
        for tab in &mut self.tabs {
            if tab.is_internal() || tab.find.query.is_empty() {
                continue;
            }
            tab.find.invalidate();
            let Some(request_id) = tab.find.begin_query(&tab.find.query.clone()) else {
                continue;
            };
            engine.send(Command::View(
                tab.id(),
                ViewCommand::Find {
                    query: tab.find.query.clone(),
                    request_id,
                },
            ));
        }
        self.engine = Some(engine);
    }

    /// Records that the engine cannot run, which every tab shows.
    pub(crate) fn fail(&mut self, error: String) {
        for tab in &mut self.tabs {
            tab.error = Some(error.clone());
            tab.awaiting_close = false;
            tab.page_dialog = None;
            tab.permission_prompt = None;
            tab.external_url = None;
            tab.permissions_in_use.fill(false);
            tab.web_fullscreen = false;
        }
        if let Some(peek) = &mut self.peek {
            peek.page.error = Some(error.clone());
            peek.page.permission_prompt = None;
        }
        self.engine_error = Some(error);
    }

    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(crate) fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub(crate) fn restore_favicon(&mut self, id: TabId, width: u32, height: u32, rgba: Vec<u8>) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.update(crate::engine::TabUpdate::Favicon(Some(
            elaris::ladybird::Favicon {
                width,
                height,
                rgba,
            },
        )));
    }

    pub(crate) fn fullscreen_tab(&self) -> Option<&Tab> {
        self.active_tab().filter(|tab| tab.web_fullscreen)
    }

    pub(crate) fn exit_web_fullscreen(&self) -> bool {
        if let Some(tab) = self.fullscreen_tab() {
            self.send(tab.id(), ViewCommand::ExitWebFullscreen);
            return true;
        }
        false
    }

    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
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
    /// active state. The single tab vector stays grouped in keyboard order: favourites,
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
        self.split = None;
        self.recent.clear();
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

    pub(super) fn insert_tab(&mut self, url: String) -> TabId {
        self.insert_tab_with_state(url, String::new(), false, TabSection::Regular, None)
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
        Ok(self.insert_tab_with_state(url, title.to_owned(), deferred_load, section, None))
    }

    fn insert_internal_tab(&mut self, route: InternalRoute) -> TabId {
        self.insert_internal_tab_in_section(route, TabSection::Regular)
    }

    fn insert_internal_tab_in_section(
        &mut self,
        route: InternalRoute,
        section: TabSection,
    ) -> TabId {
        let id = self.next_page_id();
        let mut tab = Tab::new_internal(id, route);
        tab.section = section;
        let index = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) > section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(index, tab);
        if self.active.is_none() {
            self.set_active(id);
        }
        id
    }

    fn next_page_id(&mut self) -> TabId {
        TabId::next()
    }

    pub(super) fn insert_popup(&mut self, source: TabId, request: u64, activate: bool) {
        let tab = self.insert_tab_with_state(
            "about:blank".into(),
            String::new(),
            false,
            TabSection::Regular,
            Some((source, request)),
        );
        if activate {
            self.activate_tab(tab);
        }
    }

    fn insert_tab_with_state(
        &mut self,
        url: String,
        title: String,
        deferred_load: bool,
        section: TabSection,
        popup: Option<(TabId, u64)>,
    ) -> TabId {
        let id = self.next_page_id();
        let mut tab = Tab::new(id, url);
        if popup.is_some() {
            tab.engine.pending_load = None;
        }
        tab.section = section;
        tab.title = title;
        tab.engine.deferred_load = deferred_load;
        tab.error.clone_from(&self.engine_error);
        if !tab.is_internal()
            && let Some(engine) = &self.engine
        {
            let surface = tab.surface.clone();
            if let Some((source, request)) = popup {
                engine.send(Command::AdoptPopup {
                    source,
                    request,
                    tab: id,
                    surface,
                });
            } else {
                engine.send(Command::Open(id, surface, self.private));
            }
            let zoom = self.default_zoom;
            if zoom != 100 {
                engine.send(Command::View(id, ViewCommand::SetZoom(zoom)));
            }
            if self.accessibility {
                engine.send(Command::View(id, ViewCommand::SetAccessibility(true)));
            }
        }
        let index = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) > section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(index, tab);
        if self.active.is_none() {
            self.set_active(id);
        }
        id
    }

    fn set_active(&mut self, id: TabId) {
        self.active = Some(id);
        self.recent.retain(|&tab| tab != id);
        self.recent.insert(0, id);
    }

    pub(crate) fn recent_tabs(&self) -> Vec<TabId> {
        let seen: std::collections::HashSet<_> = self.recent.iter().copied().collect();
        self.recent
            .iter()
            .copied()
            .chain(
                self.tabs
                    .iter()
                    .map(Tab::id)
                    .filter(|id| !seen.contains(id)),
            )
            .collect()
    }

    /// Shows the tab in the viewport and gives it the page's input.
    pub(crate) fn activate_tab(&mut self, id: TabId) {
        if self.tab(id).is_none() {
            return;
        }
        if let Some(split) = &mut self.split
            && !split.panes(self.split_view_limit).contains(&Some(id))
            && let Some(destination) = split
                .panes(self.split_view_limit)
                .iter()
                .position(Option::is_none)
        {
            split.panes[destination] = Some(id);
        }
        if self.active != Some(id) {
            self.reset_input();
            self.set_active(id);
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
        self.invalidate_find(id);
        let connected = self.engine.is_some();
        let open_view = self.tab(id).ok_or(Error::NoSuchTab(id))?.is_internal();
        let mut load_now = false;
        let mut surface = None;
        {
            let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
            if open_view {
                tab.internal = None;
                tab.title.clear();
                tab.surface = Surface::default();
                tab.engine = Default::default();
                tab.committed_url.clear();
                surface = Some(tab.surface.clone());
            }
            tab.thumbnail = None;
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
            engine.send(Command::Open(id, surface, self.private));
            let zoom = self.default_zoom;
            if zoom != 100 {
                engine.send(Command::View(id, ViewCommand::SetZoom(zoom)));
            }
            if self.accessibility {
                engine.send(Command::View(id, ViewCommand::SetAccessibility(true)));
            }
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
            engine.discard_tab_updates(id);
        }
        let internal = Tab::new_internal(id, route);
        *self.tab_mut(id).ok_or(Error::NoSuchTab(id))? = internal;
        Ok(())
    }

    pub(crate) fn set_default_zoom(&mut self, zoom: u16) {
        if self.default_zoom == zoom {
            return;
        }
        self.default_zoom = zoom;
        for tab in self
            .tabs
            .iter()
            .chain(self.peek.iter().map(|peek| &peek.page))
        {
            if !tab.is_internal() && tab.zoom_override.is_none() {
                self.send(tab.id(), ViewCommand::SetZoom(zoom));
            }
        }
    }

    pub(crate) fn page_zoom(&self, tab: &Tab) -> u16 {
        tab.zoom_override.unwrap_or(self.default_zoom)
    }

    pub(crate) fn change_zoom(&mut self, id: TabId, delta: i16) {
        let default = self.default_zoom;
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.is_internal()) else {
            return;
        };
        let current = tab.zoom_override.unwrap_or(default);
        let zoom = if delta == 0 {
            default
        } else {
            current.saturating_add_signed(delta).clamp(30, 500)
        };
        tab.zoom_override = (zoom != default).then_some(zoom);
        if zoom != current {
            self.zoom_changed = Some(id);
            self.send(id, ViewCommand::SetZoom(zoom));
        }
    }

    /// The tab whose zoom changed since the last call, if any.
    pub(crate) fn take_zoom_changed(&mut self) -> Option<TabId> {
        self.zoom_changed.take()
    }

    pub(crate) fn reload_bypassing_cache(&mut self, id: TabId) {
        self.reload_page(id, ViewCommand::ReloadBypassingCache);
    }

    pub(crate) fn reload(&mut self, id: TabId) {
        self.reload_page(id, ViewCommand::Reload);
    }

    fn reload_page(&mut self, id: TabId, command: ViewCommand) {
        if self.tab(id).is_some_and(|tab| !tab.is_internal()) {
            self.invalidate_find(id);
            self.send(id, command);
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

    pub(crate) fn traverse_to_step(&mut self, id: TabId, step: i32) {
        self.invalidate_find(id);
        self.send(id, ViewCommand::TraverseToStep(step));
    }

    pub(crate) fn go_back(&mut self, id: TabId) {
        self.invalidate_find(id);
        self.send(id, ViewCommand::Traverse(-1));
    }

    pub(crate) fn go_forward(&mut self, id: TabId) {
        self.invalidate_find(id);
        self.send(id, ViewCommand::Traverse(1));
    }

    /// Searches one web tab with Ladybird's case-insensitive, all-match search.
    pub(crate) fn find(&mut self, id: TabId, query: &str) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }

        if query.is_empty() {
            if tab.find.query.is_empty() && !tab.find.pending() && tab.find.result.is_none() {
                return Ok(());
            }
            tab.find.clear_query();
            let command = ViewCommand::FindEnd;
            self.send(id, command);
            return Ok(());
        }

        let Some(request_id) = tab.find.begin_query(query) else {
            return Ok(());
        };
        let query = tab.find.query.clone();
        self.send(id, ViewCommand::Find { query, request_id });
        Ok(())
    }

    /// Advances the active tab's current find query and wraps as Ladybird does.
    pub(crate) fn find_next(&mut self, id: TabId) -> Result<(), Error> {
        self.find_step(id, false)
    }

    /// Moves to the previous match in the active tab's current find query.
    pub(crate) fn find_previous(&mut self, id: TabId) -> Result<(), Error> {
        self.find_step(id, true)
    }

    /// Requests selected webpage text asynchronously for Find's initial query.
    pub(crate) fn request_selected_text(&self, id: TabId, request_id: u64) -> Result<(), Error> {
        let tab = self.tab(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }
        self.send(id, ViewCommand::SelectedText { request_id });
        Ok(())
    }

    fn find_step(&mut self, id: TabId, previous: bool) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }
        let Some(request_id) = tab.find.begin_navigation() else {
            return Ok(());
        };
        self.send(
            id,
            if previous {
                ViewCommand::FindPrevious { request_id }
            } else {
                ViewCommand::FindNext { request_id }
            },
        );
        Ok(())
    }

    /// Clears Ladybird's highlights and invalidates pending results, retaining this tab's query.
    pub(crate) fn clear_find(&mut self, id: TabId) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Ok(());
        }
        tab.find.invalidate();
        self.send(id, ViewCommand::FindEnd);
        Ok(())
    }

    pub(super) fn invalidate_find(&mut self, id: TabId) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.is_internal()) else {
            return;
        };
        let clear_highlights = tab.find.pending() || tab.find.result.is_some();
        tab.find.invalidate();
        if clear_highlights {
            self.send(id, ViewCommand::FindEnd);
        }
    }

    /// Returns whether the tab needs an asynchronous close decision from the live view.
    pub(crate) fn preflight_close(&mut self, id: TabId) -> bool {
        if self.engine.is_none() || self.engine_error.is_some() {
            return false;
        }
        let Some(tab) = self
            .tab_mut(id)
            .filter(|tab| !tab.is_internal() && tab.engine.pending_load.is_none())
        else {
            return false;
        };
        if !std::mem::replace(&mut tab.awaiting_close, true)
            && let Some(engine) = &self.engine
        {
            engine.send(Command::PreflightClose(id));
        }
        true
    }

    /// Closes the tab and activates its neighbor if it was active.
    pub(crate) fn close_tab(&mut self, id: TabId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return;
        };
        let tab = self.remove_tab_at(index);
        id.release();
        if !tab.is_internal()
            && let Some(engine) = &self.engine
        {
            engine.send(Command::Close(id));
        }
    }

    /// Moves the existing tab and its pixel stream between browsers on the UI thread.
    /// Both browsers must be connected to the same runtime if the tab has a live view.
    pub(crate) fn transfer_tab(
        &mut self,
        destination: &mut Self,
        id: TabId,
        section: TabSection,
        index: usize,
    ) -> Result<(), Error> {
        let source_index = self
            .tabs
            .iter()
            .position(|tab| tab.id() == id)
            .ok_or(Error::NoSuchTab(id))?;
        destination.prepare_transfer(
            id,
            section,
            index,
            self,
            self.tabs[source_index].is_internal(),
        )?;
        let insertion = destination
            .tabs
            .partition_point(|tab| section_rank(tab.section()) < section_rank(section))
            + index;

        let mut tab = self.remove_tab_at(source_index);
        if tab.engine.focused {
            if let Some(engine) = &self.engine {
                engine.send(Command::View(id, ViewCommand::Focus(false)));
            }
            tab.engine.focused = false;
        }
        tab.section = section;
        destination.tabs.insert(insertion, tab);
        destination.activate_tab(id);
        Ok(())
    }

    pub(crate) fn transfer_buffer(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            ..Self::default()
        }
    }

    pub(crate) fn prepare_transfer(
        &mut self,
        id: TabId,
        section: TabSection,
        index: usize,
        source: &Self,
        internal: bool,
    ) -> Result<(), Error> {
        if index > self.section_tabs(section).count() {
            return Err(Error::InvalidSectionIndex(index));
        }
        if self.tab(id).is_some() {
            return Err(Error::Address("The destination already owns this tab"));
        }
        let compatible = match (&source.engine, &self.engine) {
            (Some(source), Some(target)) => source.same_runtime(target),
            (None, None) => true,
            _ => false,
        };
        if !internal && !compatible {
            return Err(Error::Address(
                "The destination uses a different web engine",
            ));
        }
        self.tabs.reserve(1);
        Ok(())
    }

    fn remove_tab_at(&mut self, index: usize) -> Tab {
        let id = self.tabs[index].id();
        if self.active == Some(id) {
            self.reset_input();
        }
        let tab = self.tabs.remove(index);
        self.recent.retain(|&tab| tab != id);
        if let Some(split) = self.split
            && split.panes(self.split_view_limit).contains(&Some(id))
        {
            self.exit_split(
                split
                    .panes(self.split_view_limit)
                    .iter()
                    .flatten()
                    .copied()
                    .find(|pane| *pane != id),
            );
        }
        if self.active == Some(id) {
            self.active = None;
            if let Some(next) = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(Tab::id)
            {
                self.activate_tab(next);
            }
        }
        tab
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

    pub(crate) fn content_width(&self) -> f32 {
        self.window.viewport[0]
    }

    pub(crate) fn viewport_width(&self) -> f32 {
        self.displayed_split()
            .map_or(self.content_width(), |split| {
                let count = split.pane_count.min(self.split_view_limit);
                if count == 2 {
                    return split::widths(self.content_width(), split.ratio)[self.focused_side()];
                }
                split::grid_cell_rect(
                    [0.0, 0.0, self.content_width(), self.window.viewport[1]],
                    count,
                    self.focused_side(),
                    split.ratio,
                )[2]
            })
    }

    pub(crate) fn window_focused(&self) -> bool {
        self.window.focused
    }

    pub(crate) fn thumbnail(&self, id: TabId) -> Option<Arc<rune::Pixels>> {
        let tab = self.tab(id).filter(|tab| !tab.is_internal())?;
        tab.thumbnail.as_ref()?.get(&tab.surface)
    }

    pub(crate) fn set_window_focus(&mut self, focused: bool) {
        if !focused {
            self.reset_input();
        }
        self.window.focused = focused;
    }

    /// Suspends every live page while the Rune window is closed but the app remains running.
    pub(crate) fn suspend_window(&mut self) {
        self.set_window_focus(false);
        self.dismiss_peek();
        let Some(engine) = &self.engine else {
            return;
        };
        for tab in &mut self.tabs {
            if tab.is_internal() {
                continue;
            }
            if tab.engine.focused {
                tab.engine.focused = false;
                engine.send(Command::View(tab.id(), ViewCommand::Focus(false)));
            }
            if !tab.engine.hidden {
                tab.engine.hidden = true;
                engine.send(Command::View(tab.id(), ViewCommand::Visible(false)));
            }
        }
    }

    /// Releases every view owned by a window that is being destroyed. The engine itself remains
    /// shared with the other live windows.
    pub(crate) fn close_window_views(&mut self) {
        self.suspend_window();
        self.dismiss_peek();
        for tab in &self.tabs {
            let id = tab.id();
            if !tab.is_internal()
                && let Some(engine) = &self.engine
            {
                engine.discard_tab_updates(id);
                engine.send(Command::Close(id));
            }
            id.release();
        }
        self.tabs.clear();
        self.recent.clear();
        self.active = None;
        self.split = None;
        self.promoted_visit = None;
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

    /// Asks the engine to close every page and stop, without waiting for it.
    pub(crate) fn shut_down(&mut self) {
        if let Some(engine) = &self.engine {
            engine.shut_down();
        }
    }

    pub(crate) fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }

    /// Sends `command` to the tab's view, if the tab is open and there is an engine.
    pub(crate) fn send(&self, id: TabId, command: ViewCommand) {
        if let Some(engine) = &self.engine
            && let Some(tab) = self.page(id)
        {
            if command.invalidates_preview() {
                tab.surface.request_preview_invalidation();
            }
            engine.send(Command::View(id, command));
        }
    }
}

fn section_rank(section: TabSection) -> usize {
    match section {
        TabSection::Favourite => 0,
        TabSection::Regular => 1,
    }
}

#[cfg(test)]
mod tests;
