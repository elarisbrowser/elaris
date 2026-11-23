// SPDX-License-Identifier: GPL-3.0-only

//! Unit tests for configuration parsing, validation and comment-preserving writes.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::address::SearchEngine;
use crate::appearance::{AccentColor, Appearance, GlassStyle};
use crate::permissions::{AutoplayPolicy, PermissionKind, PermissionState, PermissionStore};
use crate::settings::{
    ContentBlockList, DownloadDirectory, NewTabPosition, PageStyle, Settings, SettingsChange,
    SidebarSide, StartupMode, TabLayout, WindowDecorations,
};

use super::write::NEXT_TEMP_FILE;
use super::*;

#[test]
fn tab_layout_defaults_to_vertical_and_round_trips() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-tab-layout-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(parse("").unwrap().settings.tab_layout, TabLayout::Vertical);
    for layout in [TabLayout::Horizontal, TabLayout::Vertical] {
        write_change(&path, &SettingsChange::TabLayout(layout)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .tab_layout,
            layout
        );
    }
    let invalid = parse("[appearance]\ntab_layout = 'diagonal'").unwrap();
    assert_eq!(invalid.settings.tab_layout, TabLayout::Vertical);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sidebar_side_defaults_to_left_and_round_trips() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-sidebar-side-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(parse("").unwrap().settings.sidebar_side, SidebarSide::Left);
    for side in [SidebarSide::Right, SidebarSide::Left] {
        write_change(&path, &SettingsChange::SidebarSide(side)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .sidebar_side,
            side
        );
    }
    let invalid = parse("[appearance]\nsidebar_side = 'middle'").unwrap();
    assert_eq!(invalid.settings.sidebar_side, SidebarSide::Left);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn window_decorations_default_to_auto_and_round_trip() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-decorations-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(
        parse("").unwrap().settings.window_decorations,
        WindowDecorations::Auto
    );
    for mode in WindowDecorations::ALL {
        write_change(&path, &SettingsChange::WindowDecorations(mode)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .window_decorations,
            mode
        );
    }
    let invalid = parse("[linux]\ndecorations = 'fancy'").unwrap();
    assert_eq!(invalid.settings.window_decorations, WindowDecorations::Auto);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn page_style_defaults_to_card_and_round_trips() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-page-style-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(parse("").unwrap().settings.page_style, PageStyle::Card);
    for style in [PageStyle::EdgeToEdge, PageStyle::Card] {
        write_change(&path, &SettingsChange::PageStyle(style)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .page_style,
            style
        );
    }
    let invalid = parse("[appearance]\npage_style = 'floating'").unwrap();
    assert_eq!(invalid.settings.page_style, PageStyle::Card);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn memory_saver_defaults_to_balanced_and_round_trips() {
    use elaris_memory_saver::MemorySaver;
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-memory-saver-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(
        parse("").unwrap().settings.memory_saver,
        MemorySaver::Balanced
    );
    for mode in MemorySaver::ALL {
        write_change(&path, &SettingsChange::MemorySaver(mode)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .memory_saver,
            mode
        );
    }
    let invalid = parse("[performance]\nmemory_saver = 'eco'").unwrap();
    assert_eq!(invalid.settings.memory_saver, MemorySaver::Balanced);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn keep_awake_sites_round_trip_and_skip_what_names_no_site() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-keep-awake-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert!(parse("").unwrap().settings.keep_awake_sites.is_empty());
    let sites = vec!["mail.example.com".to_owned(), "example.org".to_owned()];
    write_change(&path, &SettingsChange::KeepAwakeSites(sites.clone())).unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(parse(&contents).unwrap().settings.keep_awake_sites, sites);
    assert!(
        contents.contains("reduce_motion"),
        "other settings are kept"
    );
    write_change(&path, &SettingsChange::KeepAwakeSites(Vec::new())).unwrap();
    assert!(
        parse(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .settings
            .keep_awake_sites
            .is_empty()
    );
    let mixed = parse(
        "[performance]\nkeep_awake = ['https://www.Example.com/a', 'not a site', 'example.com']",
    )
    .unwrap();
    assert_eq!(mixed.settings.keep_awake_sites, ["example.com"]);
    assert_eq!(mixed.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn bookmarks_bar_defaults_to_shown_and_round_trips() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-bookmarks-bar-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert!(parse("").unwrap().settings.bookmarks_bar);
    for shown in [false, true] {
        write_change(&path, &SettingsChange::BookmarksBar(shown)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .bookmarks_bar,
            shown
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn new_tab_position_defaults_to_top_and_round_trips() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-new-tab-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    assert_eq!(
        parse("").unwrap().settings.new_tab_position,
        NewTabPosition::Top
    );
    for position in [NewTabPosition::Bottom, NewTabPosition::Top] {
        write_change(&path, &SettingsChange::NewTabPosition(position)).unwrap();
        assert_eq!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .settings
                .new_tab_position,
            position
        );
    }
    let invalid = parse("[appearance]\nnew_tab_position = 'middle'").unwrap();
    assert_eq!(invalid.settings.new_tab_position, NewTabPosition::Top);
    assert_eq!(invalid.issues.len(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn split_view_limit_defaults_to_two_and_config_allows_higher_values() {
    assert_eq!(parse("").unwrap().settings.split_view_limit, 2);
    let loaded = parse("[split_view]\nmax_panes = 8").unwrap();
    assert_eq!(loaded.settings.split_view_limit, 8);
    assert!(loaded.issues.is_empty());

    let invalid = parse("[split_view]\nmax_panes = 1").unwrap();
    assert_eq!(invalid.settings.split_view_limit, 2);
    assert_eq!(invalid.issues.len(), 1);
}

#[test]
fn parses_defaults_and_known_settings() {
    let defaults = parse("").expect("empty TOML uses defaults");
    assert_eq!(defaults.settings, Settings::default());
    assert!(defaults.issues.is_empty());

    let config = parse(
        "[appearance]\ntheme = \"dark\"\n[search]\nengine = \"brave\"\n[downloads]\ndirectory = \"/tmp/files\"\n[startup]\nmode = \"blank\"\n",
    )
    .expect("valid config parses");
    assert_eq!(config.settings.appearance, Appearance::Dark);
    assert_eq!(config.settings.search_engine, SearchEngine::Brave);
    assert_eq!(
        config.settings.download_directory,
        DownloadDirectory::Custom(PathBuf::from("/tmp/files"))
    );
    assert_eq!(config.settings.startup, StartupMode::BlankWindow);
}

#[test]
fn invalid_values_fall_back_with_key_specific_messages() {
    let config = parse(
        "[appearance]\ntheme = \"sepia\"\n[search]\nengine = 5\n[downloads]\ndirectory = \"relative\"\n[startup]\nmode = \"later\"\n",
    )
    .expect("valid TOML with invalid values is recoverable");
    assert_eq!(config.settings, Settings::default());
    assert_eq!(config.issues.len(), 4);
    assert!(
        config
            .issues
            .iter()
            .any(|issue| issue.contains("[search].engine"))
    );
    assert!(
        config
            .issues
            .iter()
            .any(|issue| issue.contains("[downloads].directory"))
    );
}

#[test]
fn malformed_toml_is_an_error_and_not_a_default_file() {
    assert!(parse("[appearance\ntheme = \"dark\"").is_err());
}

#[test]
fn autoplay_default_and_site_exceptions_round_trip() {
    let path = std::env::temp_dir().join(format!(
        "elaris-autoplay-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    write_change(&path, &SettingsChange::Autoplay(AutoplayPolicy::Block))
        .expect("save the autoplay default");
    write_autoplay(&path, "https://Example.com/", Some(AutoplayPolicy::Allow))
        .expect("save a site autoplay exception");
    let loaded = load_or_create(&path).expect("load autoplay settings");
    assert_eq!(loaded.settings.autoplay, AutoplayPolicy::Block);
    assert_eq!(
        loaded.permissions.autoplay("https://example.com"),
        Some(AutoplayPolicy::Allow)
    );
    assert!(loaded.issues.is_empty());
    write_autoplay(&path, "https://example.com", None).expect("remove the exception");
    let loaded = load_or_create(&path).expect("load cleared autoplay exception");
    assert_eq!(loaded.permissions, PermissionStore::default());
    fs::remove_file(path).expect("remove autoplay test config");
}

#[test]
fn content_blocking_defaults_to_off_and_round_trips_its_lists() {
    let path = std::env::temp_dir().join(format!(
        "elaris-content-blocking-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let loaded = load_or_create(&path).expect("create the default configuration");
    assert_eq!(
        loaded.settings.content_blocking,
        crate::settings::ContentBlocking::default()
    );
    assert!(!loaded.settings.content_blocking.enabled);

    write_change(&path, &SettingsChange::ContentBlocking(true)).expect("turn blocking on");
    write_change(
        &path,
        &SettingsChange::ContentBlockingList(ContentBlockList::EasyPrivacy, false),
    )
    .expect("turn EasyPrivacy off");
    write_change(
        &path,
        &SettingsChange::ContentBlockingList(ContentBlockList::Annoyances, true),
    )
    .expect("turn Annoyances on");
    let loaded = load_or_create(&path).expect("load content blocking settings");
    assert!(loaded.settings.content_blocking.enabled);
    assert_eq!(
        loaded.settings.content_blocking.lists,
        std::collections::BTreeSet::from([
            ContentBlockList::EasyList,
            ContentBlockList::Annoyances
        ])
    );
    assert!(loaded.issues.is_empty());
    fs::remove_file(path).expect("remove content blocking test config");
}

#[test]
fn site_permissions_round_trip_and_ask_removes_the_last_site_entry() {
    let path = std::env::temp_dir().join(format!(
        "elaris-permissions-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    write_permission(
        &path,
        "HTTPS://Example.com:443/path",
        PermissionKind::Camera,
        PermissionState::Allow,
    )
    .expect("save an HTTPS permission override");
    let loaded = load_or_create(&path).expect("load saved permission override");
    assert_eq!(
        loaded
            .permissions
            .state("https://example.com", PermissionKind::Camera),
        PermissionState::Allow
    );
    write_permission(
        &path,
        "https://example.com",
        PermissionKind::Camera,
        PermissionState::Ask,
    )
    .expect("remove the permission override");
    let loaded = load_or_create(&path).expect("load cleared permission overrides");
    assert_eq!(loaded.permissions, PermissionStore::default());
    fs::remove_file(path).expect("remove permission test config");
}

#[test]
fn permission_defaults_block_kinds_and_keep_site_exceptions() {
    let load = parse(
        "[permission_defaults]\nlocation = \"ask\"\nnotifications = \"block\"\ncamera = \"block\"\n\
         [permissions.\"https://example.com\"]\nnotifications = \"allow\"\n",
    )
    .expect("parse permission defaults");
    assert!(
        load.permissions
            .default_blocked(PermissionKind::Notifications)
    );
    assert!(!load.permissions.default_blocked(PermissionKind::Location));
    assert!(!load.permissions.default_blocked(PermissionKind::Camera));
    assert_eq!(
        load.permissions
            .state("https://example.com", PermissionKind::Notifications),
        PermissionState::Allow
    );
    assert_eq!(load.issues.len(), 1);
}

#[test]
fn internal_pages_cannot_be_persisted_as_site_permissions() {
    let path = std::env::temp_dir().join(format!(
        "elaris-internal-permissions-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(
        write_permission(
            &path,
            "elaris://settings",
            PermissionKind::Camera,
            PermissionState::Allow,
        )
        .is_err()
    );
    assert!(!path.exists());
}

#[test]
fn loading_a_missing_file_creates_the_readable_default_configuration() {
    let path = std::env::temp_dir().join(format!(
        "elaris-config-default-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let config = load_or_create(&path).expect("generate a default config");
    assert_eq!(config.settings, Settings::default());
    assert!(path.exists());
    assert!(
        fs::read_to_string(&path)
            .expect("read generated config")
            .contains("engine = \"duckduckgo\"")
    );
    fs::remove_file(path).expect("remove generated config");
}

#[test]
fn one_setting_write_preserves_comments_and_unknown_keys() {
    let path = std::env::temp_dir().join(format!(
        "elaris-config-preserve-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(
        &path,
        "# keep this note\n[appearance]\n# keep this theme comment\ntheme = \"system\" # inline note\ncustom = \"kept\"\n",
    )
    .expect("write test config");
    write_change(&path, &SettingsChange::Appearance(Appearance::Dark))
        .expect("update a single setting");
    let contents = fs::read_to_string(&path).expect("read updated config");
    assert!(contents.contains("# keep this note"));
    assert!(contents.contains("# keep this theme comment"));
    assert!(contents.contains("# inline note"));
    assert!(contents.contains("custom = \"kept\""));
    assert!(contents.contains("theme = \"dark\""));
    fs::remove_file(path).expect("remove test config");
}

#[test]
fn every_setting_change_round_trips_through_the_shared_model() {
    let path = std::env::temp_dir().join(format!(
        "elaris-config-round-trip-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, DEFAULT_CONFIG).expect("write initial config");
    for change in [
        SettingsChange::PageZoom(125),
        SettingsChange::Appearance(Appearance::Dark),
        SettingsChange::AccentColor(AccentColor::Pink),
        SettingsChange::SearchEngine(SearchEngine::Brave),
        SettingsChange::DownloadDirectory(DownloadDirectory::Custom(PathBuf::from(
            "/tmp/elaris-downloads",
        ))),
        SettingsChange::Startup(StartupMode::BlankWindow),
    ] {
        write_change(&path, &change).expect("persist one setting");
    }

    let loaded = load_or_create(&path).expect("reload changed config");
    assert_eq!(loaded.settings.page_zoom, 125);
    assert_eq!(loaded.settings.appearance, Appearance::Dark);
    assert_eq!(loaded.settings.accent_color, AccentColor::Pink);
    assert_eq!(loaded.settings.search_engine, SearchEngine::Brave);
    assert_eq!(
        loaded.settings.download_directory,
        DownloadDirectory::Custom(PathBuf::from("/tmp/elaris-downloads"))
    );
    assert_eq!(loaded.settings.startup, StartupMode::BlankWindow);
    fs::remove_file(path).expect("remove round-trip config");
}

#[test]
fn glass_style_and_grain_default_round_trip_and_reject_bad_values() {
    let defaults = parse("").unwrap().settings;
    assert_eq!(defaults.glass_style, GlassStyle::Liquid);
    assert_eq!(defaults.grain, 0);

    let path = std::env::temp_dir().join(format!(
        "elaris-config-glass-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, DEFAULT_CONFIG).expect("write initial config");
    write_change(&path, &SettingsChange::GlassStyle(GlassStyle::Frosted)).unwrap();
    write_change(&path, &SettingsChange::Grain(40)).unwrap();
    let loaded = load_or_create(&path).expect("reload changed config");
    assert_eq!(loaded.settings.glass_style, GlassStyle::Frosted);
    assert_eq!(loaded.settings.grain, 40);
    fs::remove_file(path).expect("remove glass config");

    let invalid = parse("[appearance]\nglass_style = 'clear'\ngrain = 101").unwrap();
    assert_eq!(invalid.settings.glass_style, GlassStyle::Liquid);
    assert_eq!(invalid.settings.grain, 0);
    assert_eq!(invalid.issues.len(), 2);
}

#[test]
fn a_malformed_file_is_never_overwritten() {
    let path = std::env::temp_dir().join(format!(
        "elaris-config-malformed-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, "[appearance\nthis is broken").expect("write malformed config");
    assert!(write_change(&path, &SettingsChange::Appearance(Appearance::Dark)).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[appearance\nthis is broken"
    );
    fs::remove_file(path).expect("remove malformed config");
}

#[test]
fn atomic_config_replacement_leaves_no_temporary_files() {
    let directory = std::env::temp_dir().join(format!(
        "elaris-config-atomic-{}-{}",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = directory.join("config.toml");
    fs::create_dir_all(&directory).expect("create temporary config directory");
    fs::write(&path, DEFAULT_CONFIG).expect("write initial config");
    write_change(&path, &SettingsChange::Startup(StartupMode::BlankWindow))
        .expect("atomically replace config");
    assert!(
        fs::read_to_string(path)
            .expect("read replaced config")
            .contains("mode = \"blank\"")
    );
    assert_eq!(
        fs::read_dir(&directory)
            .expect("list config directory")
            .count(),
        1
    );
    fs::remove_dir_all(directory).expect("remove temporary config directory");
}

#[test]
fn force_dark_defaults_to_off_and_round_trips() {
    let path = std::env::temp_dir().join(format!(
        "elaris-force-dark-{}-{}.toml",
        std::process::id(),
        NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let loaded = load_or_create(&path).expect("create the default configuration");
    assert!(!loaded.settings.force_dark);

    write_change(&path, &SettingsChange::ForceDark(true)).expect("turn force dark on");
    let loaded = load_or_create(&path).expect("load force dark");
    assert!(loaded.settings.force_dark);
    assert!(loaded.issues.is_empty());
    let _ = std::fs::remove_file(path);
}
