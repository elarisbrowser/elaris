// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver's schedule: every so often, more often while memory is short, it asks the policy
//! which of this window's background tabs to put to sleep. Under pressure it also asks the engine
//! how much memory each page holds, so the tab that frees the most goes first.

use std::collections::HashMap;
use std::time::Instant;

use rune::Tx;

use super::AppState;
use crate::browser::Polled;
use crate::tab::TabId;
use elaris_core::memory_saver::{Candidate, Pressure};
use elaris_core::permissions::{PermissionKind, PermissionState, PermissionStore};

#[derive(Default)]
pub(crate) struct MemorySaverState {
    /// When the next check is due; none until the first.
    check: Option<Instant>,
    /// The memory each page held when the engine last described its processes, by tab.
    tab_memory: HashMap<u64, u64>,
}

impl MemorySaverState {
    pub(crate) fn check_now(&mut self) {
        self.check = None;
    }
}

impl AppState {
    /// Takes in the engine's latest process figures, whoever asked for them; the Task Manager's
    /// refreshes count too. Call it before the Task Manager takes them.
    pub(super) fn note_tab_memory(&mut self, polled: &Polled) {
        if let Some((_, Ok(processes))) = &polled.processes {
            self.memory_saver.tab_memory = elaris_core::processes::tab_memory(processes);
        }
    }

    /// Puts the tabs Memory Saver chooses to sleep when a check is due, and reports whether any
    /// slept.
    pub(super) fn poll_memory_saver(&mut self, tx: &mut Tx) -> bool {
        let mode = self.settings.memory_saver;
        let now = Instant::now();
        if self.memory_saver.check.is_some_and(|due| now < due) {
            return false;
        }
        let pressure = crate::platform::memory::pressure();
        let Some(interval) = mode.check_interval(pressure) else {
            self.memory_saver.check = None;
            return false;
        };
        self.memory_saver.check = Some(now + interval);
        tx.wake_after(interval);
        if pressure != Pressure::Normal {
            // For the next check: by then the figures have arrived.
            self.request_tab_memory();
        }
        let candidates: Vec<_> = self
            .browser
            .sleep_candidates(now)
            .into_iter()
            .map(|candidate| self.with_site_state(candidate))
            .collect();
        mode.choose(pressure, &candidates)
            .into_iter()
            .fold(false, |slept, id| self.browser.sleep_tab(id) | slept)
    }

    fn request_tab_memory(&self) {
        // The answer comes back to whichever of this window's tabs asked.
        if let Some(tab) = self.browser.tabs().first() {
            self.browser.request_processes(tab.id());
        }
    }

    /// Adds what the browser alone cannot tell: the page's memory, and whether its site may
    /// show notifications, which a sleeping page could not.
    fn with_site_state(&self, mut candidate: Candidate<TabId>) -> Candidate<TabId> {
        candidate.memory = self
            .memory_saver
            .tab_memory
            .get(&candidate.id.raw())
            .copied();
        let notifies = self
            .browser
            .tab(candidate.id)
            .and_then(|tab| PermissionStore::origin(&tab.url))
            .is_some_and(|origin| {
                self.permission_store
                    .state(&origin, PermissionKind::Notifications)
                    == PermissionState::Allow
            });
        candidate.busy |= notifies;
        candidate
    }
}
