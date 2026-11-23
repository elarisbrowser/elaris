// SPDX-License-Identifier: GPL-3.0-only

//! Opening, closing, and moving tabs, including between windows.

use super::Browser;
use super::Error;
use super::tabs::section_rank;
use crate::engine::{Command, ViewCommand};
use crate::tab::{Tab, TabId, TabSection};
use elaris_core::address::url_for_address;
use elaris_core::internal::InternalRoute;

impl Browser {
    /// Opens a tab at the end of the list loading `address`, and makes it active if no tab is.
    pub(crate) fn open_tab(&mut self, address: &str) -> Result<TabId, Error> {
        if let Some(route) = InternalRoute::parse(address) {
            return Ok(self.insert_internal_tab(route));
        }
        let url = url_for_address(address).map_err(Error::Address)?;
        Ok(self.insert_tab(url))
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

    pub(super) fn next_page_id(&mut self) -> TabId {
        TabId::next()
    }

    pub(crate) fn insert_popup(&mut self, source: TabId, request: u64, activate: bool) {
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
        if let Some((source, _)) = popup {
            tab.engine.pending_load = None;
            id.set_opener(source);
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

    /// Opens another tab at the same address as `id`.
    pub(crate) fn duplicate_tab(&mut self, id: TabId) -> Option<TabId> {
        let url = self.tab(id)?.url.clone();
        self.open_tab(&url).ok()
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
            engine.send(if tab.asleep {
                Command::ForgetSleep(id)
            } else {
                Command::Close(id)
            });
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
                engine.send(if tab.asleep {
                    Command::ForgetSleep(id)
                } else {
                    Command::Close(id)
                });
            }
            id.release();
        }
        self.tabs.clear();
        self.recent.clear();
        self.active = None;
        self.split = None;
        self.promoted_visit = None;
    }
}
