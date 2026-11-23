// SPDX-License-Identifier: GPL-3.0-only

//! The storage worker thread: its command protocol, registry, and command loop.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Once};
use std::thread::JoinHandle;

use elaris_core::config;
use elaris_core::history::{Cursor, History};
use elaris_core::paths::{Paths, display_path};
use elaris_core::permissions::{PermissionKind, PermissionState};
use elaris_core::settings::SettingsChange;

use super::history::{HistoryEvent, HistoryMutation};
use super::history_cache::{
    SharedHistory, notify, publish_history, refresh_history, update_search_cache,
};
use super::queue::QueueMetrics;
use super::session::{self, Session, write_sessions_atomic};
use super::store::ConfigEvent;

pub(super) static WORKERS: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static REGISTER_EXIT: Once = Once::new();

pub(super) enum Command {
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
    SaveAutoplay(String, Option<elaris_core::permissions::AutoplayPolicy>),
    /// The download history as `download_history::encode` wrote it.
    SaveDownloads(String),
    /// The bookmarks as `Bookmarks::encode` wrote them.
    SaveBookmarks(String),
    ReloadConfig,
    Flush(Sender<()>),
    Shutdown,
}

pub(super) struct Worker {
    commands: Sender<Command>,
    pub(super) metrics: Arc<QueueMetrics>,
    pub(super) thread: JoinHandle<()>,
}

pub(super) fn register_worker(
    commands: Sender<Command>,
    metrics: Arc<QueueMetrics>,
    thread: JoinHandle<()>,
) {
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

pub(super) fn run_worker(
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
        bookmarks_file,
        ..
    } = paths;
    let mut history = match History::open(&history_file) {
        Ok(history) => Some(history),
        Err(error) => {
            eprintln!(
                "Elaris: cannot open history {}: {error}",
                display_path(&history_file)
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
            Command::SaveBookmarks(encoded) => {
                if let Err(error) =
                    session::write_private_atomic(&bookmarks_file, encoded.as_bytes(), "bookmarks")
                {
                    eprintln!("Elaris: could not save bookmarks: {error}");
                }
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
