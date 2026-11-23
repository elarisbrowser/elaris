// SPDX-License-Identifier: GPL-3.0-only

//! State for the Task Manager page, which shows Ladybird's processes and refreshes them every
//! second while it is the active tab.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use elaris_core::internal::{InternalPage, InternalRoute};
use elaris_core::processes::Overview;
use rune::{TextEdit, Tx};

use super::AppState;
use crate::browser::Polled;
use crate::tab::TabId;

/// How often the page asks the engine for new numbers, as Ladybird's Task Manager does.
const REFRESH: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct TaskManagerPage {
    pub(crate) input: TextEdit,
    pub(crate) overview: Option<Overview>,
    /// Why the processes could not be described.
    pub(crate) load_error: Option<String>,
    /// Why the last process could not be ended.
    pub(crate) action_error: Option<String>,
    /// The selected process, by Ladybird's child ID.
    pub(crate) selected: Option<u64>,
    /// Site groups showing their processes, by site key.
    pub(crate) expanded: BTreeSet<String>,
    active: Option<TabId>,
    /// When the engine was last asked, while its answer is still due.
    requested: Option<Instant>,
    last_request: Option<Instant>,
}

impl AppState {
    fn active_task_manager_tab(&self) -> Option<TabId> {
        let tab = self.browser.active_tab()?;
        (tab.internal == Some(InternalRoute::Page(InternalPage::TaskManager))).then(|| tab.id())
    }

    /// Takes in the engine's answer and asks again once a second while the page is the active tab.
    pub(super) fn poll_task_manager(&mut self, polled: &mut Polled, tx: &mut Tx) -> bool {
        let active = self.active_task_manager_tab();
        let page = &mut self.task_manager;
        let mut changed = active != page.active;
        if changed {
            page.active = active;
            page.requested = None;
            page.last_request = None;
            page.action_error = None;
        }
        if let Some((tab, result)) = polled.processes.take()
            && Some(tab) == active
        {
            page.requested = None;
            changed = true;
            match result {
                Ok(processes) => {
                    let overview = Overview::new(processes);
                    page.selected = page.selected.filter(|id| overview.process(*id).is_some());
                    page.overview = Some(overview);
                    page.load_error = None;
                }
                Err(error) => page.load_error = Some(error),
            }
        }
        if let Some((tab, error)) = polled.processes_error.take()
            && Some(tab) == active
        {
            page.action_error = Some(error);
            changed = true;
        }
        let Some(tab) = active else {
            return changed;
        };
        let now = Instant::now();
        // An answer that never came, such as from an engine that stopped, is asked for again.
        let waiting = page
            .requested
            .is_some_and(|requested| now - requested < REFRESH * 5);
        let due = page.last_request.is_none_or(|last| now - last >= REFRESH);
        if !waiting && due {
            if self.browser.request_processes(tab) {
                page.requested = Some(now);
            } else {
                page.load_error = Some("This build has no web engine.".into());
                changed = true;
            }
            page.last_request = Some(now);
        }
        let next = page
            .last_request
            .map_or(REFRESH, |last| REFRESH.saturating_sub(now - last));
        tx.wake_after(next);
        changed
    }

    pub(crate) fn task_manager_search_changed(&mut self, tx: &mut Tx) {
        tx.instant();
        tx.scroll_to_instant(crate::ui::TASK_MANAGER_SCROLL, 0.0, 0.0);
    }

    pub(crate) fn clear_task_manager_search(&mut self, tx: &mut Tx) {
        self.task_manager.input.replace_text(String::new());
        self.task_manager_search_changed(tx);
        tx.focus(crate::ui::TASK_MANAGER_SEARCH, false);
    }

    pub(crate) fn select_process(&mut self, id: u64) {
        let page = &mut self.task_manager;
        page.selected = (page.selected != Some(id)).then_some(id);
        page.action_error = None;
    }

    pub(crate) fn toggle_site(&mut self, key: &str) {
        let expanded = &mut self.task_manager.expanded;
        if !expanded.remove(key) {
            expanded.insert(key.to_owned());
        }
    }

    /// Ends the selected process, unless it is the browser's own.
    pub(crate) fn end_selected_process(&mut self) {
        let page = &mut self.task_manager;
        let (Some(tab), Some(id)) = (page.active, page.selected) else {
            return;
        };
        if !page
            .overview
            .as_ref()
            .and_then(|overview| overview.process(id))
            .is_some_and(elaris_core::processes::Process::can_end)
        {
            return;
        }
        page.action_error = None;
        page.selected = None;
        self.browser.end_process(tab, id);
        // Show the process gone as soon as the engine has ended it.
        page.last_request = None;
    }
}

/// Opens the existing Task Manager tab, or creates it once.
pub(crate) fn open_task_manager(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    let tab = state
        .browser
        .internal_tab(InternalPage::TaskManager)
        .unwrap_or_else(|| {
            state
                .browser
                .open_tab(InternalPage::TaskManager.address())
                .expect("valid built-in Task Manager route")
        });
    state.activate(tab);
    tx.focus(crate::ui::TASK_MANAGER_SEARCH, false);
}
