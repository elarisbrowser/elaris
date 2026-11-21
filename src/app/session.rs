// SPDX-License-Identifier: GPL-3.0-only

use crate::browser::Browser;
use crate::storage::{self, Session, SessionTab};
use crate::tab::{TabId, TabSection};

impl super::AppState {
    /// Runs `operation` on the active tab, if there is one.
    pub(crate) fn with_active_tab(&mut self, operation: impl FnOnce(&mut Browser, TabId)) {
        if let Some(tab) = self.browser.active_tab().map(|tab| tab.id()) {
            operation(&mut self.browser, tab);
        }
    }

    /// Why the address or active page failed.
    pub(crate) fn status(&self) -> Option<&str> {
        self.address_error
            .as_deref()
            .or_else(|| self.browser.active_tab()?.error.as_deref())
    }

    pub(super) fn restore_session(&mut self, session: &Session) {
        let mut restored = Vec::with_capacity(session.tabs.len());
        for (index, tab) in session.tabs.iter().enumerate() {
            let id = self
                .browser
                .restore_tab_in_section(
                    &tab.url,
                    &tab.title,
                    index != session.active_tab,
                    session.section_at(index),
                )
                .ok();
            if let (Some(id), Some(favicon)) = (id, tab.favicon.as_ref()) {
                self.browser.restore_favicon(
                    id,
                    favicon.width,
                    favicon.height,
                    favicon.rgba.clone(),
                );
            }
            restored.push(id);
        }
        let active = restored
            .get(session.active_tab)
            .copied()
            .flatten()
            .or_else(|| restored.iter().find_map(|tab| *tab));
        if let Some(active) = active {
            if let Some(split) = &session.split
                && let (Some(left), Some(right)) = (
                    restored.get(split.left).copied().flatten(),
                    restored.get(split.right).copied().flatten(),
                )
                && left != right
            {
                let mut panes = [None; crate::browser::split::MAX_PANES];
                panes[0] = Some(left);
                panes[1] = Some(right);
                for (index, tab_index) in split.additional.iter().enumerate() {
                    if index + 2 >= panes.len() {
                        break;
                    }
                    panes[index + 2] = restored.get(*tab_index).copied().flatten();
                }
                self.browser.split = Some(crate::browser::split::Split {
                    panes,
                    pane_count: (split.additional.len() + 2).min(crate::browser::split::MAX_PANES),
                    ratio: f32::from(split.ratio) / 10_000.0,
                });
            }
            self.browser.activate_tab(active);
        }
    }

    pub(super) fn restore_persistent_tabs(&mut self, session: &Session) {
        for (index, tab) in session.tabs.iter().enumerate() {
            let section = session.section_at(index);
            if section != TabSection::Regular
                && let Ok(id) = self
                    .browser
                    .restore_tab_in_section(&tab.url, &tab.title, true, section)
                && let Some(favicon) = tab.favicon.as_ref()
            {
                self.browser.restore_favicon(
                    id,
                    favicon.width,
                    favicon.height,
                    favicon.rgba.clone(),
                );
            }
        }
        self.browser.clear_active_tab();
    }

    pub(crate) fn persist_session_if_changed(&mut self) {
        if self.browser.private
            || self.mini
            || self.pending_detach
            || self.merged_away
            || self.tab_transfer_blocks_session()
        {
            return;
        }
        let session = self.session_snapshot();
        if self.last_saved_session.as_ref() != Some(&session) {
            self.queue_session(session);
        }
    }

    pub(super) fn persist_session(&mut self) {
        if self.browser.private
            || self.mini
            || self.pending_detach
            || self.merged_away
            || self.tab_transfer_blocks_session()
        {
            return;
        }
        let session = self.session_snapshot();
        self.queue_session(session);
    }

    pub(super) fn session_snapshot(&self) -> Session {
        let mut favourites = Vec::new();
        let tabs = self
            .browser
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                match tab.section() {
                    TabSection::Favourite => favourites.push(index),
                    TabSection::Regular => {}
                }
                SessionTab {
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                    favicon: tab.favicon_data.clone(),
                }
            })
            .collect();
        let active_tab = self
            .browser
            .active_tab()
            .and_then(|active| {
                self.browser
                    .tabs()
                    .iter()
                    .position(|tab| tab.id() == active.id())
            })
            .unwrap_or_default();
        let mut session = Session::new_with_sections(tabs, active_tab, favourites);
        if let Some(split) = self.browser.split
            && let panes = split.panes(self.browser.split_view_limit)
            && panes.len() >= 2
            && let (Some(left), Some(right)) = (panes[0], panes[1])
            && panes.iter().all(Option::is_some)
        {
            let index = |id| self.browser.tabs().iter().position(|tab| tab.id() == id);
            if let (Some(left_index), Some(right_index)) = (index(left), index(right)) {
                let additional = panes[2..]
                    .iter()
                    .flatten()
                    .filter_map(|id| index(*id))
                    .collect();
                session.split = Some(storage::SessionSplit {
                    left: left_index,
                    right: right_index,
                    additional,
                    ratio: (split.ratio * 10_000.0).round() as u16,
                });
            }
        }
        session
    }

    fn queue_session(&mut self, session: Session) {
        if let Some(storage) = &self.storage {
            super::store_window_session(self.window_id, session.clone(), storage);
            self.last_saved_session = Some(session);
        }
    }

    /// The active tab's title, when it differs from what the window shows.
    pub(super) fn new_window_title(&mut self) -> Option<String> {
        let title = self
            .browser
            .active_tab()
            .map_or("", |tab| tab.title.as_str());
        (self.window_title.as_deref() != Some(title)).then(|| {
            self.window_title = Some(title.to_owned());
            title.to_owned()
        })
    }
}
