// SPDX-License-Identifier: GPL-3.0-only

//! Loading pages in tabs, moving through their history, and zooming them.

use super::Browser;
use super::Error;
use crate::engine::{Command, Surface, ViewCommand};
use crate::tab::{Tab, TabId};
use elaris_core::address::url_for_address;
use elaris_core::internal::InternalRoute;

impl Browser {
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
            // Show the stop mark at once rather than waiting for the engine to report the load.
            if self.engine.is_some()
                && let Some(tab) = self.tab_mut(id)
            {
                tab.loading = true;
            }
        }
    }

    pub(crate) fn stop_loading(&mut self, id: TabId) {
        self.send(id, ViewCommand::StopLoading);
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
}
