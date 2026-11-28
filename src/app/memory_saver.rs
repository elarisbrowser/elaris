// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver's schedule: every so often, more often while memory is short, it asks the policy
//! which of this window's background tabs to put to sleep, and the engine how much memory each page
//! holds, so the tab that frees the most goes first. It also counts the memory sleeping tabs freed,
//! keeps the sites the user never wants asleep, and says why a tab stays awake.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rune::{TextEdit, Tx};

use super::AppState;
use crate::browser::Polled;
use crate::tab::{Tab, TabId, TabSection};
use elaris_core::permissions::{PermissionKind, PermissionState, PermissionStore};
use elaris_memory_saver::{AwakeReason, Candidate, MemorySaver, Pressure};

#[derive(Default)]
pub(crate) struct MemorySaverState {
    /// When the next check is due; none until the first.
    check: Option<Instant>,
    /// The memory each page held when the engine last described its processes, by tab.
    tab_memory: HashMap<u64, u64>,
    /// The site being typed into the keep-awake list, and why the last one could not be added.
    pub(crate) site_input: TextEdit,
    pub(crate) site_error: Option<&'static str>,
}

/// Memory sleeping tabs freed since Elaris started, across every window: the bytes known, and how
/// many tabs fell asleep.
static FREED_BYTES: AtomicU64 = AtomicU64::new(0);
static SLEPT_TABS: AtomicU64 = AtomicU64::new(0);

/// What Memory Saver has freed since Elaris started, as (bytes at least, tabs put to sleep).
pub(crate) fn freed() -> (u64, u64) {
    (
        FREED_BYTES.load(Ordering::Relaxed),
        SLEPT_TABS.load(Ordering::Relaxed),
    )
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
        // A tab's last figure is what its sleep freed; a tab never measured adds nothing.
        for id in &polled.slept {
            SLEPT_TABS.fetch_add(1, Ordering::Relaxed);
            if let Some(memory) = self.memory_saver.tab_memory.remove(&id.raw()) {
                FREED_BYTES.fetch_add(memory, Ordering::Relaxed);
            }
        }
    }

    /// Puts a tab to sleep now, as the user asked. Elaris's own reasons to keep it awake give way;
    /// Ladybird's, such as playing audio, still keep it loaded.
    pub(crate) fn sleep_now(&mut self, id: TabId) {
        self.browser.sleep_tab(id, true);
    }

    /// Why the tab stays awake: what it is doing, what the user chose, and what Ladybird last said.
    pub(crate) fn awake_reasons(&self, tab: &Tab) -> Vec<AwakeReason> {
        if tab.asleep || tab.is_internal() {
            return Vec::new();
        }
        let mut reasons = tab.awake_reasons();
        let mut add = |reason| {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        };
        if self.browser.private {
            add(AwakeReason::PrivateWindow);
        }
        if self.keeps_site_awake(tab) {
            add(AwakeReason::KeepAwakeSite);
        }
        if self.may_notify(tab) {
            add(AwakeReason::Notifications);
        }
        if tab.section() == TabSection::Favourite
            && self.settings.memory_saver == MemorySaver::Balanced
        {
            add(AwakeReason::Favourite);
        }
        for reason in &tab.kept_awake {
            add(*reason);
        }
        reasons
    }

    fn keeps_site_awake(&self, tab: &Tab) -> bool {
        let host = elaris_core::address::host_for_address(&tab.url);
        !host.is_empty() && elaris_memory_saver::keeps_awake(&self.settings.keep_awake_sites, host)
    }

    fn may_notify(&self, tab: &Tab) -> bool {
        PermissionStore::origin(&tab.url).is_some_and(|origin| {
            self.permission_store
                .state(&origin, PermissionKind::Notifications)
                == PermissionState::Allow
        })
    }

    /// Adds what the keep-awake field names to the list.
    pub(crate) fn add_keep_awake_site(&mut self) {
        let Some(site) = elaris_memory_saver::site_of(&self.memory_saver.site_input.text) else {
            self.memory_saver.site_error = Some("Enter a site, such as mail.example.com.");
            return;
        };
        self.memory_saver.site_error = None;
        self.memory_saver.site_input = TextEdit::default();
        if !self.settings.keep_awake_sites.contains(&site) {
            let mut sites = self.settings.keep_awake_sites.clone();
            sites.push(site);
            self.set_keep_awake_sites(sites);
        }
    }

    pub(crate) fn remove_keep_awake_site(&mut self, site: &str) {
        let sites = self
            .settings
            .keep_awake_sites
            .iter()
            .filter(|kept| *kept != site)
            .cloned()
            .collect();
        self.set_keep_awake_sites(sites);
    }

    /// Asks to put the tabs Memory Saver chooses to sleep when a check is due.
    pub(super) fn poll_memory_saver(&mut self, tx: &mut Tx) {
        let mode = self.settings.memory_saver;
        if mode == MemorySaver::Off {
            self.memory_saver.check_now();
            return;
        }
        let now = Instant::now();
        if self.memory_saver.check.is_some_and(|due| now < due) {
            return;
        }
        let pressure = Pressure::current();
        let Some(interval) = mode.check_interval(pressure) else {
            self.memory_saver.check = None;
            return;
        };
        self.memory_saver.check = Some(now + interval);
        tx.wake_after(interval);
        // For the next check, and for counting what sleep frees: by then the figures have arrived.
        self.request_tab_memory();
        let candidates: Vec<_> = self
            .browser
            .sleep_candidates(now)
            .into_iter()
            .map(|candidate| self.with_site_state(candidate))
            .collect();
        let memory_pressure = pressure != Pressure::Normal;
        for id in mode.choose(pressure, &candidates) {
            self.browser.sleep_tab(id, memory_pressure);
        }
        // The tabs fall asleep once Ladybird answers, which a later poll takes in.
    }

    fn request_tab_memory(&self) {
        // Only web pages hold memory to free, and the answer comes back to the tab that asked.
        if let Some(tab) = self
            .browser
            .tabs()
            .iter()
            .find(|tab| !tab.is_internal() && !tab.asleep)
        {
            self.browser.request_processes(tab.id());
        }
    }

    /// Adds what the browser alone cannot tell: the page's memory, whether its site may show
    /// notifications, which a sleeping page could not, and whether the user keeps it awake.
    fn with_site_state(&self, mut candidate: Candidate<TabId>) -> Candidate<TabId> {
        candidate.memory = self
            .memory_saver
            .tab_memory
            .get(&candidate.id.raw())
            .copied();
        if let Some(tab) = self.browser.tab(candidate.id) {
            candidate.busy |= self.may_notify(tab) || self.keeps_site_awake(tab);
        }
        candidate
    }
}
