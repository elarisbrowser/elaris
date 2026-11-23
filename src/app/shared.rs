// SPDX-License-Identifier: GPL-3.0-only

//! Process-wide settings, permissions and downloads that every window reads and writes.

#[cfg(not(test))]
use std::sync::Arc;
use std::sync::{Mutex, OnceLock, RwLock};

use super::AppState;
use crate::storage::Storage;
use elaris_core::permissions::PermissionStore;
use elaris_core::settings::Settings;

static SHARED_SETTINGS: OnceLock<RwLock<Settings>> = OnceLock::new();
static SHARED_PERMISSIONS: OnceLock<RwLock<PermissionStore>> = OnceLock::new();
#[cfg(not(test))]
static SHARED_DOWNLOADS: OnceLock<Arc<Mutex<Vec<elaris_core::downloads::Download>>>> =
    OnceLock::new();

#[cfg(not(test))]
pub(super) fn shared_settings(settings: Settings) -> Settings {
    SHARED_SETTINGS
        .get_or_init(|| RwLock::new(settings))
        .read()
        .expect("shared settings poisoned")
        .clone()
}

#[cfg(not(test))]
pub(super) fn shared_settings_snapshot() -> Settings {
    SHARED_SETTINGS
        .get()
        .expect("shared settings initialized before windows")
        .read()
        .expect("shared settings poisoned")
        .clone()
}

pub(crate) fn publish_settings(settings: &Settings) {
    *SHARED_SETTINGS
        .get_or_init(|| RwLock::new(settings.clone()))
        .write()
        .expect("shared settings poisoned") = settings.clone();
    crate::engine::wake_all_windows();
}

/// The process's download list, seeded once with the history kept from earlier runs.
#[cfg(not(test))]
pub(super) fn shared_downloads(
    storage: Option<&Storage>,
) -> Arc<Mutex<Vec<elaris_core::downloads::Download>>> {
    Arc::clone(SHARED_DOWNLOADS.get_or_init(|| {
        let history = storage
            .and_then(Storage::take_download_history)
            .unwrap_or_default();
        Arc::new(Mutex::new(history))
    }))
}

/// Saves the download history when the finished, non-private downloads changed.
pub(crate) fn persist_download_history(
    storage: &Storage,
    downloads: &[elaris_core::downloads::Download],
) {
    static SAVED: Mutex<Option<String>> = Mutex::new(None);
    let encoded = elaris_core::download_history::encode(downloads);
    let mut saved = SAVED.lock().expect("download history guard poisoned");
    if saved.as_deref() == Some(encoded.as_str()) {
        return;
    }
    if let Err(error) = storage.save_download_history(encoded.clone()) {
        eprintln!("Elaris: {error}");
        return;
    }
    *saved = Some(encoded);
}

#[cfg(not(test))]
pub(super) fn shared_permission_store(permissions: PermissionStore) -> PermissionStore {
    SHARED_PERMISSIONS
        .get_or_init(|| RwLock::new(permissions))
        .read()
        .expect("shared permissions poisoned")
        .clone()
}

#[cfg(not(test))]
pub(super) fn shared_permission_snapshot() -> PermissionStore {
    SHARED_PERMISSIONS
        .get()
        .expect("shared permissions initialized before windows")
        .read()
        .expect("shared permissions poisoned")
        .clone()
}

pub(crate) fn publish_permissions(permissions: &PermissionStore) {
    *SHARED_PERMISSIONS
        .get_or_init(|| RwLock::new(permissions.clone()))
        .write()
        .expect("shared permissions poisoned") = permissions.clone();
    crate::engine::wake_all_windows();
}

impl AppState {
    pub(crate) fn sync_shared_settings(&mut self) -> bool {
        #[cfg(not(test))]
        {
            let settings = shared_settings_snapshot();
            if self.settings == settings {
                false
            } else {
                let previous = self.engine_download_directory();
                self.prepare_tab_layout(settings.tab_layout);
                self.settings = settings;
                self.browser.set_default_zoom(self.settings.page_zoom);
                let current = self.engine_download_directory();
                if previous != current {
                    self.browser.set_download_directory(current);
                }
                true
            }
        }
        #[cfg(test)]
        false
    }

    pub(crate) fn sync_shared_permissions(&mut self) -> bool {
        #[cfg(not(test))]
        {
            let permissions = shared_permission_snapshot();
            if self.permission_store == permissions {
                false
            } else {
                self.permission_store = permissions;
                true
            }
        }
        #[cfg(test)]
        false
    }
}
