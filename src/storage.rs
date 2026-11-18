// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    test,
    expect(
        dead_code,
        reason = "App tests disable storage startup to keep tests out of user profiles"
    )
)]

//! Elaris-owned data paths, history worker, and versioned session files.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Once, OnceLock, RwLock};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};

use elaris::history::{Cursor, Entry, History, Page};

use crate::config::{self, ConfigLoad};
use crate::settings::SettingsChange;

const SESSION_VERSION: u8 = 2;
const HISTORY_CACHE_LIMIT: usize = 100;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);
static WORKERS: Mutex<Vec<Worker>> = Mutex::new(Vec::new());
static REGISTER_EXIT: Once = Once::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paths {
    pub(crate) application_dir: PathBuf,
    pub(crate) profile_dir: PathBuf,
    pub(crate) history_file: PathBuf,
    pub(crate) session_file: PathBuf,
    pub(crate) config_file: PathBuf,
}

impl Paths {
    pub(crate) fn resolve() -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let data_dir = std::env::var_os("ELARIS_DATA_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let explicit_profile_dir = std::env::var_os("ELARIS_LADYBIRD_PROFILE_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let platform = Platform::MacOs;
        #[cfg(not(target_os = "macos"))]
        let platform = Platform::Linux;

        Self::resolve_with(
            platform,
            data_dir.as_deref(),
            home.as_deref(),
            xdg_data_home.as_deref(),
            xdg_config_home.as_deref(),
            explicit_profile_dir.as_deref(),
            cfg!(debug_assertions),
        )
    }

    fn resolve_with(
        platform: Platform,
        data_dir: Option<&Path>,
        home: Option<&Path>,
        xdg_data_home: Option<&Path>,
        xdg_config_home: Option<&Path>,
        explicit_profile_dir: Option<&Path>,
        development: bool,
    ) -> io::Result<Self> {
        let application_dir = if let Some(data_dir) = data_dir {
            if !data_dir.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ELARIS_DATA_DIR must be an absolute path",
                ));
            }
            data_dir.to_path_buf()
        } else {
            application_dir(platform, home, xdg_data_home, development).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "cannot locate Elaris data directory",
                )
            })?
        };
        let profile_dir = profile_directory(&application_dir, explicit_profile_dir);
        let config_dir = if data_dir.is_some() {
            application_dir.clone()
        } else {
            config_directory(platform, home, xdg_config_home, development).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "cannot locate Elaris configuration directory",
                )
            })?
        };
        Ok(Self {
            history_file: application_dir.join("history.sqlite"),
            session_file: application_dir.join("session.json"),
            config_file: config_dir.join("config.toml"),
            application_dir,
            profile_dir,
        })
    }

    fn create_directories(&self) -> io::Result<()> {
        create_private_dir(&self.application_dir)?;
        create_private_dir(&self.profile_dir)?;
        if let Some(parent) = self.config_file.parent() {
            create_private_dir(parent)?;
        }
        Ok(())
    }
}

pub(crate) fn default_download_directory() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    #[cfg(target_os = "linux")]
    if let Some(path) = xdg_download_directory(
        &home,
        std::env::var_os("XDG_DOWNLOAD_DIR").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .as_deref(),
    ) {
        return path;
    }
    home.join("Downloads")
}

#[cfg(target_os = "linux")]
fn xdg_download_directory(
    home: &Path,
    environment_path: Option<&std::ffi::OsStr>,
    config_home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = environment_path.map(PathBuf::from)
        && path.is_absolute()
    {
        return Some(path);
    }
    let default_config_home = home.join(".config");
    let config_home = config_home
        .filter(|path| path.is_absolute())
        .unwrap_or(&default_config_home);
    let contents = fs::read_to_string(config_home.join("user-dirs.dirs")).ok()?;
    contents
        .lines()
        .filter_map(|line| parse_xdg_download_directory(line, home))
        .next_back()
}

#[cfg(any(target_os = "linux", test))]
fn parse_xdg_download_directory(line: &str, home: &Path) -> Option<PathBuf> {
    let value = line
        .strip_prefix("XDG_DOWNLOAD_DIR")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .strip_prefix('"')?;
    let (prefix, rest) = value
        .strip_prefix("$HOME")
        .map(|rest| (Some(home), rest))
        .or_else(|| value.starts_with('/').then_some((None, value)))?;
    let mut path = prefix
        .map(|home| home.as_os_str().to_os_string())
        .unwrap_or_default();
    let mut chars = rest.chars();
    let mut closed = false;
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                closed = true;
                break;
            }
            '\\' => match chars.clone().next() {
                Some(next @ ('$' | '`' | '"' | '\\')) => {
                    path.push(next.to_string());
                    chars.next();
                }
                _ => path.push("\\"),
            },
            other => path.push(other.to_string()),
        }
    }
    let trailing = chars.as_str().trim();
    let path = PathBuf::from(path);
    (closed && (trailing.is_empty() || trailing.starts_with('#')) && path.is_absolute())
        .then_some(path)
}

fn profile_directory(application_dir: &Path, explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| application_dir.join("profile"))
}

#[derive(Clone, Copy)]
enum Platform {
    #[cfg(any(target_os = "macos", test))]
    MacOs,
    #[cfg(any(not(target_os = "macos"), test))]
    Linux,
}

fn application_dir(
    platform: Platform,
    home: Option<&Path>,
    xdg_data_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let _ = xdg_data_home;
    let root = match platform {
        #[cfg(any(target_os = "macos", test))]
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
        #[cfg(any(not(target_os = "macos"), test))]
        Platform::Linux => {
            let data_home = xdg_data_home
                .filter(|path| path.is_absolute())
                .map(Path::to_path_buf)
                .or_else(|| home.map(|home| home.join(".local/share")))?;
            data_home.join("Elaris")
        }
    };
    Some(if development {
        root.join("development")
    } else {
        root
    })
}

fn config_directory(
    platform: Platform,
    home: Option<&Path>,
    _xdg_config_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    let root = match platform {
        #[cfg(any(target_os = "macos", test))]
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
        #[cfg(any(not(target_os = "macos"), test))]
        Platform::Linux => _xdg_config_home
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| home.map(|home| home.join(".config")))?
            .join("elaris"),
    };
    Some(if development {
        root.join("development")
    } else {
        root
    })
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Session {
    version: u8,
    pub(crate) tabs: Vec<SessionTab>,
    pub(crate) active_tab: usize,
    /// Positions in `tabs`, kept in sidebar order. Missing fields in version 1 mean regular tabs.
    #[serde(default)]
    pub(crate) favourites: Vec<usize>,
    #[serde(default)]
    pub(crate) pinned: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionTab {
    pub(crate) url: String,
    pub(crate) title: String,
}

impl Session {
    #[cfg(test)]
    pub(crate) fn new(tabs: Vec<SessionTab>, active_tab: usize) -> Self {
        Self::new_with_sections(tabs, active_tab, Vec::new(), Vec::new())
    }

    pub(crate) fn new_with_sections(
        tabs: Vec<SessionTab>,
        active_tab: usize,
        favourites: Vec<usize>,
        pinned: Vec<usize>,
    ) -> Self {
        Self {
            version: SESSION_VERSION,
            active_tab: if tabs.is_empty() {
                0
            } else {
                active_tab.min(tabs.len() - 1)
            },
            tabs,
            favourites,
            pinned,
        }
    }

    pub(crate) fn section_at(&self, index: usize) -> crate::tab::TabSection {
        if self.favourites.contains(&index) {
            crate::tab::TabSection::Favourite
        } else if self.pinned.contains(&index) {
            crate::tab::TabSection::Pinned
        } else {
            crate::tab::TabSection::Regular
        }
    }

    fn is_supported(&self) -> bool {
        matches!(self.version, 1 | SESSION_VERSION)
            && (self.tabs.is_empty() || self.active_tab < self.tabs.len())
            && self.favourites.iter().all(|index| *index < self.tabs.len())
            && self.pinned.iter().all(|index| *index < self.tabs.len())
            && !self
                .favourites
                .iter()
                .any(|index| self.pinned.contains(index))
    }
}

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
        let session = match load_session(&paths.session_file) {
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

fn load_session(path: &Path) -> io::Result<Option<Session>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let session: Session = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(session.is_supported().then_some(session))
}

fn write_session_atomic(path: &Path, session: &Session) -> io::Result<()> {
    let bytes = serde_json::to_vec(session)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let id = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_file_name(format!(
        ".session-{}-{timestamp}-{id}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        if let Some(parent) = path.parent()
            && let Ok(directory) = File::open(parent)
        {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("elaris-storage-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create temporary storage directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn history_worker_reports_failed_writes_and_refreshes_omnibox_after_success() {
        let temp = TempDir::new();
        let paths = Paths::resolve_with(
            Platform::MacOs,
            Some(&temp.0),
            None,
            None,
            None,
            None,
            false,
        )
        .unwrap();
        paths.create_directories().unwrap();
        let database = rusqlite::Connection::open(&paths.history_file).unwrap();
        History::open(&paths.history_file)
            .unwrap()
            .record_visit("https://github.com/", "GitHub")
            .unwrap();
        database
            .execute_batch(
                "CREATE TRIGGER block_delete BEFORE DELETE ON history BEGIN
                SELECT RAISE(ABORT, 'test failure');
             END;",
            )
            .unwrap();
        let storage = Storage::open(paths.clone()).unwrap();
        let history = storage.history();
        history.request_search("github").unwrap();
        history.request_page(1, "", None).unwrap();
        storage.flush_on_close().unwrap();
        assert!(
            matches!(history.take_events().as_slice(), [HistoryEvent::Page(1, Ok(page))]
            if page.entries.len() == 1)
        );
        assert_eq!(history.search("github", 10).len(), 1);

        for operation in [
            HistoryMutation::Remove("https://github.com/".into()),
            HistoryMutation::Clear,
        ] {
            history.mutate(operation).unwrap();
            storage.flush_on_close().unwrap();
            assert!(matches!(
                history.take_events().as_slice(),
                [HistoryEvent::Mutation(Err(_))]
            ));
            assert_eq!(history.recent(10).len(), 1);
            assert_eq!(history.search("github", 10).len(), 1);
        }

        database.execute_batch("DROP TRIGGER block_delete").unwrap();
        history
            .mutate(HistoryMutation::Remove("https://github.com/".into()))
            .unwrap();
        storage.flush_on_close().unwrap();
        assert!(matches!(
            history.take_events().as_slice(),
            [HistoryEvent::Mutation(Ok(HistoryMutation::Remove(_)))]
        ));
        assert!(history.search("github", 10).is_empty());
        assert!(history.frequent(10).is_empty());

        history
            .record_visit("https://github.com/", "GitHub")
            .unwrap();
        history
            .record_visit("https://example.org/", "Example")
            .unwrap();
        history.mutate(HistoryMutation::Clear).unwrap();
        history.request_page(2, "github", None).unwrap();
        storage.flush_on_close().unwrap();
        assert!(matches!(history.take_events().as_slice(), [
            HistoryEvent::Mutation(Ok(HistoryMutation::Clear)), HistoryEvent::Page(2, Ok(page))
        ] if page.entries.is_empty()));
        assert!(history.search("github", 10).is_empty());
        assert!(
            History::open(&paths.history_file)
                .unwrap()
                .recent(10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn macos_and_linux_paths_follow_native_conventions() {
        let home = Path::new("/home/tester");
        assert_eq!(
            application_dir(Platform::MacOs, Some(home), None, false),
            Some(home.join("Library/Application Support/Elaris"))
        );
        assert_eq!(
            application_dir(Platform::Linux, Some(home), None, false),
            Some(home.join(".local/share/Elaris"))
        );
        assert_eq!(
            application_dir(
                Platform::Linux,
                Some(home),
                Some(Path::new("/mnt/user-data")),
                false
            ),
            Some(PathBuf::from("/mnt/user-data/Elaris"))
        );
        assert_eq!(
            application_dir(
                Platform::Linux,
                Some(home),
                Some(Path::new("relative")),
                false
            ),
            Some(home.join(".local/share/Elaris"))
        );
    }

    #[test]
    fn development_paths_are_isolated_from_release_paths() {
        let home = Path::new("/home/tester");
        let release = application_dir(Platform::Linux, Some(home), None, false).unwrap();
        let development = application_dir(Platform::Linux, Some(home), None, true).unwrap();
        assert_eq!(development, release.join("development"));
        let config_release = config_directory(Platform::Linux, Some(home), None, false).unwrap();
        let config_development = config_directory(Platform::Linux, Some(home), None, true).unwrap();
        assert_eq!(config_development, config_release.join("development"));
    }

    #[test]
    fn config_paths_follow_platform_conventions_and_xdg() {
        let home = Path::new("/home/tester");
        assert_eq!(
            config_directory(Platform::MacOs, Some(home), None, false),
            Some(home.join("Library/Application Support/Elaris"))
        );
        assert_eq!(
            config_directory(Platform::Linux, Some(home), None, false),
            Some(home.join(".config/elaris"))
        );
        assert_eq!(
            config_directory(
                Platform::Linux,
                Some(home),
                Some(Path::new("/mnt/user-config")),
                false
            ),
            Some(PathBuf::from("/mnt/user-config/elaris"))
        );
        let paths = Paths::resolve_with(
            Platform::Linux,
            None,
            Some(home),
            None,
            Some(Path::new("/mnt/user-config")),
            None,
            false,
        )
        .expect("resolve Linux paths");
        assert_eq!(
            paths.config_file,
            PathBuf::from("/mnt/user-config/elaris/config.toml")
        );
    }

    #[test]
    fn parses_xdg_download_directory_without_evaluating_shell_content() {
        let home = Path::new("/home/tester");
        assert_eq!(
            parse_xdg_download_directory(
                "XDG_DOWNLOAD_DIR=\"$HOME/Shared Downloads\" # user choice",
                home
            ),
            Some(home.join("Shared Downloads"))
        );
        assert_eq!(
            parse_xdg_download_directory("XDG_DOWNLOAD_DIR=\"/mnt/files\"", home),
            Some(PathBuf::from("/mnt/files"))
        );
        assert_eq!(
            parse_xdg_download_directory("XDG_DOWNLOAD_DIR=\"$(touch /tmp/pwned)\"", home),
            None
        );
    }

    #[test]
    fn data_directory_override_is_exact_and_sets_the_default_profile() {
        let temp = TempDir::new();
        let data_dir = temp.0.join("isolated-data");
        let paths = Paths::resolve_with(
            Platform::MacOs,
            Some(&data_dir),
            Some(Path::new("/Users/tester")),
            None,
            None,
            None,
            true,
        )
        .expect("resolve explicit data directory");

        assert_eq!(paths.application_dir, data_dir);
        assert_eq!(paths.profile_dir, paths.application_dir.join("profile"));
        assert_eq!(
            paths.history_file,
            paths.application_dir.join("history.sqlite")
        );
        assert_eq!(
            paths.session_file,
            paths.application_dir.join("session.json")
        );
        assert_eq!(paths.config_file, paths.application_dir.join("config.toml"));
        assert!(!paths.application_dir.exists());
    }

    #[test]
    fn data_directory_override_keeps_the_ladybird_profile_override() {
        let temp = TempDir::new();
        let data_dir = temp.0.join("isolated-data");
        let explicit_profile = temp.0.join("ladybird-profile");
        let paths = Paths::resolve_with(
            Platform::Linux,
            Some(&data_dir),
            Some(Path::new("/home/tester")),
            Some(Path::new("/xdg-data")),
            None,
            Some(&explicit_profile),
            true,
        )
        .expect("resolve explicit paths");

        assert_eq!(paths.application_dir, data_dir);
        assert_eq!(paths.profile_dir, explicit_profile);
        assert!(!paths.application_dir.exists());
        assert!(!paths.profile_dir.exists());
    }

    #[test]
    fn relative_data_directory_overrides_are_rejected() {
        let error = Paths::resolve_with(
            Platform::Linux,
            Some(Path::new("relative-data")),
            Some(Path::new("/home/tester")),
            None,
            None,
            None,
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn explicit_ladybird_profile_directory_is_preserved() {
        let application = Path::new("/home/tester/.local/share/Elaris/development");
        let explicit = Path::new("/tmp/elaris-profile-override");
        assert_eq!(profile_directory(application, Some(explicit)), explicit);
        assert_eq!(
            profile_directory(application, None),
            application.join("profile")
        );
    }

    #[test]
    fn session_round_trips_order_and_active_tab() {
        let session = Session::new(
            vec![
                SessionTab {
                    url: "https://one.example/".into(),
                    title: "One".into(),
                },
                SessionTab {
                    url: "https://two.example/".into(),
                    title: "Two".into(),
                },
                SessionTab {
                    url: "https://three.example/".into(),
                    title: "Three".into(),
                },
            ],
            1,
        );
        let bytes = serde_json::to_vec(&session).unwrap();
        let restored: Session = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.tabs[0].url, "https://one.example/");
        assert_eq!(restored.tabs[2].url, "https://three.example/");
        assert_eq!(restored.active_tab, 1);
    }

    #[test]
    fn session_metadata_keeps_internal_page_addresses() {
        let session = Session::new(
            vec![SessionTab {
                url: "elaris://settings".into(),
                title: "Settings".into(),
            }],
            0,
        );
        let bytes = serde_json::to_vec(&session).expect("serialize session");
        let restored: Session = serde_json::from_slice(&bytes).expect("restore session");
        assert_eq!(restored.tabs[0].url, "elaris://settings");
        assert_eq!(restored.tabs[0].title, "Settings");
    }

    #[test]
    fn configuration_reload_runs_on_storage_worker_and_returns_typed_values() {
        let temp = TempDir::new();
        let data_dir = temp.0.join("data");
        let paths = Paths::resolve_with(
            Platform::Linux,
            Some(&data_dir),
            Some(Path::new("/home/tester")),
            None,
            None,
            None,
            false,
        )
        .expect("resolve isolated storage paths");
        let config_file = paths.config_file.clone();
        let storage = Storage::open(paths).expect("open isolated storage");
        let (wake, notifications) = mpsc::channel();
        storage.set_notify(Arc::new(move || {
            let _ = wake.send(());
        }));
        fs::write(
            &config_file,
            "[appearance]\ntheme = \"dark\"\n[search]\nengine = \"brave\"\n[downloads]\ndirectory = \"default\"\n[startup]\nmode = \"blank\"\n",
        )
        .expect("edit config file");
        storage
            .reload_configuration()
            .expect("queue asynchronous configuration reload");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let loaded = loop {
            notifications
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("storage worker wakes the UI");
            if let Some(event) = storage.take_config_events().into_iter().next() {
                match event {
                    ConfigEvent::Reloaded(result) => {
                        break result.expect("configuration reload succeeds");
                    }
                    ConfigEvent::Saved(_) => continue,
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "storage worker reloads config"
            );
        };
        assert_eq!(
            loaded.settings.appearance,
            crate::appearance::Appearance::Dark
        );
        assert_eq!(
            loaded.settings.search_engine,
            crate::address::SearchEngine::Brave
        );
        assert_eq!(
            loaded.settings.startup,
            crate::settings::StartupMode::BlankWindow
        );
    }

    #[test]
    fn malformed_and_outdated_sessions_are_ignored() {
        let temp = TempDir::new();
        let path = temp.0.join("session.json");
        fs::write(&path, "not json").unwrap();
        assert_eq!(
            load_session(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::write(&path, r#"{"version":9,"tabs":[],"active_tab":0}"#).unwrap();
        assert!(load_session(&path).unwrap().is_none());
    }

    #[test]
    fn closing_waits_for_the_latest_session_snapshot() {
        let temp = TempDir::new();
        let paths = Paths::resolve_with(
            Platform::Linux,
            Some(&temp.0),
            None,
            None,
            None,
            None,
            false,
        )
        .expect("resolve isolated storage paths");
        let session_file = paths.session_file.clone();
        let storage = Storage::open(paths).expect("open isolated storage");
        let before = Session::new(
            vec![SessionTab {
                url: "about:blank".into(),
                title: "Closing tab".into(),
            }],
            0,
        );
        let after = Session::new(Vec::new(), 0);
        storage.save_session(before).expect("queue open tab");
        storage
            .save_session(after.clone())
            .expect("queue closed tab");
        storage.flush_on_close().expect("wait for queued writes");
        assert_eq!(
            load_session(&session_file).expect("read saved session"),
            Some(after)
        );
    }

    #[test]
    fn session_write_replaces_atomically_and_leaves_no_temporary_file() {
        let temp = TempDir::new();
        let path = temp.0.join("session.json");
        let session = Session::new(
            vec![SessionTab {
                url: "https://example.com/".into(),
                title: "Example".into(),
            }],
            0,
        );
        fs::write(&path, "old session").unwrap();
        write_session_atomic(&path, &session).unwrap();
        assert_eq!(load_session(&path).unwrap(), Some(session));
        assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
    }

    #[test]
    fn empty_session_has_no_active_tab() {
        let session = Session::new(Vec::new(), 0);
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab, 0);
        assert!(session.is_supported());
    }
}
