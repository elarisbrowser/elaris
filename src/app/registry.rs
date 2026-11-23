// SPDX-License-Identifier: GPL-3.0-only

//! The process-wide registry of Rune windows: their handles, which are private, which is active,
//! and which are open.

use super::is_mini_window;
use super::mini;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub(super) static WINDOW_REGISTRY: OnceLock<Mutex<HashMap<u64, rune::WindowHandle>>> =
    OnceLock::new();
static ACTIVE_WINDOW: AtomicU64 = AtomicU64::new(0);

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn native_window(id: u64) -> Option<rune::WindowHandle> {
    WINDOW_REGISTRY.get()?.lock().ok()?.get(&id).cloned()
}

pub(crate) fn register_window(id: u64, handle: rune::WindowHandle) {
    WINDOW_REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("window registry poisoned")
        .insert(id, handle);
}

static PRIVATE_WINDOWS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

pub(super) fn mark_private_window(id: u64) {
    PRIVATE_WINDOWS
        .get_or_init(Default::default)
        .lock()
        .expect("private window registry poisoned")
        .insert(id);
}

/// How many private windows are open.
pub(crate) fn private_window_count() -> usize {
    PRIVATE_WINDOWS.get().map_or(0, |windows| {
        windows
            .lock()
            .expect("private window registry poisoned")
            .len()
    })
}

/// Tabs only move between windows of the same kind, since a page cannot change session.
pub(crate) fn is_private_window(id: u64) -> bool {
    PRIVATE_WINDOWS.get().is_some_and(|windows| {
        windows
            .lock()
            .expect("private window registry poisoned")
            .contains(&id)
    })
}

pub(crate) fn unregister_window(id: u64) {
    mini::forget(id);
    if let Some(windows) = PRIVATE_WINDOWS.get() {
        windows
            .lock()
            .expect("private window registry poisoned")
            .remove(&id);
    }
    if let Some(registry) = WINDOW_REGISTRY.get() {
        registry
            .lock()
            .expect("window registry poisoned")
            .remove(&id);
    }
    let _ = ACTIVE_WINDOW.compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed);
}

pub(crate) fn activate_window(id: u64) {
    ACTIVE_WINDOW.store(id, Ordering::Relaxed);
}

/// How many full browser windows are open. Mini windows are not part of the saved layout.
pub(crate) fn open_window_count() -> usize {
    WINDOW_REGISTRY.get().map_or(0, |registry| {
        registry
            .lock()
            .expect("window registry poisoned")
            .keys()
            .filter(|&&id| !is_mini_window(id))
            .count()
    })
}

pub(crate) fn reopen_or_create_window() {
    let existing = WINDOW_REGISTRY.get().and_then(|registry| {
        registry
            .lock()
            .expect("window registry poisoned")
            .iter()
            .find(|&(&id, _)| !is_mini_window(id))
            .map(|(_, handle)| handle.clone())
    });
    if let Some(handle) = existing {
        handle.open();
    } else {
        crate::open_browser_window();
    }
}
