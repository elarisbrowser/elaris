// SPDX-License-Identifier: GPL-3.0-only

use rune::{PointerPress, Tx};

use super::{RECENTLY_CLOSED_LIMIT, RecentlyClosedTab};
use crate::omnibox;
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::{TabDropTarget, viewport::focus_key};

impl super::AppState {
    #[cfg(target_os = "macos")]
    pub(crate) fn can_reopen_closed_tab(&self) -> bool {
        !self.recently_closed.is_empty()
    }

    pub(crate) fn activate(&mut self, tab: TabId) {
        if self.horizontal_tabs() {
            self.omnibox.close();
        }
        crate::ui::dismiss_tab_preview(self);
        self.cancel_tab_drag();
        self.browser.activate_tab(tab);
        self.refresh_active_page();
    }

    pub(crate) fn place_split_tab(&mut self, tab: TabId, side: usize) {
        self.browser.place_split(tab, side);
        self.refresh_active_page();
    }

    pub(crate) fn begin_split_picker(&mut self, tab: TabId, side: usize) {
        self.cancel_tab_drag();
        self.dismiss_find();
        self.browser.begin_split(tab, side);
        self.refresh_active_page();
        self.split_selection = 0;
    }

    pub(crate) fn promote_peek(
        &mut self,
        destination: crate::browser::peek::Destination,
        tx: &mut Tx,
    ) {
        if self.browser.promote_peek(destination).is_some() {
            self.refresh_active_page();
            tx.focus(focus_key(&self.browser), false);
            tx.animated();
        }
    }

    fn refresh_active_page(&mut self) {
        self.site_permissions_open = false;
        self.page_menu = Default::default();
        self.sync_find_tab();
        self.address_error = None;
    }

    /// Closes the tab and opens the omnibox when the window becomes empty.
    pub(crate) fn close(&mut self, tab: TabId) {
        if !self.browser.preflight_close(tab) {
            self.finish_close(tab);
        }
    }

    pub(super) fn finish_close(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        self.site_permissions_open = false;
        self.page_menu = Default::default();
        if let Some(tab) = self.browser.tab(tab) {
            self.remember_closed(RecentlyClosedTab::from_tab(tab));
        }
        self.browser.close_tab(tab);
        self.sync_find_tab();
        if self.browser.tabs().is_empty() {
            self.open_omnibox(omnibox::Mode::New);
        }
    }

    /// Reopens a recently closed tab and returns its new identity.
    pub(crate) fn reopen_recently_closed(&mut self, index: usize) -> Option<TabId> {
        let closed = self.recently_closed.get(index)?.clone();
        let tab = self
            .browser
            .restore_tab_in_section(&closed.url, &closed.title, false, closed.section)
            .ok()?;
        self.recently_closed.remove(index);
        self.omnibox.close();
        self.activate(tab);
        Some(tab)
    }

    pub(crate) fn duplicate(&mut self, tab: TabId) {
        if let Some(tab) = self.browser.duplicate_tab(tab) {
            self.activate(tab);
        }
    }

    pub(crate) fn close_others(&mut self, tab: TabId) {
        self.cancel_tab_drag();
        if self.browser.tab(tab).is_none() {
            return;
        }
        let closed: Vec<RecentlyClosedTab> = self
            .browser
            .tabs()
            .iter()
            .filter(|candidate| candidate.id() != tab)
            .map(RecentlyClosedTab::from_tab)
            .collect();
        for closed_tab in closed.into_iter().rev() {
            self.remember_closed(closed_tab);
        }
        self.browser.close_other_tabs(tab);
    }

    #[cfg(test)]
    pub(crate) fn recently_closed(&self) -> &[RecentlyClosedTab] {
        &self.recently_closed
    }

    fn remember_closed(&mut self, tab: RecentlyClosedTab) {
        self.recently_closed.insert(0, tab);
        self.recently_closed.truncate(RECENTLY_CLOSED_LIMIT);
    }

    pub(crate) fn has_split_picker(&self) -> bool {
        self.split_picker_side().is_some()
    }

    pub(crate) fn split_picker_side(&self) -> Option<usize> {
        self.browser
            .split?
            .panes(self.browser.split_view_limit)
            .iter()
            .position(Option::is_none)
    }

    pub(crate) fn cancel_tab_drag(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        crate::drag_overlay_macos::hide(self.window_id);
        super::cross_window_drag::clear(self.window_id)
            | self.split_preview.take().is_some()
            | self.tab_drag.cancel()
    }

    #[cfg(test)]
    pub(crate) fn commit_tab_move(
        &mut self,
        tab: TabId,
        section: TabSection,
        target_index: usize,
    ) -> Result<(), crate::browser::Error> {
        self.browser.move_tab_to(tab, section, target_index)?;
        self.persist_session_if_changed();
        Ok(())
    }

    pub(crate) fn commit_tab_move_group(
        &mut self,
        tabs: &[TabId],
        section: TabSection,
        target_index: usize,
    ) -> Result<(), crate::browser::Error> {
        for (offset, tab) in tabs.iter().copied().enumerate() {
            self.browser
                .move_tab_to(tab, section, target_index + offset)?;
        }
        self.persist_session_if_changed();
        Ok(())
    }

    pub(crate) fn move_tab_to_section_animated(
        &mut self,
        tab: TabId,
        section: TabSection,
        tx: &mut Tx,
    ) {
        let Some(source_section) = self.browser.tab(tab).map(Tab::section) else {
            return;
        };
        let target_index = self
            .browser
            .section_tabs(section)
            .count()
            .saturating_sub(usize::from(source_section == section));
        let target = TabDropTarget {
            section,
            index: target_index,
        };
        if let (Some(from), Some(to)) = (
            self.tab_drag.tab_rect(tab),
            Some(
                self.tab_drag
                    .target_rect(self.browser.tabs(), target, source_section),
            ),
        ) {
            self.tab_drag
                .transition(tab, source_section, section, target_index, from, to);
        }
        if let Err(error) = self.browser.move_tab_to(tab, section, target_index) {
            self.tab_drag.cancel();
            eprintln!("Elaris: {error}");
            return;
        }
        self.persist_session_if_changed();
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        tx.wake_after(std::time::Duration::ZERO);
    }

    pub(crate) fn reload(&mut self, tab: TabId) {
        self.browser.reload(tab);
    }

    pub(crate) fn open_context_menu(&mut self, tab: Option<TabId>, point: PointerPress) {
        self.page_menu = Default::default();
        self.context_menu = Some((tab, point));
    }

    pub(crate) fn context_menu(&self) -> Option<(Option<TabId>, PointerPress)> {
        self.context_menu
    }

    pub(crate) fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
    }

    pub(crate) fn close_active(&mut self) {
        if let Some(tab) = self.browser.active_tab().map(Tab::id) {
            self.close(tab);
        }
    }
}
