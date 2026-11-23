// SPDX-License-Identifier: GPL-3.0-only

use rune::{PointerPress, Tx};

use super::{RECENTLY_CLOSED_LIMIT, RecentlyClosedTab};
use crate::omnibox;
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::{TabDropTarget, viewport::focus_key};

enum CloseAction {
    Tabs {
        keep: Option<TabId>,
    },
    /// Closes the window, or approves this window's part of an app quit.
    Window {
        #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
        quit: bool,
    },
}

/// Targets stay open until the entire close request has been approved.
pub(super) struct ClosingTabs {
    targets: Vec<TabId>,
    action: CloseAction,
    checking: usize,
    cancelled: bool,
    started: std::time::Instant,
}

impl super::AppState {
    #[cfg(target_os = "macos")]
    #[cfg_attr(elaris_ui = "linux", allow(dead_code))]
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
        self.navigation_menu = None;
        self.zoom_menu = None;
        self.sync_find_tab();
        self.address_error = None;
    }

    /// Closes the tab and opens the omnibox when the window becomes empty.
    pub(crate) fn close(&mut self, tab: TabId) {
        if self.browser.tab(tab).is_some() {
            self.begin_close(vec![tab], CloseAction::Tabs { keep: None });
        }
    }

    /// Elaris's own Linux close button: closes the window once every page has agreed to unload.
    #[cfg(elaris_ui = "linux")]
    pub(crate) fn request_window_close(&mut self, tx: &mut Tx) {
        self.request_window_unload(false, tx);
    }

    /// Rune's close request (the close button, the system's Close): closes now when no page
    /// objects, otherwise once every page has agreed to unload.
    pub(crate) fn close_requested(&mut self, _: &mut Tx) -> bool {
        if self.closing_tabs.is_some() {
            return false;
        }
        let targets = self.browser.tabs().iter().map(Tab::id).collect();
        self.begin_close(targets, CloseAction::Window { quit: false })
    }

    /// Asks this window's pages to unload as part of quitting the app.
    #[cfg(target_os = "macos")]
    pub(crate) fn request_quit_close(&mut self, tx: &mut Tx) {
        self.request_window_unload(true, tx);
    }

    fn request_window_unload(&mut self, quit: bool, tx: &mut Tx) {
        let targets = self.browser.tabs().iter().map(Tab::id).collect();
        if self.closing_tabs.is_some() {
            self.close_window_cancelled(quit);
        } else if self.begin_close(targets, CloseAction::Window { quit }) {
            self.close_window_approved(quit, tx);
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    fn close_window_approved(&self, quit: bool, tx: &mut Tx) {
        #[cfg(target_os = "macos")]
        if quit {
            crate::platform::macos::menu::quit_window_approved(self.window_id, tx);
            return;
        }
        tx.close_window();
    }

    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    fn close_window_cancelled(&self, quit: bool) {
        #[cfg(target_os = "macos")]
        if quit {
            crate::platform::macos::menu::quit_cancelled();
        }
    }

    fn begin_close(&mut self, targets: Vec<TabId>, action: CloseAction) -> bool {
        if self.closing_tabs.is_some() {
            return false;
        }
        self.cancel_tab_drag();
        self.closing_tabs = Some(ClosingTabs {
            targets,
            action,
            checking: 0,
            cancelled: false,
            started: std::time::Instant::now(),
        });
        self.advance_close()
    }

    /// Returns true when the host should close the window.
    fn advance_close(&mut self) -> bool {
        let Some(mut close) = self.closing_tabs.take() else {
            return false;
        };
        while let Some(&tab) = close.targets.get(close.checking) {
            let Some(target) = self.browser.tab(tab) else {
                return false;
            };
            if target.awaiting_close {
                return false;
            }
            if self.browser.preflight_close(tab) {
                self.closing_tabs = Some(close);
                return false;
            }
            close.checking += 1;
        }

        let keep = match close.action {
            CloseAction::Tabs { keep } => keep,
            CloseAction::Window { .. } => return true,
        };
        self.site_permissions_open = false;
        self.page_menu = Default::default();
        self.navigation_menu = None;
        self.zoom_menu = None;
        elaris_core::perf::record(elaris_core::perf::Stage::TabClose, close.started.elapsed());
        for tab in close.targets.into_iter().rev() {
            if let Some(tab_state) = self.browser.tab(tab) {
                self.remember_closed(RecentlyClosedTab::from_tab(tab_state));
                self.browser.close_tab(tab);
            }
        }
        if let Some(keep) = keep {
            self.browser.exit_split(None);
            self.activate(keep);
        }
        self.sync_find_tab();
        if self.browser.tabs().is_empty() {
            self.open_omnibox(omnibox::Mode::New);
        }
        false
    }

    pub(super) fn poll_tab_close(&mut self, polled: &crate::browser::Polled, tx: &mut Tx) {
        let Some(close) = &mut self.closing_tabs else {
            return;
        };
        let checking = close.targets[close.checking];
        close.cancelled |= close
            .targets
            .iter()
            .any(|&tab| self.browser.tab(tab).is_none())
            || matches!(close.action, CloseAction::Tabs { keep: Some(tab) } if self.browser.tab(tab).is_none())
            || polled
                .committed
                .iter()
                .any(|(tab, _, _)| close.targets.contains(tab));
        if matches!(close.action, CloseAction::Window { .. }) {
            close.cancelled |= self.browser.tabs().len() != close.targets.len();
        }
        let quit = matches!(close.action, CloseAction::Window { quit: true });
        let decision = polled
            .close_decisions
            .iter()
            .find_map(|&(tab, approved)| (tab == checking).then_some(approved));
        match decision {
            Some(true) if !close.cancelled => {
                let previous = self.browser.active_tab().map(Tab::id);
                close.checking += 1;
                // The approval arrives from the engine, not input, so the closing rows need
                // asking to fade and close their space rather than unmount at once.
                tx.animated();
                if self.advance_close() {
                    self.close_window_approved(quit, tx);
                } else if self.closing_tabs.is_none()
                    && self.browser.active_tab().map(Tab::id) != previous
                {
                    let key = if self.omnibox.open {
                        "omnibox-input"
                    } else {
                        focus_key(&self.browser)
                    };
                    tx.focus(key, false);
                }
            }
            Some(_) => {
                self.closing_tabs = None;
                self.close_window_cancelled(quit);
            }
            None if self
                .browser
                .tab(checking)
                .is_none_or(|tab| !tab.awaiting_close) =>
            {
                self.closing_tabs = None;
                self.close_window_cancelled(quit);
            }
            None => {}
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
        if self.browser.tab(tab).is_none() {
            return;
        }
        let targets = self
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .filter(|&id| id != tab)
            .collect();
        self.begin_close(targets, CloseAction::Tabs { keep: Some(tab) });
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
        crate::platform::macos::drag_overlay::hide(self.window_id);
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
        self.navigation_menu = None;
        self.zoom_menu = None;
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
