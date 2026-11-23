// SPDX-License-Identifier: GPL-3.0-only

//! Connecting the browser to its engine, and sending commands to the engine.

use super::Browser;
#[cfg(any(feature = "ladybird-bridge", test))]
use crate::engine::Engine;
use crate::engine::{Command, ViewCommand};
use crate::tab::TabId;

impl Browser {
    pub(crate) fn send_engine(&self, command: Command) {
        if let Some(engine) = &self.engine {
            engine.send(command);
        }
    }

    /// Whether pages can open yet: the engine has started and this browser is connected to it.
    pub(crate) fn engine_ready(&self) -> bool {
        self.engine.is_some()
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

    /// Asks the engine to close every page and stop, without waiting for it.
    pub(crate) fn shut_down(&mut self) {
        if let Some(engine) = &self.engine {
            engine.shut_down();
        }
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
