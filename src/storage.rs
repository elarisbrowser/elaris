// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    test,
    expect(
        dead_code,
        reason = "App tests disable storage startup to keep tests out of user profiles"
    )
)]

//! Elaris-owned data paths, history worker, and versioned session files.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Once, OnceLock, RwLock};
use std::thread::JoinHandle;

use elaris::history::{Cursor, Entry, History, Page};

mod paths;
mod session;

pub(crate) use paths::{Paths, default_download_directory};
use session::write_sessions_atomic;
pub(crate) use session::{Session, SessionFavicon, SessionSplit, SessionTab};

use crate::config::{self, ConfigLoad};
use crate::permissions::{PermissionKind, PermissionState};
use crate::settings::SettingsChange;

const HISTORY_CACHE_LIMIT: usize = 100;
const HISTORY_SEARCH_CACHE_LIMIT: usize = 32;
const STORAGE_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
static WORKERS: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static REGISTER_EXIT: Once = Once::new();
static PROCESS_STORAGE: OnceLock<Result<Arc<Storage>, String>> = OnceLock::new();

#[derive(Default)]
struct HistorySnapshot {
    recent: Vec<Entry>,
    frequent: Vec<Entry>,
    searches: HashMap<String, Vec<Entry>>,
    revision: u64,
}

pub(crate) type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct QueueMetrics {
    queued: AtomicUsize,
    peak_queued: AtomicUsize,
    session_writes: AtomicU64,
    worst_session_latency_ns: AtomicU64,
    report: bool,
}

#[derive(Clone, Copy, Debug, Default)]
#[cfg(test)]
pub(crate) struct QueueMetricsSnapshot {
    pub(crate) queued: usize,
    pub(crate) peak_queued: usize,
    pub(crate) session_writes: u64,
    pub(crate) worst_session_latency: std::time::Duration,
}

impl QueueMetrics {
    fn enqueue(&self, sender: &Sender<Command>, command: Command) -> Result<(), ()> {
        let queued = self.queued.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_queued.fetch_max(queued, Ordering::Relaxed);
        if sender.send(command).is_err() {
            self.queued.fetch_sub(1, Ordering::Relaxed);
            return Err(());
        }
        Ok(())
    }

    fn dequeued(&self) {
        self.queued.fetch_sub(1, Ordering::Relaxed);
    }

    fn session_write_finished(&self, queued_at: std::time::Instant) {
        let writes = self.session_writes.fetch_add(1, Ordering::Relaxed) + 1;
        let latency = queued_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        self.worst_session_latency_ns
            .fetch_max(latency, Ordering::Relaxed);
        if self.report {
            eprintln!(
                "Elaris storage: session_write_attempts={writes} queued={} peak_queued={} worst_save_ms={:.2}",
                self.queued.load(Ordering::Relaxed),
                self.peak_queued.load(Ordering::Relaxed),
                self.worst_session_latency_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            );
        }
    }

    #[cfg(test)]
    fn snapshot(&self) -> QueueMetricsSnapshot {
        QueueMetricsSnapshot {
            queued: self.queued.load(Ordering::Relaxed),
            peak_queued: self.peak_queued.load(Ordering::Relaxed),
            session_writes: self.session_writes.load(Ordering::Relaxed),
            worst_session_latency: std::time::Duration::from_nanos(
                self.worst_session_latency_ns.load(Ordering::Relaxed),
            ),
        }
    }
}

#[derive(Default)]
struct SharedHistory {
    snapshot: RwLock<HistorySnapshot>,
    notify: OnceLock<Notify>,
    page_revisions: Mutex<HashMap<u64, u64>>,
    active_windows: Mutex<std::collections::HashSet<u64>>,
    history_events: Mutex<Vec<HistoryEvent>>,
}

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
    commands: Sender<Command>,
    metrics: Arc<QueueMetrics>,
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

pub(crate) struct Storage {
    session: Mutex<Option<Vec<Session>>>,
    download_history: Mutex<Option<Vec<elaris::downloads::Download>>>,
    config_load: Mutex<Option<Result<ConfigLoad, String>>>,
    commands: Sender<Command>,
    metrics: Arc<QueueMetrics>,
    history: HistoryHandle,
    config_events: Arc<Mutex<Vec<ConfigEvent>>>,
}

pub(crate) enum ConfigEvent {
    Saved(Result<(), String>),
    Reloaded(Result<ConfigLoad, String>),
}

impl Storage {
    pub(crate) fn open_shared(paths: Paths) -> io::Result<Arc<Self>> {
        match PROCESS_STORAGE
            .get_or_init(|| Self::open(paths).map(Arc::new).map_err(|e| e.to_string()))
        {
            Ok(storage) => Ok(Arc::clone(storage)),
            Err(error) => Err(io::Error::other(error.clone())),
        }
    }

    pub(crate) fn open(paths: Paths) -> io::Result<Self> {
        paths.create_directories()?;
        let config_load = Some(config::load_or_create(&paths.config_file));
        let session = match session::load_sessions(&paths.session_file) {
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
        let metrics = Arc::new(QueueMetrics {
            report: std::env::var_os("ELARIS_STORAGE_PERF").is_some(),
            ..QueueMetrics::default()
        });
        let shared = Arc::new(SharedHistory::default());
        let worker_shared = Arc::clone(&shared);
        let config_events = Arc::new(Mutex::new(Vec::new()));
        let worker_config_events = Arc::clone(&config_events);
        let worker_metrics = Arc::clone(&metrics);
        let worker_paths = paths.clone();
        let download_history = crate::download_history::load(&paths.download_history_file)
            .unwrap_or_else(|error| {
                eprintln!(
                    "Elaris: ignoring unreadable download history {}: {error}",
                    paths.download_history_file.display()
                );
                Vec::new()
            });
        let worker = std::thread::Builder::new()
            .name("elaris-storage".to_owned())
            .spawn(move || {
                run_worker(
                    receiver,
                    worker_shared,
                    worker_paths,
                    worker_config_events,
                    worker_metrics,
                );
            })?;
        register_worker(commands.clone(), Arc::clone(&metrics), worker);
        Ok(Self {
            session: Mutex::new(session),
            download_history: Mutex::new(Some(download_history)),
            config_load: Mutex::new(config_load),
            commands: commands.clone(),
            metrics: Arc::clone(&metrics),
            history: HistoryHandle {
                commands,
                metrics,
                shared,
            },
            config_events,
        })
    }

    /// The download history read at launch, once.
    pub(crate) fn take_download_history(&self) -> Option<Vec<elaris::downloads::Download>> {
        self.download_history
            .lock()
            .expect("download history lock poisoned")
            .take()
    }

    pub(crate) fn save_download_history(&self, encoded: String) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::SaveDownloads(encoded))
            .map_err(|_| "download history storage worker has stopped".to_owned())
    }

    pub(crate) fn take_sessions(&self) -> Option<Vec<Session>> {
        self.session.lock().expect("session lock poisoned").take()
    }

    pub(crate) fn history(&self) -> &HistoryHandle {
        &self.history
    }

    pub(crate) fn take_config_load(&self) -> Option<Result<ConfigLoad, String>> {
        self.config_load
            .lock()
            .expect("configuration load lock poisoned")
            .clone()
    }

    pub(crate) fn save_setting(&self, change: SettingsChange) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::SaveSetting(change))
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn save_permission(
        &self,
        origin: String,
        kind: PermissionKind,
        state: PermissionState,
    ) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::SavePermission(origin, kind, state))
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn save_autoplay(
        &self,
        origin: String,
        policy: Option<crate::permissions::AutoplayPolicy>,
    ) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::SaveAutoplay(origin, policy))
            .map_err(|_| "configuration storage worker has stopped".to_owned())
    }

    pub(crate) fn reload_configuration(&self) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::ReloadConfig)
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

    pub(crate) fn save_sessions_for_launch(
        &self,
        sessions: Vec<Session>,
        restore_on_launch: bool,
    ) -> Result<(), String> {
        self.metrics
            .enqueue(
                &self.commands,
                Command::SaveSessions(sessions, restore_on_launch, None, std::time::Instant::now()),
            )
            .map_err(|_| "session storage worker has stopped".to_owned())
    }

    /// Waits for the atomic replacement before an emptied window can close.
    pub(crate) fn commit_merged_session(&self, sessions: Vec<Session>) -> Result<(), String> {
        let (finished, pending) = mpsc::channel();
        self.metrics
            .enqueue(
                &self.commands,
                Command::SaveSessions(sessions, true, Some(finished), std::time::Instant::now()),
            )
            .map_err(|_| "session storage worker has stopped".to_owned())?;
        pending
            .recv()
            .map_err(|_| "session storage worker has stopped".to_owned())?
    }

    #[cfg(test)]
    pub(crate) fn save_session(&self, session: Session) -> Result<(), String> {
        self.save_sessions_for_launch(vec![session], true)
    }

    /// Give pending saves a chance to finish before the native host exits.
    pub(crate) fn flush_on_close(&self) -> Result<(), String> {
        let (finished, pending) = mpsc::channel();
        self.metrics
            .enqueue(&self.commands, Command::Flush(finished))
            .map_err(|_| "storage worker has stopped".to_owned())?;
        pending
            .recv_timeout(STORAGE_FLUSH_TIMEOUT)
            .map_err(|error| format!("waiting for pending storage writes: {error}"))
    }

    pub(crate) fn set_notify(&self, notify: Notify) {
        let _ = self.history.shared.notify.set(notify);
    }

    #[cfg(test)]
    pub(crate) fn queue_metrics(&self) -> QueueMetricsSnapshot {
        self.metrics.snapshot()
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        let _ = self.metrics.enqueue(&self.commands, Command::Shutdown);
        #[cfg(test)]
        {
            // Tests can remove their temporary profile as soon as Storage is dropped.
            let worker = {
                let mut workers = WORKERS
                    .lock()
                    .expect("storage worker registry lock poisoned");
                workers
                    .iter()
                    .position(|worker| Arc::ptr_eq(&worker.metrics, &self.metrics))
                    .map(|index| workers.swap_remove(index))
            };
            if let Some(worker) = worker {
                worker.thread.join().expect("storage worker completes");
            }
        }
    }
}

enum Command {
    RecordVisit(String, String),
    UpdateTitle(String, String),
    Search(String),
    HistoryPage(u64, u64, String, Option<Cursor>),
    HistoryMutation(u64, HistoryMutation),
    SaveSessions(
        Vec<Session>,
        bool,
        Option<Sender<Result<(), String>>>,
        std::time::Instant,
    ),
    SaveSetting(SettingsChange),
    SavePermission(String, PermissionKind, PermissionState),
    SaveAutoplay(String, Option<crate::permissions::AutoplayPolicy>),
    /// The download history as `download_history::encode` wrote it.
    SaveDownloads(String),
    ReloadConfig,
    Flush(Sender<()>),
    Shutdown,
}

struct Worker {
    commands: Sender<Command>,
    metrics: Arc<QueueMetrics>,
    thread: JoinHandle<()>,
}

fn register_worker(commands: Sender<Command>, metrics: Arc<QueueMetrics>, thread: JoinHandle<()>) {
    WORKERS
        .lock()
        .expect("storage worker registry lock poisoned")
        .push(Worker {
            commands,
            metrics,
            thread,
        });
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
        let _ = worker.metrics.enqueue(&worker.commands, Command::Shutdown);
    }
    for worker in workers {
        let _ = worker.thread.join();
    }
}

fn run_worker(
    receiver: mpsc::Receiver<Command>,
    shared: Arc<SharedHistory>,
    paths: Paths,
    config_events: Arc<Mutex<Vec<ConfigEvent>>>,
    metrics: Arc<QueueMetrics>,
) {
    let Paths {
        history_file,
        session_file,
        config_file,
        download_history_file,
        ..
    } = paths;
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
    let mut pending_command = None;
    loop {
        let command = if let Some(command) = pending_command.take() {
            metrics.dequeued();
            command
        } else {
            match receiver.recv() {
                Ok(command) => {
                    metrics.dequeued();
                    command
                }
                Err(_) => break,
            }
        };
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
            Command::HistoryPage(window, revision, query, after) => {
                if shared
                    .page_revisions
                    .lock()
                    .expect("history page revisions poisoned")
                    .get(&window)
                    != Some(&revision)
                {
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
                if shared
                    .page_revisions
                    .lock()
                    .expect("history page revisions poisoned")
                    .get(&window)
                    == Some(&revision)
                {
                    publish_history(&shared, HistoryEvent::Page(window, revision, result));
                }
            }
            Command::HistoryMutation(window, mutation) => {
                let result = history.as_ref().and_then(|history| {
                    let result = match &mutation {
                        HistoryMutation::Remove(urls) => history.remove_all(urls),
                        HistoryMutation::Clear => history.clear(),
                        HistoryMutation::ClearSince(since) => history.clear_since(*since),
                    };
                    if result.is_ok() {
                        refresh_history(history, &shared);
                    }
                    result.ok()
                });
                let message = match mutation {
                    HistoryMutation::Remove(_) => "Could not remove these addresses from history.",
                    HistoryMutation::Clear | HistoryMutation::ClearSince(_) => {
                        "Could not clear browsing history."
                    }
                };
                let active = shared
                    .active_windows
                    .lock()
                    .expect("history window registry poisoned")
                    .contains(&window);
                if active {
                    publish_history(
                        &shared,
                        HistoryEvent::Mutation(
                            window,
                            result.map(|()| mutation).ok_or_else(|| message.to_owned()),
                        ),
                    );
                }
            }
            Command::SaveSessions(sessions, restore_on_launch, None, queued_at) => {
                // Ordinary snapshots supersede adjacent snapshots. Keep the latest complete
                // multi-window state, but stop at every other storage command and at explicit
                // acknowledgements so ordering and transfer commit semantics remain intact.
                let mut latest = (sessions, restore_on_launch, queued_at);
                while let Ok(command) = receiver.try_recv() {
                    match command {
                        Command::SaveSessions(sessions, restore_on_launch, None, queued_at) => {
                            metrics.dequeued();
                            latest = (sessions, restore_on_launch, queued_at);
                        }
                        command => {
                            pending_command = Some(command);
                            break;
                        }
                    }
                }
                if let Err(error) = write_sessions_atomic(&session_file, &latest.0, latest.1)
                    .map_err(|error| format!("cannot save session: {error}"))
                {
                    eprintln!("Elaris: {error}");
                }
                metrics.session_write_finished(latest.2);
            }
            Command::SaveSessions(sessions, restore_on_launch, Some(finished), queued_at) => {
                let result = write_sessions_atomic(&session_file, &sessions, restore_on_launch)
                    .map_err(|error| format!("cannot save session: {error}"));
                metrics.session_write_finished(queued_at);
                let _ = finished.send(result);
            }
            Command::SaveSetting(change) => {
                let result = config::write_change(&config_file, &change);
                config_events
                    .lock()
                    .expect("configuration event lock poisoned")
                    .push(ConfigEvent::Saved(result));
                notify(&shared);
            }
            Command::SaveDownloads(encoded) => {
                if let Err(error) = session::write_private_atomic(
                    &download_history_file,
                    encoded.as_bytes(),
                    "downloads",
                ) {
                    eprintln!("Elaris: could not save download history: {error}");
                }
            }
            Command::SaveAutoplay(origin, policy) => {
                let result = config::write_autoplay(&config_file, &origin, policy);
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

fn update_search_cache(history: Option<&History>, shared: &SharedHistory, query: String) {
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

fn notify(shared: &SharedHistory) {
    if let Some(notify) = shared.notify.get() {
        notify();
    }
}

#[cfg(test)]
mod tests;
