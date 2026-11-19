// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::Update;
use elaris::ladybird::MouseKind;
use rune::{Key as RuneKey, KeyEvent, Mods};
use std::sync::Arc;

#[test]
fn only_the_active_tab_is_shown_resized_and_focused() {
    let (mut browser, fake) = ready();
    browser.set_viewport_focus(true);
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    assert_eq!(
        sent(&fake),
        [
            Command::Open(first, surface(&browser, first)),
            Command::Open(second, surface(&browser, second)),
            view(first, resize(100, 50, 1.0)),
            view(first, ViewCommand::Load("https://example.com".into())),
            view(first, ViewCommand::Focus(true)),
            // A background tab lays out once, to load its first page, then hides.
            view(second, ViewCommand::Visible(false)),
            view(second, resize(100, 50, 1.0)),
            view(second, ViewCommand::Load("https://example.org".into())),
        ]
    );

    // A window resize reaches only the tab on show.
    browser.set_viewport([120.0, 50.0]);
    browser.poll();
    assert_eq!(sent(&fake), [view(first, resize(120, 50, 1.0))]);

    browser.activate_tab(second);
    browser.poll();
    assert_eq!(
        sent(&fake),
        [
            view(first, ViewCommand::Focus(false)),
            view(first, ViewCommand::Visible(false)),
            view(second, ViewCommand::Visible(true)),
            view(second, resize(120, 50, 1.0)),
            view(second, ViewCommand::Focus(true)),
        ]
    );
}

#[test]
fn only_the_active_tabs_frames_mean_animation() {
    let (mut browser, _fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    assert!(!browser.poll().animating);
    surface(&browser, second).fake_frame();
    assert!(!browser.poll().animating);
    surface(&browser, first).fake_frame();
    assert!(browser.poll().animating);
    assert!(!browser.poll().animating);
}

#[test]
fn restored_background_tabs_load_only_when_selected() {
    let (mut browser, fake) = connected();
    let first = browser
        .restore_tab("https://one.example/", "One", true)
        .unwrap();
    let second = browser
        .restore_tab("https://two.example/", "Two", false)
        .unwrap();
    browser.activate_tab(second);
    lay_out(&mut browser, [100.0, 50.0], 1.0, false);

    browser.poll();
    let commands = sent(&fake);
    assert!(commands.contains(&Command::Open(first, surface(&browser, first))));
    assert!(commands.contains(&Command::Open(second, surface(&browser, second))));
    assert!(!commands.iter().any(|command| {
        matches!(command, Command::View(id, ViewCommand::Load(_)) if *id == first)
    }));
    assert!(commands.contains(&view(
        second,
        ViewCommand::Load("https://two.example/".into())
    )));

    browser.activate_tab(first);
    browser.poll();
    assert!(sent(&fake).contains(&view(
        first,
        ViewCommand::Load("https://one.example/".into())
    )));
}

#[test]
fn split_views_keep_their_own_sizes_input_and_frames() {
    let (mut browser, fake) = ready();
    browser.set_viewport([1008.0, 500.0]);
    browser.set_viewport_focus(true);
    let left = browser.open_tab("one.example").unwrap();
    let right = browser
        .restore_tab("https://two.example", "Two", true)
        .unwrap();
    let background = browser
        .restore_tab("https://three.example", "Three", true)
        .unwrap();
    browser.poll();
    sent(&fake);

    browser.place_split(right, 1);
    browser.resize_split(400.0);
    browser.poll();
    let commands = sent(&fake);
    assert!(commands.contains(&view(left, resize(400, 500, 1.0))));
    assert!(commands.contains(&view(right, resize(600, 500, 1.0))));
    assert!(commands.contains(&view(right, ViewCommand::Visible(true))));
    assert!(commands.contains(&view(
        right,
        ViewCommand::Load("https://two.example".into())
    )));
    assert!(!commands.iter().any(|command| matches!(
        command,
        Command::Open(..) | Command::Close(_) | Command::View(_, ViewCommand::Visible(false))
    )));
    let unfocus = commands
        .iter()
        .position(|command| *command == view(left, ViewCommand::Focus(false)));
    let focus = commands
        .iter()
        .position(|command| *command == view(right, ViewCommand::Focus(true)));
    assert!(matches!((unfocus, focus), (Some(old), Some(new)) if old < new));

    browser.hover_page(left, MouseKind::Move, [3.0, 5.0], Mods::default());
    assert!(browser.key(
        &KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default()
        },
        true,
        false,
        None
    ));
    assert!(matches!(sent(&fake).as_slice(), [
            Command::View(mouse, ViewCommand::Mouse(_)),
            Command::View(key, ViewCommand::Key(_))
        ] if *mouse == left && *key == right));

    surface(&browser, left).fake_frame();
    assert!(browser.poll().animating);
    surface(&browser, background).fake_frame();
    assert!(!browser.poll().animating);
}

#[test]
fn the_first_page_waits_for_its_viewport_and_colour_scheme() {
    let (mut browser, fake) = connected();
    let tab = browser.open_tab("about:blank").unwrap();
    browser.poll();
    assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);

    lay_out(&mut browser, [100.0, 50.0], 2.0, true);
    browser.poll();
    assert_eq!(
        sent(&fake),
        [
            // Building sends the scheme at once, ahead of the poll's commands.
            Command::ColorScheme(true),
            view(tab, resize(200, 100, 2.0)),
            view(tab, ViewCommand::Load("about:blank".into())),
        ]
    );
    browser.poll();
    assert_eq!(sent(&fake), []);
}

#[test]
fn thumbnails_follow_stable_pane_ids_and_reject_late_navigation_frames() {
    use crate::engine::{TabUpdate, Thumbnail};

    let (mut browser, fake) = ready();
    let left = browser.open_tab("one.example").unwrap();
    let right = browser.open_tab("two.example").unwrap();
    browser.place_split(right, 1);
    browser.poll();
    sent(&fake);

    let left_pixels = Arc::new(rune::Pixels::new());
    let right_pixels = Arc::new(rune::Pixels::new());
    let thumbnail = |browser: &Browser, id, pixels| {
        let source = surface(browser, id).preview_source;
        Thumbnail {
            revision: source.revision(),
            source,
            pixels: Arc::downgrade(pixels),
        }
    };

    let left_old = thumbnail(&browser, left, &left_pixels);
    fake.updates
        .send(Update::Thumbnail(
            left,
            thumbnail(&browser, left, &left_pixels),
        ))
        .unwrap();
    fake.updates
        .send(Update::Thumbnail(
            right,
            thumbnail(&browser, right, &right_pixels),
        ))
        .unwrap();
    browser.poll();
    assert!(Arc::ptr_eq(&browser.thumbnail(left).unwrap(), &left_pixels));
    assert!(Arc::ptr_eq(
        &browser.thumbnail(right).unwrap(),
        &right_pixels
    ));
    assert_eq!(browser.focused_side(), 1);
    assert!(sent(&fake).is_empty());

    browser.navigate(left, "three.example").unwrap();
    assert!(browser.thumbnail(left).is_none());
    fake.updates
        .send(Update::Thumbnail(left, left_old))
        .unwrap();
    browser.poll();
    assert!(browser.thumbnail(left).is_none());
    assert!(browser.thumbnail(right).is_some());

    fake.updates
        .send(Update::Thumbnail(
            left,
            thumbnail(&browser, left, &left_pixels),
        ))
        .unwrap();
    browser.poll();
    assert!(browser.thumbnail(left).is_some());
    fake.updates
        .send(Update::Tab(
            left,
            TabUpdate::NavigationCommitted("https://four.example".into()),
        ))
        .unwrap();
    browser.poll();
    assert!(browser.thumbnail(left).is_none());
    browser.close_tab(right);
    assert!(browser.thumbnail(right).is_none());
}
