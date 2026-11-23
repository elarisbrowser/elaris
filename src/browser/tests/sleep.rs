// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use std::time::{Duration, Instant};

fn answer(browser: &mut Browser, fake: &crate::engine::FakeEngine, id: TabId, kept: bool) {
    fake.updates
        .send(crate::engine::Update::Tab(
            id,
            crate::engine::TabUpdate::Discarded {
                kept,
                blockers: if kept { 0 } else { 1 << 7 },
            },
        ))
        .unwrap();
    browser.poll();
}

#[test]
fn a_sleeping_tab_closes_once_ladybird_keeps_its_state_and_restores_it_when_shown() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    sent(&fake);

    assert!(browser.sleep_tab(second, true));
    assert!(!browser.sleep_tab(second, true));
    assert_eq!(
        sent(&fake),
        [view(
            second,
            ViewCommand::Discard {
                memory_pressure: true
            }
        )]
    );
    assert!(!browser.tab(second).unwrap().asleep);

    answer(&mut browser, &fake, second, true);
    assert!(browser.tab(second).unwrap().asleep);
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
        ViewCommand::Restore("https://example.org".into())
    )));
    assert!(!browser.tab(second).unwrap().asleep);
}

#[test]
fn a_tab_ladybird_keeps_loaded_stays_awake_and_counts_as_just_seen() {
    let (mut browser, fake) = ready();
    browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    let later = Instant::now() + Duration::from_secs(600);
    browser.sleep_tab(second, false);
    assert!(browser.sleep_candidates(later).is_empty());
    sent(&fake);

    answer(&mut browser, &fake, second, false);
    assert!(!browser.tab(second).unwrap().asleep);
    assert!(sent(&fake).is_empty());
    assert!(
        browser
            .sleep_candidates(Instant::now())
            .iter()
            .all(|c| c.idle < Duration::from_secs(1))
    );
}

#[test]
fn a_sleeping_tab_that_closes_forgets_its_kept_state() {
    let (mut browser, fake) = ready();
    browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    browser.sleep_tab(second, false);
    answer(&mut browser, &fake, second, true);
    sent(&fake);
    browser.close_tab(second);
    assert_eq!(sent(&fake), [Command::ForgetSleep(second)]);
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

    browser.sleep_tab(quiet, false);
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
