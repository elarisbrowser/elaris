// SPDX-License-Identifier: GPL-3.0-only

//! Typed settings and comment-preserving TOML configuration persistence.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::address::SearchEngine;
use crate::appearance::{AccentColor, Appearance};
use crate::permissions::{PermissionKind, PermissionState, PermissionStore};
use crate::settings::{DownloadDirectory, Settings, SettingsChange, StartupMode, TabLayout};

const DEFAULT_CONFIG: &str = "# Elaris Browser configuration\n\n[appearance]\ntheme = \"system\"\naccent = \"blue\"\nglass = true\ntint_opacity = 100\ntab_layout = \"vertical\"\n\n[search]\nengine = \"duckduckgo\"\n\n[downloads]\ndirectory = \"default\"\n\n[startup]\nmode = \"restore\"\n\n[performance]\nreduce_motion = false\n";
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigLoad {
    pub(crate) settings: Settings,
    pub(crate) permissions: PermissionStore,
    pub(crate) issues: Vec<String>,
}

/// Loads `config.toml`, creating a default file only when it does not yet exist.
pub(crate) fn load_or_create(path: &Path) -> Result<ConfigLoad, String> {
    match fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            write_atomic(path, DEFAULT_CONFIG.as_bytes()).map_err(|error| {
                format!("cannot create configuration {}: {error}", path.display())
            })?;
            parse(DEFAULT_CONFIG)
        }
        Err(error) => Err(format!(
            "cannot read configuration {}: {error}",
            path.display()
        )),
    }
}

/// Parses valid settings independently so one invalid value does not discard other preferences.
pub(crate) fn parse(contents: &str) -> Result<ConfigLoad, String> {
    let document = contents
        .parse::<DocumentMut>()
        .map_err(|error| format!("malformed TOML: {error}"))?;
    let defaults = Settings::default();
    let mut settings = defaults.clone();
    let mut issues = Vec::new();

    if let Some(value) = string_value(&document, "appearance", "tab_layout", &mut issues) {
        if let Some(layout) = TabLayout::ALL
            .into_iter()
            .find(|layout| layout.config_key() == value)
        {
            settings.tab_layout = layout;
        } else {
            issues.push(format!("Invalid value for [appearance].tab_layout: {value:?}; expected vertical or horizontal"));
        }
    }
    if let Some(value) = string_value(&document, "appearance", "theme", &mut issues) {
        settings.appearance = match value.as_str() {
            "system" => Appearance::System,
            "light" => Appearance::Light,
            "dark" => Appearance::Dark,
            _ => {
                issues.push(format!(
                    "Invalid value for [appearance].theme: {value:?}; expected system, light or dark"
                ));
                defaults.appearance
            }
        };
    }
    if let Some(value) = string_value(&document, "appearance", "accent", &mut issues) {
        settings.accent_color = AccentColor::from_config_key(&value).unwrap_or_else(|| {
            issues.push(format!(
                "Invalid value for [appearance].accent: {value:?}; expected blue, purple, pink, orange, green or gray"
            ));
            defaults.accent_color
        });
    }
    if let Some(value) = bool_value(&document, "appearance", "glass", &mut issues) {
        settings.glass_enabled = value;
    }
    if let Some(value) = integer_value(&document, "appearance", "tint_opacity", &mut issues) {
        if (0..=100).contains(&value) {
            settings.tint_opacity = value as u8;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].tint_opacity: {value}; expected 0 through 100"
            ));
        }
    }
    if let Some(value) = bool_value(&document, "performance", "reduce_motion", &mut issues) {
        settings.reduce_motion = value;
    }
    if let Some(value) = string_value(&document, "search", "engine", &mut issues) {
        settings.search_engine = SearchEngine::from_config_key(&value).unwrap_or_else(|| {
            issues.push(format!(
                "Invalid value for [search].engine: {value:?}; expected duckduckgo, google, bing or brave"
            ));
            defaults.search_engine
        });
    }
    if let Some(value) = string_value(&document, "downloads", "directory", &mut issues) {
        settings.download_directory = if value == "default" {
            DownloadDirectory::Default
        } else {
            match DownloadDirectory::custom(PathBuf::from(&value)) {
                Ok(directory) => directory,
                Err(error) => {
                    issues.push(format!(
                        "Invalid value for [downloads].directory: {value:?}; {error}"
                    ));
                    defaults.download_directory
                }
            }
        };
    }
    if let Some(value) = string_value(&document, "startup", "mode", &mut issues) {
        settings.startup = match value.as_str() {
            "restore" => StartupMode::RestoreSession,
            "blank" => StartupMode::BlankWindow,
            _ => {
                issues.push(format!(
                    "Invalid value for [startup].mode: {value:?}; expected restore or blank"
                ));
                defaults.startup
            }
        };
    }

    let permissions = parse_permissions(&document, &mut issues);
    Ok(ConfigLoad {
        settings,
        permissions,
        issues,
    })
}

fn parse_permissions(document: &DocumentMut, issues: &mut Vec<String>) -> PermissionStore {
    let mut store = PermissionStore::default();
    let Some(item) = document.get("permissions") else {
        return store;
    };
    let Some(sites) = item.as_table() else {
        issues.push("[permissions] must be a TOML table".to_owned());
        return store;
    };
    for (origin, item) in sites {
        let Some(normalized) = PermissionStore::origin(origin) else {
            issues.push(format!("Ignoring invalid permission origin {origin:?}"));
            continue;
        };
        let Some(values) = item.as_table() else {
            issues.push(format!("[permissions.{origin:?}] must be a TOML table"));
            continue;
        };
        for (name, value) in values {
            let Some(kind) = PermissionKind::from_key(name) else {
                issues.push(format!("Ignoring unknown permission {name:?} for {origin}"));
                continue;
            };
            match value.as_str().and_then(PermissionState::from_key) {
                Some(PermissionState::Ask) => {}
                Some(state) => store.set(&normalized, kind, state),
                None => issues.push(format!(
                    "[permissions.{origin:?}].{name} must be allow, block or ask"
                )),
            }
        }
    }
    store
}

/// Saves or removes one site's permission override while preserving other config values.
pub(crate) fn write_permission(
    path: &Path,
    origin: &str,
    kind: PermissionKind,
    state: PermissionState,
) -> Result<(), String> {
    let origin = PermissionStore::origin(origin)
        .ok_or_else(|| "website permissions require an HTTP or HTTPS origin".to_owned())?;
    let mut document = match fs::read_to_string(path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot update malformed TOML: {error}"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => DEFAULT_CONFIG
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot parse built-in configuration: {error}"))?,
        Err(error) => return Err(format!("cannot read configuration: {error}")),
    };
    if document.get("permissions").is_none() {
        document["permissions"] = Item::Table(Table::new());
    }
    let sites = document["permissions"]
        .as_table_mut()
        .ok_or_else(|| "[permissions] must be a TOML table".to_owned())?;
    if state == PermissionState::Ask {
        if let Some(site) = sites.get_mut(&origin).and_then(Item::as_table_mut) {
            site.remove(kind.key());
            if site.is_empty() {
                sites.remove(&origin);
            }
        }
    } else {
        if sites.get(&origin).is_none() {
            sites.insert(&origin, Item::Table(Table::new()));
        }
        let site = sites
            .get_mut(&origin)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| format!("[permissions.{origin:?}] must be a TOML table"))?;
        site.insert(kind.key(), Value::from(state.key()).into());
    }
    if sites.is_empty() {
        document.as_table_mut().remove("permissions");
    }
    write_atomic(path, document.to_string().as_bytes())
        .map_err(|error| format!("cannot save configuration {}: {error}", path.display()))
}

fn string_value(
    document: &DocumentMut,
    section: &str,
    key: &str,
    issues: &mut Vec<String>,
) -> Option<String> {
    let section_item = document.get(section)?;
    let Some(table) = section_item.as_table() else {
        issues.push(format!("[{section}] must be a TOML table"));
        return None;
    };
    let item = table.get(key)?;
    match item.as_str() {
        Some(value) => Some(value.to_owned()),
        None => {
            issues.push(format!("[{section}].{key} must be a string"));
            None
        }
    }
}

fn scalar_item<'a>(
    document: &'a DocumentMut,
    section: &str,
    key: &str,
    issues: &mut Vec<String>,
) -> Option<&'a Item> {
    let section_item = document.get(section)?;
    let Some(table) = section_item.as_table() else {
        issues.push(format!("[{section}] must be a TOML table"));
        return None;
    };
    table.get(key)
}

fn bool_value(
    document: &DocumentMut,
    section: &str,
    key: &str,
    issues: &mut Vec<String>,
) -> Option<bool> {
    let item = scalar_item(document, section, key, issues)?;
    match item.as_bool() {
        Some(value) => Some(value),
        None => {
            issues.push(format!("[{section}].{key} must be a boolean"));
            None
        }
    }
}

fn integer_value(
    document: &DocumentMut,
    section: &str,
    key: &str,
    issues: &mut Vec<String>,
) -> Option<i64> {
    let item = scalar_item(document, section, key, issues)?;
    match item.as_integer() {
        Some(value) => Some(value),
        None => {
            issues.push(format!("[{section}].{key} must be an integer"));
            None
        }
    }
}

/// Updates one known setting while preserving unrelated keys, comments and formatting.
pub(crate) fn write_change(path: &Path, change: &SettingsChange) -> Result<(), String> {
    let mut document = match fs::read_to_string(path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot update malformed TOML: {error}"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => DEFAULT_CONFIG
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot parse built-in configuration: {error}"))?,
        Err(error) => return Err(format!("cannot read configuration: {error}")),
    };
    let (section, key, value) = match change {
        SettingsChange::TabLayout(value) => {
            ("appearance", "tab_layout", value.config_key().to_owned())
        }
        SettingsChange::Appearance(value) => (
            "appearance",
            "theme",
            match value {
                Appearance::System => "system".to_owned(),
                Appearance::Light => "light".to_owned(),
                Appearance::Dark => "dark".to_owned(),
            },
        ),
        SettingsChange::AccentColor(value) => {
            ("appearance", "accent", value.config_key().to_owned())
        }
        SettingsChange::SearchEngine(value) => ("search", "engine", value.config_key().to_owned()),
        SettingsChange::DownloadDirectory(value) => (
            "downloads",
            "directory",
            match value {
                DownloadDirectory::Default => "default".to_owned(),
                DownloadDirectory::Custom(path) => path
                    .to_str()
                    .ok_or_else(|| "download path is not valid UTF-8".to_owned())?
                    .to_owned(),
            },
        ),
        SettingsChange::Startup(value) => (
            "startup",
            "mode",
            match value {
                StartupMode::RestoreSession => "restore".to_owned(),
                StartupMode::BlankWindow => "blank".to_owned(),
            },
        ),
        SettingsChange::GlassEnabled(value) => ("appearance", "glass", value.to_string()),
        SettingsChange::TintOpacity(value) => ("appearance", "tint_opacity", value.to_string()),
        SettingsChange::ReduceMotion(value) => ("performance", "reduce_motion", value.to_string()),
    };
    let item = document
        .as_table_mut()
        .entry(section)
        .or_insert(Item::Table(toml_edit::Table::new()));
    let table = item
        .as_table_mut()
        .ok_or_else(|| format!("[{section}] must be a TOML table"))?;
    let mut new_value = if key == "glass" || key == "reduce_motion" {
        Value::from(value == "true")
    } else if key == "tint_opacity" {
        Value::from(value.parse::<i64>().expect("opacity is an integer"))
    } else {
        Value::from(value)
    };
    if let Some(item) = table.get_mut(key) {
        if let Some(old_value) = item.as_value() {
            *new_value.decor_mut() = old_value.decor().clone();
        }
        *item = Item::Value(new_value);
    } else {
        table.insert(key, Item::Value(new_value));
    }

    write_atomic(path, document.to_string().as_bytes())
        .map_err(|error| format!("cannot save configuration {}: {error}", path.display()))
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let id = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_file_name(format!(
        ".config-{}-{timestamp}-{id}.tmp",
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
        file.write_all(contents)?;
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
}
