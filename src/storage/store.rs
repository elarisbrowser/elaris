// SPDX-License-Identifier: GPL-3.0-only

//! The `Storage` handle the app uses to read launch state and queue asynchronous saves.

use std::io;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use elaris_core::config::{self, ConfigLoad};
use elaris_core::paths::{Paths, display_path};
use elaris_core::permissions::{PermissionKind, PermissionState};
use elaris_core::settings::SettingsChange;

use super::history::HistoryHandle;
use super::history_cache::{Notify, SharedHistory};
use super::queue::QueueMetrics;
#[cfg(test)]
use super::queue::QueueMetricsSnapshot;
use super::session::{self, Session};
#[cfg(test)]
use super::worker::WORKERS;
use super::worker::{Command, register_worker, run_worker};

const STORAGE_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
static PROCESS_STORAGE: OnceLock<Result<Arc<Storage>, String>> = OnceLock::new();

pub(crate) struct Storage {
    session: Mutex<Option<Vec<Session>>>,
    download_history: Mutex<Option<Vec<elaris_core::downloads::Download>>>,
    bookmarks: Mutex<Option<elaris_core::bookmarks::Bookmarks>>,
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
                    display_path(&paths.session_file)
                );
                None
            }
        };
        let (commands, receiver) = mpsc::channel();
        let metrics = Arc::new(QueueMetrics {
            report: elaris_core::dev::var("ELARIS_STORAGE_PERF").is_some(),
            ..QueueMetrics::default()
        });
        let shared = Arc::new(SharedHistory::default());
        let worker_shared = Arc::clone(&shared);
        let config_events = Arc::new(Mutex::new(Vec::new()));
        let worker_config_events = Arc::clone(&config_events);
        let worker_metrics = Arc::clone(&metrics);
        let worker_paths = paths.clone();
        let download_history = elaris_core::download_history::load(&paths.download_history_file)
            .unwrap_or_else(|error| {
                eprintln!(
                    "Elaris: ignoring unreadable download history {}: {error}",
                    display_path(&paths.download_history_file)
                );
                Vec::new()
            });
        let bookmarks = load_bookmarks(&paths.bookmarks_file);
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
            bookmarks: Mutex::new(Some(bookmarks)),
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
    pub(crate) fn take_download_history(&self) -> Option<Vec<elaris_core::downloads::Download>> {
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

    /// The bookmarks read at launch, once.
    pub(crate) fn take_bookmarks(&self) -> Option<elaris_core::bookmarks::Bookmarks> {
        self.bookmarks
            .lock()
            .expect("bookmarks lock poisoned")
            .take()
    }

    pub(crate) fn save_bookmarks(&self, encoded: String) -> Result<(), String> {
        self.metrics
            .enqueue(&self.commands, Command::SaveBookmarks(encoded))
            .map_err(|_| "bookmarks storage worker has stopped".to_owned())
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
        policy: Option<elaris_core::permissions::AutoplayPolicy>,
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

/// Reads the bookmarks file. One that can't be read is moved aside rather than overwritten, so
/// nothing the user saved is lost to an empty list.
fn load_bookmarks(path: &std::path::Path) -> elaris_core::bookmarks::Bookmarks {
    elaris_core::bookmarks::Bookmarks::load(path).unwrap_or_else(|error| {
        let aside = path.with_file_name("bookmarks.unreadable.json");
        match std::fs::rename(path, &aside) {
            Ok(()) => eprintln!(
                "Elaris: bookmarks {} could not be read ({error}); kept as {}",
                display_path(path),
                display_path(&aside)
            ),
            Err(rename) => eprintln!(
                "Elaris: bookmarks {} could not be read ({error}) or kept aside ({rename})",
                display_path(path)
            ),
        }
        elaris_core::bookmarks::Bookmarks::default()
    })
}
