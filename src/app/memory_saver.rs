// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver's schedule: every so often, more often while memory is short, it asks the policy
//! which of this window's background tabs to put to sleep.

use std::time::Instant;

use rune::Tx;

use super::AppState;

impl AppState {
    /// Puts the tabs Memory Saver chooses to sleep when a check is due, and reports whether any
    /// slept.
    pub(super) fn poll_memory_saver(&mut self, tx: &mut Tx) -> bool {
        let mode = self.settings.memory_saver;
        let now = Instant::now();
        if self.memory_saver_check.is_some_and(|due| now < due) {
            return false;
        }
        let pressure = crate::platform::memory::pressure();
        let Some(interval) = mode.check_interval(pressure) else {
            self.memory_saver_check = None;
            return false;
        };
        self.memory_saver_check = Some(now + interval);
        tx.wake_after(interval);
        let chosen = mode.choose(pressure, &self.browser.sleep_candidates(now));
        chosen
            .into_iter()
            .fold(false, |slept, id| self.browser.sleep_tab(id) | slept)
    }
}
