// SPDX-License-Identifier: GPL-3.0-only

use super::paths::{
    Platform, application_dir, config_directory, parse_xdg_download_directory, profile_directory,
};
use super::session::{load_session, load_sessions, write_session_atomic, write_sessions_atomic};
use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("elaris-storage-{}-{id}", std::process::id()));
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
    history.request_page(7, 1, "", None).unwrap();
    storage.flush_on_close().unwrap();
    assert!(
        matches!(history.take_events(7).as_slice(), [HistoryEvent::Page(7, 1, Ok(page))]
            if page.entries.len() == 1)
    );
    assert_eq!(history.search("github", 10).len(), 1);

    for operation in [
        HistoryMutation::Remove("https://github.com/".into()),
        HistoryMutation::Clear,
    ] {
        history.mutate(7, operation).unwrap();
        storage.flush_on_close().unwrap();
        assert!(matches!(
            history.take_events(7).as_slice(),
            [HistoryEvent::Mutation(7, Err(_))]
        ));
        assert_eq!(history.recent(10).len(), 1);
        assert_eq!(history.search("github", 10).len(), 1);
    }

    database.execute_batch("DROP TRIGGER block_delete").unwrap();
    history
        .mutate(7, HistoryMutation::Remove("https://github.com/".into()))
        .unwrap();
    storage.flush_on_close().unwrap();
    assert!(matches!(
        history.take_events(7).as_slice(),
        [HistoryEvent::Mutation(7, Ok(HistoryMutation::Remove(_)))]
    ));
    assert!(history.search("github", 10).is_empty());
    assert!(history.frequent(10).is_empty());

    history
        .record_visit("https://github.com/", "GitHub")
        .unwrap();
    history
        .record_visit("https://example.org/", "Example")
        .unwrap();
    history.mutate(7, HistoryMutation::Clear).unwrap();
    history.request_page(7, 2, "github", None).unwrap();
    storage.flush_on_close().unwrap();
    assert!(matches!(history.take_events(7).as_slice(), [
            HistoryEvent::Mutation(7, Ok(HistoryMutation::Clear)), HistoryEvent::Page(7, 2, Ok(page))
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
fn shared_history_page_results_stay_with_the_requesting_window() {
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
    .unwrap();
    let storage = Storage::open(paths).unwrap();
    let history = storage.history();
    history.record_visit("https://one.example/", "One").unwrap();
    history.record_visit("https://two.example/", "Two").unwrap();
    history.request_page(101, 1, "one", None).unwrap();
    history.request_page(202, 1, "two", None).unwrap();
    storage.flush_on_close().unwrap();

    let first = history.take_events(101);
    let second = history.take_events(202);
    assert!(
        matches!(first.as_slice(), [HistoryEvent::Page(101, 1, Ok(page))] if page.entries.len() == 1)
    );
    assert!(
        matches!(second.as_slice(), [HistoryEvent::Page(202, 1, Ok(page))] if page.entries.len() == 1)
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
                favicon: None,
            },
            SessionTab {
                url: "https://two.example/".into(),
                title: "Two".into(),
                favicon: Some(SessionFavicon {
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 4],
                }),
            },
            SessionTab {
                url: "https://three.example/".into(),
                title: "Three".into(),
                favicon: None,
            },
        ],
        1,
    );
    let bytes = serde_json::to_vec(&session).unwrap();
    let restored: Session = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.tabs[0].url, "https://one.example/");
    assert_eq!(restored.tabs[2].url, "https://three.example/");
    assert_eq!(restored.tabs[1].favicon, session.tabs[1].favicon);
    assert_eq!(restored.active_tab, 1);
}

#[test]
fn session_metadata_keeps_internal_page_addresses() {
    let session = Session::new(
        vec![SessionTab {
            url: "elaris://settings".into(),
            title: "Settings".into(),
            favicon: None,
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
            favicon: None,
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
fn immediate_shutdown_drains_snapshot_burst_and_restores_latest_state() {
    const SNAPSHOT_COUNT: usize = 192;

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
    let storage = Storage::open(paths.clone()).expect("open isolated storage");
    let expected = Session::new(
        vec![SessionTab {
            url: "https://last-before-quit.example/".into(),
            title: "Last intended session".into(),
            favicon: None,
        }],
        0,
    );

    for index in 0..SNAPSHOT_COUNT {
        storage
            .save_session(Session::new(
                vec![SessionTab {
                    url: format!("https://snapshot-{index}.example/"),
                    title: format!("Snapshot {index}"),
                    favicon: None,
                }],
                0,
            ))
            .expect("queue session before immediate quit");
    }
    storage
        .save_session(expected.clone())
        .expect("queue final intended session");
    storage
        .flush_on_close()
        .expect("quit waits for the last session replacement");
    assert_eq!(storage.queue_metrics().queued, 0);
    drop(storage);

    let restored = Storage::open(paths)
        .expect("reopen storage after shutdown")
        .take_sessions();
    assert_eq!(restored, Some(vec![expected]));
}

#[test]
fn acknowledged_session_commit_is_a_snapshot_coalescing_barrier() {
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
    let queued = Session::new(
        vec![SessionTab {
            url: "https://queued.example/".into(),
            title: "Queued snapshot".into(),
            favicon: None,
        }],
        0,
    );
    let committed = Session::new(
        vec![SessionTab {
            url: "https://committed.example/".into(),
            title: "Acknowledged snapshot".into(),
            favicon: None,
        }],
        0,
    );
    let after_commit = Session::new(
        vec![SessionTab {
            url: "https://after.example/".into(),
            title: "Later snapshot".into(),
            favicon: None,
        }],
        0,
    );

    storage
        .save_session(queued)
        .expect("queue ordinary snapshot");
    storage
        .commit_merged_session(vec![committed.clone()])
        .expect("wait for acknowledged snapshot");
    assert_eq!(
        load_sessions(&session_file).expect("read acknowledged snapshot"),
        Some(vec![committed])
    );

    storage
        .save_session(after_commit.clone())
        .expect("queue later snapshot");
    storage.flush_on_close().expect("drain later snapshot");
    assert_eq!(
        load_session(&session_file).expect("read final snapshot"),
        Some(after_commit)
    );
}

#[test]
fn mixed_storage_burst_preserves_latest_state_and_reports_queue_cost() {
    const BURST_SIZE: usize = 128;

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
    let history_file = paths.history_file.clone();
    let storage = Storage::open(paths).expect("open isolated storage");

    storage
        .history()
        .request_search("burst.example")
        .expect("queue history search");
    let history_started = std::time::Instant::now();
    for index in 0..BURST_SIZE {
        storage
            .history()
            .record_visit(
                &format!("https://{index}.burst.example/"),
                &format!("Burst page {index}"),
            )
            .expect("queue history visit");
    }
    storage.flush_on_close().expect("drain history queue");
    let history_elapsed = history_started.elapsed();

    let sessions_started = std::time::Instant::now();
    for index in 0..BURST_SIZE {
        storage
            .save_session(Session::new(
                vec![SessionTab {
                    url: format!("https://{index}.burst.example/"),
                    title: format!("Snapshot {index}"),
                    favicon: None,
                }],
                0,
            ))
            .expect("queue session snapshot");
    }
    storage.flush_on_close().expect("drain session queue");
    let sessions_elapsed = sessions_started.elapsed();

    assert_eq!(
        load_session(&session_file).expect("read final session"),
        Some(Session::new(
            vec![SessionTab {
                url: format!("https://{}.burst.example/", BURST_SIZE - 1),
                title: format!("Snapshot {}", BURST_SIZE - 1),
                favicon: None,
            }],
            0,
        ))
    );
    assert_eq!(
        History::open(&history_file)
            .expect("open final history database")
            .recent(BURST_SIZE)
            .expect("read recorded history")
            .len(),
        BURST_SIZE
    );
    println!(
        "headless storage burst: {BURST_SIZE} visits {:?}, {BURST_SIZE} session snapshots {:?}",
        history_elapsed, sessions_elapsed
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
            favicon: None,
        }],
        0,
    );
    fs::write(&path, "old session").unwrap();
    write_session_atomic(&path, &session).unwrap();
    assert_eq!(load_session(&path).unwrap(), Some(session));
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn multi_window_sessions_round_trip_and_legacy_single_sessions_still_load() {
    let temp = TempDir::new();
    let path = temp.0.join("session.json");
    let first = Session::new(
        vec![SessionTab {
            url: "https://first.example/".into(),
            title: "First".into(),
            favicon: None,
        }],
        0,
    );
    let second = Session::new(
        vec![SessionTab {
            url: "https://second.example/".into(),
            title: "Second".into(),
            favicon: None,
        }],
        0,
    );
    write_sessions_atomic(&path, &[first.clone(), second.clone()], true).unwrap();
    assert_eq!(load_sessions(&path).unwrap(), Some(vec![first, second]));

    write_sessions_atomic(&path, &[Session::new(Vec::new(), 0)], false).unwrap();
    assert!(load_sessions(&path).unwrap().is_none());

    fs::write(&path, r#"{"version":4,"tabs":[],"active_tab":0}"#).unwrap();
    assert_eq!(load_sessions(&path).unwrap().unwrap().len(), 1);
}

#[test]
fn empty_session_has_no_active_tab() {
    let session = Session::new(Vec::new(), 0);
    assert!(session.tabs.is_empty());
    assert_eq!(session.active_tab, 0);
    assert!(session.is_supported());
}

#[test]
fn legacy_pins_restore_as_regular_tabs_and_are_omitted_on_save() {
    use crate::tab::TabSection;

    let temp = TempDir::new();
    let path = temp.0.join("session.json");
    fs::write(
        &path,
        r#"{"version":4,"tabs":[{"url":"https://example.com","title":"Former pin"},
        {"url":"https://example.org","title":"Favourite"}],"active_tab":0,
        "favourites":[1],"pinned":[0]}"#,
    )
    .unwrap();

    let session = load_session(&path).unwrap().unwrap();
    assert_eq!(session.active_tab, 0);
    assert_eq!(session.tabs[0].url, "https://example.com");
    assert_eq!(session.section_at(0), TabSection::Regular);
    assert_eq!(session.section_at(1), TabSection::Favourite);
    write_session_atomic(&path, &session).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(saved["windows"][0].get("pinned").is_none());
    assert_eq!(load_session(&path).unwrap(), Some(session));
}
