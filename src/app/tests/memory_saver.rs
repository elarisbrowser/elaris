// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{Command, ViewCommand};
use crate::tab::TabSection;
use elaris_memory_saver::AwakeReason;

/// A window with an engine, showing its first tab, with a second in the background.
fn window() -> (AppState, crate::engine::FakeEngine, crate::tab::TabId) {
    let mut state = AppState::new(Some("https://example.com/"));
    let (engine, fake) = crate::engine::Engine::fake();
    state.browser.connect(engine);
    let background = state.browser.open_tab("https://mail.example.org/").unwrap();
    (state, fake, background)
}

#[test]
fn kept_sites_are_added_from_what_names_them_and_keep_their_tabs_awake() {
    let (mut state, _fake, background) = window();
    state
        .memory_saver
        .site_input
        .replace_text("not a site".to_owned());
    state.add_keep_awake_site();
    assert!(state.memory_saver.site_error.is_some());
    assert!(state.settings.keep_awake_sites.is_empty());

    state
        .memory_saver
        .site_input
        .replace_text("https://www.Example.org/inbox".to_owned());
    state.add_keep_awake_site();
    assert_eq!(state.settings.keep_awake_sites, ["example.org"]);
    assert!(state.memory_saver.site_error.is_none());
    assert!(state.memory_saver.site_input.text.is_empty());
    let tab = state.browser.tab(background).unwrap();
    assert!(
        state
            .awake_reasons(tab)
            .contains(&AwakeReason::KeepAwakeSite)
    );

    state.remove_keep_awake_site("example.org");
    assert!(state.settings.keep_awake_sites.is_empty());
    let tab = state.browser.tab(background).unwrap();
    assert!(
        !state
            .awake_reasons(tab)
            .contains(&AwakeReason::KeepAwakeSite)
    );
}

#[test]
fn a_tab_says_why_it_stays_awake_from_elaris_and_ladybird() {
    let (mut state, _fake, background) = window();
    {
        let tab = state.browser.tab_mut(background).unwrap();
        tab.playing_audio = true;
        tab.section = TabSection::Favourite;
        tab.kept_awake = vec![AwakeReason::UnsavedEdits, AwakeReason::PlayingAudio];
    }
    let tab = state.browser.tab(background).unwrap();
    assert_eq!(
        state.awake_reasons(tab),
        [
            AwakeReason::PlayingAudio,
            AwakeReason::Favourite,
            AwakeReason::UnsavedEdits
        ]
    );
    state.browser.tab_mut(background).unwrap().asleep = true;
    let tab = state.browser.tab(background).unwrap();
    assert!(state.awake_reasons(tab).is_empty());
}

#[test]
fn sleeping_a_tab_now_asks_ladybird_as_under_memory_pressure() {
    let (mut state, fake, background) = window();
    fake.commands.try_iter().count();
    state.sleep_now(background);
    assert!(fake.commands.try_iter().any(|command| command
        == Command::View(
            background,
            ViewCommand::Discard {
                memory_pressure: true
            }
        )));
}

#[test]
fn the_memory_a_sleeping_tab_held_counts_as_freed() {
    let (mut state, _fake, background) = window();
    let mut process = elaris_core::processes::Process {
        id: 2,
        pid: 102,
        kind: "WebContent".into(),
        private: false,
        title: None,
        site: "example.org".into(),
        owner: None,
        cpu: 0.0,
        memory: 300,
        spare: false,
        tabs: Vec::new(),
        frames: Vec::new(),
    };
    process.tabs.push(elaris_core::processes::ProcessTab {
        view: background.raw(),
        title: String::new(),
    });
    let (bytes, tabs) = crate::app::memory_saver_freed();
    let polled = crate::browser::Polled {
        processes: Some((background, Ok(vec![process]))),
        slept: vec![background],
        ..Default::default()
    };
    state.note_tab_memory(&polled);
    let (freed_bytes, freed_tabs) = crate::app::memory_saver_freed();
    assert!(freed_bytes >= bytes + 300);
    assert!(freed_tabs > tabs);
}
