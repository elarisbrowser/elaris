// SPDX-License-Identifier: GPL-3.0-only

//! Saves single settings and site permissions without losing comments, then replaces the file atomically.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::address::CustomSearchEngine;
use crate::appearance::Appearance;
use crate::keybindings::KeyBindings;
use crate::permissions::{AutoplayPolicy, PermissionKind, PermissionState, PermissionStore};
use crate::settings::{DownloadDirectory, SettingsChange, StartupMode};

use super::{AUTOPLAY_KEY, DEFAULT_CONFIG};

pub(super) static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

/// Saves or removes one site's permission override while preserving other config values.
pub fn write_permission(
    path: &Path,
    origin: &str,
    kind: PermissionKind,
    state: PermissionState,
) -> Result<(), String> {
    let value = (state != PermissionState::Ask).then(|| state.key());
    write_site_setting(path, origin, kind.key(), value)
}

/// Saves or removes one site's autoplay exception while preserving other config values.
pub fn write_autoplay(
    path: &Path,
    origin: &str,
    policy: Option<AutoplayPolicy>,
) -> Result<(), String> {
    write_site_setting(path, origin, AUTOPLAY_KEY, policy.map(AutoplayPolicy::key))
}

fn write_site_setting(
    path: &Path,
    origin: &str,
    key: &str,
    value: Option<&str>,
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
    if let Some(value) = value {
        if sites.get(&origin).is_none() {
            sites.insert(&origin, Item::Table(Table::new()));
        }
        let site = sites
            .get_mut(&origin)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| format!("[permissions.{origin:?}] must be a TOML table"))?;
        site.insert(key, Value::from(value).into());
    } else {
        if let Some(site) = sites.get_mut(&origin).and_then(Item::as_table_mut) {
            site.remove(key);
            if site.is_empty() {
                sites.remove(&origin);
            }
        }
    }
    if sites.is_empty() {
        document.as_table_mut().remove("permissions");
    }
    write_atomic(path, document.to_string().as_bytes()).map_err(|error| {
        format!(
            "cannot save configuration {}: {error}",
            crate::paths::display_path(path)
        )
    })
}

/// Updates one known setting while preserving unrelated keys, comments and formatting.
pub fn write_change(path: &Path, change: &SettingsChange) -> Result<(), String> {
    let document = match fs::read_to_string(path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot update malformed TOML: {error}"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => DEFAULT_CONFIG
            .parse::<DocumentMut>()
            .map_err(|error| format!("cannot parse built-in configuration: {error}"))?,
        Err(error) => return Err(format!("cannot read configuration: {error}")),
    };
    if let SettingsChange::SearchEngines(engines) = change {
        return write_search_engines(path, document, engines);
    }
    if let SettingsChange::KeyBindings(bindings) = change {
        return write_keybindings(path, document, bindings);
    }
    if let SettingsChange::KeepAwakeSites(sites) = change {
        let sites: toml_edit::Array = sites.iter().map(String::as_str).collect();
        return put(
            path,
            document,
            "performance",
            "keep_awake",
            Value::Array(sites),
        );
    }
    let (section, key, value) = match change {
        SettingsChange::KeepAwakeSites(_)
        | SettingsChange::SearchEngines(_)
        | SettingsChange::KeyBindings(_) => unreachable!("lists and tables are saved above"),
        SettingsChange::PageZoom(value) => ("pages", "zoom", value.to_string()),
        SettingsChange::Autoplay(value) => ("pages", "autoplay", value.key().to_owned()),
        SettingsChange::TabLayout(value) => {
            ("appearance", "tab_layout", value.config_key().to_owned())
        }
        SettingsChange::AddressBar(value) => {
            ("appearance", "address_bar", value.config_key().to_owned())
        }
        SettingsChange::CompactTabs(value) => ("appearance", "compact_tabs", value.to_string()),
        SettingsChange::OpenTabOnStartup(value) => {
            ("appearance", "open_tab_on_startup", value.to_string())
        }
        SettingsChange::SidebarSide(value) => {
            ("appearance", "sidebar_side", value.config_key().to_owned())
        }
        SettingsChange::NewTabPosition(value) => (
            "appearance",
            "new_tab_position",
            value.config_key().to_owned(),
        ),
        SettingsChange::BookmarksBar(value) => ("appearance", "bookmarks_bar", value.to_string()),
        SettingsChange::PageStyle(value) => {
            ("appearance", "page_style", value.config_key().to_owned())
        }
        SettingsChange::WindowDecorations(value) => {
            ("linux", "decorations", value.config_key().to_owned())
        }
        SettingsChange::SplitViewLimit(value) => {
            ("split_view", "max_panes", (*value).clamp(2, 64).to_string())
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
        SettingsChange::SearchEngine(key) => ("search", "engine", key.clone()),
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
        SettingsChange::GlassStyle(value) => {
            ("appearance", "glass_style", value.config_key().to_owned())
        }
        SettingsChange::TintOpacity(value) => ("appearance", "tint_opacity", value.to_string()),
        SettingsChange::GrainEnabled(value) => ("appearance", "grain_enabled", value.to_string()),
        SettingsChange::Grain(value) => ("appearance", "grain", value.to_string()),
        SettingsChange::ForceDark(value) => ("pages", "force_dark", value.to_string()),
        SettingsChange::ReduceMotion(value) => ("performance", "reduce_motion", value.to_string()),
        SettingsChange::MemorySaver(value) => {
            ("performance", "memory_saver", value.config_key().to_owned())
        }
        SettingsChange::ContentBlocking(value) => {
            ("content_blocking", "enabled", value.to_string())
        }
        SettingsChange::ContentBlockingList(list, value) => {
            ("content_blocking", list.config_key(), value.to_string())
        }
    };
    let new_value = if section == "content_blocking"
        || key == "glass"
        || key == "grain_enabled"
        || key == "reduce_motion"
        || key == "force_dark"
        || key == "bookmarks_bar"
        || key == "compact_tabs"
        || key == "open_tab_on_startup"
    {
        Value::from(value == "true")
    } else if matches!(key, "tint_opacity" | "grain" | "max_panes" | "zoom") {
        Value::from(
            value
                .parse::<i64>()
                .map_err(|_| format!("{key} must be a whole number"))?,
        )
    } else {
        Value::from(value)
    };
    put(path, document, section, key, new_value)
}

/// Rewrites `[search.engines]`, one `[search.engines.<key>]` table per provider.
fn write_search_engines(
    path: &Path,
    mut document: DocumentMut,
    engines: &[CustomSearchEngine],
) -> Result<(), String> {
    let search = document
        .as_table_mut()
        .entry("search")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| "[search] must be a TOML table".to_owned())?;
    if engines.is_empty() {
        search.remove("engines");
    } else {
        let table = emptied_table(search, "engines", "[search.engines]")?;
        // Only the providers' own headers show, not a bare [search.engines].
        table.set_implicit(true);
        for engine in engines {
            let mut entry = Table::new();
            entry.insert("name", Item::Value(Value::from(engine.name())));
            entry.insert("url", Item::Value(Value::from(engine.url())));
            table.insert(engine.key(), Item::Table(entry));
        }
    }
    save(path, &document)
}

/// Rewrites `[keybindings]`: one chord as a string, several as a list, none as `""`.
fn write_keybindings(
    path: &Path,
    mut document: DocumentMut,
    bindings: &KeyBindings,
) -> Result<(), String> {
    if bindings.is_empty() {
        document.as_table_mut().remove("keybindings");
    } else {
        let table = emptied_table(document.as_table_mut(), "keybindings", "[keybindings]")?;
        for (action, chords) in bindings {
            let value = match chords.as_slice() {
                [] => Value::from(""),
                [chord] => Value::from(chord.to_string()),
                chords => Value::Array(chords.iter().map(ToString::to_string).collect()),
            };
            table.insert(action, Item::Value(value));
        }
    }
    save(path, &document)
}

/// The table at `key`, created if missing and emptied if not, so its header comment survives.
fn emptied_table<'a>(
    parent: &'a mut Table,
    key: &str,
    name: &str,
) -> Result<&'a mut Table, String> {
    let table = parent
        .entry(key)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("{name} must be a TOML table"))?;
    table.clear();
    Ok(table)
}

fn save(path: &Path, document: &DocumentMut) -> Result<(), String> {
    write_atomic(path, document.to_string().as_bytes()).map_err(|error| {
        format!(
            "cannot save configuration {}: {error}",
            crate::paths::display_path(path)
        )
    })
}

/// Sets one value, keeping the comments and spacing around the one it replaces, and saves the file.
fn put(
    path: &Path,
    mut document: DocumentMut,
    section: &str,
    key: &str,
    mut new_value: Value,
) -> Result<(), String> {
    let item = document
        .as_table_mut()
        .entry(section)
        .or_insert(Item::Table(toml_edit::Table::new()));
    let table = item
        .as_table_mut()
        .ok_or_else(|| format!("[{section}] must be a TOML table"))?;
    if let Some(item) = table.get_mut(key) {
        if let Some(old_value) = item.as_value() {
            *new_value.decor_mut() = old_value.decor().clone();
        }
        *item = Item::Value(new_value);
    } else {
        table.insert(key, Item::Value(new_value));
    }

    write_atomic(path, document.to_string().as_bytes()).map_err(|error| {
        format!(
            "cannot save configuration {}: {error}",
            crate::paths::display_path(path)
        )
    })
}

pub(super) fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
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
