// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::time::{Duration, Instant};

#[test]
fn a_sleeping_tab_closes_its_view_and_reloads_its_page_when_shown() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    sent(&fake);

    assert!(browser.sleep_tab(second));
    assert!(!browser.sleep_tab(second));
    assert_eq!(sent(&fake), [Command::Close(second)]);
    // A sleeping tab is told nothing while hidden.
    browser.set_viewport([120.0, 50.0]);
    browser.poll();
    assert_eq!(sent(&fake), [view(first, resize(120, 50, 1.0))]);

    browser.activate_tab(second);
    browser.poll();
    let commands = sent(&fake);
    assert!(commands.contains(&Command::Open(second, surface(&browser, second), false)));
    assert!(commands.contains(&view(second, resize(120, 50, 1.0))));
    assert!(commands.contains(&view(
        second,
        ViewCommand::Load("https://example.org".into())
    )));
    assert!(!browser.tab(second).unwrap().asleep);
}

#[test]
fn only_idle_background_tabs_doing_nothing_may_sleep() {
    let (mut browser, _fake) = ready();
    let active = browser.open_tab("example.com").unwrap();
    let quiet = browser.open_tab("example.org").unwrap();
    let playing = browser.open_tab("example.net").unwrap();
    browser.poll();
    browser.tab_mut(playing).unwrap().playing_audio = true;

    let later = Instant::now() + Duration::from_secs(60);
    let candidates = browser.sleep_candidates(later);
    let ids: Vec<_> = candidates.iter().map(|candidate| candidate.id).collect();
    assert_eq!(ids, [quiet, playing]);
    assert!(!ids.contains(&active));
    assert!(!candidates[0].busy && candidates[1].busy);
    assert!(candidates[0].idle >= Duration::from_secs(60));

    browser.sleep_tab(quiet);
    assert_eq!(browser.sleep_candidates(later).len(), 1);
}

#[test]
fn private_windows_never_sleep_tabs() {
    let (mut browser, _fake) = ready();
    browser.private = true;
    browser.open_tab("example.com").unwrap();
    browser.open_tab("example.org").unwrap();
    browser.poll();
    assert!(browser.sleep_candidates(Instant::now()).is_empty());
}

#[test]
fn a_tab_with_an_open_popup_stays_awake_until_the_popup_closes() {
    let (mut browser, _fake) = ready();
    let opener = browser.open_tab("example.com").unwrap();
    browser.open_tab("example.org").unwrap();
    browser.insert_popup(opener, 1, false);
    browser.poll();
    let busy = |browser: &Browser| {
        browser
            .sleep_candidates(Instant::now())
            .iter()
            .find(|candidate| candidate.id == opener)
            .unwrap()
            .busy
    };
    browser.activate_tab(browser.tabs()[1].id());
    assert!(busy(&browser));
    let popup = browser.tabs().last().unwrap().id();
    browser.close_tab(popup);
    assert!(!busy(&browser));
}
