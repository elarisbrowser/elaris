// SPDX-License-Identifier: GPL-3.0-only

//! Each window's saved session, kept in one process-wide registry and written to storage as a
//! whole.

use crate::storage::{Session, Storage};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

pub(super) static SESSION_REGISTRY: OnceLock<Mutex<BTreeMap<u64, Session>>> = OnceLock::new();

fn save_registered_sessions(storage: &Storage, restore_on_launch: bool) {
    let registry = SESSION_REGISTRY
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("session registry poisoned");
    send_session_registry_snapshot(&registry, storage, restore_on_launch);
}

pub(super) fn send_session_registry_snapshot(
    registry: &BTreeMap<u64, Session>,
    storage: &Storage,
    restore_on_launch: bool,
) {
    let sessions = registry.values().cloned().collect();
    if let Err(error) = storage.save_sessions_for_launch(sessions, restore_on_launch) {
        eprintln!("Elaris: {error}");
    }
}

pub(crate) fn store_window_session(id: u64, session: Session, storage: &Storage) {
    store_window_session_in(
        SESSION_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new())),
        id,
        session,
        storage,
    );
}

fn store_window_session_in(
    registry: &Mutex<BTreeMap<u64, Session>>,
    id: u64,
    session: Session,
    storage: &Storage,
) {
    let mut registry = registry.lock().expect("session registry poisoned");
    registry.insert(id, session);
    send_session_registry_snapshot(&registry, storage, true);
}

pub(super) fn seed_window_session(id: u64, session: Session) {
    SESSION_REGISTRY
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .expect("session registry poisoned")
        .insert(id, session);
}

pub(crate) fn remove_window_session(id: u64, storage: &Storage) {
    remove_window_session_from(
        SESSION_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new())),
        id,
        storage,
    );
}

fn remove_window_session_from(
    registry: &Mutex<BTreeMap<u64, Session>>,
    id: u64,
    storage: &Storage,
) {
    let mut registry = registry.lock().expect("session registry poisoned");
    registry.remove(&id);
    send_session_registry_snapshot(&registry, storage, true);
}

pub(crate) fn save_closed_final_session(storage: &Storage) {
    save_registered_sessions(storage, false);
}
