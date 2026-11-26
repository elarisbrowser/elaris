// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::omnibox;
use crate::storage::SessionTab;
use rune::Mods;

#[test]
fn starts_with_an_empty_window_without_an_about_blank_tab() {
    let state = AppState::new(None);
    assert!(state.browser.tabs().is_empty());
    assert!(!state.omnibox.open);
    assert_eq!(state.omnibox.input.text, "");
    assert_eq!(state.address_error, None);
}

#[test]
fn a_new_browser_window_has_independent_empty_window_state() {
    let first = AppState::new(Some("first.example"));
    let second = AppState::new_window();
    assert_ne!(first.window_id, second.window_id);
    assert!(first.browser.active_tab().is_some());
    assert!(second.browser.active_tab().is_none());
    assert!(second.browser.tabs().is_empty());
    assert!(second.omnibox.open);
}
#[test]
fn blank_window_startup_does_not_create_a_web_tab() {
    let mut state = AppState::new(None);
    state.settings.set_startup(StartupMode::BlankWindow);
    state.open_start_page(None, None);
    assert!(state.browser.tabs().is_empty());
    assert!(!state.omnibox.open);
}

#[test]
fn horizontal_blank_window_startup_opens_the_configured_new_tab() {
    let mut state = AppState::new(None);
    state.settings.set_startup(StartupMode::BlankWindow);
    state.settings.tab_layout = elaris_core::settings::TabLayout::Horizontal;
    state.settings.open_tab_on_startup = true;
    state.open_start_page(None, None);

    assert_eq!(state.browser.tabs().len(), 1);
    assert_eq!(state.browser.active_tab().unwrap().label(), "New Tab");
    assert!(state.omnibox.open);
    assert_eq!(state.omnibox.mode, omnibox::Mode::New);
}

#[test]
fn horizontal_session_startup_opens_a_new_tab_without_discarding_restored_tabs() {
    let mut state = AppState::new(None);
    state.settings.tab_layout = elaris_core::settings::TabLayout::Horizontal;
    state.settings.open_tab_on_startup = true;
    let session = Session::new(
        vec![SessionTab {
            url: "https://restored.example/".into(),
            title: "Restored".into(),
            favicon: None,
        }],
        0,
    );

    state.open_start_page(None, Some(&session));

    assert_eq!(state.browser.tabs().len(), 2);
    assert_eq!(state.browser.tabs()[0].url, "https://restored.example/");
    assert_eq!(state.browser.active_tab().unwrap().url, "about:blank");
    assert!(state.omnibox.open);
    assert_eq!(state.omnibox.mode, omnibox::Mode::New);
}

#[test]
fn horizontal_startup_can_leave_the_window_without_tabs() {
    let mut state = AppState::new(None);
    state.settings.tab_layout = elaris_core::settings::TabLayout::Horizontal;
    state.settings.open_tab_on_startup = false;
    state.open_start_page(None, None);
    assert!(state.browser.tabs().is_empty());
    assert!(!state.omnibox.open);
}

#[test]
fn reduced_motion_centers_the_resized_omnibox_immediately() {
    let mut state = AppState::new(None);
    state.open_omnibox(omnibox::Mode::New);
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (960.0, 640.0),
        rune::Env::new(false, 1.0),
    );
    runtime.set_system_reduced_motion(true);
    runtime.frame(0.0);
    runtime.key(KeyEvent {
        key: Key::Text("x".into()),
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);
    let panel = runtime
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some("omnibox-panel"))
        .unwrap();
    assert_eq!(panel.rect, panel.target);
    let [x, y, width, height] = panel.rect;
    assert_eq!((x + width / 2.0, y + height / 2.0), (480.0, 320.0));
}
