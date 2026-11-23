// SPDX-License-Identifier: GPL-3.0-only

//! Loads `config.toml` into typed settings, collecting recoverable validation issues.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use crate::address::{CustomSearchEngine, SearchEngine};
use crate::appearance::{AccentColor, Appearance, GlassStyle};
use crate::keybindings::{Chord, KeyBindings};
use crate::permissions::{AutoplayPolicy, PermissionKind, PermissionState, PermissionStore};
use crate::settings::{
    AddressBarPosition, ContentBlockList, DownloadDirectory, NewTabPosition, PageStyle, Settings,
    SidebarSide, StartupMode, TabLayout, WindowDecorations,
};
use elaris_memory_saver::MemorySaver;

use super::write::write_atomic;
use super::{AUTOPLAY_KEY, DEFAULT_CONFIG};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigLoad {
    pub settings: Settings,
    pub permissions: PermissionStore,
    pub issues: Vec<String>,
}

/// Loads `config.toml`, creating a default file only when it does not yet exist.
pub fn load_or_create(path: &Path) -> Result<ConfigLoad, String> {
    match fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            write_atomic(path, DEFAULT_CONFIG.as_bytes()).map_err(|error| {
                format!(
                    "cannot create configuration {}: {error}",
                    crate::paths::display_path(path)
                )
            })?;
            parse(DEFAULT_CONFIG)
        }
        Err(error) => Err(format!(
            "cannot read configuration {}: {error}",
            crate::paths::display_path(path)
        )),
    }
}

/// Parses valid settings independently so one invalid value does not discard other preferences.
pub fn parse(contents: &str) -> Result<ConfigLoad, String> {
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
    if let Some(value) = string_value(&document, "appearance", "address_bar", &mut issues) {
        if let Some(position) = AddressBarPosition::ALL
            .into_iter()
            .find(|position| position.config_key() == value)
        {
            settings.address_bar = position;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].address_bar: {value:?}; expected sidebar or toolbar"
            ));
        }
    }
    if let Some(value) = bool_value(&document, "appearance", "compact_tabs", &mut issues) {
        settings.compact_tabs = value;
    }
    if let Some(value) = string_value(&document, "appearance", "sidebar_side", &mut issues) {
        if let Some(side) = SidebarSide::ALL
            .into_iter()
            .find(|side| side.config_key() == value)
        {
            settings.sidebar_side = side;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].sidebar_side: {value:?}; expected left or right"
            ));
        }
    }
    if let Some(value) = string_value(&document, "appearance", "new_tab_position", &mut issues) {
        if let Some(position) = NewTabPosition::ALL
            .into_iter()
            .find(|position| position.config_key() == value)
        {
            settings.new_tab_position = position;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].new_tab_position: {value:?}; expected top or bottom"
            ));
        }
    }
    if let Some(value) = bool_value(&document, "appearance", "bookmarks_bar", &mut issues) {
        settings.bookmarks_bar = value;
    }
    if let Some(value) = string_value(&document, "appearance", "page_style", &mut issues) {
        if let Some(style) = PageStyle::ALL
            .into_iter()
            .find(|style| style.config_key() == value)
        {
            settings.page_style = style;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].page_style: {value:?}; expected card or edge_to_edge"
            ));
        }
    }
    if let Some(value) = string_value(&document, "linux", "decorations", &mut issues) {
        if let Some(mode) = WindowDecorations::from_config_key(&value) {
            settings.window_decorations = mode;
        } else {
            issues.push(format!(
                "Invalid value for [linux].decorations: {value:?}; expected auto, system or elaris"
            ));
        }
    }
    if let Some(value) = integer_value(&document, "split_view", "max_panes", &mut issues) {
        if (2..=64).contains(&value) {
            settings.split_view_limit = value as usize;
        } else {
            issues.push(format!(
                "Invalid value for [split_view].max_panes: {value}; expected 2 through 64"
            ));
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
    if let Some(value) = string_value(&document, "appearance", "glass_style", &mut issues) {
        settings.glass_style = GlassStyle::from_config_key(&value).unwrap_or_else(|| {
            issues.push(format!(
                "Invalid value for [appearance].glass_style: {value:?}; expected frosted or liquid"
            ));
            defaults.glass_style
        });
    }
    if let Some(value) = bool_value(&document, "appearance", "grain_enabled", &mut issues) {
        settings.grain_enabled = value;
    }
    if let Some(value) = integer_value(&document, "appearance", "grain", &mut issues) {
        if (0..=100).contains(&value) {
            settings.grain = value as u8;
        } else {
            issues.push(format!(
                "Invalid value for [appearance].grain: {value}; expected 0 through 100"
            ));
        }
    }
    if let Some(value) = integer_value(&document, "pages", "zoom", &mut issues) {
        if (30..=500).contains(&value) {
            settings.page_zoom = value as u16;
        } else {
            issues.push(format!(
                "Invalid value for [pages].zoom: {value}; expected 30 through 500"
            ));
        }
    }
    if let Some(value) = string_value(&document, "pages", "autoplay", &mut issues) {
        settings.autoplay = AutoplayPolicy::from_key(&value).unwrap_or_else(|| {
            issues.push(format!(
                "Invalid value for [pages].autoplay: {value:?}; expected allow, block-audio or block"
            ));
            defaults.autoplay
        });
    }
    if let Some(value) = bool_value(&document, "pages", "force_dark", &mut issues) {
        settings.force_dark = value;
    }
    if let Some(value) = bool_value(&document, "performance", "reduce_motion", &mut issues) {
        settings.reduce_motion = value;
    }
    if let Some(item) = scalar_item(&document, "performance", "keep_awake", &mut issues) {
        match item.as_array() {
            Some(sites) => {
                for site in sites {
                    match site.as_str().and_then(elaris_memory_saver::site_of) {
                        Some(site) if !settings.keep_awake_sites.contains(&site) => {
                            settings.keep_awake_sites.push(site);
                        }
                        Some(_) => {}
                        None => {
                            issues.push(format!("Invalid site in [performance].keep_awake: {site}"))
                        }
                    }
                }
            }
            None => issues.push("[performance].keep_awake must be a list of sites".to_owned()),
        }
    }
    if let Some(value) = string_value(&document, "performance", "memory_saver", &mut issues) {
        if let Some(mode) = MemorySaver::from_config_key(&value) {
            settings.memory_saver = mode;
        } else {
            issues.push(format!(
                "Invalid value for [performance].memory_saver: {value:?}; expected off, balanced or maximum"
            ));
        }
    }
    if let Some(value) = bool_value(&document, "content_blocking", "enabled", &mut issues) {
        settings.content_blocking.enabled = value;
    }
    for list in ContentBlockList::ALL {
        if let Some(value) = bool_value(
            &document,
            "content_blocking",
            list.config_key(),
            &mut issues,
        ) {
            if value {
                settings.content_blocking.lists.insert(list);
            } else {
                settings.content_blocking.lists.remove(&list);
            }
        }
    }
    settings.custom_search_engines = parse_search_engines(&document, &mut issues);
    if let Some(value) = string_value(&document, "search", "engine", &mut issues) {
        let custom = &settings.custom_search_engines;
        settings.search_engine =
            SearchEngine::from_config_key(&value, custom).unwrap_or_else(|| {
                let mut expected = String::from("duckduckgo, google, bing, brave");
                for engine in custom {
                    expected.push_str(", ");
                    expected.push_str(engine.key());
                }
                issues.push(format!(
                    "Invalid value for [search].engine: {value:?}; expected one of {expected}"
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

    settings.keybindings = parse_keybindings(&document, &mut issues);
    let permissions = parse_permissions(&document, &mut issues);
    Ok(ConfigLoad {
        settings,
        permissions,
        issues,
    })
}

/// Reads `[search.engines.<key>]` tables, each with a `url` holding `%s` and an optional `name`.
fn parse_search_engines(
    document: &DocumentMut,
    issues: &mut Vec<String>,
) -> Vec<CustomSearchEngine> {
    let Some(item) = document
        .get("search")
        .and_then(|search| search.get("engines"))
    else {
        return Vec::new();
    };
    let Some(engines) = item.as_table() else {
        issues.push("[search.engines] must be a TOML table".to_owned());
        return Vec::new();
    };
    let mut parsed = Vec::new();
    for (key, item) in engines {
        let Some(table) = item.as_table() else {
            issues.push(format!("[search.engines.{key}] must be a TOML table"));
            continue;
        };
        let Some(url) = table.get("url").and_then(Item::as_str) else {
            issues.push(format!("[search.engines.{key}].url must be a string"));
            continue;
        };
        let name = match table.get("name") {
            None => "",
            Some(name) => match name.as_str() {
                Some(name) => name,
                None => {
                    issues.push(format!("[search.engines.{key}].name must be a string"));
                    ""
                }
            },
        };
        match CustomSearchEngine::new(key, name, url) {
            Ok(engine) => parsed.push(engine),
            Err(error) => issues.push(format!("Ignoring search engine {key:?}: {error}")),
        }
    }
    parsed
}

/// Reads `[keybindings]`: each action takes one chord, a list of them, or `""` to remove its keys.
/// An action with any invalid chord keeps its default keys.
fn parse_keybindings(document: &DocumentMut, issues: &mut Vec<String>) -> KeyBindings {
    let mut bindings = KeyBindings::new();
    let Some(item) = document.get("keybindings") else {
        return bindings;
    };
    let Some(table) = item.as_table() else {
        issues.push("[keybindings] must be a TOML table".to_owned());
        return bindings;
    };
    for (action, value) in table {
        let texts: Option<Vec<&str>> = match value.as_value() {
            Some(toml_edit::Value::String(text)) => Some(vec![text.value().as_str()]),
            Some(toml_edit::Value::Array(list)) => list.iter().map(|item| item.as_str()).collect(),
            _ => None,
        };
        let Some(texts) = texts else {
            issues.push(format!(
                "[keybindings].{action} must be a chord such as \"mod+t\", or a list of them"
            ));
            continue;
        };
        let chords: Result<Vec<Chord>, String> = texts
            .into_iter()
            .filter(|text| !text.trim().is_empty())
            .map(Chord::parse)
            .collect();
        match chords {
            Ok(chords) => {
                bindings.insert(action.to_owned(), chords);
            }
            Err(error) => issues.push(format!("Invalid chord for [keybindings].{action}: {error}")),
        }
    }
    bindings
}

fn parse_permissions(document: &DocumentMut, issues: &mut Vec<String>) -> PermissionStore {
    let mut store = PermissionStore::default();
    if let Some(item) = document.get("permission_defaults") {
        if let Some(table) = item.as_table() {
            for (key, value) in table {
                match (PermissionKind::from_key(key), value.as_str()) {
                    (Some(kind), Some("block")) if kind != PermissionKind::Camera => {
                        store.block_by_default(kind)
                    }
                    (Some(kind), Some("ask")) if kind != PermissionKind::Camera => {}
                    _ => issues.push(format!("Ignoring invalid permission default {key}")),
                }
            }
        } else {
            issues.push("[permission_defaults] must be a TOML table".to_owned());
        }
    }
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
            if name == AUTOPLAY_KEY {
                match value.as_str().and_then(AutoplayPolicy::from_key) {
                    Some(policy) => store.set_autoplay(&normalized, Some(policy)),
                    None => issues.push(format!(
                        "[permissions.{origin:?}].autoplay must be allow, block-audio or block"
                    )),
                }
                continue;
            }
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
