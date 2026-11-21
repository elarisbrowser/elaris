// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::address::SearchEngine;
use crate::appearance::Appearance;
use crate::omnibox;
use crate::settings::{ContentBlockList, DownloadDirectory};
use rune::Mods;
use std::collections::BTreeSet;

#[test]
fn the_settings_shortcut_reuses_its_existing_native_tab() {
    let mut state = AppState::new(Some("example.com"));
    let website = state.browser.active_tab().unwrap().id();
    let mut tx = Tx::default();
    let event = KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    };
    assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
    let settings = state.browser.active_tab().unwrap().id();
    assert!(state.browser.active_tab().unwrap().is_internal());
    assert_eq!(state.browser.tabs().len(), 2);

    state.activate(website);
    assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
    assert_eq!(state.browser.tabs().len(), 2);
    assert_eq!(state.browser.active_tab().unwrap().id(), settings);
}
#[test]
fn settings_updates_drive_search_download_and_startup_preferences() {
    let mut state = AppState::new(Some("example.com"));
    state.set_appearance(Appearance::Dark);
    state.set_search_engine(SearchEngine::Brave);
    state.set_startup(StartupMode::BlankWindow);
    let download_path = PathBuf::from("/tmp/elaris-test-downloads");
    state.set_download_directory(DownloadDirectory::Custom(download_path.clone()));

    assert_eq!(state.settings.appearance, Appearance::Dark);
    assert_eq!(state.settings.search_engine, SearchEngine::Brave);
    assert_eq!(state.settings.startup, StartupMode::BlankWindow);
    assert_eq!(state.download_directory(), download_path);

    state.open_omnibox(omnibox::Mode::Current);
    state
        .omnibox
        .input
        .replace_text("browser settings".to_owned());
    state.submit_omnibox();
    assert_eq!(
        active_url(&state),
        "https://search.brave.com/search?q=browser%20settings"
    );
}

#[test]
fn content_blocking_settings_keep_the_enabled_lists() {
    let mut state = AppState::new(Some("example.com"));
    assert!(!state.settings.content_blocking.enabled);

    state.set_content_blocking(true);
    state.set_content_blocking_list(ContentBlockList::CookieNotices, true);
    state.set_content_blocking_list(ContentBlockList::EasyPrivacy, false);

    assert!(state.settings.content_blocking.enabled);
    assert_eq!(
        state.settings.content_blocking.lists,
        BTreeSet::from([ContentBlockList::EasyList, ContentBlockList::CookieNotices])
    );
}
