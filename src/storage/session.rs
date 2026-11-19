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
    #[serde(default)]
    pub(crate) pinned: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) split: Option<SessionSplit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionSplit {
    pub(crate) left: usize,
    pub(crate) right: usize,
    /// Left pane share in ten-thousandths. The focused pane is stored in active_tab.
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
            split: None,
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

    pub(super) fn is_supported(&self) -> bool {
        matches!(self.version, 1 | 2 | 3 | SESSION_VERSION)
            && (self.tabs.is_empty() || self.active_tab < self.tabs.len())
            && self.split.as_ref().is_none_or(|split| {
                split.left < self.tabs.len()
                    && split.right < self.tabs.len()
                    && split.left != split.right
                    && split.ratio <= 10_000
                    && [split.left, split.right].contains(&self.active_tab)
            })
            && self.favourites.iter().all(|index| *index < self.tabs.len())
            && self.pinned.iter().all(|index| *index < self.tabs.len())
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
            && !self
                .favourites
                .iter()
                .any(|index| self.pinned.contains(index))
    }
}

pub(super) fn load_session(path: &Path) -> io::Result<Option<Session>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let session: Session = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(session.is_supported().then_some(session))
}

pub(super) fn write_session_atomic(path: &Path, session: &Session) -> io::Result<()> {
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
