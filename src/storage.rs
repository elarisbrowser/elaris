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

use elaris::history::{Entry, History};

const SESSION_VERSION: u8 = 1;
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
}

impl Paths {
    pub(crate) fn resolve() -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
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
            explicit_profile_dir.as_deref(),
            cfg!(debug_assertions),
        )
    }

    fn resolve_with(
        platform: Platform,
        data_dir: Option<&Path>,
        home: Option<&Path>,
        xdg_data_home: Option<&Path>,
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
        Ok(Self {
            history_file: application_dir.join("history.sqlite"),
            session_file: application_dir.join("session.json"),
            application_dir,
            profile_dir,
        })
    }

    fn create_directories(&self) -> io::Result<()> {
        create_private_dir(&self.application_dir)?;
        create_private_dir(&self.profile_dir)
    }
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionTab {
    pub(crate) url: String,
    pub(crate) title: String,
}

impl Session {
    pub(crate) fn new(tabs: Vec<SessionTab>, active_tab: usize) -> Self {
        Self {
            version: SESSION_VERSION,
            active_tab: if tabs.is_empty() {
                0
            } else {
                active_tab.min(tabs.len() - 1)
            },
            tabs,
        }
    }

    fn is_supported(&self) -> bool {
        self.version == SESSION_VERSION
            && (self.tabs.is_empty() || self.active_tab < self.tabs.len())
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

    #[expect(
        dead_code,
        reason = "History management can use this without adding UI in this milestone"
    )]
    pub(crate) fn remove(&self, url: &str) -> Result<(), String> {
        self.send(Command::Remove(url.to_owned()))
    }

    #[expect(
        dead_code,
        reason = "History management can use this without adding UI in this milestone"
    )]
    pub(crate) fn clear(&self) -> Result<(), String> {
        self.send(Command::Clear)
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "history storage worker has stopped".to_owned())
    }

    fn set_notify(&self, notify: Notify) {
        let _ = self.shared.notify.set(notify);
    }
}

pub(crate) struct Storage {
    session: Option<Session>,
    commands: Sender<Command>,
    history: HistoryHandle,
}

impl Storage {
    pub(crate) fn open(paths: Paths) -> io::Result<Self> {
        paths.create_directories()?;
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
        let history_file = paths.history_file.clone();
        let session_file = paths.session_file.clone();
        let worker = std::thread::Builder::new()
            .name("elaris-storage".to_owned())
            .spawn(move || {
                run_worker(receiver, worker_shared, history_file, session_file);
            })?;
        register_worker(commands.clone(), worker);
        Ok(Self {
            session,
            commands: commands.clone(),
            history: HistoryHandle { commands, shared },
        })
    }

    pub(crate) fn take_session(&mut self) -> Option<Session> {
        self.session.take()
    }

    pub(crate) fn history(&self) -> &HistoryHandle {
        &self.history
    }

    pub(crate) fn save_session(&self, session: Session) -> Result<(), String> {
        self.commands
            .send(Command::SaveSession(session))
            .map_err(|_| "session storage worker has stopped".to_owned())
    }

    pub(crate) fn set_notify(&self, notify: Notify) {
        self.history.set_notify(notify);
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
    Remove(String),
    Clear,
    SaveSession(Session),
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
) {
    let history = match History::open(&history_file) {
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
            Command::Remove(url) => {
                if let Some(history) = &history {
                    if let Err(error) = history.remove(&url) {
                        eprintln!("Elaris: cannot remove history entry: {error}");
                    }
                    refresh_history(history, &shared);
                }
            }
            Command::Clear => {
                if let Some(history) = &history {
                    if let Err(error) = history.clear() {
                        eprintln!("Elaris: cannot clear history: {error}");
                    }
                    refresh_history(history, &shared);
                }
            }
            Command::SaveSession(session) => {
                if let Err(error) = write_session_atomic(&session_file, &session) {
                    eprintln!("Elaris: cannot save session: {error}");
                }
            }
            Command::Shutdown => break,
        }
    }
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
