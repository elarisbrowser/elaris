// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{TabUpdate, Update};
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
fn shared_engine_updates_are_demultiplexed_to_the_owning_browser_window() {
    let (engine, fake) = Engine::fake();
    let mut first = Browser::default();
    let mut second = Browser::default();
    first.connect(engine.clone());
    second.connect(engine);
    let first_tab = first.open_tab("first.example").unwrap();
    let second_tab = second.open_tab("second.example").unwrap();
    assert_ne!(first_tab, second_tab);

    fake.updates
        .send(Update::Tab(
            first_tab,
            TabUpdate::Title("First window".into()),
        ))
        .unwrap();
    fake.updates
        .send(Update::Tab(
            second_tab,
            TabUpdate::Title("Second window".into()),
        ))
        .unwrap();

    first.poll();
    assert_eq!(first.tab(first_tab).unwrap().title, "First window");
    assert!(second.tab(second_tab).unwrap().title.is_empty());
    second.poll();
    assert_eq!(second.tab(second_tab).unwrap().title, "Second window");
}

#[test]
fn updates_for_a_closed_window_tab_are_discarded_without_affecting_other_windows() {
    let (engine, fake) = Engine::fake();
    let mut first = Browser::default();
    let mut second = Browser::default();
    first.connect(engine.clone());
    second.connect(engine);
    let closed_tab = first.open_tab("closed.example").unwrap();
    let live_tab = second.open_tab("live.example").unwrap();
    first.close_tab(closed_tab);
    fake.updates
        .send(Update::Tab(closed_tab, TabUpdate::Title("stale".into())))
        .unwrap();
    fake.updates
        .send(Update::Tab(live_tab, TabUpdate::Title("live".into())))
        .unwrap();

    first.poll();
    second.poll();
    assert_eq!(second.tab(live_tab).unwrap().title, "live");
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
fn closing_a_window_suspends_pages_without_stopping_the_engine() {
    let (mut browser, fake) = ready();
    let tab = browser.open_tab("example.com").unwrap();
    browser.poll();
    sent(&fake);
    browser.set_window_focus(true);
    browser.suspend_window();

    assert_eq!(sent(&fake), [view(tab, ViewCommand::Visible(false)),]);
    assert!(!browser.window_focused());
}

#[test]
fn closing_a_window_releases_its_views_without_stopping_the_shared_engine() {
    let (mut browser, fake) = ready();
    let tab = browser.open_tab("example.com").unwrap();
    browser.poll();
    sent(&fake);

    browser.close_window_views();

    assert_eq!(
        sent(&fake),
        [view(tab, ViewCommand::Visible(false)), Command::Close(tab),]
    );
    assert!(browser.tabs().is_empty());
    assert!(!tab.is_live());
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
