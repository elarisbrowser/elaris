// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::storage::HistoryMutation;
use rune::Mods;

#[test]
fn history_shortcut_restores_native_tab_and_keyboard_opens_a_result() {
    use crate::engine::Command;

    let url = "https://example.org/guide";
    let db = elaris::history::History::open(std::path::Path::new(":memory:")).unwrap();
    db.record_visit(url, "Example guide").unwrap();
    let mut state = AppState::new(Some("elaris://history"));
    let restored = state.session_snapshot();
    state = AppState::new(None);
    state.restore_session(&restored);
    let tab = state.active_history_tab().unwrap();
    let (engine, fake) = crate::engine::Engine::fake();
    state.browser.connect(engine);
    assert!(
        !fake
            .commands
            .try_iter()
            .any(|command| matches!(command, Command::Open(..)))
    );

    let mut runtime =
        rune::Runtime::new(state, crate::ui::view, (880.0, 600.0), rune::Env::default());
    runtime.on_key = Some(crate::shortcuts::on_key);
    runtime.frame(0.0);
    let key = if cfg!(target_os = "macos") { 'y' } else { 'h' };
    for key in [key, key, 'f'] {
        runtime.key(KeyEvent {
            key: Key::Command(key),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.browser.tabs().len(), 1);
        assert_eq!(runtime.state.active_history_tab(), Some(tab));
        assert!(!runtime.state.find_open);
        assert!(runtime.elements().iter().any(|element| {
            element.focused && element.key.as_deref() == Some(crate::ui::HISTORY_SEARCH)
        }));
    }
    runtime.update(move |state, _| {
        state.history_page.page = Some(db.page("guide", None).unwrap());
    });
    runtime.frame(0.0);
    runtime.key(KeyEvent {
        key: Key::Down,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(runtime.elements().iter().any(|element| {
        element.focused
            && element
                .label
                .as_deref()
                .is_some_and(|label| label.contains("Example guide"))
    }));
    runtime.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(runtime.state.browser.active_tab().unwrap().id(), tab);
    assert_eq!(active_url(&runtime.state), url);
    assert!(runtime.state.active_history_tab().is_none());
    assert_eq!(
        fake.commands
            .try_iter()
            .filter(|command| matches!(command, Command::Open(id, _) if *id == tab))
            .count(),
        1
    );
}
#[test]
fn removing_a_history_row_collapses_with_motion_and_snaps_with_reduced_motion() {
    for reduced_motion in [false, true] {
        let db = elaris::history::History::open(std::path::Path::new(":memory:")).unwrap();
        db.record_visit("https://older.example/", "Older").unwrap();
        db.record_visit("https://recent.example/", "Recent")
            .unwrap();
        let mut state = AppState::new(Some("elaris://history"));
        state.history_page.page = Some(db.page("", None).unwrap());
        let mut runtime =
            rune::Runtime::new(state, crate::ui::view, (880.0, 600.0), rune::Env::default());
        runtime.set_system_reduced_motion(reduced_motion);
        runtime.frame(0.0);
        let y = |runtime: &rune::Runtime<AppState>| {
            runtime
                .elements()
                .into_iter()
                .find(|node| node.key.as_deref() == Some("history-entry:https://older.example/"))
                .unwrap()
                .rect[1]
        };
        let before = y(&runtime);
        runtime.update(|state, tx| {
            state.history_page.page.as_mut().unwrap().entries.remove(0);
            tx.animated();
        });
        runtime.frame(1.0 / 60.0);
        let mid = y(&runtime);
        let exiting = runtime.elements().iter().any(|node| {
            node.key.as_deref() == Some("history-entry:https://recent.example/") && node.exiting
        });
        runtime.frame(1.0);
        let after = y(&runtime);
        assert!(before > after);
        assert_eq!(exiting, !reduced_motion);
        if reduced_motion {
            assert_eq!(mid, after);
        } else {
            assert!(mid <= before && mid > after, "{before}, {mid}, {after}");
        }
    }
}
#[test]
fn history_mutation_preserves_tabs_and_query_on_success_or_failure() {
    let data = std::env::temp_dir().join(format!("elaris-app-history-{}", std::process::id()));
    let paths = storage::Paths {
        application_dir: data.clone(),
        profile_dir: data.join("profile"),
        history_file: data.join("history.sqlite"),
        session_file: data.join("session.json"),
        config_file: data.join("config.toml"),
    };
    let mut state = AppState::new(Some("elaris://history"));
    state.storage = Some(Storage::open(paths.clone()).unwrap());
    for (url, section) in [
        ("https://favourite.example/", TabSection::Favourite),
        ("https://pinned.example/", TabSection::Pinned),
    ] {
        state
            .browser
            .restore_tab_in_section(url, "", true, section)
            .unwrap();
    }
    let session = state.session_snapshot();

    let record = |state: &AppState, url: &str| {
        state
            .storage
            .as_ref()
            .unwrap()
            .history()
            .record_visit(url, "Example")
            .unwrap();
    };
    let pump = |state: &mut AppState| {
        state.storage.as_ref().unwrap().flush_on_close().unwrap();
        state.poll_history_page(&mut Tx::default(), false);
    };

    let url = "https://example.com/";
    record(&state, url);
    record(&state, "https://example.org/");
    state.history_page.input.replace_text("example");
    state.poll_history_page(&mut Tx::default(), false);
    pump(&mut state);
    assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 2);

    let db = rusqlite::Connection::open(&paths.history_file).unwrap();
    db.execute_batch("CREATE TRIGGER prevent_delete BEFORE DELETE ON history BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    state.mutate_history(HistoryMutation::Remove(url.into()));
    pump(&mut state);
    assert!(state.history_page.action_error.is_some());
    assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 2);
    db.execute_batch("DROP TRIGGER prevent_delete").unwrap();

    state.mutate_history(HistoryMutation::Remove(url.into()));
    pump(&mut state);
    pump(&mut state);
    assert_eq!(state.history_page.page.as_ref().unwrap().entries.len(), 1);
    assert!(state.history_page.action_error.is_none());

    state.history_page.confirming = true;
    state.mutate_history(HistoryMutation::Clear);
    pump(&mut state);
    pump(&mut state);
    assert!(state.history_page.page.as_ref().unwrap().entries.is_empty());
    assert_eq!(state.history_page.input.text, "example");
    assert_eq!(state.session_snapshot(), session);
    std::fs::remove_dir_all(data).unwrap();
}
