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

#[test]
fn settings_remains_scrollable_inside_a_multi_pane_grid() {
    let mut state = AppState::new(Some("elaris://settings"));
    let settings = state.browser.active_tab().unwrap().id();
    let tabs = [
        state.browser.open_tab("one.example").unwrap(),
        state.browser.open_tab("two.example").unwrap(),
        state.browser.open_tab("three.example").unwrap(),
    ];
    state.browser.split_view_limit = 4;
    state.browser.place_split(settings, 0);
    state.browser.place_split(tabs[0], 1);
    assert_eq!(state.browser.visible_tabs().len(), 2);
    state.browser.activate_tab(settings);

    let mut runtime = runtime(state, 1000.0);
    assert!(runtime.presented("add-split-pane").is_some());
    let [x, y, width, height] = rect(&runtime, "add-split-pane");
    runtime.pointer_down(x + width / 2.0, y + height / 2.0);
    runtime.pointer_up(x + width / 2.0, y + height / 2.0);
    runtime.frame(0.01);
    assert_eq!(runtime.state.browser.visible_tabs().len(), 3);
    runtime.update(|state, _| state.browser.place_split(tabs[1], 2));
    runtime.frame(0.01);
    let [x, y, width, height] = rect(&runtime, "add-split-pane");
    runtime.pointer_down(x + width / 2.0, y + height / 2.0);
    runtime.pointer_up(x + width / 2.0, y + height / 2.0);
    runtime.frame(0.01);
    assert_eq!(runtime.state.browser.visible_tabs().len(), 4);
    runtime.update(|state, _| state.browser.place_split(tabs[2], 3));
    runtime.frame(0.01);
    let settings_scroll = rect(&runtime, "settings-scroll");
    assert!(rect(&runtime, "split-grid-row-1")[3] > 0.0);
    for tab in [settings, tabs[0], tabs[1], tabs[2]] {
        assert!(rect(&runtime, &format!("web-pane-{}", tab.raw()))[3] > 0.0);
    }
    assert!(
        settings_scroll[3] > 0.0,
        "settings scroll region has no height: {settings_scroll:?}; viewport={:?}, grid={:?}, row={:?}, pane={:?}, page={:?}",
        rect(&runtime, "viewport-container"),
        rect(&runtime, "split-grid"),
        rect(&runtime, "split-grid-row-0"),
        rect(&runtime, &format!("web-pane-{}", settings.raw())),
        rect(&runtime, "page-layout"),
    );
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

#[test]
fn title_aware_widths_wait_for_loading_and_dragging_to_finish() {
    let mut state = AppState::new(Some("one.example"));
    let short = state.browser.active_tab().unwrap().id();
    let long = state.browser.open_tab("two.example").unwrap();
    for (id, title) in [(short, "Mail"), (long, "A considerably longer title")] {
        let tab = state.browser.tab_mut(id).unwrap();
        tab.title = title.into();
        tab.loading = false;
    }
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let width = |runtime: &Runtime<AppState>, id| rect(runtime, &horizontal_tabs::key(id))[2];
    let initial = width(&window, short);
    assert!(initial < width(&window, long));

    window.update(|state, _| {
        let tab = state.browser.tab_mut(short).unwrap();
        tab.loading = true;
        tab.title = "A longer title during navigation".into();
    });
    settle(&mut window);
    assert_eq!(width(&window, short), initial);

    let [x, y] = start_drag(&mut window, long);
    window.pointer_move(x + 10.0, y);
    window.frame(1.0 / 60.0);
    assert!(window.state.tab_drag.is_dragging());
    window.update(|state, _| state.browser.tab_mut(short).unwrap().loading = false);
    settle(&mut window);
    assert_eq!(width(&window, short), initial);
    release(&mut window, [x + 10.0, y]);
    assert!(width(&window, short) > initial);
}

#[test]
fn split_markers_follow_the_displayed_pair() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    state.place_split_tab(second, 1);
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let marker = |id: TabId| format!("split-tab-marker-{}", id.raw());
    assert!(window.presented(&marker(first)).is_some());
    assert!(window.presented(&marker(second)).is_some());
    assert!(window.presented("focused-pane-border").is_none());
    assert!(window.presented(&marker(third)).is_none());

    window.update(|state, _| state.activate(third));
    settle(&mut window);
    assert!(window.presented(&marker(first)).is_none());
    assert!(window.presented(&marker(second)).is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn traffic_light_placeholders_follow_the_native_titlebar_position() {
    let mut window = runtime(AppState::new(Some("one.example")), 960.0);
    window.set_titlebar_inset(
        theme::TRAFFIC_LIGHTS.0 + 2.0 * rune::Chrome::TRAFFIC_PITCH + rune::Chrome::TRAFFIC_LIGHT,
    );
    settle(&mut window);
    for (layout, height) in [
        (TabLayout::Vertical, theme::HEADER_HEIGHT),
        (TabLayout::Horizontal, theme::HORIZONTAL_STRIP_HEIGHT),
        (TabLayout::Vertical, theme::HEADER_HEIGHT),
    ] {
        window.update(|state, _| state.set_tab_layout(layout));
        settle(&mut window);
        let (_, native_y) = theme::native_titlebar(height)
            .traffic_lights_origin()
            .unwrap();
        assert_eq!(rect(&window, "window-controls")[1], native_y);
    }
}

#[test]
fn shared_hover_highlight_slides_between_tabs_and_snaps_with_reduced_motion() {
    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let start = center(rect(&window, &horizontal_tabs::key(first)));
    let end = center(rect(&window, &horizontal_tabs::key(second)));

    window.pointer_move(start[0], start[1]);
    settle(&mut window);
    let from = rect(&window, "horizontal-tabs-hover")[0];
    window.pointer_move(end[0], end[1]);
    window.frame(1.0 / 60.0);
    let in_motion = rect(&window, "horizontal-tabs-hover")[0];
    let to = rect(&window, &horizontal_tabs::key(second))[0];
    assert!(
        (from..to).contains(&in_motion),
        "{from} -> {in_motion} -> {to}"
    );
    settle(&mut window);
    assert_eq!(rect(&window, "horizontal-tabs-hover")[0], to);

    window.set_system_reduced_motion(true);
    window.pointer_move(start[0], start[1]);
    window.frame(0.0);
    assert_eq!(rect(&window, "horizontal-tabs-hover")[0], from);
}

#[test]
fn shared_toolbar_hover_excludes_disabled_controls() {
    let mut state = AppState::new(Some("one.example"));
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let key = "controls-hover-horizontal-navigation";

    let [x, y] = center(rect(&window, "back"));
    window.pointer_move(x, y);
    window.frame(0.0);
    assert!(window.presented(key).is_none());

    let [x, y] = center(rect(&window, "downloads-button"));
    window.pointer_move(x, y);
    settle(&mut window);
    assert!(window.presented(key).is_some());
    window.pointer_move(500.0, 280.0);
    settle(&mut window);
    assert!(window.presented(key).is_none());
}

#[test]
fn omnibox_selection_slides_between_hovered_results() {
    let mut state = AppState::new(Some("one.example"));
    state.browser.open_tab("two.example").unwrap();
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    window.update(|state, _| state.open_omnibox(crate::omnibox::Mode::New));
    settle(&mut window);
    let from = rect(&window, "omnibox-result-highlight")[1];
    let second = rect(&window, "omnibox-result-1");
    let [x, y] = center(second);
    window.pointer_move(x, y);
    window.frame(1.0 / 60.0);
    let moving = rect(&window, "omnibox-result-highlight")[1];
    assert!(
        (from..second[1]).contains(&moving),
        "{from} -> {moving} -> {}",
        second[1]
    );
    settle(&mut window);
    assert_eq!(rect(&window, "omnibox-result-highlight")[1], second[1]);
}

#[test]
fn context_menu_highlight_slides_between_items() {
    let mut state = AppState::new(Some("one.example"));
    let tab = state.browser.active_tab().unwrap().id();
    state.set_tab_layout(TabLayout::Horizontal);
    let mut window = runtime(state, 960.0);
    let [x, y] = center(rect(&window, &horizontal_tabs::key(tab)));
    window.secondary_down(x, y);
    window.secondary_up(x, y);
    settle(&mut window);
    let first = rect(&window, "sidebar-context-menu-item0");
    let second = rect(&window, "sidebar-context-menu-item1");
    let indicator = "context-menu-hover-highlight";

    let [x, y] = center(first);
    window.pointer_move(x, y);
    settle(&mut window);
    assert_eq!(rect(&window, indicator)[1], first[1]);

    let [x, y] = center(second);
    window.pointer_move(x, y);
    window.frame(1.0 / 60.0);
    window.frame(1.0 / 60.0);
    let moving = rect(&window, indicator)[1];
    assert!((first[1]..second[1]).contains(&moving), "{moving}");
    settle(&mut window);
    assert_eq!(rect(&window, indicator)[1], second[1]);

    window.key(rune::KeyEvent {
        key: rune::Key::Up,
        mods: Mods::default(),
    });
    settle(&mut window);
    assert_eq!(rect(&window, indicator)[1], first[1]);
}

#[test]
fn dropping_tabs_at_the_bottom_edge_adds_panes_until_the_limit() {
    let mut state = AppState::new(Some("one.example"));
    state.set_tab_layout(TabLayout::Horizontal);
    state.set_split_view_limit(4);
    let first = state.browser.active_tab().unwrap().id();
    let tabs = [
        "two.example",
        "three.example",
        "four.example",
        "five.example",
    ]
    .map(|url| state.browser.open_tab(url).unwrap());
    state.browser.activate_tab(first);
    state.place_split_tab(tabs[0], 1);
    let mut window = runtime(state, 1000.0);

    for (index, tab) in tabs[1..3].iter().copied().enumerate() {
        start_drag(&mut window, tab);
        let [x, y, width, height] = rect(&window, "page-layout");
        let end = [x + width * 0.4, y + height - 20.0];
        window.pointer_move(end[0], end[1]);
        window.frame(1.0 / 60.0);
        assert_eq!(window.state.split_preview, Some(index + 2));
        assert!(window.presented("split-add-preview").is_some());
        release(&mut window, end);
        assert_eq!(window.state.browser.visible_tabs().len(), index + 3);
        assert_eq!(window.state.browser.visible_tabs()[0], Some(first));
        assert_eq!(window.state.browser.visible_tabs()[index + 2], Some(tab));
    }

    let [x, y, width, height] = rect(&window, "page-layout");
    let target = crate::ui::viewport::drop_side(
        &window.state,
        tabs[3],
        [x + width * 0.4, y + height - 20.0],
    );
    assert!(target.is_some_and(|side| side < 4));
}
