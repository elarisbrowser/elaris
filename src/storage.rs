// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    test,
    expect(
        dead_code,
        reason = "App tests disable storage startup to keep tests out of user profiles"
    )
)]

//! Elaris-owned data paths, history worker, and versioned session files.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Once, OnceLock, RwLock};
use std::thread::JoinHandle;

use elaris::history::{Cursor, Entry, History, Page};

mod paths;
mod session;

pub(crate) use paths::{Paths, default_download_directory};
use session::write_session_atomic;
pub(crate) use session::{Session, SessionFavicon, SessionSplit, SessionTab};

use crate::config::{self, ConfigLoad};
use crate::permissions::{PermissionKind, PermissionState};
use crate::settings::SettingsChange;

const HISTORY_CACHE_LIMIT: usize = 100;
static WORKERS: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static REGISTER_EXIT: Once = Once::new();

#[derive(Default)]
struct HistorySnapshot {
    recent: Vec<Entry>,
    frequent: Vec<Entry>,
    query: Option<String>,
    matches: Vec<Entry>,
    revision: u64,
}

pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct SharedHistory {
    snapshot: RwLock<HistorySnapshot>,
    notify: OnceLock<Notify>,
    page_revision: AtomicU64,
    history_events: Mutex<Vec<HistoryEvent>>,
}

#[derive(Debug)]
pub(crate) enum HistoryEvent {
    Page(u64, Result<Page, String>),
    Mutation(Result<HistoryMutation, String>),
}

#[derive(Debug)]
pub(crate) enum HistoryMutation {
    Remove(String),
    Clear,
}

pub(crate) struct HistoryHandle {
    commands: Sender<Command>,
    shared: Arc<SharedHistory>,
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
        if snapshot.query.as_deref() != Some(query.trim()) {
            return Vec::new();
        }
        snapshot.matches.iter().take(limit).cloned().collect()
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
        revision: u64,
        query: &str,
        after: Option<Cursor>,
    ) -> Result<(), String> {
        self.shared.page_revision.store(revision, Ordering::Relaxed);
        self.send(Command::HistoryPage(revision, query.to_owned(), after))
    }

    pub(crate) fn mutate(&self, mutation: HistoryMutation) -> Result<(), String> {
        self.send(Command::HistoryMutation(mutation))
    }

    pub(crate) fn take_events(&self) -> Vec<HistoryEvent> {
        std::mem::take(
            &mut *self
                .shared
                .history_events
                .lock()
                .expect("history events lock poisoned"),
        )
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "history storage worker has stopped".to_owned())
    }
}

pub(crate) struct Storage {
    session: Option<Session>,
    config_load: Option<Result<ConfigLoad, String>>,
    commands: Sender<Command>,
    history: HistoryHandle,
    config_events: Arc<Mutex<Vec<ConfigEvent>>>,
}

pub(crate) enum ConfigEvent {
    Saved(Result<(), String>),
    Reloaded(Result<ConfigLoad, String>),
}

impl Storage {
    pub(crate) fn open(paths: Paths) -> io::Result<Self> {
        paths.create_directories()?;
        let config_load = Some(config::load_or_create(&paths.config_file));
        let session = match session::load_session(&paths.session_file) {
            Ok(session) => session,
            Err(error) => {
                eprintln!(
                    "Elaris: ignoring unreadable session {}: {error}",
                    paths.session_file.display()
                );
                None
            }
        };
        let (commands, receiver) = mpsc::channel();
        let shared = Arc::new(SharedHistory::default());
        let worker_shared = Arc::clone(&shared);
        let config_events = Arc::new(Mutex::new(Vec::new()));
        let worker_config_events = Arc::clone(&config_events);
        let history_file = paths.history_file.clone();
        let session_file = paths.session_file.clone();
        let config_file = paths.config_file.clone();
        let worker = std::thread::Builder::new()
            .name("elaris-storage".to_owned())
            .spawn(move || {
                run_worker(
                    receiver,
                    worker_shared,
                    history_file,
                    session_file,
                    config_file,
                    worker_config_events,
                );
            })?;
        register_worker(commands.clone(), worker);
        Ok(Self {
            session,
            config_load,
            commands: commands.clone(),
            history: HistoryHandle { commands, shared },
            config_events,
        })
    }

    pub(crate) fn take_session(&mut self) -> Option<Session> {
        self.session.take()
    }

    pub(crate) fn history(&self) -> &HistoryHandle {
        &self.history
    }

    pub(crate) fn take_config_load(&mut self) -> Option<Result<ConfigLoad, String>> {
        self.config_load.take()
    }

    pub(crate) fn save_setting(&self, change: SettingsChange) -> Result<(), String> {
        self.commands
            .send(Command::SaveSetting(change))
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn save_permission(
        &self,
        origin: String,
        kind: PermissionKind,
        state: PermissionState,
    ) -> Result<(), String> {
        self.commands
            .send(Command::SavePermission(origin, kind, state))
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn reload_configuration(&self) -> Result<(), String> {
        self.commands
            .send(Command::ReloadConfig)
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn take_config_events(&self) -> Vec<ConfigEvent> {
        std::mem::take(
            &mut *self
                .config_events
                .lock()
                .expect("configuration event lock poisoned"),
        )
    }

    pub(crate) fn save_session(&self, session: Session) -> Result<(), String> {
        self.commands
            .send(Command::SaveSession(session))
            .map_err(|_| "session storage worker has stopped".to_owned())
    }

    /// Give pending saves a chance to finish before the native host exits.
    pub(crate) fn flush_on_close(&self) -> Result<(), String> {
        let (finished, pending) = mpsc::channel();
        self.commands
            .send(Command::Flush(finished))
            .map_err(|_| "storage worker has stopped".to_owned())?;
        pending
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|error| format!("waiting for pending storage writes: {error}"))
    }

    pub(crate) fn set_notify(&self, notify: Notify) {
        let _ = self.history.shared.notify.set(notify);
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

enum Command {
    RecordVisit(String, String),
    UpdateTitle(String, String),
    Search(String),
    HistoryPage(u64, String, Option<Cursor>),
    HistoryMutation(HistoryMutation),
    SaveSession(Session),
    SaveSetting(SettingsChange),
    SavePermission(String, PermissionKind, PermissionState),
    ReloadConfig,
    Flush(Sender<()>),
    Shutdown,
}

struct Worker {
    commands: Sender<Command>,
    thread: JoinHandle<()>,
}

fn register_worker(commands: Sender<Command>, thread: JoinHandle<()>) {
    WORKERS
        .lock()
        .expect("storage worker registry lock poisoned")
        .push(Worker { commands, thread });
    REGISTER_EXIT.call_once(|| {
        // SAFETY: the callback is a process-lifetime function with no captured state.
        unsafe {
            atexit(finish_workers_at_exit);
        }
    });
}

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> i32;
}

extern "C" fn finish_workers_at_exit() {
    let workers = std::mem::take(
        &mut *WORKERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for worker in &workers {
        let _ = worker.commands.send(Command::Shutdown);
    }
    for worker in workers {
        let _ = worker.thread.join();
    }
}

fn run_worker(
    receiver: mpsc::Receiver<Command>,
    shared: Arc<SharedHistory>,
    history_file: PathBuf,
    session_file: PathBuf,
    config_file: PathBuf,
    config_events: Arc<Mutex<Vec<ConfigEvent>>>,
) {
    let mut history = match History::open(&history_file) {
        Ok(history) => Some(history),
        Err(error) => {
            eprintln!(
                "Elaris: cannot open history {}: {error}",
                history_file.display()
            );
            None
        }
    };
    if let Some(history) = &history {
        refresh_history(history, &shared);
    }
    while let Ok(command) = receiver.recv() {
        match command {
            Command::RecordVisit(url, title) => {
                if let Some(history) = &history {
                    if let Err(error) = history.record_visit(&url, &title) {
                        eprintln!("Elaris: cannot record history visit: {error}");
                    }
                    refresh_history(history, &shared);
                }
            }
            Command::UpdateTitle(url, title) => {
                if let Some(history) = &history {
                    if let Err(error) = history.update_title(&url, &title) {
                        eprintln!("Elaris: cannot update history title: {error}");
                    }
                    refresh_history(history, &shared);
                }
            }
            Command::Search(query) => {
                update_search_cache(history.as_ref(), &shared, query);
            }
            Command::HistoryPage(revision, query, after) => {
                if revision != shared.page_revision.load(Ordering::Relaxed) {
                    continue;
                }
                if history.is_none() {
                    history = History::open(&history_file).ok();
                    if let Some(history) = &history {
                        refresh_history(history, &shared);
                    }
                }
                let result = history
                    .as_ref()
                    .and_then(|history| history.page(&query, after.as_ref()).ok())
                    .ok_or_else(|| "Could not load browsing history.".to_owned());
                if revision == shared.page_revision.load(Ordering::Relaxed) {
                    publish_history(&shared, HistoryEvent::Page(revision, result));
                }
            }
            Command::HistoryMutation(mutation) => {
                let result = history.as_ref().and_then(|history| {
                    let result = match &mutation {
                        HistoryMutation::Remove(url) => history.remove(url).map(|_| ()),
                        HistoryMutation::Clear => history.clear(),
                    };
                    if result.is_ok() {
                        refresh_history(history, &shared);
                    }
                    result.ok()
                });
                let message = match mutation {
                    HistoryMutation::Remove(_) => "Could not remove this address from history.",
                    HistoryMutation::Clear => "Could not clear browsing history.",
                };
                publish_history(
                    &shared,
                    HistoryEvent::Mutation(
                        result.map(|()| mutation).ok_or_else(|| message.to_owned()),
                    ),
                );
            }
            Command::SaveSession(session) => {
                if let Err(error) = write_session_atomic(&session_file, &session) {
                    eprintln!("Elaris: cannot save session: {error}");
                }
            }
            Command::SaveSetting(change) => {
                let result = config::write_change(&config_file, &change);
                config_events
                    .lock()
                    .expect("configuration event lock poisoned")
                    .push(ConfigEvent::Saved(result));
                notify(&shared);
            }
            Command::SavePermission(origin, kind, state) => {
                let result = config::write_permission(&config_file, &origin, kind, state);
                config_events
                    .lock()
                    .expect("configuration event lock poisoned")
                    .push(ConfigEvent::Saved(result));
                notify(&shared);
            }
            Command::ReloadConfig => {
                let result = config::load_or_create(&config_file);
                config_events
                    .lock()
                    .expect("configuration event lock poisoned")
                    .push(ConfigEvent::Reloaded(result));
                notify(&shared);
            }
            Command::Flush(finished) => {
                let _ = finished.send(());
            }
            Command::Shutdown => break,
        }
    }
}

fn publish_history(shared: &SharedHistory, event: HistoryEvent) {
    shared
        .history_events
        .lock()
        .expect("history events lock poisoned")
        .push(event);
    notify(shared);
}

fn refresh_history(history: &History, shared: &SharedHistory) {
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
    let query = shared
        .snapshot
        .read()
        .expect("history snapshot lock poisoned")
        .query
        .clone();
    let matches = query.as_ref().map_or_else(Vec::new, |query| {
        history
            .search(query, HISTORY_CACHE_LIMIT)
            .unwrap_or_else(|error| {
                eprintln!("Elaris: cannot search history: {error}");
                Vec::new()
            })
    });
    {
        let mut snapshot = shared
            .snapshot
            .write()
            .expect("history snapshot lock poisoned");
        snapshot.recent = recent;
        snapshot.frequent = frequent;
        snapshot.matches = matches;
        snapshot.revision = snapshot.revision.wrapping_add(1);
    }
    notify(shared);
}

fn update_search_cache(history: Option<&History>, shared: &SharedHistory, query: String) {
    let matches = history
        .and_then(|history| history.search(&query, HISTORY_CACHE_LIMIT).ok())
        .unwrap_or_default();
    let mut snapshot = shared
        .snapshot
        .write()
        .expect("history snapshot lock poisoned");
    snapshot.query = Some(query);
    snapshot.matches = matches;
    snapshot.revision = snapshot.revision.wrapping_add(1);
    drop(snapshot);
    notify(shared);
}

fn notify(shared: &SharedHistory) {
    if let Some(notify) = shared.notify.get() {
        notify();
    }
}

#[cfg(test)]
mod tests;
