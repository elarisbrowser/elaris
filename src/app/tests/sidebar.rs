// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use rune::Mods;

#[test]
fn favourite_tiles_fill_each_responsive_row() {
    use rune::{Env, Runtime};

    for favourite_count in 0..=9 {
        let mut candidate = AppState::new(Some("one.example"));
        let mut candidate_ids = vec![candidate.browser.active_tab().unwrap().id()];
        for index in 1..9 {
            candidate_ids.push(
                candidate
                    .browser
                    .open_tab(&format!("tab-{index}.example"))
                    .unwrap(),
            );
        }
        for (index, id) in candidate_ids
            .iter()
            .take(favourite_count)
            .copied()
            .enumerate()
        {
            candidate
                .browser
                .move_tab_to(id, TabSection::Favourite, index)
                .unwrap();
        }
        let mut runtime = Runtime::new(
            candidate,
            crate::ui::view,
            (960.0, 640.0),
            Env::new(false, 1.0),
        );
        runtime.frame(0.0);

        let elements = runtime.elements();
        if favourite_count == 0 {
            assert!(
                !elements
                    .iter()
                    .any(|element| element.key.as_deref() == Some("favourites-grid"))
            );
            continue;
        }
        let grid = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("favourites-grid"))
            .expect("the favourites grid is visible")
            .rect;
        let tiles: Vec<_> = candidate_ids
            .iter()
            .take(favourite_count)
            .map(|id| {
                elements
                    .iter()
                    .find(|element| {
                        element.key.as_deref() == Some(&format!("favourite-{}", id.raw()))
                    })
                    .expect("each favourite has a tile")
                    .rect
            })
            .collect();

        for (row_index, row) in tiles.chunks(4).enumerate() {
            let row_count = row.len();
            let gap = if row_count > 1 {
                row[1][0] - row[0][0] - row[0][2]
            } else {
                0.0
            };
            let expected_width = (grid[2] - gap * (row_count - 1) as f32) / row_count as f32;
            assert!((row[0][0] - grid[0]).abs() < 0.1);
            assert!(
                (row[0][1]
                    - (grid[1]
                        + row_index as f32 * (48.0 + if row_count > 1 { gap } else { 8.0 })))
                .abs()
                    < 0.1
            );
            for (column, tile) in row.iter().enumerate() {
                assert!((tile[2] - expected_width).abs() < 0.1);
                assert_eq!(tile[3], 48.0);
                if column > 0 {
                    assert!((tile[0] - row[column - 1][0] - expected_width - gap).abs() < 0.1);
                }
            }
            let last = row.last().unwrap();
            assert!((last[0] + last[2] - (grid[0] + grid[2])).abs() < 0.1);
        }
    }

    // The same count reflows against the narrower sidebar's actual content width.
    let mut narrow = AppState::new(Some("one.example"));
    narrow.sidebar.width = 220.0;
    let mut narrow_ids = vec![narrow.browser.active_tab().unwrap().id()];
    for index in 1..4 {
        narrow_ids.push(
            narrow
                .browser
                .open_tab(&format!("narrow-{index}.example"))
                .unwrap(),
        );
    }
    for (index, id) in narrow_ids.iter().copied().enumerate() {
        narrow
            .browser
            .move_tab_to(id, TabSection::Favourite, index)
            .unwrap();
    }
    let mut runtime = Runtime::new(
        narrow,
        crate::ui::view,
        (960.0, 640.0),
        Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    let elements = runtime.elements();
    let grid = elements
        .iter()
        .find(|element| element.key.as_deref() == Some("favourites-grid"))
        .unwrap()
        .rect;
    let tile = elements
        .iter()
        .find(|element| {
            element.key.as_deref() == Some(&format!("favourite-{}", narrow_ids[0].raw()))
        })
        .unwrap()
        .rect;
    let next = elements
        .iter()
        .find(|element| {
            element.key.as_deref() == Some(&format!("favourite-{}", narrow_ids[1].raw()))
        })
        .unwrap()
        .rect;
    let gap = next[0] - tile[0] - tile[2];
    assert!((tile[2] - (grid[2] - 3.0 * gap) / 4.0).abs() < 0.1);
}
#[test]
fn adding_a_favourite_animates_existing_tile_and_favicon_during_preview() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let favourite = state.browser.active_tab().unwrap().id();
    let regular = state.browser.open_tab("two.example").unwrap();
    state
        .browser
        .move_tab_to(favourite, TabSection::Favourite, 0)
        .unwrap();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    runtime.frame(0.0);

    let element = |runtime: &Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap()
    };
    let favourite_tile = format!("favourite-{}", favourite.raw());
    let initial_tile = element(&runtime, &favourite_tile);
    let initial_icon = element(&runtime, &format!("favourite-icon-{}", favourite.raw()));
    let regular_row = element(&runtime, &format!("tab-{}", regular.raw()));
    let grid = element(&runtime, "favourites-grid").rect;
    let pointer = [
        regular_row.rect[0] + 24.0,
        regular_row.rect[1] + regular_row.rect[3] / 2.0,
    ];
    runtime.pointer_move(pointer[0], pointer[1]);
    runtime.pointer_down(pointer[0], pointer[1]);
    runtime.frame(0.0);

    let destination = [grid[0] + grid[2] * 0.75, grid[1] + grid[3] / 2.0];
    runtime.pointer_move(destination[0], destination[1]);
    runtime.frame(1.0 / 60.0);

    assert!(runtime.state.tab_drag.is_dragging());
    assert_eq!(
        runtime.state.browser.tab(regular).unwrap().section(),
        TabSection::Regular
    );
    let preview = element(&runtime, &favourite_tile);
    let preview_icon = element(&runtime, &format!("favourite-icon-{}", favourite.raw()));
    assert!(preview.target[2] < initial_tile.rect[2] * 0.75);
    assert!(
        preview.rect[2] < initial_tile.rect[2] && preview.rect[2] > preview.target[2],
        "the existing tile eases to its new width during the drag: {:?} -> {:?}",
        preview.rect,
        preview.target
    );
    assert!(
        preview_icon.target[0] < initial_icon.rect[0]
            && preview_icon.rect[0] < initial_icon.rect[0]
            && preview_icon.rect[0] > preview_icon.target[0],
        "the favicon follows the tile's live horizontal layout motion: {:?} -> {:?}",
        preview_icon.rect,
        preview_icon.target
    );
    assert_eq!(
        runtime.state.browser.active_tab().map(Tab::id),
        Some(favourite),
        "dragging a background tab must not activate it"
    );
}
#[test]
fn favourite_morph_keeps_the_dragged_favicon_moving_inside_the_tile() {
    use rune::{Env, Runtime};

    let state = AppState::new(Some("one.example"));
    let tab = state.browser.active_tab().unwrap().id();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    let rect = |runtime: &Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap()
            .rect
    };
    let row = rect(&runtime, &format!("tab-{}", tab.raw()));
    let scroll = rect(&runtime, "tabs-scroll");
    let (x, y) = (row[0] + 24.0, row[1] + row[3] / 2.0);
    let destination_y = scroll[1] - 20.0;
    runtime.pointer_move(x, y);
    runtime.pointer_down(x, y);
    runtime.frame(0.0);
    runtime.pointer_move(x, destination_y);
    runtime.frame(1.0 / 60.0);
    assert!(runtime.state.tab_drag.is_dragging());

    let tracking_point = [x + 40.0, destination_y + 4.0];
    runtime.pointer_move(tracking_point[0], tracking_point[1]);
    runtime.frame(1.0 / 60.0);
    let live_visual = rect(&runtime, "tab-drag-visual");
    assert!((live_visual[0] - (tracking_point[0] - 24.0)).abs() < 0.1);
    assert!((live_visual[1] - (tracking_point[1] - 17.0)).abs() < 0.1);

    runtime.pointer_up(tracking_point[0], tracking_point[1]);
    for _ in 0..5 {
        runtime.frame(1.0 / 60.0);
    }
    assert_eq!(
        runtime.state.browser.tab(tab).map(Tab::section),
        Some(TabSection::Favourite)
    );

    let visual = rect(&runtime, "tab-drag-visual");
    let icon = rect(&runtime, "drag-tab-icon");
    assert!(
        icon[0] + icon[2] / 2.0 > visual[0] + visual[2] / 3.0,
        "the favicon moves toward the tile center during the morph: icon {icon:?}, visual {visual:?}"
    );
}
#[test]
fn dragging_a_background_tab_reorders_live_without_activating_it() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    runtime.frame(0.0);

    let rect = |runtime: &Runtime<AppState>, id: TabId| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(&format!("tab-{}", id.raw())))
            .unwrap()
            .rect
    };
    let second_rect = rect(&runtime, second);
    let third_rect = rect(&runtime, third);
    let before_order = [first, second, third];
    let pointer_x = second_rect[0] + 24.0;
    let pointer_y = second_rect[1] + second_rect[3] / 2.0;
    runtime.pointer_move(pointer_x, pointer_y);
    runtime.pointer_down(pointer_x, pointer_y);
    runtime.frame(0.0);
    runtime.pointer_move(pointer_x, third_rect[1] + third_rect[3] / 2.0 + 5.0);
    runtime.frame(1.0 / 60.0);

    assert!(runtime.state.tab_drag.is_dragging());
    assert_eq!(
        runtime
            .state
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .collect::<Vec<_>>(),
        before_order,
        "the browser order stays committed until release"
    );
    let third_mid_y = rect(&runtime, third)[1];
    assert!(
        third_mid_y < third_rect[1] && third_mid_y > second_rect[1],
        "the neighbouring row starts moving into the preview slot"
    );

    let release_y = third_rect[1] + third_rect[3] / 2.0 + 5.0;
    let dragged_visual = runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("tab-drag-visual"))
        .expect("drag overlay is visible");
    assert!(
        (dragged_visual.rect[0] - second_rect[0]).abs() < 0.1,
        "drag visual x {} differs from row x {}",
        dragged_visual.rect[0],
        second_rect[0]
    );
    assert!(
        (dragged_visual.rect[1] - (release_y - second_rect[3] / 2.0)).abs() < 0.1,
        "drag visual y {} differs from expected row y {}",
        dragged_visual.rect[1],
        release_y - second_rect[3] / 2.0
    );
    runtime.pointer_up(pointer_x, release_y);
    runtime.frame(1.0 / 60.0);

    assert_eq!(
        runtime
            .state
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .collect::<Vec<_>>(),
        [first, third, second]
    );
    assert_eq!(runtime.state.browser.active_tab().map(Tab::id), Some(first));
    assert!(
        runtime.state.tab_drag.visual().is_some(),
        "the drop is still settling"
    );
}
#[test]
fn a_tab_click_without_drag_still_selects_the_tab() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    runtime.frame(0.0);

    let row = runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(&format!("tab-{}", second.raw())))
        .unwrap()
        .rect;
    let (x, y) = (row[0] + 24.0, row[1] + row[3] / 2.0);
    runtime.pointer_move(x, y);
    runtime.pointer_down(x, y);
    runtime.frame(0.0);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);

    assert_eq!(
        runtime.state.browser.active_tab().map(Tab::id),
        Some(second)
    );
    assert_eq!(
        runtime
            .state
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .collect::<Vec<_>>(),
        [first, second],
        "clicking leaves tab order unchanged"
    );
    assert!(!runtime.state.tab_drag.is_dragging());
}
#[test]
fn escape_cancels_a_live_tab_preview_without_persisting_an_order_change() {
    use rune::{Env, Runtime};

    let mut state = AppState::new(Some("one.example"));
    let first = state.browser.active_tab().unwrap().id();
    let second = state.browser.open_tab("two.example").unwrap();
    let third = state.browser.open_tab("three.example").unwrap();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    let row = |runtime: &Runtime<AppState>, id: TabId| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(&format!("tab-{}", id.raw())))
            .unwrap()
            .rect
    };
    let second_rect = row(&runtime, second);
    let third_rect = row(&runtime, third);
    let (x, y) = (second_rect[0] + 24.0, second_rect[1] + second_rect[3] / 2.0);
    runtime.pointer_move(x, y);
    runtime.pointer_down(x, y);
    runtime.frame(0.0);
    runtime.pointer_move(x, third_rect[1] + third_rect[3] / 2.0 + 5.0);
    runtime.frame(1.0 / 60.0);
    assert!(runtime.state.tab_drag.is_dragging());

    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);

    assert_eq!(
        runtime
            .state
            .browser
            .tabs()
            .iter()
            .map(Tab::id)
            .collect::<Vec<_>>(),
        [first, second, third]
    );
    assert!(!runtime.state.tab_drag.is_dragging());
    assert!(
        runtime.state.tab_drag.visual().is_some(),
        "cancellation settles the shared visual back to its source"
    );
}

#[test]
fn new_tab_at_top_lands_below_the_button_and_pushes_tabs_down() {
    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    let mut state = AppState::new(Some("one.example"));
    state.settings.new_tab_position = elaris_core::settings::NewTabPosition::Top;
    let first = state.browser.active_tab().unwrap().id();
    state.browser.open_tab("two.example").unwrap();
    let mut runtime = Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
    for _ in 0..30 {
        runtime.frame(1.0 / 60.0);
    }
    let rect = |runtime: &Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap()
            .rect
    };
    let first_row = format!("tab-{}", first.raw());
    let button = rect(&runtime, "new-tab");
    let start_y = rect(&runtime, &first_row)[1];
    assert!(
        start_y >= button[1] + button[3],
        "tabs start below the button"
    );

    runtime.update(crate::app::new_tab);
    runtime.frame(0.0);
    runtime.key(KeyEvent {
        key: Key::Text("three.example".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);

    let new_tab = runtime.state.browser.active_tab().unwrap().id();
    assert_eq!(
        runtime
            .state
            .browser
            .section_tabs(TabSection::Regular)
            .next()
            .map(Tab::id),
        Some(new_tab),
        "the new tab leads the regular list"
    );
    let moving_y = rect(&runtime, &first_row)[1];
    assert!(
        moving_y > start_y,
        "existing tabs start moving down on the first frame: {start_y} -> {moving_y}"
    );

    for _ in 0..60 {
        runtime.frame(1.0 / 60.0);
    }
    let button = rect(&runtime, "new-tab");
    let new_row = rect(&runtime, &format!("tab-{}", new_tab.raw()));
    assert!(
        (new_row[1] - (button[1] + button[3] + crate::ui::theme::TAB_GAP)).abs() < 0.5,
        "the new tab sits directly below the button: {new_row:?} vs {button:?}"
    );
    assert!(
        rect(&runtime, &first_row)[1] > moving_y,
        "the existing tab settles below the new one"
    );
}
