// SPDX-License-Identifier: GPL-3.0-only

//! Which tab is active, and the order the tabs were last used in.

use super::Browser;
use crate::tab::{Tab, TabId};

impl Browser {
    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    pub(crate) fn clear_active_tab(&mut self) {
        self.reset_input();
        self.active = None;
        self.split = None;
        self.recent.clear();
    }

    pub(super) fn set_active(&mut self, id: TabId) {
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
}
