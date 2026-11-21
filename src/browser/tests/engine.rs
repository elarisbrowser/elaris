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
    assert_eq!(
        sent(&fake),
        [Command::Open(tab, surface(&browser, tab), false)]
    );
    assert_eq!(browser.active_tab().map(Tab::id), Some(tab));
}

#[test]
fn a_private_browser_opens_every_page_in_the_private_session() {
    let (engine, fake) = Engine::fake();
    let mut browser = Browser {
        private: true,
        ..Browser::default()
    };
    browser.connect(engine);
    let tab = browser.open_tab("example.com").unwrap();
    assert!(
        sent(&fake)
            .iter()
            .any(|command| matches!(command, Command::Open(id, _, true) if *id == tab))
    );
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

#[test]
fn transferring_a_tab_retains_its_view_and_routes_pending_updates_to_its_new_owner() {
    let (engine, fake) = Engine::fake();
    let mut source = Browser::default();
    let mut destination = Browser::default();
    source.connect(engine.clone());
    destination.connect(engine);
    let tab = source.open_tab("first.example").unwrap();
    let existing = destination.open_tab("second.example").unwrap();
    let pixels = surface(&source, tab);
    sent(&fake);

    fake.updates
        .send(Update::Tab(tab, TabUpdate::Title("Still loading".into())))
        .unwrap();
    source
        .transfer_tab(&mut destination, tab, crate::tab::TabSection::Regular, 1)
        .unwrap();
    assert!(source.tabs().is_empty());
    assert!(source.active_tab().is_none());
    assert!(tab.is_live());
    assert_eq!(destination.tabs()[0].id(), existing);
    assert_eq!(destination.active_tab().unwrap().id(), tab);
    assert_eq!(surface(&destination, tab), pixels);
    source.close_window_views();
    source.poll();
    destination.poll();
    assert_eq!(destination.tab(tab).unwrap().title, "Still loading");

    destination
        .transfer_tab(&mut source, tab, crate::tab::TabSection::Favourite, 0)
        .unwrap();
    assert_eq!(source.active_tab().unwrap().id(), tab);
    assert_eq!(
        source.tabs()[0].section(),
        crate::tab::TabSection::Favourite
    );
    assert_eq!(surface(&source, tab), pixels);
    assert_eq!(destination.active_tab().unwrap().id(), existing);
    assert!(
        sent(&fake)
            .iter()
            .all(|command| !matches!(command, Command::Open(..) | Command::Close(_)))
    );
}

#[test]
fn invalid_transfer_leaves_both_browsers_unchanged() {
    use crate::tab::TabSection;

    let (mut source, _) = connected();
    let (mut other_engine, _) = connected();
    let tab = source.open_tab("first.example").unwrap();
    let pixels = surface(&source, tab);
    assert!(
        source
            .transfer_tab(&mut other_engine, tab, TabSection::Regular, 1)
            .is_err()
    );
    assert!(
        source
            .transfer_tab(&mut other_engine, tab, TabSection::Regular, 0)
            .is_err()
    );
    assert_eq!(source.active_tab().unwrap().id(), tab);
    assert_eq!(surface(&source, tab), pixels);
    assert!(other_engine.tabs().is_empty());
}

#[cfg(feature = "ladybird-bridge")]
#[test]
#[ignore = "requires a built Ladybird bridge, helpers and resources"]
fn live_document_survives_a_browser_transfer() {
    use crate::tab::TabSection;
    use std::time::{Duration, Instant};

    let fixture = std::env::temp_dir().join(format!("elaris-transfer-{}", std::process::id()));
    std::fs::create_dir_all(&fixture).unwrap();
    let page = fixture.join("transfer.html");
    std::fs::write(
        &page,
        r#"<!doctype html><script>
        let ticks = 0;
        setInterval(() => document.title = performance.timeOrigin + ":" + ++ticks, 30);
        </script>"#,
    )
    .unwrap();
    let engine = Engine::start(
        std::sync::Arc::new(|| {}),
        fixture.join("profile"),
        None,
        60.0,
    )
    .unwrap();
    let mut source = Browser::default();
    let mut destination = Browser::default();
    source.connect(engine.clone());
    destination.connect(engine);
    lay_out(&mut source, [640.0, 480.0], 1.0, false);
    lay_out(&mut destination, [720.0, 480.0], 2.0, false);
    let id = source.open_tab(page.to_str().unwrap()).unwrap();
    let wait = |browser: &mut Browser, after: u64| {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            browser.poll();
            let tab = browser.tab(id).unwrap();
            if let Some((document, ticks)) = tab.title.split_once(':')
                && let Ok(ticks) = ticks.parse::<u64>()
                && ticks > after
                && tab.surface.frames() > 0
            {
                break (document.to_owned(), ticks);
            }
            assert!(
                Instant::now() < deadline,
                "No live document update: {:?}, {:?}",
                tab.title,
                tab.error
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let (document, ticks) = wait(&mut source, 0);
    source
        .transfer_tab(&mut destination, id, TabSection::Regular, 0)
        .unwrap();
    source.close_window_views();
    let (after, _) = wait(&mut destination, ticks);
    assert_eq!(document, after);
    destination.shut_down();
}
