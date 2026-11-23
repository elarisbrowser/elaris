// SPDX-License-Identifier: GPL-3.0-only

//! Sleeping tabs. A sleeping tab keeps its place, address, title, favicon and zoom, but its view
//! is closed, so the engine frees the page's memory. It loads its page again when next shown.

use std::time::Instant;

use super::Browser;
use crate::engine::Command;
use crate::tab::{Applied, Tab, TabId, TabSection};
use elaris_memory_saver::Candidate;

impl Tab {
    /// Doing something that closing its view would break or lose.
    pub(crate) fn busy(&self) -> bool {
        self.playing_audio
            || self.permissions_in_use.contains(&true)
            || self.loading
            || self.web_fullscreen
            || self.awaiting_close
            || self.page_dialog.is_some()
            || self.permission_prompt.is_some()
            || self.external_url.is_some()
            // Shown again, a page ended for its memory would grow back to the limit.
            || self.crash.is_some()
            || self.id().has_open_popup()
    }
}

impl Browser {
    /// The background tabs that could sleep, and how long each has gone unseen. Private windows
    /// offer none: closing their last view would end the private session.
    pub(crate) fn sleep_candidates(&self, now: Instant) -> Vec<Candidate<TabId>> {
        if self.private || self.engine.is_none() || self.engine_error.is_some() {
            return Vec::new();
        }
        let visible = self.visible_tabs();
        self.tabs
            .iter()
            .filter(|tab| {
                !tab.is_internal()
                    && !tab.asleep
                    && Some(tab.id()) != self.active
                    && !visible.contains(&Some(tab.id()))
                    // A tab that has never loaded holds nothing to free.
                    && tab.engine.pending_load.is_none()
            })
            .map(|tab| Candidate {
                id: tab.id(),
                idle: now.saturating_duration_since(tab.last_seen),
                favourite: tab.section() == TabSection::Favourite,
                busy: tab.busy(),
                memory: None,
            })
            .collect()
    }

    /// Closes the tab's view and remembers its page, to load again when the tab is next shown.
    pub(crate) fn sleep_tab(&mut self, id: TabId) -> bool {
        let Some(engine) = self.engine.clone() else {
            return false;
        };
        let Some(tab) = self
            .tab_mut(id)
            .filter(|tab| !tab.is_internal() && !tab.asleep)
        else {
            return false;
        };
        engine.discard_tab_updates(id);
        engine.send(Command::Close(id));
        tab.asleep = true;
        tab.thumbnail = None;
        tab.clear_page_hover();
        tab.ime_caret = None;
        tab.find.invalidate();
        tab.accessibility = Default::default();
        tab.accessibility_events.clear();
        // The page's session history goes with its view.
        tab.navigation_items.clear();
        tab.can_go_back = false;
        tab.can_go_forward = false;
        tab.engine = Applied {
            pending_load: Some(tab.url.clone()),
            deferred_load: true,
            ..Applied::default()
        };
        true
    }
}
