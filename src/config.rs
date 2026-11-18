// SPDX-License-Identifier: GPL-3.0-only

//! Typed settings and comment-preserving TOML configuration persistence.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use toml_edit::{DocumentMut, Item, Value};

use crate::address::SearchEngine;
use crate::appearance::Appearance;
use crate::settings::{DownloadDirectory, Settings, SettingsChange, StartupMode};

const DEFAULT_CONFIG: &str = "# Elaris Browser configuration\n\n[appearance]\ntheme = \"system\"\n\n[search]\nengine = \"duckduckgo\"\n\n[downloads]\ndirectory = \"default\"\n\n[startup]\nmode = \"restore\"\n";
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigLoad {
    pub(crate) settings: Settings,
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
            "blank" => StartupMode::NewTab,
            _ => {
                issues.push(format!(
                    "Invalid value for [startup].mode: {value:?}; expected restore or blank"
                ));
                defaults.startup
            }
        };
    }

    Ok(ConfigLoad { settings, issues })
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
        SettingsChange::Appearance(value) => (
            "appearance",
            "theme",
            match value {
                Appearance::System => "system".to_owned(),
                Appearance::Light => "light".to_owned(),
                Appearance::Dark => "dark".to_owned(),
            },
        ),
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
                StartupMode::NewTab => "blank".to_owned(),
            },
        ),
    };
    let item = document
        .as_table_mut()
        .entry(section)
        .or_insert(Item::Table(toml_edit::Table::new()));
    let table = item
        .as_table_mut()
        .ok_or_else(|| format!("[{section}] must be a TOML table"))?;
    let mut new_value = Value::from(value);
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
        assert_eq!(config.settings.startup, StartupMode::NewTab);
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
            SettingsChange::SearchEngine(SearchEngine::Brave),
            SettingsChange::DownloadDirectory(DownloadDirectory::Custom(PathBuf::from(
                "/tmp/elaris-downloads",
            ))),
            SettingsChange::Startup(StartupMode::NewTab),
        ] {
            write_change(&path, &change).expect("persist one setting");
        }

        let loaded = load_or_create(&path).expect("reload changed config");
        assert_eq!(loaded.settings.appearance, Appearance::Dark);
        assert_eq!(loaded.settings.search_engine, SearchEngine::Brave);
        assert_eq!(
            loaded.settings.download_directory,
            DownloadDirectory::Custom(PathBuf::from("/tmp/elaris-downloads"))
        );
        assert_eq!(loaded.settings.startup, StartupMode::NewTab);
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
        write_change(&path, &SettingsChange::Startup(StartupMode::NewTab))
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
