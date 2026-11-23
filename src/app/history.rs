// SPDX-License-Identifier: GPL-3.0-only

//! State and storage coordination for the native latest-visited list.

use std::collections::BTreeSet;

use elaris_core::history::{Cursor, Page};
use rune::{TextEdit, Tx};

use super::AppState;
use crate::storage::{HistoryEvent, HistoryMutation};
use crate::tab::TabId;
use elaris_core::internal::{InternalPage, InternalRoute};

/// The destructive action awaiting confirmation in a modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryConfirm {
    Clear,
    Delete,
}

#[derive(Default)]
pub(crate) struct HistoryPage {
    pub(crate) input: TextEdit,
    pub(crate) page: Option<Page>,
    pub(crate) loading: bool,
    pub(crate) load_error: Option<String>,
    pub(crate) action_error: Option<String>,
    pub(crate) confirming: Option<HistoryConfirm>,
    pub(crate) writing: bool,
    pub(crate) active: Option<TabId>,
    pub(crate) previous: Vec<Cursor>,
    /// Addresses ticked for removal on the current page.
    pub(crate) selected: BTreeSet<String>,
    revision: u64,
}

impl HistoryPage {
    pub(crate) fn request(&mut self) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.loading = true;
        self.load_error = None;
        self.revision
    }

    pub(crate) fn receive(&mut self, event: HistoryEvent) -> bool {
        let HistoryEvent::Page(_, revision, result) = event else {
            return false;
        };
        if revision != self.revision {
            return false;
        }
        self.loading = false;
        match result {
            Ok(page) => self.page = Some(page),
            Err(error) => self.load_error = Some(error),
        }
        true
    }
}

impl AppState {
    pub(super) fn active_history_tab(&self) -> Option<TabId> {
        let tab = self.browser.active_tab()?;
        (tab.internal == Some(InternalRoute::Page(InternalPage::History))).then(|| tab.id())
    }

    pub(crate) fn request_history_page(&mut self) {
        let history = &mut self.history_page;
        let revision = history.request();
        let result = self.storage.as_ref().is_some_and(|storage| {
            storage
                .history()
                .request_page(
                    self.window_id,
                    revision,
                    &history.input.text,
                    history.previous.last().cloned(),
                )
                .is_ok()
        });
        if !result {
            history.receive(HistoryEvent::Page(
                self.window_id,
                revision,
                Err("Browsing history is unavailable.".into()),
            ));
        }
    }

    pub(crate) fn history_search_changed(&mut self, tx: &mut Tx) {
        self.history_page.previous.clear();
        self.history_page.page = None;
        self.history_page.action_error = None;
        self.history_page.selected.clear();
        self.request_history_page();
        tx.instant();
        tx.scroll_to_instant(crate::ui::HISTORY_SCROLL, 0.0, 0.0);
    }

    pub(crate) fn clear_history_search(&mut self, tx: &mut Tx) {
        self.history_page.input.replace_text(String::new());
        self.history_search_changed(tx);
        tx.focus(crate::ui::HISTORY_SEARCH, false);
    }

    pub(crate) fn move_history_page(&mut self, older: bool, tx: &mut Tx) {
        if self.history_page.loading || self.history_page.writing {
            return;
        }
        if older {
            let Some(next) = self
                .history_page
                .page
                .as_ref()
                .and_then(|page| page.next.clone())
            else {
                return;
            };
            self.history_page.previous.push(next);
        } else if self.history_page.previous.pop().is_none() {
            return;
        }
        self.history_page.page = None;
        self.history_page.selected.clear();
        self.request_history_page();
        tx.instant();
        tx.scroll_to_instant(crate::ui::HISTORY_SCROLL, 0.0, 0.0);
    }

    pub(crate) fn mutate_history(&mut self, mutation: HistoryMutation) {
        if self.active_history_tab().is_none()
            || self.history_page.writing
            || (matches!(mutation, HistoryMutation::Clear)
                && self.history_page.confirming != Some(HistoryConfirm::Clear))
        {
            return;
        }
        self.history_page.confirming = None;
        self.history_page.action_error = None;
        self.history_page.writing = self
            .storage
            .as_ref()
            .is_some_and(|storage| storage.history().mutate(self.window_id, mutation).is_ok());
        if !self.history_page.writing {
            self.history_page.action_error =
                Some("Could not save changes to browsing history.".into());
        }
    }

    pub(crate) fn toggle_history_selection(&mut self, url: &str) {
        let selected = &mut self.history_page.selected;
        if !selected.remove(url) {
            selected.insert(url.to_owned());
        }
    }

    /// Removes every ticked address.
    pub(crate) fn delete_selected_history(&mut self) {
        if !self.history_page.selected.is_empty() {
            let urls = self.history_page.selected.iter().cloned().collect();
            self.mutate_history(HistoryMutation::Remove(urls));
        }
    }

    pub(crate) fn open_history_entry(&mut self, url: &str, background: bool, tx: &mut Tx) {
        let Some(tab) = self.active_history_tab() else {
            return;
        };
        let mods = tx.mods();
        let result = if background || mods.cmd {
            self.browser.open_tab(url).map(|opened| {
                if !background && mods.shift {
                    self.activate(opened);
                }
            })
        } else {
            self.browser.navigate(tab, url)
        };
        if let Err(error) = result {
            self.history_page.action_error = Some(error.to_string());
        } else if self.active_history_tab().is_none() {
            tx.focus(crate::ui::viewport::focus_key(&self.browser), false);
        }
    }

    pub(super) fn poll_history_page(&mut self, tx: &mut Tx, visits_changed: bool) -> bool {
        let active = self.active_history_tab();
        let mut changed = active != self.history_page.active;
        if changed {
            self.history_page.active = active;
            self.history_page.confirming = None;
            self.history_page.selected.clear();
        }
        if active.is_some() && (changed || visits_changed) {
            self.request_history_page();
            changed = true;
        }
        let events = self.storage.as_ref().map_or_else(Vec::new, |storage| {
            storage.history().take_events(self.window_id)
        });
        for event in events {
            match event {
                HistoryEvent::Mutation(_, result) => {
                    changed = true;
                    self.history_page.writing = false;
                    match result {
                        Ok(HistoryMutation::Clear | HistoryMutation::ClearSince(_)) => {
                            self.history_page.previous.clear();
                            self.history_page.page = None;
                            self.request_history_page();
                            tx.scroll_to_instant(crate::ui::HISTORY_SCROLL, 0.0, 0.0);
                        }
                        Ok(HistoryMutation::Remove(urls)) => {
                            if let Some(page) = &mut self.history_page.page {
                                page.entries.retain(|row| !urls.contains(&row.entry.url));
                            }
                            for url in &urls {
                                self.history_page.selected.remove(url);
                            }
                            self.request_history_page();
                            tx.animated();
                        }
                        Err(error) => self.history_page.action_error = Some(error),
                    }
                }
                page => changed |= self.history_page.receive(page),
            }
        }
        if active.is_some()
            && !self.history_page.loading
            && self.history_page.load_error.is_none()
            && let Some(page) = &self.history_page.page
        {
            use std::time::{Duration, SystemTime, UNIX_EPOCH};
            let midnight = UNIX_EPOCH + Duration::from_secs(page.next_midnight);
            match midnight.duration_since(SystemTime::now()) {
                Ok(delay) => tx.wake_after(delay),
                Err(_) => {
                    self.request_history_page();
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elaris_core::history::History;

    #[test]
    fn stale_results_cannot_replace_the_active_search() {
        let db = History::open(std::path::Path::new(":memory:")).unwrap();
        db.record_visit("https://example.org/", "Example").unwrap();
        let mut state = HistoryPage::default();
        let initial = state.request();
        assert!(state.loading);
        state.input.replace_text("missing");
        let newer = state.request();
        assert!(!state.receive(HistoryEvent::Page(
            1,
            initial,
            db.page("", None).map_err(|e| e.to_string())
        )));
        assert!(state.loading);
        assert!(state.page.is_none());
        assert!(state.receive(HistoryEvent::Page(
            1,
            newer,
            db.page("missing", None).map_err(|e| e.to_string())
        )));
        assert!(state.page.unwrap().entries.is_empty());
        assert!(!state.loading);
    }
}
