// SPDX-License-Identifier: GPL-3.0-only

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

const SESSION_VERSION: u8 = 4;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Session {
    version: u8,
    pub(crate) tabs: Vec<SessionTab>,
    pub(crate) active_tab: usize,
    /// Positions in `tabs`, kept in sidebar order. Missing fields in version 1 mean regular tabs.
    #[serde(default)]
    pub(crate) favourites: Vec<usize>,
    // Older sessions store pinning separately from the tabs themselves.
    #[serde(
        default,
        rename = "pinned",
        skip_serializing,
        deserialize_with = "ignore_legacy_pins"
    )]
    _legacy_pins: (),
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) split: Option<SessionSplit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionSplit {
    pub(crate) left: usize,
    pub(crate) right: usize,
    /// Left pane share in ten-thousandths. The active tab may be outside this group.
    pub(crate) ratio: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionTab {
    pub(crate) url: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) favicon: Option<SessionFavicon>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionFavicon {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

impl Session {
    #[cfg(test)]
    pub(crate) fn new(tabs: Vec<SessionTab>, active_tab: usize) -> Self {
        Self::new_with_sections(tabs, active_tab, Vec::new())
    }

    pub(crate) fn new_with_sections(
        tabs: Vec<SessionTab>,
        active_tab: usize,
        favourites: Vec<usize>,
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
            _legacy_pins: (),
            split: None,
        }
    }

    pub(crate) fn section_at(&self, index: usize) -> crate::tab::TabSection {
        if self.favourites.contains(&index) {
            crate::tab::TabSection::Favourite
        } else {
            crate::tab::TabSection::Regular
        }
    }

    pub(super) fn is_supported(&self) -> bool {
        matches!(self.version, 1 | 2 | 3 | SESSION_VERSION)
            && (self.tabs.is_empty() || self.active_tab < self.tabs.len())
            && self.split.as_ref().is_none_or(|split| {
                split.left < self.tabs.len()
                    && split.right < self.tabs.len()
                    && split.left != split.right
                    && split.ratio <= 10_000
            })
            && self.favourites.iter().all(|index| *index < self.tabs.len())
            && self.tabs.iter().all(|tab| {
                tab.favicon.as_ref().is_none_or(|favicon| {
                    favicon.width > 0
                        && favicon.height > 0
                        && usize::try_from(favicon.width)
                            .ok()
                            .and_then(|width| {
                                usize::try_from(favicon.height)
                                    .ok()
                                    .and_then(|height| width.checked_mul(height))
                            })
                            .and_then(|pixels| pixels.checked_mul(4))
                            == Some(favicon.rgba.len())
                })
            })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionCollection {
    format: u8,
    #[serde(default = "restore_sessions_by_default")]
    restore_on_launch: bool,
    windows: Vec<Session>,
}

fn ignore_legacy_pins<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer).map(|_| ())
}

fn restore_sessions_by_default() -> bool {
    true
}

pub(super) fn load_sessions(path: &Path) -> io::Result<Option<Vec<Session>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let sessions = if value.get("windows").is_some() {
        let collection: SessionCollection = serde_json::from_value(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if collection.format != 1 || !collection.restore_on_launch {
            return Ok(None);
        }
        collection.windows
    } else {
        // Version 1 through 4 single-window files remain valid and migrate on their next save.
        vec![
            serde_json::from_value(value)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        ]
    };
    Ok((!sessions.is_empty() && sessions.iter().all(Session::is_supported)).then_some(sessions))
}

pub(super) fn write_sessions_atomic(
    path: &Path,
    sessions: &[Session],
    restore_on_launch: bool,
) -> io::Result<()> {
    let collection = SessionCollection {
        format: 1,
        restore_on_launch,
        windows: sessions.to_vec(),
    };
    let bytes = serde_json::to_vec(&collection)
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
pub(super) fn load_session(path: &Path) -> io::Result<Option<Session>> {
    Ok(load_sessions(path)?.and_then(|sessions| sessions.into_iter().next()))
}

#[cfg(test)]
pub(super) fn write_session_atomic(path: &Path, session: &Session) -> io::Result<()> {
    write_sessions_atomic(path, std::slice::from_ref(session), true)
}
