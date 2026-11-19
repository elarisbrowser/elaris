// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::Update;
use crate::tab::Tab;
use rune::{Key as RuneKey, KeyEvent, Mods};

#[test]
fn a_tab_opened_before_the_engine_opens_once_it_connects() {
    let mut browser = Browser::default();
    let tab = browser.open_tab("example.com").unwrap();
    let (engine, fake) = Engine::fake();
    browser.connect(engine);
    assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);
    assert_eq!(browser.active_tab().map(Tab::id), Some(tab));
}

#[test]
fn an_engine_failure_shows_in_every_tab_and_new_ones() {
    let (mut browser, fake) = ready();
    let tab = browser.open_tab("example.com").unwrap();
    fake.updates.send(Update::Failed("gone".into())).unwrap();
    assert!(browser.poll().changed);
    assert_eq!(browser.tab(tab).unwrap().error.as_deref(), Some("gone"));
    let later = browser.open_tab("example.org").unwrap();
    assert_eq!(browser.tab(later).unwrap().error.as_deref(), Some("gone"));
}

#[test]
fn shutting_down_asks_the_engine_to_stop() {
    let (mut browser, fake) = connected();
    browser.shut_down();
    assert_eq!(sent(&fake), [Command::Shutdown]);
}

#[test]
fn without_an_engine_tabs_keep_their_page_and_take_no_keys() {
    let mut browser = Browser::default();
    let tab = browser.open_tab("example.com").unwrap();
    browser.navigate(tab, "example.org").unwrap();
    assert_eq!(
        browser.tab(tab).unwrap().engine.pending_load.as_deref(),
        Some("https://example.org")
    );
    let key = KeyEvent {
        key: RuneKey::Enter,
        mods: Mods::default(),
    };
    assert!(!browser.key(&key, true, false, None));
    assert!(!browser.poll().changed);
}
