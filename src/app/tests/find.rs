// SPDX-License-Identifier: GPL-3.0-only

use super::*;

#[test]
fn find_bar_tracks_a_narrow_page_and_uses_reversible_motion() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("example.com"));
    state.sidebar.collapsed = true;
    let mut runtime = Runtime::new(state, crate::ui::view, (420.0, 360.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime.update(|state, tx| state.open_find(tx));
    runtime.frame(1.0 / 120.0);

    let elements = runtime.elements();
    let page = elements
        .iter()
        .find(|element| element.key.as_deref() == Some(crate::ui::VIEWPORT))
        .expect("page viewport is rendered");
    let bar = elements
        .iter()
        .find(|element| element.key.as_deref() == Some("find-bar"))
        .expect("find bar is rendered");
    assert!(bar.rect[0] >= page.rect[0]);
    assert!(bar.rect[0] + bar.rect[2] <= page.rect[0] + page.rect[2]);
    assert!(bar.opacity < 1.0, "opening should fade in");
    #[cfg(feature = "ladybird-bridge")]
    assert!(elements.iter().any(|element| {
        element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
    }));

    runtime.update(|state, tx| state.close_find(tx));
    runtime.frame(1.0 / 60.0);
    let closing = runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("find-bar"))
        .expect("closing bar remains for its exit transition");
    assert!(closing.exiting);
    assert!(closing.opacity < 1.0, "closing should fade out");
    runtime.frame(1.0);
    assert!(
        !runtime
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("find-bar") && !element.exiting)
    );
}
#[test]
fn find_bar_reduced_motion_lands_and_closes_without_transition() {
    use rune::{Env, Runtime};

    let mut runtime = Runtime::new(
        AppState::new(Some("example.com")),
        crate::ui::view,
        (960.0, 640.0),
        Env::new(false, 1.0),
    );
    runtime.set_system_reduced_motion(true);
    runtime.frame(0.0);
    runtime.update(|state, tx| state.open_find(tx));
    runtime.frame(1.0 / 60.0);
    let bar = runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("find-bar"))
        .expect("find bar is rendered");
    assert_eq!(bar.rect, bar.target);
    assert_eq!(bar.opacity, 1.0);

    runtime.update(|state, tx| state.close_find(tx));
    runtime.frame(1.0);
    assert!(
        !runtime
            .elements()
            .iter()
            .any(|element| element.key.as_deref() == Some("find-bar") && !element.exiting)
    );
}
#[test]
fn find_queries_follow_tabs_and_a_closed_tab_cannot_seed_another() {
    let mut state = AppState::new(Some("first.example"));
    let first = state.browser.active_tab().unwrap().id();
    let (engine, _fake) = crate::engine::Engine::fake();
    state.browser.connect(engine);
    let second = state.browser.open_tab("second.example").unwrap();
    state.open_find(&mut Tx::default());
    state.find_input.replace_text("alpha");
    state.find_input_changed();
    assert_eq!(state.browser.tab(first).unwrap().find.query, "alpha");

    state.activate(second);
    assert!(state.find_input.text.is_empty());
    state.find_input.replace_text("beta");
    state.find_input_changed();
    assert_eq!(state.browser.tab(second).unwrap().find.query, "beta");

    state.activate(first);
    assert_eq!(state.find_input.text, "alpha");
    state.close(first);
    assert_eq!(state.browser.active_tab().unwrap().id(), second);
    assert_eq!(state.find_input.text, "beta");
    assert_eq!(state.browser.tab(second).unwrap().find.query, "beta");
}
#[cfg(feature = "ladybird-bridge")]
#[test]
fn find_shortcuts_buttons_and_escape_keep_input_focus_and_route_to_the_active_tab() {
    use crate::engine::{Command, ViewCommand};
    use rune::{Key, KeyEvent, Mods, Runtime};

    let state = AppState::new(Some("example.com"));
    let tab = state.browser.active_tab().unwrap().id();
    let (engine, fake) = crate::engine::Engine::fake();
    let mut state = state;
    state.browser.connect(engine);
    let mut runtime = Runtime::new(
        state,
        crate::ui::view,
        (960.0, 640.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);

    runtime.update(|state, tx| {
        crate::shortcuts::on_key(
            state,
            &KeyEvent {
                key: Key::Command('f'),
                mods: Mods {
                    cmd: true,
                    ..Mods::default()
                },
            },
            tx,
        );
    });
    runtime.frame(0.3);
    assert!(runtime.state.find_open);
    assert!(runtime.elements().iter().any(|element| {
        element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
    }));
    let _ = fake.commands.try_iter().collect::<Vec<_>>();

    runtime.key(KeyEvent {
        key: Key::Text("match".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(runtime.state.find_input.text, "match");
    let commands = fake.commands.try_iter().collect::<Vec<_>>();
    assert!(commands.iter().any(|command| matches!(command,
        Command::View(id, ViewCommand::Find { query, .. }) if *id == tab && query == "match"
    )));

    for (key, shift, expected_previous) in [
        (Key::Enter, false, false),
        (Key::Enter, true, true),
        (Key::Command('g'), false, false),
        (Key::Command('G'), true, true),
    ] {
        let command = matches!(&key, Key::Command(_));
        runtime.key(KeyEvent {
            key,
            mods: Mods {
                cmd: command,
                shift,
                ..Mods::default()
            },
        });
        runtime.frame(0.0);
        let commands = fake.commands.try_iter().collect::<Vec<_>>();
        let has_next = commands.iter().any(|command| {
            matches!(command,
                Command::View(id, ViewCommand::FindNext { .. }) if *id == tab
            )
        });
        let has_previous = commands.iter().any(|command| {
            matches!(command,
                Command::View(id, ViewCommand::FindPrevious { .. }) if *id == tab
            )
        });
        assert_eq!(has_next, !expected_previous);
        assert_eq!(has_previous, expected_previous);
    }

    for (button_key, expected_previous) in
        [("find-previous-button", true), ("find-next-button", false)]
    {
        let button = runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(button_key))
            .expect("find navigation button is rendered");
        let point = [
            button.rect[0] + button.rect[2] / 2.0,
            button.rect[1] + button.rect[3] / 2.0,
        ];
        runtime.pointer_down(point[0], point[1]);
        runtime.frame(0.0);
        runtime.pointer_up(point[0], point[1]);
        runtime.frame(0.0);
        let focused = runtime
            .elements()
            .into_iter()
            .filter(|element| element.focused)
            .map(|element| (element.key, element.label))
            .collect::<Vec<_>>();
        assert!(
            runtime.elements().iter().any(|element| {
                element.focused && element.key.as_deref() == Some(crate::ui::FIND_INPUT)
            }),
            "focused elements after {button_key}: {focused:?}"
        );
        let commands = fake.commands.try_iter().collect::<Vec<_>>();
        let has_next = commands.iter().any(|command| {
            matches!(command,
                Command::View(id, ViewCommand::FindNext { .. }) if *id == tab
            )
        });
        let has_previous = commands.iter().any(|command| {
            matches!(command,
                Command::View(id, ViewCommand::FindPrevious { .. }) if *id == tab
            )
        });
        assert_eq!(has_next, !expected_previous);
        assert_eq!(has_previous, expected_previous);
    }

    runtime.state.find_input.anchor = 2;
    runtime.state.find_input.focus = 2;
    runtime.key(KeyEvent {
        key: Key::Command('f'),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    });
    runtime.frame(0.0);
    assert_eq!(runtime.state.find_input.anchor, 0);
    assert_eq!(runtime.state.find_input.focus, "match".len());

    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(!runtime.state.find_open);
    assert!(
        runtime.elements().iter().any(|element| {
            element.focused && element.key.as_deref() == Some(crate::ui::VIEWPORT)
        })
    );
    assert!(fake.commands.try_iter().any(|command| matches!(command,
        Command::View(id, ViewCommand::FindEnd) if id == tab
    )));
}
