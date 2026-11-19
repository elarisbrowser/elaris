// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{TabUpdate, Update};
use elaris::ladybird::{MouseEvent, MouseKind};
use rune::{Key as RuneKey, KeyEvent, Mods, PointerButton};

#[test]
fn closing_the_active_tab_activates_its_neighbour() {
    let (mut browser, _fake) = ready();
    let [a, b, c] = ["a.com", "b.com", "c.com"].map(|a| browser.open_tab(a).unwrap());
    browser.activate_tab(b);

    // An inactive tab closes without changing which one is active.
    browser.close_tab(a);
    assert_eq!(browser.active_tab().map(Tab::id), Some(b));

    // The active one hands over to the tab after it, or before it when it was last.
    browser.close_tab(b);
    assert_eq!(browser.active_tab().map(Tab::id), Some(c));
    let d = browser.open_tab("d.com").unwrap();
    browser.activate_tab(d);
    browser.close_tab(d);
    assert_eq!(browser.active_tab().map(Tab::id), Some(c));
    browser.close_tab(c);
    assert!(browser.active_tab().is_none());
}

#[test]
fn finds_the_tab_beside_another_wrapping_around() {
    let (mut browser, _fake) = ready();
    let [a, b, c] = ["a.com", "b.com", "c.com"].map(|a| browser.open_tab(a).unwrap());
    assert_eq!(browser.tab_beside(a, 1), Some(b));
    assert_eq!(browser.tab_beside(c, 1), Some(a));
    assert_eq!(browser.tab_beside(a, -1), Some(c));
    browser.close_tab(b);
    assert_eq!(browser.tab_beside(b, 1), None);
}

#[test]
fn input_goes_only_to_the_active_tab_in_device_pixels() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    browser.open_tab("example.org").unwrap();
    browser.poll();
    sent(&fake);

    browser.note_scale(2.0);
    browser.drag_pointer(
        MouseKind::Down,
        [1.0, 2.0],
        PointerButton::Primary,
        Mods::default(),
        1,
    );
    let enter = KeyEvent {
        key: RuneKey::Enter,
        mods: Mods::default(),
    };
    assert!(browser.key(&enter, true, false, None));
    assert!(browser.key(&enter, false, false, None));
    let shortcut = KeyEvent {
        key: RuneKey::Command('q'),
        mods: Mods::default(),
    };
    assert!(!browser.key(&shortcut, true, false, None));
    let sent = sent(&fake);
    assert_eq!(sent.len(), 3);
    assert!(matches!(
        sent[0],
        Command::View(id, ViewCommand::Mouse(MouseEvent { x: 2, y: 4, .. })) if id == first
    ));
    assert!(
        sent[1..]
            .iter()
            .all(|c| matches!(c, Command::View(id, ViewCommand::Key(_)) if *id == first))
    );
}

#[test]
fn moves_tabs_in_both_directions_without_replacing_their_pages() {
    let (mut browser, fake) = connected();
    let [a, b, c, d] = ["a.example", "b.example", "c.example", "d.example"]
        .map(|address| browser.open_tab(address).unwrap());
    browser.activate_tab(c);
    let surfaces = [a, b, c, d].map(|id| (id, surface(&browser, id)));
    sent(&fake);

    browser.move_tab(d, 0).unwrap();
    assert_eq!(
        browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
        [d, a, b, c]
    );
    browser.move_tab(d, 3).unwrap();
    assert_eq!(
        browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
        [a, b, c, d]
    );
    browser.move_tab(a, 3).unwrap();
    assert_eq!(
        browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
        [b, c, d, a]
    );
    browser.move_tab(a, 3).unwrap();

    assert_eq!(browser.active_tab().map(Tab::id), Some(c));
    for (id, old_surface) in surfaces {
        assert_eq!(surface(&browser, id), old_surface);
    }
    assert!(
        sent(&fake).is_empty(),
        "reordering sends no engine commands"
    );
    assert_eq!(browser.tab_beside(c, 1), Some(d));
    assert_eq!(browser.tab_beside(c, -1), Some(b));
}

#[test]
fn rejects_missing_tabs_and_out_of_range_positions() {
    let mut browser = Browser::default();
    let first = browser.open_tab("a.example").unwrap();
    let second = browser.open_tab("b.example").unwrap();
    let missing = TabId::from_raw(99);

    assert_eq!(browser.move_tab(missing, 0), Err(Error::NoSuchTab(missing)));
    assert_eq!(browser.move_tab(first, 2), Err(Error::InvalidTabIndex(2)));
    assert_eq!(
        browser.move_tab(first, usize::MAX),
        Err(Error::InvalidTabIndex(usize::MAX))
    );
    assert_eq!(
        browser.tabs().iter().map(Tab::id).collect::<Vec<_>>(),
        [first, second]
    );
}

#[test]
fn tab_ids_are_never_reused() {
    let (mut browser, _fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    browser.close_tab(first);
    let second = browser.open_tab("example.com").unwrap();
    assert_ne!(first, second);
    assert!(browser.tab(first).is_none());
    browser.activate_tab(first);
    assert_eq!(browser.active_tab().map(Tab::id), Some(second));
}

#[test]
fn updates_reach_only_their_own_tab_and_never_a_closed_one() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    let send = |id, update| fake.updates.send(Update::Tab(id, update)).unwrap();
    send(first, TabUpdate::Title("First".into()));
    send(second, TabUpdate::Title("Second".into()));
    send(second, TabUpdate::Loading(true));
    assert!(browser.poll().changed);
    assert_eq!(browser.tab(first).unwrap().title, "First");
    assert!(!browser.tab(first).unwrap().loading);
    assert_eq!(browser.tab(second).unwrap().title, "Second");
    assert!(browser.tab(second).unwrap().loading);

    browser.close_tab(second);
    send(second, TabUpdate::Title("Late".into()));
    assert!(!browser.poll().changed);
    assert_eq!(browser.tab(first).unwrap().title, "First");
}
