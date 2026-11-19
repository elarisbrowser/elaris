// SPDX-License-Identifier: GPL-3.0-only

use rune::{Env, KeyEvent, Mods, Runtime};

use super::*;
use crate::engine::{Engine, FakeEngine, Update};
use crate::tab::TabId;
use crate::ui::viewport;

const TARGET: &str = "https://preview.example/";

fn window(reduced: bool) -> (Runtime<AppState>, FakeEngine, TabId) {
    let mut state = AppState::new(Some("source.example"));
    let source = state.browser.active_tab().unwrap().id();
    let (engine, fake) = Engine::fake();
    state.browser.connect(engine);
    let mut window = Runtime::new(
        state,
        crate::ui::view,
        (1080.0, 720.0),
        Env::new(false, 1.0),
    );
    window.on_key = Some(crate::shortcuts::on_key);
    window.set_system_reduced_motion(reduced);
    window.frame(0.0);
    window.frame(0.016);
    (window, fake, source)
}

fn open(window: &mut Runtime<AppState>, fake: &FakeEngine, source: TabId, time: f64) -> String {
    fake.updates
        .send(Update::Peek(source, TARGET.into(), 1))
        .unwrap();
    window.update_if(|state, tx| crate::app::poll(state, time, tx));
    window.frame(time);
    let id = window.state.browser.peek.as_ref().unwrap().page.id();
    format!("peek-panel-{}", id.raw())
}

#[test]
fn panel_tracks_content_and_escape_restores_page_focus() {
    let (mut window, fake, source) = window(false);
    let key = open(&mut window, &fake, source, 0.01);
    let opening = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    assert!(opening.rect[2] < 0.05 * opening.target[2]);
    assert!(opening.rect[3] < 0.05 * opening.target[3]);
    window.update_if(|state, tx| crate::app::poll(state, 0.09, tx));
    window.frame(0.016);
    let expanding = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    window.update_if(|state, tx| crate::app::poll(state, 0.36, tx));
    window.frame(0.016);
    let rebound = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
    window.frame(1.0);
    let settled = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    let [x, y, w, h] = window.state.split_rect;
    assert!((settled.rect[0] + settled.rect[2] / 2.0 - (x + w / 2.0)).abs() < 0.05);
    assert!(expanding.rect[2] > opening.rect[2] && expanding.rect[2] < settled.rect[2]);
    assert!(expanding.rect[3] > opening.rect[3] && expanding.rect[3] < settled.rect[3]);
    assert!(rebound.rect[2] > settled.rect[2]);
    assert!(settled.rect[1] < y);
    assert!(settled.rect[2] < w);
    assert!(settled.rect[3] > h);
    assert!(
        window
            .elements()
            .iter()
            .any(|el| el.focused && el.key.as_deref() == Some(PAGE))
    );
    let origin = [80.0, 120.0];
    window.state.browser.peek.as_mut().unwrap().origin = origin;
    window.key(KeyEvent {
        key: rune::Key::Escape,
        mods: Mods::default(),
    });
    window.frame(0.01);
    assert!(window.state.browser.peek.is_none());
    let at_dismissal = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    assert!((at_dismissal.rect[2] - settled.rect[2]).abs() < 0.05);
    window.update_if(|state, tx| crate::app::poll(state, 2.0, tx));
    window.update_if(|state, tx| crate::app::poll(state, 2.18, tx));
    window.frame(0.016);
    let shrinking = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(key.as_str()))
        .unwrap();
    assert!(shrinking.rect[2] < settled.rect[2] / 3.0);
    let [sx, sy, sw, sh] = shrinking.rect;
    assert!((sx + sw / 2.0 - (x + origin[0])).abs() < 80.0);
    assert!((sy + sh / 2.0 - (y + origin[1])).abs() < 60.0);
    window.update_if(|state, tx| crate::app::poll(state, 2.25, tx));
    window.frame(0.016);
    assert!(
        !window
            .elements()
            .iter()
            .any(|el| el.key.as_deref() == Some(key.as_str()))
    );
    assert!(
        window.elements().iter().any(|el| el.focused
            && el.key.as_deref() == Some(viewport::focus_key(&window.state.browser)))
    );
}

#[test]
fn reduced_motion_keeps_the_panel_in_place_and_outside_click_dismisses() {
    let (mut window, fake, source) = window(true);
    let key = open(&mut window, &fake, source, 0.01);
    let panel = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(&key))
        .unwrap();
    assert!(
        panel
            .rect
            .into_iter()
            .zip(panel.target)
            .all(|(drawn, target)| (drawn - target).abs() < 0.05)
    );
    assert!(panel.opacity < 1.0);
    window.pointer_down(24.0, 160.0);
    window.pointer_up(24.0, 160.0);
    window.frame(0.05);
    assert!(window.state.browser.peek.is_none());
    assert_eq!(window.state.browser.tabs().len(), 1);
    assert_eq!(window.state.browser.active_tab().unwrap().id(), source);
}

#[test]
fn open_in_tab_expands_into_the_viewport_before_transferring_the_live_page() {
    let (mut window, fake, source) = window(false);
    let key = open(&mut window, &fake, source, 0.01);
    window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
    window.frame(1.0);
    let get_rect = |window: &Runtime<AppState>, key: &str| {
        window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key))
            .unwrap()
            .rect
    };
    let start = get_rect(&window, &key);
    let id = window.state.browser.peek.as_ref().unwrap().page.id();
    let [x, y, w, h] = get_rect(&window, "peek-open-tab");
    let [x, y] = [x + w / 2.0, y + h / 2.0];
    window.pointer_down(x, y);
    window.pointer_up(x, y);
    window.frame(0.016);
    assert_eq!(window.state.browser.active_tab().unwrap().id(), source);
    assert!((get_rect(&window, &key)[2] - start[2]).abs() < 0.05);
    window.update_if(|state, tx| crate::app::poll(state, 2.0, tx));
    window.update_if(|state, tx| crate::app::poll(state, 2.12, tx));
    window.frame(0.016);
    let midway = get_rect(&window, &key);
    let area = window.state.split_rect;
    assert!(midway[0] < start[0] && midway[0] > area[0]);
    assert!(midway[2] > start[2] && midway[2] < area[2]);
    assert!(midway[1] > start[1] && midway[1] < area[1]);
    assert!(midway[3] < start[3] && midway[3] > area[3]);

    window.update_if(|state, tx| crate::app::poll(state, 2.25, tx));
    window.frame(0.016);
    assert!(window.state.browser.peek.is_none());
    assert_eq!(window.state.browser.active_tab().unwrap().id(), id);
    let viewport = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(viewport::focus_key(&window.state.browser)))
        .unwrap();
    assert!(viewport.focused);
    assert_eq!(viewport.rect, area);
}

#[test]
fn detached_button_promotes_to_the_right_split_pane() {
    let (mut window, fake, source) = window(false);
    let panel_key = open(&mut window, &fake, source, 0.01);
    window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
    window.frame(1.0);
    let [x, y, w, h] = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some("peek-open-split"))
        .unwrap()
        .rect;
    let panel = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(panel_key.as_str()))
        .unwrap();
    assert!(x >= panel.rect[0] + panel.rect[2] + peek::ACTION_GAP - 0.05);
    let close = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some("peek-close"))
        .unwrap();
    assert!((close.rect[1] - panel.rect[1]).abs() < 0.05);
    let id = window.state.browser.peek.as_ref().unwrap().page.id();
    let [px, py] = [x + w / 2.0, y + h / 2.0];
    window.pointer_down(px, py);
    window.pointer_up(px, py);
    window.frame(0.01);
    assert_eq!(window.state.browser.visible_tabs(), [Some(source), None]);
    window.update_if(|state, tx| crate::app::poll(state, 2.0, tx));
    window.update_if(|state, tx| crate::app::poll(state, 2.12, tx));
    window.frame(0.016);
    let moving = window
        .elements()
        .into_iter()
        .find(|el| el.key.as_deref() == Some(panel_key.as_str()))
        .unwrap()
        .rect;
    assert!(moving[0] > panel.rect[0] && moving[2] < panel.rect[2]);
    window.update_if(|state, tx| crate::app::poll(state, 2.25, tx));
    window.frame(0.016);
    assert!(window.state.browser.peek.is_none());
    assert_eq!(
        window.state.browser.visible_tabs(),
        [Some(source), Some(id)]
    );
}
