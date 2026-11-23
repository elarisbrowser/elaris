// SPDX-License-Identifier: GPL-3.0-only

//! Browsing history events, requests, and the handle that reads cached results.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use elaris_core::history::{Cursor, Entry, Page};

use super::history_cache::SharedHistory;
use super::queue::QueueMetrics;
use super::worker::Command;

const HISTORY_SEARCH_CACHE_LIMIT: usize = 32;

#[derive(Debug)]
pub(crate) enum HistoryEvent {
    Page(u64, u64, Result<Page, String>),
    Mutation(u64, Result<HistoryMutation, String>),
}

#[derive(Debug)]
pub(crate) enum HistoryMutation {
    /// Removes every visit to each of these addresses.
    Remove(Vec<String>),
    Clear,
    /// Removes entries last visited at or after this many microseconds since the Unix epoch.
    ClearSince(i64),
}

pub(crate) struct HistoryHandle {
    pub(super) commands: Sender<Command>,
    pub(super) metrics: Arc<QueueMetrics>,
    pub(super) shared: Arc<SharedHistory>,
}

impl HistoryHandle {
    pub(crate) fn record_visit(&self, url: &str, title: &str) -> Result<(), String> {
        self.send(Command::RecordVisit(url.to_owned(), title.to_owned()))
    }

    pub(crate) fn update_title(&self, url: &str, title: &str) -> Result<(), String> {
        self.send(Command::UpdateTitle(url.to_owned(), title.to_owned()))
    }

    pub(crate) fn request_search(&self, query: &str) -> Result<(), String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }
        let mut snapshot = self
            .shared
            .snapshot
            .write()
            .expect("history snapshot lock poisoned");
        if snapshot.searches.len() >= HISTORY_SEARCH_CACHE_LIMIT
            && !snapshot.searches.contains_key(query)
            && let Some(oldest) = snapshot.searches.keys().next().cloned()
        {
            snapshot.searches.remove(&oldest);
        }
        drop(snapshot);
        self.send(Command::Search(query.to_owned()))
    }

    pub(crate) fn recent(&self, limit: usize) -> Vec<Entry> {
        self.shared
            .snapshot
            .read()
            .expect("history snapshot lock poisoned")
            .recent
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    pub(crate) fn frequent(&self, limit: usize) -> Vec<Entry> {
        self.shared
            .snapshot
            .read()
            .expect("history snapshot lock poisoned")
            .frequent
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    pub(crate) fn search(&self, query: &str, limit: usize) -> Vec<Entry> {
        let snapshot = self
            .shared
            .snapshot
            .read()
            .expect("history snapshot lock poisoned");
        snapshot
            .searches
            .get(query.trim())
            .into_iter()
            .flatten()
            .take(limit)
            .cloned()
            .collect()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.shared
            .snapshot
            .read()
            .expect("history snapshot lock poisoned")
            .revision
    }

    pub(crate) fn request_page(
        &self,
        window: u64,
        revision: u64,
        query: &str,
        after: Option<Cursor>,
    ) -> Result<(), String> {
        self.shared
            .page_revisions
            .lock()
            .expect("history page revisions poisoned")
            .insert(window, revision);
        self.shared
            .active_windows
            .lock()
            .expect("history window registry poisoned")
            .insert(window);
        self.send(Command::HistoryPage(
            window,
            revision,
            query.to_owned(),
            after,
        ))
    }

    pub(crate) fn mutate(&self, window: u64, mutation: HistoryMutation) -> Result<(), String> {
        self.shared
            .active_windows
            .lock()
            .expect("history window registry poisoned")
            .insert(window);
        self.send(Command::HistoryMutation(window, mutation))
    }

    pub(crate) fn forget_window(&self, window: u64) {
        self.shared
            .page_revisions
            .lock()
            .expect("history page revisions poisoned")
            .remove(&window);
        self.shared
            .active_windows
            .lock()
            .expect("history window registry poisoned")
            .remove(&window);
        self.shared
            .history_events
            .lock()
            .expect("history events lock poisoned")
            .retain(|event| match event {
                HistoryEvent::Page(owner, _, _) | HistoryEvent::Mutation(owner, _) => {
                    *owner != window
                }
            });
    }

    pub(crate) fn take_events(&self, window: u64) -> Vec<HistoryEvent> {
        let mut events = self
            .shared
            .history_events
            .lock()
            .expect("history events lock poisoned");
        let mut owned = Vec::new();
        let mut remaining = Vec::new();
        for event in events.drain(..) {
            let event_window = match &event {
                HistoryEvent::Page(owner, _, _) | HistoryEvent::Mutation(owner, _) => *owner,
            };
            if event_window == window {
                owned.push(event);
            } else {
                remaining.push(event);
            }
        }
        *events = remaining;
        owned
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, command)
            .map_err(|_| "history storage worker has stopped".to_owned())
    }
}
