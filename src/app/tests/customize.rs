// SPDX-License-Identifier: GPL-3.0-only

//! Rebinding shortcuts and adding search engines from Settings.

use super::*;
use crate::shortcuts::Shortcut;
use elaris_core::address::SearchEngine;
use elaris_core::keybindings::Chord;
use rune::Mods;

fn chord(key: char, shift: bool) -> KeyEvent {
    KeyEvent {
        key: Key::Command(key),
        mods: Mods {
            cmd: true,
            shift,
            ..Mods::default()
        },
    }
}

fn plain(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

#[test]
fn a_recorded_chord_replaces_the_actions_keys() {
    let mut state = AppState::new(None);
    state.toggle_recording(Shortcut::Find);
    // Close tab has Command-W: the first press shows the conflict and changes nothing.
    assert!(state.record_key(&chord('w', false)));
    let recorder = &state.shortcut_recorder;
    assert_eq!(recorder.recording, Some(Shortcut::Find));
    assert_eq!(
        recorder.pending,
        Some((Chord::new('w', false, false), Shortcut::Close))
    );
    assert!(recorder.problem);
    assert!(state.settings.keybindings.is_empty());

    // Pressing it again moves it.
    assert!(state.record_key(&chord('w', false)));
    assert_eq!(state.shortcut_recorder.recording, None);
    assert_eq!(state.shortcut_recorder.pending, None);
    assert_eq!(
        state.settings.keybindings["find"],
        [Chord::new('w', false, false)]
    );
    let notice = state.shortcut_recorder.notice.clone().unwrap();
    assert!(notice.ends_with("moved here from close tab."), "{notice}");
    assert!(!state.shortcut_recorder.problem);
    assert!(state.shortcut_customized(Shortcut::Find));

    state.reset_shortcut(Shortcut::Find);
    assert!(!state.shortcut_customized(Shortcut::Find));
}

#[test]
fn reserved_and_unmodified_keys_keep_listening() {
    let mut state = AppState::new(None);
    state.toggle_recording(Shortcut::NewTab);
    assert!(state.record_key(&chord('c', false)));
    let notice = state.shortcut_recorder.notice.clone().unwrap();
    assert!(notice.ends_with("is reserved for Copy."), "{notice}");
    assert!(state.record_key(&plain(Key::Text("k".into()))));
    assert!(
        state
            .shortcut_recorder
            .notice
            .as_ref()
            .unwrap()
            .starts_with("Hold")
    );
    assert_eq!(state.shortcut_recorder.recording, Some(Shortcut::NewTab));
    assert!(state.settings.keybindings.is_empty());

    // Nothing reaches other handlers while listening, not even Tab or Control-Tab.
    assert!(state.record_key(&plain(Key::Tab)));
    let control_tab = KeyEvent {
        key: Key::Tab,
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
    };
    assert!(state.record_key(&control_tab));
    assert_eq!(state.shortcut_recorder.recording, Some(Shortcut::NewTab));
    assert!(state.record_key(&plain(Key::Escape)));
    assert_eq!(state.shortcut_recorder.notice, None);
    assert_eq!(state.shortcut_recorder.recording, None);
    assert!(state.settings.keybindings.is_empty());
}

#[test]
fn backspace_removes_the_keys_and_reset_all_restores_them() {
    let mut state = AppState::new(None);
    state.toggle_recording(Shortcut::CopyAddress);
    assert!(state.record_key(&plain(Key::Backspace)));
    assert!(state.settings.keybindings["copy_address"].is_empty());
    state.toggle_recording(Shortcut::NewTab);
    state.toggle_recording(Shortcut::NewTab);
    assert_eq!(state.shortcut_recorder.recording, None);

    state.reset_all_shortcuts();
    assert!(state.settings.keybindings.is_empty());
}

#[test]
fn the_form_adds_engines_and_removing_the_chosen_one_falls_back() {
    let mut state = AppState::new(None);
    state.search_engine_form.name.text = "Kagi".into();
    state.search_engine_form.url.text = "kagi.com/?q=%s".into();
    state.add_search_engine();
    assert!(state.search_engine_form.error.is_some());
    assert!(state.settings.custom_search_engines.is_empty());

    state.search_engine_form.url.text = "https://kagi.com/?q=%s".into();
    state.add_search_engine();
    assert_eq!(state.search_engine_form.error, None);
    assert!(state.search_engine_form.name.text.is_empty());
    assert_eq!(state.settings.custom_search_engines[0].key(), "kagi");

    state.set_search_engine(SearchEngine::Custom(0));
    state.remove_search_engine(0);
    assert!(state.settings.custom_search_engines.is_empty());
    assert_eq!(state.settings.search_engine, SearchEngine::default());
}

/// Where the element with `key` is, if it is showing.
fn find(runtime: &rune::Runtime<AppState>, key: &str) -> Option<[f32; 4]> {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        .map(|element| element.rect)
}

#[test]
fn the_keyboard_page_records_a_chord_before_it_runs_as_a_shortcut() {
    let mut state = AppState::new(Some("elaris://settings"));
    state.page_category = Some("settings-keyboard");
    let tabs = state.browser.tabs().len();
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    let [x, y, width, height] = find(&runtime, "shortcut-keys-new_tab").expect("New tab's keys");
    runtime.pointer_down(x + width / 2.0, y + height / 2.0);
    runtime.pointer_up(x + width / 2.0, y + height / 2.0);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.shortcut_recorder.recording,
        Some(Shortcut::NewTab)
    );

    // Command-W would close the tab. While recording it only shows the conflict with Close tab,
    // and a second press moves it to New Tab.
    runtime.key(chord('w', false));
    runtime.frame(0.0);
    assert_eq!(runtime.state.browser.tabs().len(), tabs);
    assert!(runtime.state.settings.keybindings.is_empty());
    assert!(runtime.state.shortcut_recorder.pending.is_some());
    runtime.key(chord('w', false));
    runtime.frame(0.0);
    assert_eq!(runtime.state.browser.tabs().len(), tabs);
    assert_eq!(
        runtime.state.settings.keybindings["new_tab"],
        [Chord::new('w', false, false)]
    );
    assert_eq!(runtime.state.shortcut_recorder.recording, None);
    assert!(find(&runtime, "shortcut-reset-new_tab").is_some());
}

#[test]
fn the_search_section_lists_custom_engines_with_the_add_form() {
    let mut state = AppState::new(Some("elaris://settings"));
    state.page_category = Some("settings-browsing");
    state
        .settings
        .add_custom_search_engine("Kagi", "https://kagi.com/?q=%s")
        .unwrap();
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    for key in [
        "search-engine-kagi",
        "search-engine-remove-kagi",
        "search-engine-url",
    ] {
        assert!(find(&runtime, key).is_some(), "{key}");
    }
}

#[test]
fn conflicting_config_keys_are_reported_on_their_rows() {
    let mut state = AppState::new(Some("elaris://settings"));
    let config = elaris_core::config::parse(
        "[keybindings]\nfind = \"mod+c\"\nnew_tab = \"mod+k\"\nreload = \"mod+k\"\n",
    )
    .unwrap();
    state.settings.keybindings = config.settings.keybindings;
    let conflicts = crate::shortcuts::conflicts(&state.settings.keybindings);
    let on = |shortcut| {
        conflicts
            .iter()
            .any(|conflict| conflict.shortcut == Some(shortcut))
    };
    assert!(on(Shortcut::Find));
    assert!(on(Shortcut::Reload));
    assert!(!on(Shortcut::NewTab));

    // The page still draws every row, conflicting ones included.
    state.page_category = Some("settings-keyboard");
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    assert!(find(&runtime, "shortcut-keys-find").is_some());
    assert!(find(&runtime, "shortcut-reset-find").is_some());
}
