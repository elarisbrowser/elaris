// SPDX-License-Identifier: GPL-3.0-only

use rune::Tx;

use crate::tab::{Tab, TabId};
use crate::ui::viewport::focus_key;

impl super::AppState {
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
        tx.focus(focus_key(&self.browser), false);
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
    pub(super) fn sync_find_tab(&mut self) {
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

    pub(super) fn replace_find_input(&mut self, query: String, select_all: bool) {
        self.find_input.replace_text(query);
        if select_all {
            self.find_input.anchor = 0;
            self.find_input.focus = self.find_input.text.len();
        }
    }
}
