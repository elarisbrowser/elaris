// SPDX-License-Identifier: GPL-3.0-only

//! Coordinates a live tab handoff across independently polled Rune windows.

use std::cell::RefCell;

use rune::Tx;

use super::{AppState, SESSION_REGISTRY, WINDOW_REGISTRY};
use crate::browser::{Browser, split::Split};
use crate::storage::Session;
use crate::tab::{TabId, TabSection};

thread_local! {
    static TRANSFER: RefCell<Option<Transfer>> = const { RefCell::new(None) };
}

struct Transfer {
    source: u64,
    destination: u64,
    tab: TabId,
    section: TabSection,
    index: usize,
    phase: Phase,
}

enum Phase {
    Requested {
        runtime: Box<Browser>,
        internal: bool,
    },
    Prepared,
    InTransit(Box<InTransit>),
    Cancelled(Box<InTransit>),
    Committed(Session),
}

struct InTransit {
    browser: Browser,
    source_session: Session,
    section: TabSection,
    index: usize,
    active: Option<TabId>,
    split: Option<Split>,
    deferred_load: bool,
}

impl Transfer {
    fn involves(&self, id: u64) -> bool {
        self.source == id || self.destination == id
    }
}

pub(super) fn in_progress() -> bool {
    TRANSFER.with_borrow(Option::is_some)
}

impl AppState {
    /// Requests a transfer while the source still owns the tab. Only one handoff is in flight.
    pub(crate) fn request_tab_transfer(
        &self,
        tab: TabId,
        destination: u64,
        section: TabSection,
        index: usize,
    ) -> bool {
        let Some(candidate) = self.browser.tab(tab) else {
            return false;
        };
        if self.window_id == destination || !self.workers_started {
            return false;
        }
        let registered = WINDOW_REGISTRY.get().is_some_and(|registry| {
            let registry = registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            [self.window_id, destination]
                .into_iter()
                .all(|id| registry.get(&id).is_some_and(rune::WindowHandle::is_ready))
        });
        if !registered {
            return false;
        }
        let accepted = TRANSFER.with_borrow_mut(|pending| {
            if pending.is_some() {
                return false;
            }
            *pending = Some(Transfer {
                source: self.window_id,
                destination,
                tab,
                section,
                index,
                phase: Phase::Requested {
                    runtime: Box::new(self.browser.transfer_buffer()),
                    internal: candidate.is_internal(),
                },
            });
            true
        });
        if accepted {
            crate::engine::wake_all_windows();
        }
        accepted
    }

    pub(super) fn tab_transfer_blocks_session(&self) -> bool {
        TRANSFER.with_borrow(|pending| {
            pending.as_ref().is_some_and(|transfer| {
                transfer.involves(self.window_id)
                    && matches!(transfer.phase, Phase::InTransit(_) | Phase::Cancelled(_))
            })
        })
    }

    pub(crate) fn cancel_tab_transfer(&mut self) -> bool {
        let changed = TRANSFER.with_borrow_mut(|pending| {
            let Some(mut transfer) = pending.take() else {
                return false;
            };
            if !transfer.involves(self.window_id) {
                *pending = Some(transfer);
                return false;
            }
            if matches!(transfer.phase, Phase::Committed(_)) {
                *pending = Some(transfer);
                return false;
            }
            if let Phase::InTransit(mut moved) | Phase::Cancelled(mut moved) = transfer.phase {
                if self.window_id == transfer.source {
                    moved.restore(self, transfer.tab);
                } else {
                    transfer.phase = Phase::Cancelled(moved);
                    *pending = Some(transfer);
                }
            }
            true
        });
        if changed {
            super::cross_window_drag::clear(self.window_id);
            crate::engine::wake_all_windows();
        }
        changed
    }

    pub(super) fn closing_tab_transfer(&mut self) {
        self.cancel_tab_transfer();
        TRANSFER.with_borrow_mut(|pending| {
            if pending.as_ref().is_some_and(|transfer| {
                transfer.source == self.window_id && matches!(transfer.phase, Phase::Committed(_))
            }) {
                pending.take();
                self.merged_away = self.browser.tabs().is_empty();
            }
        });
    }

    pub(super) fn advance_tab_transfer(&mut self, tx: &mut Tx) -> bool {
        let preparing_source = TRANSFER.with_borrow(|pending| {
            pending.as_ref().is_some_and(|transfer| {
                transfer.source == self.window_id && matches!(transfer.phase, Phase::Prepared)
            })
        });
        if preparing_source {
            self.persist_session_if_changed();
        }
        let progressed = TRANSFER.with_borrow_mut(|pending| {
            let Some(mut transfer) = pending.take() else {
                return false;
            };
            let id = self.window_id;
            transfer.phase = match transfer.phase {
                Phase::Requested { runtime, internal } if id == transfer.destination => {
                    if !self.workers_started
                        || self
                            .browser
                            .prepare_transfer(
                                transfer.tab,
                                transfer.section,
                                transfer.index,
                                &runtime,
                                internal,
                            )
                            .is_err()
                    {
                        super::cross_window_drag::clear(id);
                        return true;
                    }
                    Phase::Prepared
                }
                Phase::Prepared if id == transfer.source => {
                    let Some(moved) = InTransit::extract(self, transfer.tab) else {
                        super::cross_window_drag::clear(id);
                        return true;
                    };
                    Phase::InTransit(Box::new(moved))
                }
                Phase::InTransit(mut moved) if id == transfer.destination => {
                    match moved.commit(
                        self,
                        transfer.source,
                        transfer.tab,
                        transfer.section,
                        transfer.index,
                    ) {
                        Ok(()) => {
                            super::detach::committed(self, transfer.tab);
                            super::cross_window_drag::finish(self, transfer.tab, true, tx);
                            if let Some(window) = WINDOW_REGISTRY.get().and_then(|registry| {
                                registry
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .get(&id)
                                    .cloned()
                            }) {
                                window.focus();
                            }
                            tx.focus_when_shown(
                                crate::ui::viewport::focus_key(&self.browser),
                                false,
                            );
                            Phase::Committed(moved.source_session)
                        }
                        Err(error) => {
                            super::cross_window_drag::finish(self, transfer.tab, false, tx);
                            eprintln!("Elaris: {error}");
                            Phase::Cancelled(moved)
                        }
                    }
                }
                Phase::Committed(session) if id == transfer.source => {
                    self.last_saved_session = Some(session);
                    self.tab_drag.cancel();
                    if self.browser.tabs().is_empty()
                        && WINDOW_REGISTRY.get().is_some_and(|registry| {
                            registry
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .get(&transfer.destination)
                                .is_some_and(rune::WindowHandle::is_ready)
                        })
                    {
                        self.merged_away = true;
                        tx.close_window();
                    }

                    return true;
                }
                Phase::InTransit(mut moved) if id == transfer.source => {
                    moved.source_session = self.session_snapshot();
                    transfer.phase = Phase::InTransit(moved);
                    *pending = Some(transfer);
                    return false;
                }
                Phase::Cancelled(mut moved) if id == transfer.source => {
                    moved.restore(self, transfer.tab);
                    return true;
                }
                phase => {
                    transfer.phase = phase;
                    *pending = Some(transfer);
                    return false;
                }
            };
            *pending = Some(transfer);
            true
        });
        if progressed {
            crate::engine::wake_all_windows();
        }
        progressed
    }

    fn persist_transfer(&mut self, source: u64, source_session: &Session) -> Result<(), String> {
        // Both windows belong to the same browsing mode. Private handoffs are memory-only.
        if self.browser.private {
            return Ok(());
        }
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        let destination_session = self.session_snapshot();
        let mut registry = SESSION_REGISTRY
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut updated = registry.clone();
        if source_session.tabs.is_empty() {
            updated.remove(&source);
        } else {
            updated.insert(source, source_session.clone());
        }
        updated.insert(self.window_id, destination_session.clone());
        let snapshot = updated.values().cloned().collect();
        if source_session.tabs.is_empty() {
            storage.commit_merged_session(snapshot)?;
        } else {
            storage.save_sessions_for_launch(snapshot, true)?;
        }
        *registry = updated;
        self.last_saved_session = Some(destination_session);
        Ok(())
    }
}

impl InTransit {
    fn extract(source: &mut AppState, id: TabId) -> Option<Self> {
        let tab = source.browser.tab(id)?;
        let section = tab.section();
        let index = source
            .browser
            .section_tabs(section)
            .position(|tab| tab.id() == id)?;
        let active = source.browser.active_tab().map(crate::tab::Tab::id);
        let split = source.browser.split;
        let deferred_load = tab.engine.deferred_load;
        let mut browser = source.browser.transfer_buffer();
        source
            .browser
            .transfer_tab(&mut browser, id, section, 0)
            .ok()?;
        Some(Self {
            browser,
            source_session: source.session_snapshot(),
            section,
            index,
            active,
            split,
            deferred_load,
        })
    }

    fn commit(
        &mut self,
        destination: &mut AppState,
        source: u64,
        tab: TabId,
        section: TabSection,
        index: usize,
    ) -> Result<(), String> {
        let active = destination.browser.active_tab().map(crate::tab::Tab::id);
        let split = destination.browser.split;
        self.browser
            .transfer_tab(&mut destination.browser, tab, section, index)
            .map_err(|error| error.to_string())?;
        if let Err(error) = destination.persist_transfer(source, &self.source_session) {
            if let Err(error) =
                destination
                    .browser
                    .transfer_tab(&mut self.browser, tab, self.section, 0)
            {
                eprintln!("Elaris: could not put a tab back after a failed transfer: {error:?}");
            }
            destination.browser.split = split;
            if let Some(id) = active {
                destination.browser.activate_tab(id);
            } else {
                destination.browser.clear_active_tab();
            }
            return Err(error);
        }
        Ok(())
    }

    fn restore(&mut self, source: &mut AppState, id: TabId) {
        let index = self
            .index
            .min(source.browser.section_tabs(self.section).count());
        if let Err(error) = self
            .browser
            .transfer_tab(&mut source.browser, id, self.section, index)
        {
            eprintln!("Elaris: could not restore a tab after a failed transfer: {error:?}");
        }
        if let Some(split) = self.split
            && split
                .panes
                .into_iter()
                .flatten()
                .all(|id| source.browser.tab(id).is_some())
        {
            source.browser.split = Some(split);
        }
        if let Some(active) = self.active {
            source.browser.activate_tab(active);
        } else {
            source.browser.clear_active_tab();
        }
        if let Some(tab) = source.browser.tab_mut(id) {
            tab.engine.deferred_load = self.deferred_load;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: &AppState, destination: &AppState, tab: TabId, index: usize) {
        TRANSFER.with_borrow_mut(|pending| {
            assert!(pending.is_none());
            *pending = Some(Transfer {
                source: source.window_id,
                destination: destination.window_id,
                tab,
                section: TabSection::Regular,
                index,
                phase: Phase::Requested {
                    runtime: Box::new(source.browser.transfer_buffer()),
                    internal: false,
                },
            });
        });
    }

    #[test]
    fn handoff_keeps_the_old_snapshot_until_both_new_owners_can_be_saved_together() {
        let mut source = AppState::new(Some("one.example"));
        let mut destination = AppState::new(Some("two.example"));
        destination.workers_started = true;
        let (engine, fake) = crate::engine::Engine::fake();
        source.browser.connect(engine.clone());
        destination.browser.connect(engine);
        while fake.commands.try_recv().is_ok() {}
        let data =
            std::env::temp_dir().join(format!("elaris-transfer-session-{}", std::process::id()));
        let paths = crate::storage::Paths {
            application_dir: data.clone(),
            profile_dir: data.join("profile"),
            history_file: data.join("history.sqlite"),
            session_file: data.join("session.json"),
            download_history_file: data.join("downloads.json"),
            bookmarks_file: data.join("bookmarks.json"),
            config_file: data.join("config.toml"),
        };
        let storage = std::sync::Arc::new(crate::storage::Storage::open(paths).unwrap());
        source.storage = Some(storage.clone());
        destination.storage = Some(storage.clone());
        source.persist_session();
        destination.persist_session();
        storage.flush_on_close().unwrap();
        let prior = std::fs::read(data.join("session.json")).unwrap();
        let tab = source.browser.active_tab().unwrap().id();
        let surface = source.browser.tab(tab).unwrap().surface.clone();
        request(&source, &destination, tab, 1);

        let tx = &mut Tx::default();
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert!(source.browser.tab(tab).is_none());
        assert!(destination.browser.tab(tab).is_none());
        source.persist_session_if_changed();
        destination.persist_session_if_changed();
        storage.flush_on_close().unwrap();
        assert_eq!(std::fs::read(data.join("session.json")).unwrap(), prior);

        assert!(destination.advance_tab_transfer(tx));
        assert!(!source.cancel_tab_transfer());
        assert!(in_progress());
        source.closing_tab_transfer();
        assert!(source.merged_away);
        assert!(!in_progress());
        source.persist_session();
        assert_eq!(destination.browser.active_tab().unwrap().id(), tab);
        assert_eq!(destination.browser.tab(tab).unwrap().surface, surface);
        assert!(fake.commands.try_iter().all(|command| !matches!(
            command,
            crate::engine::Command::Open(..) | crate::engine::Command::Close(_)
        )));
        storage.flush_on_close().unwrap();
        let json = std::fs::read_to_string(data.join("session.json")).unwrap();
        assert_eq!(json.matches("one.example").count(), 1);
        let sessions = SESSION_REGISTRY.get().unwrap().lock().unwrap();
        assert!(!sessions.contains_key(&source.window_id));
        assert_eq!(
            sessions.get(&destination.window_id),
            Some(&destination.session_snapshot())
        );
        drop(sessions);
        super::super::remove_window_session(source.window_id, &storage);
        super::super::remove_window_session(destination.window_id, &storage);
        storage.flush_on_close().unwrap();
        drop(source);
        drop(destination);
        drop(storage);
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn private_tab_handoff_keeps_both_windows_out_of_the_session_registry() {
        let mut source = AppState::new(Some("https://example.com/"));
        let mut destination = AppState::new(Some("https://example.org/"));
        source.browser.private = true;
        destination.browser.private = true;
        destination.workers_started = true;
        let data = std::env::temp_dir().join(format!(
            "elaris-private-transfer-{}-{}",
            std::process::id(),
            destination.window_id
        ));
        let storage = crate::storage::Storage::open(crate::storage::Paths {
            application_dir: data.clone(),
            profile_dir: data.join("profile"),
            history_file: data.join("history.sqlite"),
            session_file: data.join("session.json"),
            download_history_file: data.join("downloads.json"),
            bookmarks_file: data.join("bookmarks.json"),
            config_file: data.join("config.toml"),
        })
        .unwrap();
        destination.storage = Some(std::sync::Arc::new(storage));
        let tab = source.browser.active_tab().unwrap().id();
        request(&source, &destination, tab, 1);
        let tx = &mut Tx::default();
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert_eq!(destination.browser.active_tab().unwrap().id(), tab);

        destination
            .storage
            .as_ref()
            .unwrap()
            .flush_on_close()
            .unwrap();
        assert!(!data.join("session.json").exists());
        if let Some(registry) = SESSION_REGISTRY.get() {
            let registry = registry.lock().unwrap();
            assert!(!registry.contains_key(&source.window_id));
            assert!(!registry.contains_key(&destination.window_id));
        }
        drop(destination);
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn failed_merge_save_restores_tab_and_leaves_source_open() {
        let mut source = AppState::new(Some("one.example"));
        let mut destination = AppState::new(Some("two.example"));
        destination.workers_started = true;
        let data = std::env::temp_dir().join(format!("elaris-failed-merge-{}", std::process::id()));
        let session_file = data.join("session.json");
        let storage = crate::storage::Storage::open(crate::storage::Paths {
            application_dir: data.clone(),
            profile_dir: data.join("profile"),
            history_file: data.join("history.sqlite"),
            session_file: session_file.clone(),
            download_history_file: session_file.with_file_name("downloads.json"),
            bookmarks_file: session_file.with_file_name("bookmarks.json"),
            config_file: data.join("config.toml"),
        })
        .unwrap();
        // An existing directory prevents the worker from atomically replacing the session file.
        std::fs::create_dir(&session_file).unwrap();
        destination.storage = Some(std::sync::Arc::new(storage));
        let tab = source.browser.active_tab().unwrap().id();
        request(&source, &destination, tab, 1);
        let tx = &mut Tx::default();
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert_eq!(source.browser.active_tab().unwrap().id(), tab);
        assert!(destination.browser.tab(tab).is_none());
        assert!(!source.merged_away);
        drop(destination);
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn destination_closure_restores_the_original_owner_and_selection() {
        let mut source = AppState::new(Some("one.example"));
        let mut destination = AppState::new(Some("two.example"));
        destination.workers_started = true;
        let tab = source.browser.active_tab().unwrap().id();
        let previous = source.session_snapshot();
        request(&source, &destination, tab, 1);

        let tx = &mut Tx::default();
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        destination.closing_tab_transfer();
        assert!(source.tab_transfer_blocks_session());
        assert!(source.advance_tab_transfer(tx));
        assert_eq!(source.browser.active_tab().unwrap().id(), tab);
        assert_eq!(source.session_snapshot(), previous);
        assert!(!source.tab_transfer_blocks_session());
        assert!(destination.browser.tab(tab).is_none());
    }
    #[test]
    fn invalidated_destination_index_rolls_back_the_handoff() {
        let mut source = AppState::new(Some("one.example"));
        let mut destination = AppState::new(Some("two.example"));
        destination.workers_started = true;
        let tab = source.browser.active_tab().unwrap().id();
        let previous = source.session_snapshot();
        let destination_tab = destination.browser.active_tab().unwrap().id();
        request(&source, &destination, tab, 1);

        let tx = &mut Tx::default();
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        destination.browser.close_tab(destination_tab);
        assert!(destination.advance_tab_transfer(tx));
        assert!(source.advance_tab_transfer(tx));
        assert_eq!(source.session_snapshot(), previous);
        assert!(destination.browser.tab(tab).is_none());
    }
    #[test]
    fn escape_and_app_deactivation_rollback_a_pending_final_tab_transfer() {
        for escape in [true, false] {
            let mut source = AppState::new(Some("source.example"));
            let mut destination = AppState::new(Some("destination.example"));
            destination.workers_started = true;
            let tab = source.browser.active_tab().unwrap().id();
            let surface = source.browser.tab(tab).unwrap().surface.clone();
            let before = source.session_snapshot();
            let tx = &mut Tx::default();
            request(&source, &destination, tab, 1);
            assert!(destination.advance_tab_transfer(tx));
            assert!(source.advance_tab_transfer(tx));
            if escape {
                assert!(crate::shortcuts::on_key(
                    &mut source,
                    &rune::KeyEvent {
                        key: rune::Key::Escape,
                        mods: rune::Mods::default(),
                    },
                    tx
                ));
            } else {
                super::super::lifecycle::cancel_deactivated_drag(&mut source, tx);
            }
            assert!(!in_progress());
            assert!(!source.merged_away);
            assert_eq!(source.session_snapshot(), before);
            assert_eq!(source.browser.tab(tab).unwrap().surface, surface);
            assert!(destination.browser.tab(tab).is_none());
            assert!(source.tab_drag.visual().is_none());
            assert!(!destination.advance_tab_transfer(tx));
        }
    }
}
