// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::omnibox;
use crate::storage::{Paths, SessionTab, Storage};
use rune::Mods;
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_STORAGE_STRESS: AtomicU64 = AtomicU64::new(1);

#[test]
fn restored_sessions_keep_internal_pages_in_their_tab_positions() {
    let mut state = AppState::new(None);
    let session = Session::new(
        vec![
            SessionTab {
                url: "https://one.example/".into(),
                title: "One".into(),
                favicon: None,
            },
            SessionTab {
                url: "elaris://settings".into(),
                title: "Settings".into(),
                favicon: None,
            },
            SessionTab {
                url: "https://three.example/".into(),
                title: "Three".into(),
                favicon: None,
            },
        ],
        1,
    );
    state.restore_session(&session);
    assert_eq!(state.browser.tabs().len(), 3);
    assert_eq!(state.browser.active_tab().unwrap().url, "elaris://settings");
    assert!(state.browser.active_tab().unwrap().is_internal());
    assert!(state.browser.tabs()[0].engine.deferred_load);
    assert!(state.browser.tabs()[2].engine.deferred_load);
}

#[test]
fn concurrent_window_churn_keeps_the_final_session_snapshot_complete() {
    const WINDOWS: usize = 6;
    const UPDATES_PER_WINDOW: usize = 64;

    let id = NEXT_STORAGE_STRESS.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "elaris-window-session-stress-{}-{id}",
        std::process::id()
    ));
    let paths = Paths {
        application_dir: root.clone(),
        profile_dir: root.join("profile"),
        history_file: root.join("history.sqlite"),
        session_file: root.join("session.json"),
        download_history_file: root.join("downloads.json"),
        config_file: root.join("config.toml"),
    };
    let storage = Arc::new(Storage::open(paths.clone()).expect("open isolated storage"));
    let registry = Arc::new(Mutex::new(BTreeMap::new()));

    let workers: Vec<_> = (0..WINDOWS)
        .map(|window| {
            let storage = Arc::clone(&storage);
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                let window_id = 20_000 + window as u64;
                let destination_id = 20_000 + ((window + 1) % WINDOWS) as u64;
                for update in 0..UPDATES_PER_WINDOW {
                    let mut sessions = registry.lock().expect("session registry poisoned");
                    let mut source = sessions
                        .remove(&window_id)
                        .unwrap_or_else(|| Session::new(Vec::new(), 0));
                    source.tabs.push(SessionTab {
                        url: format!("https://window-{window}-update-{update}.example/"),
                        title: format!("Window {window} update {update}"),
                        favicon: None,
                    });
                    let destination = sessions
                        .entry(destination_id)
                        .or_insert_with(|| Session::new(Vec::new(), 0));
                    destination.tabs.push(source.tabs.remove(0));
                    if update % 11 == 0 {
                        destination.tabs.append(&mut source.tabs);
                    } else {
                        sessions.insert(window_id, source);
                    }
                    super::super::send_session_registry_snapshot(&sessions, &storage, true);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("window persistence worker completes");
    }

    let expected: Vec<_> = {
        let current = registry.lock().expect("session registry poisoned");
        super::super::send_session_registry_snapshot(&current, &storage, true);
        current.values().cloned().collect()
    };
    storage
        .commit_merged_session(expected.clone())
        .expect("acknowledge final multi-window snapshot");
    storage
        .flush_on_close()
        .expect("drain all window snapshots");

    let restore_storage = Storage::open(paths.clone()).expect("reopen window snapshot");
    let restored = restore_storage
        .take_sessions()
        .expect("restore window snapshot");
    assert_eq!(restored, expected);
    let urls: Vec<_> = restored
        .iter()
        .flat_map(|session| session.tabs.iter().map(|tab| tab.url.as_str()))
        .collect();
    assert_eq!(urls.len(), WINDOWS * UPDATES_PER_WINDOW);
    assert_eq!(
        urls.iter().copied().collect::<HashSet<_>>().len(),
        urls.len()
    );

    let metrics = storage.queue_metrics();
    assert_eq!(metrics.queued, 0);
    assert!(metrics.peak_queued > 0);
    assert!(metrics.session_writes > 0);
    assert!(!metrics.worst_session_latency.is_zero());
    println!(
        "headless window churn: peak_queue={} session_writes={} worst_save_ms={:.2}",
        metrics.peak_queued,
        metrics.session_writes,
        metrics.worst_session_latency.as_secs_f64() * 1000.0,
    );

    drop(storage);
    drop(restore_storage);
    std::fs::remove_dir_all(root).expect("remove isolated stress profile");
}
#[test]
fn split_session_restores_placement_ratio_and_focused_tab() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    state
        .browser
        .move_tab_to(third, TabSection::Favourite, 0)
        .unwrap();
    state.browser.place_split(second, 0);
    state.browser.set_viewport([1008.0, 600.0]);
    state.browser.resize_split(625.0);
    state.activate(first);

    let json = serde_json::to_string(&state.session_snapshot()).unwrap();
    let session: Session = serde_json::from_str(&json).unwrap();
    let mut restored = AppState::new(None);
    restored.restore_session(&session);
    let split = restored.browser.split.unwrap();
    let left = restored.browser.tab(split.panes[0].unwrap()).unwrap();
    let right = restored.browser.tab(split.panes[1].unwrap()).unwrap();
    assert_eq!(left.url, "https://two.example");
    assert_eq!(right.url, "https://one.example");
    assert_eq!(split.ratio, 0.625);
    assert_eq!(restored.browser.focused_side(), 1);
    assert_eq!(restored.session_snapshot(), session);
}

#[test]
fn split_session_keeps_its_group_when_a_nonmember_tab_is_active() {
    let mut state = AppState::new(Some("one.example"));
    let left = state.browser.active_tab().unwrap().id();
    let right = state.browser.open_tab("two.example").unwrap();
    let other = state.browser.open_tab("three.example").unwrap();
    state.browser.place_split(left, 0);
    state.browser.place_split(right, 1);
    state.activate(other);

    let session = state.session_snapshot();
    let mut restored = AppState::new(None);
    restored.restore_session(&session);

    let restored_other = restored.browser.active_tab().unwrap().id();
    assert_ne!(restored_other, other);
    assert_eq!(
        restored.browser.tab(restored_other).unwrap().url,
        "https://three.example"
    );
    let split = restored.browser.split.unwrap();
    let left = split.panes[0].unwrap();
    let right = split.panes[1].unwrap();
    assert_eq!(
        restored.browser.tab(left).unwrap().url,
        "https://one.example"
    );
    assert_eq!(
        restored.browser.tab(right).unwrap().url,
        "https://two.example"
    );
    restored.activate(right);
    assert_eq!(restored.browser.visible_tabs(), [Some(left), Some(right)]);
}

#[test]
fn split_picker_can_cancel_and_select_an_existing_tab_by_keyboard() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    state
        .browser
        .move_tab_to(third, TabSection::Favourite, 0)
        .unwrap();
    let mut runtime = Runtime::new(
        state,
        crate::ui::view,
        (1000.0, 640.0),
        Env::new(false, 1.0),
    );
    let open = |state: &mut AppState, tx: &mut Tx| {
        state.begin_split_picker(first, 1);
        tx.focus(crate::ui::viewport::PICKER, false);
    };
    let key = |runtime: &mut Runtime<AppState>, key| {
        runtime.key(KeyEvent {
            key,
            mods: Mods::default(),
        });
    };

    runtime.frame(0.0);
    runtime.update(open);
    runtime.frame(1.0 / 60.0);
    key(&mut runtime, Key::Escape);
    runtime.frame(2.0 / 60.0);
    assert!(runtime.state.browser.split.is_none());
    assert_eq!(runtime.state.browser.tabs().len(), 3);

    runtime.update(open);
    runtime.frame(3.0 / 60.0);
    runtime.frame(4.0 / 60.0);
    key(&mut runtime, Key::Down);
    runtime.frame(5.0 / 60.0);
    key(&mut runtime, Key::Enter);
    runtime.frame(6.0 / 60.0);
    assert_eq!(
        runtime.state.browser.visible_tabs(),
        [Some(second), Some(first)]
    );
    assert_eq!(runtime.state.browser.focused_side(), 0);
    assert_eq!(runtime.state.browser.tabs().len(), 3);
}
#[test]
fn dragging_the_current_tab_opens_a_picker_and_escape_cancels_a_preview() {
    use rune::{Env, Runtime};

    let state = AppState::new(Some("one.example"));
    let tab = state.browser.active_tab().unwrap().id();
    let mut runtime = Runtime::new(
        state,
        crate::ui::view,
        (1040.0, 640.0),
        Env::new(false, 1.0),
    );
    runtime.on_key = Some(crate::shortcuts::on_key);
    runtime.frame(0.0);
    let tab_key = format!("tab-{}", tab.raw());
    let [sx, sy, _, sh] = runtime
        .elements()
        .into_iter()
        .find(|node| node.key.as_deref() == Some(&tab_key))
        .unwrap()
        .rect;
    let start = [sx + 24.0, sy + sh / 2.0];
    let [x, y, w, h] = runtime
        .elements()
        .into_iter()
        .find(|node| node.key.as_deref() == Some(VIEWPORT))
        .unwrap()
        .rect;
    let target = [x + w * 0.75, y + h / 2.0];

    runtime.pointer_move(start[0], start[1]);
    runtime.pointer_down(start[0], start[1]);
    runtime.frame(0.0);
    runtime.pointer_move(target[0], target[1]);
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.split_preview, Some(1));
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.pointer_up(target[0], target[1]);
    runtime.frame(2.0 / 60.0);
    assert_eq!(runtime.state.split_preview, None);
    assert!(runtime.state.browser.split.is_none());

    runtime.update(|state, tx| {
        poll(state, 2.0 / 60.0, tx);
    });
    runtime.update(|state, tx| {
        poll(state, 1.0, tx);
    });
    runtime.frame(1.0);
    runtime.pointer_move(start[0], start[1]);
    runtime.pointer_down(start[0], start[1]);
    runtime.frame(1.0);
    runtime.pointer_move(target[0], target[1]);
    runtime.frame(1.0 + 1.0 / 60.0);
    runtime.pointer_up(target[0], target[1]);
    runtime.frame(1.0 + 2.0 / 60.0);
    assert_eq!(runtime.state.browser.visible_tabs(), [None, Some(tab)]);
    assert_eq!(runtime.state.browser.tabs().len(), 1);
}
#[test]
fn sidebar_drag_retargets_the_preview_and_commits_without_reordering() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let mut runtime = Runtime::new(
        state,
        crate::ui::view,
        (1040.0, 640.0),
        Env::new(false, 1.0),
    );
    runtime.on_key = Some(crate::shortcuts::on_key);
    runtime.frame(0.0);
    let node = |runtime: &Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap()
    };
    let [x, y, _, h] = node(&runtime, &format!("tab-{}", second.raw())).rect;
    let start = [x + 24.0, y + h / 2.0];
    let [x, y, w, h] = node(&runtime, VIEWPORT).rect;
    let left = [x + w * 0.25, y + h / 2.0];
    let right = [x + w * 0.75, y + h / 2.0];

    runtime.pointer_move(start[0], start[1]);
    runtime.pointer_down(start[0], start[1]);
    runtime.frame(0.0);
    runtime.pointer_move(left[0], left[1]);
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.split_preview, Some(0));
    assert!(runtime.state.browser.split.is_none());
    let source = node(&runtime, &format!("web-pane-{}", first.raw()));
    assert!(
        source.target[2] < source.rect[2] && source.rect[2] < w,
        "{:?} -> {:?}",
        source.rect,
        source.target
    );

    runtime.pointer_move(right[0], right[1]);
    runtime.frame(2.0 / 60.0);
    assert_eq!(runtime.state.split_preview, Some(1));
    runtime.pointer_up(right[0], right[1]);
    runtime.frame(3.0 / 60.0);
    assert_eq!(
        runtime.state.browser.visible_tabs(),
        [Some(first), Some(second)]
    );
    assert_eq!(runtime.state.browser.focused_side(), 1);
    assert_eq!(runtime.state.split_preview, None);
    assert_eq!(runtime.state.browser.tabs()[1].id(), second);

    runtime.set_system_reduced_motion(true);
    let [divider_x, divider_y, _, _] = node(&runtime, "split-divider").target;
    let pointer = [divider_x + 4.0, divider_y + 50.0];
    let distance = node(&runtime, VIEWPORT).target[2] - 300.0;
    runtime.frame(4.0 / 60.0);
    runtime.pointer_down(pointer[0], pointer[1]);
    runtime.pointer_move(pointer[0] - distance, pointer[1]);
    runtime.frame(5.0 / 60.0);
    runtime.pointer_up(pointer[0] - distance, pointer[1]);
    runtime.frame(6.0 / 60.0);
    assert!((node(&runtime, VIEWPORT).rect[2] - 300.0).abs() < 1.0);
    let [right_x, right_y, _, _] = node(&runtime, "right-page-viewport").rect;
    runtime.pointer_down(right_x + 10.0, right_y + 10.0);
    runtime.pointer_up(right_x + 10.0, right_y + 10.0);
    runtime.frame(7.0 / 60.0);
    assert_eq!(runtime.state.browser.focused_side(), 1);
    let [left_x, left_y, _, _] = node(&runtime, VIEWPORT).rect;
    runtime.pointer_down(left_x + 10.0, left_y + 10.0);
    runtime.pointer_up(left_x + 10.0, left_y + 10.0);
    runtime.frame(8.0 / 60.0);
    assert_eq!(runtime.state.browser.focused_side(), 0);
}
#[test]
fn session_restoration_keeps_tab_order_and_active_tab() {
    let mut state = AppState::new(None);
    let session = Session::new(
        vec![
            SessionTab {
                url: "https://one.example/".into(),
                title: "One".into(),
                favicon: None,
            },
            SessionTab {
                url: "https://two.example/".into(),
                title: "Two".into(),
                favicon: None,
            },
            SessionTab {
                url: "https://three.example/".into(),
                title: "Three".into(),
                favicon: None,
            },
        ],
        1,
    );

    state.restore_session(&session);

    let tabs = state.browser.tabs();
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[0].url, "https://one.example/");
    assert_eq!(tabs[1].url, "https://two.example/");
    assert_eq!(tabs[2].url, "https://three.example/");
    assert_eq!(
        state.browser.active_tab().unwrap().url,
        "https://two.example/"
    );
    assert_eq!(
        state.browser.recent_tabs(),
        [tabs[1].id(), tabs[0].id(), tabs[2].id()]
    );
    state.browser.activate_tab(tabs[1].id());
    let tabs = state.browser.tabs();
    assert_eq!(
        state.browser.recent_tabs(),
        [tabs[1].id(), tabs[0].id(), tabs[2].id()]
    );
    assert!(tabs[0].engine.deferred_load);
    assert!(!tabs[1].engine.deferred_load);
    assert!(tabs[2].engine.deferred_load);
}
#[test]
fn restoring_an_empty_session_does_not_create_a_blank_tab() {
    let mut state = AppState::new(None);
    state.restore_session(&Session::new(Vec::new(), 0));
    assert!(state.browser.tabs().is_empty());
    assert!(!state.omnibox.open);
}
#[test]
fn unsent_omnibox_text_is_not_part_of_the_session() {
    let mut state = AppState::new(None);
    state.open_omnibox(omnibox::Mode::New);
    state
        .omnibox
        .input
        .replace_text("typed but not submitted".to_owned());
    assert!(state.session_snapshot().tabs.is_empty());
}
#[test]
fn session_snapshot_keeps_order_and_active_index() {
    let mut state = AppState::new(Some("one.example"));
    state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    state.activate(third);

    let session = state.session_snapshot();

    assert_eq!(session.tabs[0].url, "https://one.example");
    assert_eq!(session.tabs[1].url, "https://two.example");
    assert_eq!(session.tabs[2].url, "https://three.example");
    assert_eq!(session.active_tab, 2);
}
#[test]
fn a_committed_reorder_is_saved_in_order_without_changing_the_active_tab() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    state.activate(second);

    state
        .commit_tab_move(first, TabSection::Regular, 2)
        .unwrap();

    assert_eq!(
        state.browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
        [second, third, first]
    );
    assert_eq!(state.browser.active_tab().map(Tab::id), Some(second));
    let session = state.session_snapshot();
    assert_eq!(
        session
            .tabs
            .iter()
            .map(|tab| tab.url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://two.example",
            "https://three.example",
            "https://one.example"
        ]
    );
    assert_eq!(session.active_tab, 0);
}
