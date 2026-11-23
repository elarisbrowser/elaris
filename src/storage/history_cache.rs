// SPDX-License-Identifier: GPL-3.0-only

//! The history snapshot shared with the app, and the helpers that refresh it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use elaris_core::history::{Entry, History};

use super::history::HistoryEvent;

const HISTORY_CACHE_LIMIT: usize = 100;

#[derive(Default)]
pub(super) struct HistorySnapshot {
    pub(super) recent: Vec<Entry>,
    pub(super) frequent: Vec<Entry>,
    pub(super) searches: HashMap<String, Vec<Entry>>,
    pub(super) revision: u64,
}

pub(super) type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub(super) struct SharedHistory {
    pub(super) snapshot: RwLock<HistorySnapshot>,
    pub(super) notify: OnceLock<Notify>,
    pub(super) page_revisions: Mutex<HashMap<u64, u64>>,
    pub(super) active_windows: Mutex<std::collections::HashSet<u64>>,
    pub(super) history_events: Mutex<Vec<HistoryEvent>>,
}

pub(super) fn publish_history(shared: &SharedHistory, event: HistoryEvent) {
    shared
        .history_events
        .lock()
        .expect("history events lock poisoned")
        .push(event);
    notify(shared);
}

pub(super) fn refresh_history(history: &History, shared: &SharedHistory) {
    let recent = history.recent(HISTORY_CACHE_LIMIT).unwrap_or_else(|error| {
        eprintln!("Elaris: cannot read recent history: {error}");
        Vec::new()
    });
    let frequent = history
        .frequent(HISTORY_CACHE_LIMIT)
        .unwrap_or_else(|error| {
            eprintln!("Elaris: cannot read frequent history: {error}");
            Vec::new()
        });
    let queries: Vec<String> = shared
        .snapshot
        .read()
        .expect("history snapshot lock poisoned")
        .searches
        .keys()
        .cloned()
        .collect();
    let searches: HashMap<String, Vec<Entry>> = queries
        .into_iter()
        .map(|query| {
            let matches = history
                .search(&query, HISTORY_CACHE_LIMIT)
                .unwrap_or_else(|error| {
                    eprintln!("Elaris: cannot search history: {error}");
                    Vec::new()
                });
            (query, matches)
        })
        .collect();
    {
        let mut snapshot = shared
            .snapshot
            .write()
            .expect("history snapshot lock poisoned");
        snapshot.recent = recent;
        snapshot.frequent = frequent;
        snapshot.searches = searches;
        snapshot.revision = snapshot.revision.wrapping_add(1);
    }
    notify(shared);
}

pub(super) fn update_search_cache(
    history: Option<&History>,
    shared: &SharedHistory,
    query: String,
) {
    let matches = history
        .and_then(|history| history.search(&query, HISTORY_CACHE_LIMIT).ok())
        .unwrap_or_default();
    let mut snapshot = shared
        .snapshot
        .write()
        .expect("history snapshot lock poisoned");
    snapshot.searches.insert(query, matches);
    snapshot.revision = snapshot.revision.wrapping_add(1);
    drop(snapshot);
    notify(shared);
}

pub(super) fn notify(shared: &SharedHistory) {
    if let Some(notify) = shared.notify.get() {
        notify();
    }
}
