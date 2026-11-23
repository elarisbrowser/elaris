// SPDX-License-Identifier: GPL-3.0-only

//! Sleeping tabs. Elaris decides which tabs sleep; Ladybird keeps what each holds. A sleeping tab
//! keeps its place, address, title, favicon and zoom, and Ladybird its session history and
//! sessionStorage, but its view is closed, so the engine frees the page's memory. When next shown
//! it gets its history back and loads its current page again.

use std::time::Instant;

use super::Browser;
use crate::engine::{Command, ViewCommand};
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
                    && !tab.engine.sleep_requested
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

    /// Asks the engine to put the tab to sleep. Ladybird has the tab's pages save their state,
    /// checks once more that nothing would break, and answers through [`Self::finish_sleep`].
    pub(crate) fn sleep_tab(&mut self, id: TabId, memory_pressure: bool) -> bool {
        let Some(engine) = self.engine.clone() else {
            return false;
        };
        let Some(tab) = self
            .tab_mut(id)
            .filter(|tab| !tab.is_internal() && !tab.asleep && !tab.engine.sleep_requested)
        else {
            return false;
        };
        tab.engine.sleep_requested = true;
        engine.send(Command::View(id, ViewCommand::Discard { memory_pressure }));
        true
    }

    /// Takes in Ladybird's answer to [`Self::sleep_tab`]. Once it has kept what the tab holds, its
    /// history and sessionStorage among them, the view closes. A tab Ladybird kept loaded, or that
    /// the user came back to meanwhile, stays awake and counts as just seen, so it isn't asked
    /// about again until it has been idle as long once more.
    pub(super) fn finish_sleep(&mut self, id: TabId, kept: bool) -> bool {
        let Some(engine) = self.engine.clone() else {
            return false;
        };
        let visible = self.visible_tabs().contains(&Some(id)) || self.active == Some(id);
        let Some(tab) = self.tab_mut(id).filter(|tab| tab.engine.sleep_requested) else {
            return false;
        };
        tab.engine.sleep_requested = false;
        if !kept || visible {
            if kept {
                engine.send(Command::ForgetSleep(id));
            }
            tab.last_seen = Instant::now();
            return false;
        }
        engine.discard_tab_updates(id);
        engine.send(Command::Close(id));
        tab.asleep = true;
        tab.thumbnail = None;
        tab.clear_page_hover();
        tab.ime_caret = None;
        tab.find.invalidate();
        tab.accessibility = Default::default();
        tab.accessibility_events.clear();
        tab.translation = Default::default();
        tab.engine = Applied {
            pending_load: Some(tab.url.clone()),
            deferred_load: true,
            restore: true,
            ..Applied::default()
        };
        true
    }
}
