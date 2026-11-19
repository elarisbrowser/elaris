// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::omnibox;
use rune::Mods;

#[test]
fn cmd_shift_t_reopens_the_last_closed_tab_in_its_original_section() {
    let mut state = AppState::new(Some("example.com"));
    let original = state.browser.active_tab().unwrap().id();
    state
        .browser
        .move_tab_to(original, TabSection::Favourite, 0)
        .unwrap();
    state.close(original);
    assert!(state.omnibox.open);

    let event = KeyEvent {
        key: Key::Command('t'),
        mods: Mods {
            cmd: true,
            shift: true,
            ..Mods::default()
        },
    };
    assert!(crate::shortcuts::on_key(
        &mut state,
        &event,
        &mut Tx::default()
    ));

    let reopened = state.browser.active_tab().unwrap();
    assert_ne!(reopened.id(), original);
    assert_eq!(reopened.url, "https://example.com");
    assert_eq!(reopened.section(), TabSection::Favourite);
    assert!(!state.omnibox.open);
    assert!(state.recently_closed().is_empty());
}
#[test]
fn close_others_keeps_recently_closed_pinned_tabs_reopenable() {
    let mut state = AppState::new(Some("pinned.example"));
    let pinned = state.browser.active_tab().unwrap().id();
    state
        .browser
        .move_tab_to(pinned, TabSection::Pinned, 0)
        .unwrap();
    let kept = state.browser.open_tab("kept.example").unwrap();
    state.activate(kept);

    state.close_others(kept);
    assert_eq!(state.browser.tabs().len(), 1);
    assert_eq!(state.recently_closed().len(), 1);

    let reopened = state.reopen_recently_closed(0).unwrap();
    let tab = state.browser.tab(reopened).unwrap();
    assert_eq!(tab.url, "https://pinned.example");
    assert_eq!(tab.section(), TabSection::Pinned);
    assert_eq!(state.browser.active_tab().map(Tab::id), Some(reopened));
}
#[test]
fn omnibox_routes_internal_pages_and_keeps_ordinary_web_navigation() {
    let mut state = AppState::new(Some("example.com"));
    let tab = state.browser.active_tab().unwrap().id();
    state.open_omnibox(omnibox::Mode::Current);
    state
        .omnibox
        .input
        .replace_text("elaris://settings".to_owned());
    state.submit_omnibox();
    assert_eq!(state.browser.active_tab().unwrap().id(), tab);
    assert!(state.browser.active_tab().unwrap().is_internal());
    assert_eq!(active_url(&state), "elaris://settings");

    state.open_omnibox(omnibox::Mode::Current);
    state.omnibox.input.replace_text("example.org".to_owned());
    state.submit_omnibox();
    assert!(!state.browser.active_tab().unwrap().is_internal());
    assert_eq!(active_url(&state), "https://example.org");
}
#[test]
fn closing_the_last_tab_returns_to_the_empty_omnibox() {
    let mut state = AppState::new(Some("example.com"));
    let only = state.browser.active_tab().unwrap().id();
    state.close(only);
    assert!(state.browser.tabs().is_empty());
    assert!(state.omnibox.open);
    assert!(state.omnibox.input.text.is_empty());
}
