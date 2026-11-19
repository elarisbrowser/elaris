// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::omnibox;
use crate::omnibox::Action;
use crate::omnibox::Suggestion;
use rune::Mods;

#[test]
fn two_suggestions_fit_a_compact_panel() {
    let mut state = AppState::new(None);
    for address in ["example.org", "example.com"] {
        state.browser.open_tab(address).unwrap();
    }
    state.open_omnibox(omnibox::Mode::New);
    assert_eq!(state.suggestions().len(), 2);
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (960.0, 640.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    let elements = runtime.elements();
    let panel = elements
        .iter()
        .find(|el| el.key.as_deref() == Some("omnibox-panel"))
        .unwrap();
    assert!((190.0..=240.0).contains(&panel.target[3]));
}
#[test]
fn startup_omnibox_accepts_typing_and_opens_the_first_tab_on_enter() {
    let mut state = AppState::new(None);
    state.open_omnibox(omnibox::Mode::New);
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (960.0, 640.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    runtime.key(KeyEvent {
        key: Key::Text("example.com".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(runtime.state.browser.tabs().is_empty());
    runtime.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(runtime.state.browser.tabs().len(), 1);
    assert_eq!(active_url(&runtime.state), "https://example.com");
    assert!(!runtime.state.omnibox.open);
}
#[test]
fn opens_a_domain_it_was_given() {
    let state = AppState::new(Some("example.com"));
    assert_eq!(active_url(&state), "https://example.com");
    assert_eq!(state.address_error, None);
}
#[test]
fn treats_an_ordinary_start_argument_as_a_search() {
    let state = AppState::new(Some("not an address"));
    assert_eq!(state.browser.tabs().len(), 1);
    assert_eq!(
        active_url(&state),
        "https://duckduckgo.com/?q=not%20an%20address"
    );
    assert_eq!(state.address_error, None);
}
#[test]
fn submitting_navigates_the_current_tab_for_urls_and_searches() {
    let mut state = AppState::new(Some("example.org"));
    state.omnibox.input.replace_text("example.com/a".to_owned());
    state.submit_omnibox();
    assert_eq!(active_url(&state), "https://example.com/a");
    assert_eq!(state.address_error, None);

    state
        .omnibox
        .input
        .replace_text("best Rust GUI framework".to_owned());
    state.submit_omnibox();
    assert_eq!(
        active_url(&state),
        "https://duckduckgo.com/?q=best%20Rust%20GUI%20framework"
    );
    assert_eq!(state.browser.tabs().len(), 1);
    assert_eq!(state.address_error, None);
}
#[test]
fn cmd_l_opens_with_the_current_url_selected_even_when_sidebar_is_collapsed() {
    let mut state = AppState::new(Some("example.com"));
    state.sidebar.collapsed = true;
    let mut tx = Tx::default();
    let event = KeyEvent {
        key: Key::Command('l'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    };
    assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
    assert!(state.omnibox.open);
    assert_eq!(state.omnibox.input.text, "https://example.com");
    assert_eq!(
        (state.omnibox.input.anchor, state.omnibox.input.focus),
        (0, state.omnibox.input.text.len())
    );
    assert_eq!(active_url(&state), "https://example.com");
}
#[test]
fn escape_closes_without_changing_the_page() {
    let mut state = AppState::new(Some("example.com"));
    state.open_omnibox(omnibox::Mode::Current);
    let mut tx = Tx::default();
    assert!(omnibox_input_key(
        &mut state,
        &KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        },
        &mut tx,
    ));
    assert!(!state.omnibox.open);
    assert_eq!(active_url(&state), "https://example.com");
}
#[test]
fn choosing_a_navigation_result_reuses_the_active_tab() {
    let mut state = AppState::new(Some("example.com"));
    let active = state.browser.active_tab().unwrap().id();
    state.open_omnibox(omnibox::Mode::Current);
    state.omnibox.input.replace_text("example.org".to_owned());
    let suggestion = state.suggestions().remove(0);
    state.choose_suggestion(suggestion, &mut Tx::default());
    assert_eq!(active_url(&state), "https://example.org");
    assert_eq!(state.browser.active_tab().unwrap().id(), active);
    assert_eq!(state.browser.tabs().len(), 1);
    assert!(!state.omnibox.open);
}
#[test]
fn choosing_a_history_result_uses_the_omnibox_tab_mode() {
    let mut state = AppState::new(Some("example.org"));
    let current = state.browser.active_tab().unwrap().id();
    let recent = Suggestion {
        section: omnibox::Section::Recent,
        title: "Example Domain".to_owned(),
        detail: "example.com".to_owned(),
        glyph: omnibox::Glyph::History,
        action: Action::VisitRecent("https://example.com/".to_owned()),
    };

    state.open_omnibox(omnibox::Mode::Current);
    state.choose_suggestion(recent.clone(), &mut Tx::default());
    assert_eq!(state.browser.active_tab().unwrap().id(), current);
    assert_eq!(active_url(&state), "https://example.com/");
    assert_eq!(state.browser.tabs().len(), 1);

    state.open_omnibox(omnibox::Mode::New);
    state.choose_suggestion(recent, &mut Tx::default());
    assert_eq!(state.browser.tabs().len(), 2);
    assert_eq!(active_url(&state), "https://example.com/");
    assert_ne!(state.browser.active_tab().unwrap().id(), current);
}
#[test]
fn new_tab_opens_empty_and_dismisses_without_creating_tabs() {
    let mut state = AppState::new(Some("example.com"));
    let mut tx = Tx::default();
    assert!(crate::shortcuts::on_key(
        &mut state,
        &KeyEvent {
            key: Key::Command('t'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        },
        &mut tx,
    ));
    for _ in 0..3 {
        assert!(state.omnibox.open);
        submit_omnibox(&mut state, &mut tx);
        state.omnibox.close();
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(active_url(&state), "https://example.com");
        new_tab(&mut state, &mut tx);
    }
    for (query, expected, count) in [
        ("example.org", "https://example.org", 2),
        ("two words", "https://duckduckgo.com/?q=two%20words", 3),
    ] {
        new_tab(&mut state, &mut tx);
        state.omnibox.input.replace_text(query);
        submit_omnibox(&mut state, &mut tx);
        assert_eq!(state.browser.tabs().len(), count);
        assert_eq!(active_url(&state), expected);
    }
}
#[test]
fn the_status_reports_unsupported_schemes_and_new_tabs_clear_them() {
    let mut state = AppState::new(None);
    assert_eq!(state.status(), None);
    state
        .omnibox
        .input
        .replace_text("ftp://example.com".to_owned());
    state.submit_omnibox();
    assert!(state.status().is_some());
    new_tab(&mut state, &mut Tx::default());
    assert_eq!(state.status(), None);
}
#[test]
fn launcher_selects_the_current_address() {
    let mut state = AppState::new(Some("example.com"));
    open_from_launcher(&mut state, &mut Tx::default());
    assert!(state.omnibox.open);
    assert_eq!(state.omnibox.input.text, "https://example.com");
}
#[test]
fn selecting_an_open_tab_activates_it_without_opening_a_duplicate() {
    let mut state = AppState::new(Some("example.com"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("github.com").unwrap();
    state.activate(second);
    state.open_omnibox(omnibox::Mode::New);
    state.omnibox.input.replace_text("example.com");
    let tab_suggestion = state
        .suggestions()
        .into_iter()
        .find(|suggestion| suggestion.action == Action::SwitchTab(first))
        .unwrap();
    state.choose_suggestion(tab_suggestion, &mut Tx::default());
    assert_eq!(state.browser.active_tab().unwrap().id(), first);
    assert_eq!(state.browser.tabs().len(), 2);
    assert_eq!(active_url(&state), "https://example.com");
    assert!(!state.omnibox.open);
}
#[test]
fn omnibox_keys_stay_in_the_input_and_escape_restores_page_focus() {
    use rune::{Env, Runtime};

    let mut runtime = Runtime::new(
        AppState::new(Some("example.com")),
        crate::ui::view,
        (960.0, 640.0),
        Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    runtime.update(|state, tx| {
        state.open_omnibox(omnibox::Mode::New);
        tx.focus("omnibox-input", false);
    });
    runtime.frame(0.0);
    assert!(
        runtime
            .elements()
            .iter()
            .any(|element| element.focused && element.key.as_deref() == Some("omnibox-input"))
    );

    runtime.key(KeyEvent {
        key: Key::Text("x".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(runtime.state.omnibox.input.text, "x");
    assert_eq!(active_url(&runtime.state), "https://example.com");

    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(!runtime.state.omnibox.open);
    assert!(
        runtime
            .elements()
            .iter()
            .any(|element| element.focused && element.key.as_deref() == Some(VIEWPORT))
    );
}
