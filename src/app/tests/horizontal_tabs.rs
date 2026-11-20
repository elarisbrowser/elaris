// SPDX-License-Identifier: GPL-3.0-only

use rune::{Env, Mods, Runtime};

use super::*;
use crate::settings::TabLayout;
use crate::ui::{horizontal_tabs, theme};

fn runtime(state: AppState, width: f32) -> Runtime<AppState> {
    let mut runtime = Runtime::new(state, crate::ui::view, (width, 640.0), Env::new(false, 1.0));
    runtime.poll = Some(crate::app::poll);
    runtime.frame(0.0);
    runtime.frame(1.0);
    runtime
}

fn rect(runtime: &Runtime<AppState>, key: &str) -> [f32; 4] {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing {key}"))
        .rect
}

fn center([x, y, width, height]: [f32; 4]) -> [f32; 2] {
    [x + width / 2.0, y + height / 2.0]
}

fn start_drag(runtime: &mut Runtime<AppState>, id: TabId) -> [f32; 2] {
    let rect = rect(runtime, &horizontal_tabs::key(id));
    let x = rect[0] + theme::ROW_PADDING + 2.0;
    let y = center(rect)[1];
    runtime.pointer_move(x, y);
    runtime.pointer_down(x, y);
    assert_eq!(runtime.take_titlebar_action(), None);
    runtime.frame(0.0);
    [x, y]
}

fn release(runtime: &mut Runtime<AppState>, [x, y]: [f32; 2]) {
    runtime.pointer_up(x, y);
    settle(runtime);
}

fn settle(runtime: &mut Runtime<AppState>) {
    for _ in 0..35 {
        runtime.frame(1.0 / 60.0);
    }
}

#[test]
fn layout_switch_preserves_split_tabs_and_window_state() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    state.place_split_tab(second, 1);
    let split = state.browser.visible_tabs();
    let mut other = runtime(AppState::new(Some("other.example")), 800.0);
    let mut window = runtime(state, 960.0);
    assert!(rect(&window, "page-layout")[0] > 0.0);

    window.update(|state, _| state.set_tab_layout(TabLayout::Horizontal));
    window.frame(1.0);
    assert_eq!(window.state.browser.visible_tabs(), split);
    assert!(window.state.browser.tab(first).is_some());
    let page = rect(&window, "page-layout");
    let inset = theme::VIEWPORT_INSET;
    assert_eq!(
        page,
        [
            inset,
            theme::HORIZONTAL_CHROME_HEIGHT + inset,
            960.0 - 2.0 * inset,
            640.0 - theme::HORIZONTAL_CHROME_HEIGHT - 2.0 * inset
        ]
    );
    window.frame(0.0);
    let right_pane = rect(&window, &format!("web-pane-{}", second.raw()));
    assert_eq!(
        crate::ui::viewport::page_origin(&window.state),
        [right_pane[0], right_pane[1]]
    );
    let anchor = rect(&window, "site-controls");
    let nav = rect(&window, "horizontal-navigation");
    let tab = rect(&window, &horizontal_tabs::key(first));
    let bar = rect(&window, "horizontal-address-bar");
    assert!((0.0..=4.0).contains(&(bar[1] - (tab[1] + tab[3]))));
    assert!(anchor[1] > nav[1] && anchor[1] + anchor[3] < page[1]);
    assert!(rect(&other, "page-layout")[0] > 0.0);
    other.update(|state, _| {
        state.apply_config_load(
            crate::config::parse("[appearance]\ntab_layout = 'horizontal'").unwrap(),
        );
    });
    other.frame(1.0);
    assert_eq!(rect(&other, "page-layout")[2], 800.0 - 2.0 * inset);
    assert!(active_url(&other.state).contains("other.example"));

    window.update(|state, _| state.set_tab_layout(TabLayout::Vertical));
    window.frame(1.0);
    assert_eq!(window.state.browser.visible_tabs(), split);
    assert!(rect(&window, "page-layout")[0] > 0.0);
}

#[test]
fn pointer_reorders_tabs_and_moves_them_between_sections_and_split() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    let fourth = state.browser.open_tab("four.example").unwrap();
    state
        .browser
        .move_tab_to(first, TabSection::Favourite, 0)
        .unwrap();
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);

    let favourite = rect(&window, &horizontal_tabs::key(first));
    let second_rect = rect(&window, &horizontal_tabs::key(second));
    let regular = rect(&window, &horizontal_tabs::key(third));
    assert!(favourite[0] < second_rect[0] && second_rect[0] < regular[0]);
    assert_eq!(favourite[2], theme::HORIZONTAL_COMPACT_TAB);
    assert_eq!(second_rect[2], regular[2]);
    let start = start_drag(&mut window, third);
    let fourth_rect = rect(&window, &horizontal_tabs::key(fourth));
    let end = [fourth_rect[0] + fourth_rect[2] * 0.75, start[1]];
    window.pointer_move(end[0], end[1]);
    window.frame(1.0 / 60.0);
    assert!(
        window.state.tab_drag.is_dragging(),
        "start {start:?}, end {end:?}, tab {regular:?}, strip {:?}",
        rect(&window, horizontal_tabs::SCROLL)
    );
    let live = rect(&window, "tab-drag-visual");
    assert!((live[0] - (regular[0] + end[0] - start[0])).abs() < 0.1);
    release(&mut window, end);
    assert_eq!(
        window
            .state
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .collect::<Vec<_>>(),
        [first, second, fourth, third]
    );

    start_drag(&mut window, fourth);
    let end = center(favourite);
    window.pointer_move(end[0], end[1]);
    window.frame(1.0 / 60.0);
    release(&mut window, end);
    assert_eq!(
        window.state.browser.tab(fourth).unwrap().section(),
        TabSection::Favourite
    );

    start_drag(&mut window, third);
    let end = [820.0, 310.0];
    window.pointer_move(end[0], end[1]);
    window.frame(1.0 / 60.0);
    assert!(window.state.split_preview.is_some());
    window.pointer_up(end[0], end[1]);
    window.frame(1.0 / 60.0);
    assert!(window.state.tab_drag.visual().is_none());
    settle(&mut window);
    assert!(window.state.browser.split.is_some());
    let panes = window.state.browser.visible_tabs();
    assert_eq!(panes[1], Some(third));

    start_drag(&mut window, third);
    let left = [120.0, 310.0];
    window.pointer_move(left[0], left[1]);
    window.frame(1.0 / 60.0);
    release(&mut window, left);
    assert_eq!(window.state.browser.visible_tabs(), [panes[1], panes[0]]);

    start_drag(&mut window, second);
    window.pointer_move(end[0], end[1]);
    window.frame(1.0 / 60.0);
    release(&mut window, end);
    assert_eq!(
        window.state.browser.visible_tabs(),
        [Some(third), Some(second)]
    );
}

#[test]
fn overflow_reveals_keyboard_selected_tab_and_keeps_new_tab_available() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let mut last = first;
    for index in 0..14 {
        last = state
            .browser
            .open_tab(&format!("tab-{index}.example"))
            .unwrap();
    }
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 640.0);
    let scroll = rect(&window, horizontal_tabs::SCROLL);
    let new_tab = rect(&window, "new-tab");
    assert!(new_tab[0] >= scroll[0] + scroll[2]);
    assert!(rect(&window, &horizontal_tabs::key(last))[0] > new_tab[0]);
    assert_eq!(
        rect(&window, &horizontal_tabs::key(first))[2],
        theme::HORIZONTAL_TAB_MIN
    );

    window.update(|state, tx| {
        crate::shortcuts::dispatch(state, crate::shortcuts::Shortcut::SelectLastTab, tx);
    });
    for _ in 0..35 {
        window.frame(1.0 / 60.0);
    }
    let selected = rect(&window, &horizontal_tabs::key(last));
    assert!(selected[0] >= scroll[0] && selected[0] + selected[2] <= scroll[0] + scroll[2] + 1.0);
    window.update(|state, tx| {
        assert!(crate::shortcuts::on_key(
            state,
            &rune::KeyEvent {
                key: rune::Key::Command('l'),
                mods: Mods {
                    cmd: true,
                    ..Mods::default()
                },
            },
            tx,
        ));
    });
    window.frame(0.0);
    assert!(window.state.omnibox.open);
    assert_eq!(window.state.omnibox.input.text, active_url(&window.state));
}

#[test]
fn overflow_menu_lists_clipped_tabs_and_reveals_the_selection() {
    let mut state = AppState::new(Some("one.example"));
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 640.0);
    assert!(window.presented("tab-overflow").is_none());

    window.update(|state, _| {
        for index in 0..8 {
            state
                .browser
                .open_tab(&format!("overflow-{index}.example"))
                .unwrap();
        }
    });
    settle(&mut window);
    let strip = rect(&window, horizontal_tabs::SCROLL);
    let first_clipped = window
        .state
        .browser
        .tabs()
        .iter()
        .find(|tab| {
            let bounds = rect(&window, &horizontal_tabs::key(tab.id()));
            bounds[0] + bounds[2] > strip[0] + strip[2] + 0.5
        })
        .unwrap()
        .id();

    let [x, y] = center(rect(&window, "tab-overflow"));
    window.pointer_down(x, y);
    window.pointer_up(x, y);
    settle(&mut window);
    let [x, y] = center(rect(&window, "tab-overflow-item0"));
    window.pointer_down(x, y);
    window.pointer_up(x, y);
    settle(&mut window);

    assert_eq!(
        window.state.browser.active_tab().map(Tab::id),
        Some(first_clipped)
    );
    assert!(window.presented("tab-overflow-menu").is_none());
    let selected = rect(&window, &horizontal_tabs::key(first_clipped));
    assert!(selected[0] >= strip[0] && selected[0] + selected[2] <= strip[0] + strip[2] + 0.5);

    window.set_viewport((1800.0, 640.0), 1.0);
    settle(&mut window);
    assert!(window.presented("tab-overflow").is_none());
}

#[test]
fn drag_near_strip_edge_scrolls_and_updates_the_insertion_target() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    for index in 0..14 {
        state
            .browser
            .open_tab(&format!("tab-{index}.example"))
            .unwrap();
    }
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 640.0);
    let start = start_drag(&mut window, first);
    let scroll = rect(&window, horizontal_tabs::SCROLL);
    let edge = [scroll[0] + scroll[2] - 5.0, start[1]];
    window.pointer_move(edge[0], edge[1]);
    window.frame(1.0 / 60.0);
    let initial_offset = window.state.tab_drag.scroll_live().get()[0];
    let initial = window
        .state
        .tab_drag
        .incoming_target(window.state.browser.tabs(), edge)
        .unwrap();
    settle(&mut window);
    assert!(window.state.tab_drag.is_dragging());
    assert!(window.state.tab_drag.scroll_live().get()[0] > initial_offset);
    let after = window
        .state
        .tab_drag
        .incoming_target(window.state.browser.tabs(), edge)
        .unwrap();
    assert!(after.index > initial.index);
    release(&mut window, edge);
    assert_ne!(window.state.browser.tabs()[0].id(), first);
}

#[test]
fn url_bar_edits_in_place_and_anchors_results_below_the_toolbar() {
    let mut state = AppState::new(Some("one.example"));
    let id = state.browser.active_tab().unwrap().id();
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let bar = rect(&window, "horizontal-address-bar");
    let launcher = rect(&window, "omnibox-launcher");
    let site = rect(&window, "site-controls");
    assert!(site[0] + site[2] <= launcher[0]);
    let [x, y] = center(launcher);
    window.pointer_move(x, y);
    window.pointer_down(x, y);
    window.frame(0.05);
    assert_eq!(rect(&window, "omnibox-launcher"), launcher);
    assert_eq!(rect(&window, "horizontal-address-bar"), bar);
    window.pointer_up(x, y);
    window.frame(1.0 / 60.0);
    assert!(window.state.omnibox.open);
    assert_eq!(window.state.omnibox.input.text, "https://one.example");
    window.update(|_, tx| assert_eq!(tx.focused(), Some("omnibox-input")));

    window.key(rune::KeyEvent {
        key: rune::Key::Text("example.org".into()),
        mods: Mods::default(),
    });
    window.frame(1.0 / 60.0);
    settle(&mut window);
    assert_eq!(window.state.omnibox.input.text, "example.org");
    let results = rect(&window, "address-suggestions");
    assert_eq!([results[0], results[2]], [bar[0], bar[2]]);
    assert!(results[1] >= bar[1] + bar[3]);
    let first_result = rect(&window, "omnibox-result-0");
    assert_eq!(first_result[0] - results[0], first_result[1] - results[1]);
    assert!(window.presented("omnibox-panel").is_none());

    assert!(window.presented("site-controls").is_none());
    let engine = rect(&window, "search-engine-button");
    let editor = rect(&window, "omnibox-input");
    assert!(engine[0] + engine[2] <= editor[0]);

    let [x, y] = center(engine);
    window.pointer_move(x, y);
    window.pointer_down(x, y);
    window.pointer_up(x, y);
    settle(&mut window);
    assert!(window.state.omnibox.search_engine_menu_open);

    let [x, y] = center(rect(&window, "search-engine-menu-item1"));
    window.pointer_move(x, y);
    window.pointer_down(x, y);
    window.pointer_up(x, y);
    settle(&mut window);
    assert_eq!(
        window.state.settings.search_engine,
        crate::address::SearchEngine::Google
    );
    assert!(!window.state.omnibox.search_engine_menu_open);
    assert_eq!(window.state.omnibox.input.text, "example.org");
    window.update(|_, tx| assert_eq!(tx.focused(), Some("omnibox-input")));

    window.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: Mods::default(),
    });
    window.frame(1.0 / 60.0);
    assert!(!window.state.omnibox.open);
    assert_eq!(window.state.browser.active_tab().unwrap().id(), id);
    assert_eq!(active_url(&window.state), "https://example.org");
    assert!(window.presented("site-controls").is_some());
}

#[test]
fn horizontal_drag_area_keeps_primary_drag_and_secondary_tab_menu() {
    let mut state = AppState::new(Some("one.example"));
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let strip = rect(&window, horizontal_tabs::SCROLL);
    let empty_inside = [strip[0] + strip[2] - 8.0, center(strip)[1]];
    window.pointer_down(empty_inside[0], empty_inside[1]);
    assert_eq!(window.take_titlebar_action(), None);
    window.pointer_up(empty_inside[0], empty_inside[1]);
    window.frame(0.0);

    let new_tab = rect(&window, "new-tab");
    assert!((new_tab[0] - (strip[0] + strip[2]) - theme::NAV_GAP).abs() < 0.5);
    let drag_area = rect(&window, "horizontal-window-drag");
    assert!(drag_area[2] > strip[2]);
    let [x, y] = center(drag_area);
    window.pointer_down(x, y);
    assert!(window.take_titlebar_action().is_some());
    window.pointer_up(x, y);
    window.frame(0.0);

    window.secondary_down(x, y);
    window.secondary_up(x, y);
    window.frame(0.0);
    assert!(matches!(window.state.context_menu(), Some((None, _))));
    assert_eq!(window.take_titlebar_action(), None);
}
