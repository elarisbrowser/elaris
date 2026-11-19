// SPDX-License-Identifier: GPL-3.0-only

use super::*;

fn tabs(count: usize) -> Vec<Tab> {
    (0..count)
        .map(|index| Tab::new(TabId::from_raw(index as u64 + 1), "about:blank".into()))
        .collect()
}

fn begin(state: &mut TabDragState, tabs: &[Tab], index: usize) {
    state.set_section_rect(TabSection::Regular, [0.0, 100.0, 220.0, 300.0], tabs);
    for (row, tab) in tabs.iter().enumerate() {
        state.record_tab_rect(
            TabSection::Regular,
            tab.id(),
            [0.0, 100.0 + row as f32 * 38.0, 220.0, 34.0],
        );
    }
    state.set_viewport([0.0, 100.0, 220.0, 300.0]);
    let y = 100.0 + index as f32 * 38.0 + 17.0;
    state.press(
        tabs[index].id(),
        tabs,
        [20.0, y],
        [10.0, 17.0],
        [200.0, 34.0],
    );
}

#[test]
fn a_click_does_not_start_a_preview() {
    let tabs = tabs(3);
    let mut state = TabDragState::default();
    begin(&mut state, &tabs, 1);
    assert_eq!(state.move_to(&tabs, [24.0, 159.0]), MoveResult::NoChange);
    assert!(!state.is_dragging());
    assert_eq!(state.end(&tabs, false), (false, None));
}

#[test]
fn crossing_the_threshold_starts_the_drag_without_changing_browser_order() {
    let tabs = tabs(3);
    let mut state = TabDragState::default();
    begin(&mut state, &tabs, 1);
    assert_eq!(state.move_to(&tabs, [20.0, 159.0]), MoveResult::NoChange);
    assert!(!state.is_dragging());
    assert_ne!(state.move_to(&tabs, [20.0, 159.1]), MoveResult::Cancelled);
    assert!(state.is_dragging());
    assert_eq!(
        state.preview_items(&tabs, TabSection::Regular),
        vec![
            PreviewItem::Tab(tabs[0].id()),
            PreviewItem::DropSlot(tabs[1].id(), true),
            PreviewItem::Tab(tabs[2].id()),
        ]
    );
}

#[test]
fn dragging_either_group_member_moves_both_as_one_slot() {
    let tabs = tabs(4);
    let mut state = TabDragState::default();
    state.set_section_rect(TabSection::Regular, [0.0, 100.0, 220.0, 220.0], &tabs);
    for (index, tab) in tabs.iter().enumerate() {
        let row = match index {
            0 => 0,
            1 | 2 => 1,
            _ => 2,
        };
        state.record_tab_rect(
            TabSection::Regular,
            tab.id(),
            [0.0, 100.0 + row as f32 * 38.0, 220.0, 34.0],
        );
    }
    state.press_members(
        tabs[2].id(),
        vec![tabs[1].id(), tabs[2].id()],
        &tabs,
        [20.0, 155.0],
        [10.0, 17.0],
        [420.0, 34.0],
    );
    assert_eq!(
        state.move_to(&tabs, [20.0, 240.0]),
        MoveResult::PreviewChanged
    );
    assert_eq!(
        state.preview_items(&tabs, TabSection::Regular),
        vec![
            PreviewItem::Tab(tabs[0].id()),
            PreviewItem::Tab(tabs[3].id()),
            PreviewItem::DropSlot(tabs[2].id(), true),
        ]
    );
    let (_, drop) = state.end(&tabs, false);
    assert_eq!(drop.unwrap().tabs, [tabs[1].id(), tabs[2].id()]);
}

#[test]
fn a_single_tab_can_move_to_another_section() {
    let tabs = tabs(1);
    let mut state = TabDragState::default();
    state.set_section_rect(TabSection::Regular, [0.0, 100.0, 220.0, 100.0], &tabs);
    state.set_section_rect(TabSection::Favourite, [0.0, 20.0, 220.0, 70.0], &tabs);
    state.set_favourite_grid([0.0, 35.0, 220.0, 40.0]);
    state.record_tab_rect(TabSection::Regular, tabs[0].id(), [0.0, 100.0, 220.0, 34.0]);
    state.press(
        tabs[0].id(),
        &tabs,
        [20.0, 117.0],
        [10.0, 17.0],
        [200.0, 34.0],
    );
    assert_eq!(
        state.move_to(&tabs, [20.0, 60.0]),
        MoveResult::PreviewChanged
    );
    assert_eq!(
        state.preview_items(&tabs, TabSection::Regular),
        vec![PreviewItem::SourceSlot(tabs[0].id())]
    );
    assert_eq!(
        state.preview_items(&tabs, TabSection::Favourite),
        vec![PreviewItem::DropSlot(tabs[0].id(), false)]
    );
}

#[test]
fn crossing_sections_previews_a_new_slot_without_changing_browser_order() {
    let tabs = tabs(3);
    let mut state = TabDragState::default();
    state.set_section_rect(TabSection::Regular, [0.0, 130.0, 220.0, 220.0], &tabs);
    state.set_section_rect(TabSection::Favourite, [0.0, 20.0, 220.0, 80.0], &tabs);
    state.set_favourite_grid([0.0, 35.0, 220.0, 40.0]);
    for (index, tab) in tabs.iter().enumerate() {
        state.record_tab_rect(
            TabSection::Regular,
            tab.id(),
            [0.0, 130.0 + index as f32 * 38.0, 220.0, 34.0],
        );
    }
    state.press(
        tabs[1].id(),
        &tabs,
        [20.0, 147.0],
        [10.0, 17.0],
        [200.0, 34.0],
    );
    state.move_to(&tabs, [20.0, 60.0]);
    assert_eq!(
        state.preview_items(&tabs, TabSection::Regular),
        vec![
            PreviewItem::Tab(tabs[0].id()),
            PreviewItem::SourceSlot(tabs[1].id()),
            PreviewItem::Tab(tabs[2].id())
        ]
    );
    assert_eq!(
        state.preview_items(&tabs, TabSection::Favourite),
        vec![PreviewItem::DropSlot(tabs[1].id(), false)]
    );
    assert_eq!(
        tabs.iter().map(Tab::id).collect::<Vec<_>>(),
        [TabId::from_raw(1), TabId::from_raw(2), TabId::from_raw(3),]
    );
}

#[test]
fn scroll_offset_participates_in_edge_autoscroll() {
    let tabs = tabs(4);
    let mut state = TabDragState::default();
    begin(&mut state, &tabs, 0);
    state.scroll.set([100.0, 400.0]);
    state.move_to(&tabs, [20.0, 123.0]);
    assert!(state.is_dragging());
    assert!(state.scroll_target(1.0).is_some());
}

#[test]
fn cancellation_or_a_changed_tab_order_cannot_commit_a_drop() {
    let mut tabs = tabs(3);
    let mut state = TabDragState::default();
    begin(&mut state, &tabs, 1);
    state.move_to(&tabs, [20.0, 195.0]);
    assert_eq!(state.end(&tabs, true), (true, None));
    assert!(!state.is_dragging());
    assert!(
        state.visual().is_some(),
        "cancel settles back to the source"
    );

    state.cancel();
    begin(&mut state, &tabs, 1);
    state.move_to(&tabs, [20.0, 195.0]);
    tabs.remove(1);
    assert_eq!(state.move_to(&tabs, [20.0, 160.0]), MoveResult::Cancelled);
    assert_eq!(state.end(&tabs, false), (false, None));
}

#[test]
fn favourite_rows_wrap_at_four_and_fill_the_last_row() {
    let expected = [
        vec![],
        std::iter::once(0..1).collect(),
        std::iter::once(0..2).collect(),
        std::iter::once(0..3).collect(),
        std::iter::once(0..4).collect(),
        vec![0..4, 4..5],
        vec![0..4, 4..6],
        vec![0..4, 4..7],
        vec![0..4, 4..8],
        vec![0..4, 4..8, 8..9],
    ];
    for (count, ranges) in expected.iter().enumerate() {
        assert_eq!(favourite_row_ranges(count, 4), *ranges, "count {count}");
    }
}

#[test]
fn favourite_tile_widths_distribute_available_space_without_overflow() {
    for content_width in [40.0, 96.0, 196.0, 360.0] {
        for count in 1..=4 {
            let tile_width = favourite_tile_width(content_width, count);
            let row_width = tile_width * count as f32 + theme::FAVOURITE_GAP * (count - 1) as f32;
            assert!(row_width <= content_width + f32::EPSILON);
            if content_width
                >= theme::FAVOURITE_TILE_MIN_WIDTH * count as f32
                    + theme::FAVOURITE_GAP * (count - 1) as f32
            {
                assert!((row_width - content_width).abs() < f32::EPSILON);
            }
        }
    }
    assert_eq!(favourite_columns(200.0), 4);
    assert_eq!(favourite_columns(180.0), 3);
    assert_eq!(favourite_tile_width(24.0, 1), 24.0);
}

#[test]
fn favourite_destination_rect_uses_partial_row_width_and_index() {
    let tabs = tabs(5);
    let mut state = TabDragState::default();
    state.set_favourite_grid([10.0, 20.0, 200.0, 96.0]);
    let existing: Vec<_> = tabs
        .into_iter()
        .enumerate()
        .map(|(index, mut tab)| {
            if index < 4 {
                tab.section = TabSection::Favourite;
            }
            tab
        })
        .collect();
    let target = state.target_rect(
        &existing,
        TabDropTarget {
            section: TabSection::Favourite,
            index: 4,
        },
        TabSection::Regular,
    );
    assert_eq!(target, [10.0, 76.0, 200.0, 48.0]);
}

#[test]
fn favourite_insertion_uses_responsive_grid_slots_across_rows() {
    let mut tabs = tabs(6);
    for tab in &mut tabs[..5] {
        tab.section = TabSection::Favourite;
    }
    let mut state = TabDragState::default();
    state.set_section_rect(TabSection::Favourite, [10.0, 20.0, 200.0, 104.0], &tabs);
    state.set_favourite_grid([10.0, 20.0, 200.0, 104.0]);
    for (index, tab) in tabs[..5].iter().enumerate() {
        let row = index / 4;
        let col = index % 4;
        let width = if row == 0 { 44.0 } else { 200.0 };
        let x = if row == 0 {
            10.0 + col as f32 * 52.0
        } else {
            10.0
        };
        let y = 20.0 + row as f32 * 56.0;
        state.record_tab_rect(TabSection::Favourite, tab.id(), [x, y, width, 48.0]);
    }
    let source = tabs[5].id();
    state.press(source, &tabs, [100.0, 160.0], [10.0, 17.0], [200.0, 34.0]);
    state.move_to(&tabs, [60.0, 100.0]);
    assert_eq!(
        state.gesture.as_ref().and_then(|gesture| match gesture {
            Gesture::Dragging(dragging) => dragging.target,
            Gesture::Pressed(_) => None,
        }),
        Some(TabDropTarget {
            section: TabSection::Favourite,
            index: 4,
        })
    );
}

#[test]
fn menu_transition_uses_the_committed_destination_layout() {
    let tab = tabs(1).remove(0);
    let mut state = TabDragState::default();
    state.transition(
        tab.id(),
        TabSection::Regular,
        TabSection::Favourite,
        0,
        [12.0, 180.0, 200.0, 34.0],
        [12.0, 124.0, 200.0, 48.0],
    );

    let initial = state.visual().unwrap();
    assert_eq!(initial.to, [12.0, 180.0]);
    assert_eq!(initial.to_size, [200.0, 34.0]);
    assert!(!state.settle_tick(0.0));

    assert!(state.record_tab_rect(TabSection::Favourite, tab.id(), [16.0, 196.0, 184.0, 48.0],));
    let moving = state.visual().unwrap();
    assert_eq!(moving.section, TabSection::Favourite);
    assert_eq!(moving.to, [16.0, 196.0]);
    assert_eq!(moving.to_size, [184.0, 48.0]);
}

#[test]
fn cross_section_drop_waits_for_the_destination_after_layout_reflow() {
    let tabs = tabs(1);
    let mut state = TabDragState::default();
    state.set_section_rect(TabSection::Regular, [12.0, 120.0, 196.0, 100.0], &tabs);
    state.set_section_rect(TabSection::Favourite, [12.0, 64.0, 196.0, 48.0], &tabs);
    state.set_favourite_grid([12.0, 64.0, 196.0, 48.0]);
    state.record_tab_rect(
        TabSection::Regular,
        tabs[0].id(),
        [12.0, 120.0, 196.0, 34.0],
    );
    state.press(
        tabs[0].id(),
        &tabs,
        [32.0, 137.0],
        [12.0, 17.0],
        [196.0, 34.0],
    );
    state.move_to(&tabs, [32.0, 92.0]);
    state.record_placeholder(TabSection::Favourite, [12.0, 64.0, 196.0, 48.0]);

    let (dragged, drop) = state.end(&tabs, false);
    assert!(dragged);
    assert_eq!(drop.unwrap().target.section, TabSection::Favourite);
    let waiting = state.visual().unwrap();
    assert_eq!(waiting.section, TabSection::Regular);
    assert_eq!(waiting.to, [20.0, 75.0]);
    assert!(!state.settle_tick(0.0));

    assert!(state.record_tab_rect(
        TabSection::Favourite,
        tabs[0].id(),
        [12.0, 120.0, 196.0, 48.0],
    ));
    let settled = state.visual().unwrap();
    assert_eq!(settled.section, TabSection::Favourite);
    assert_eq!(settled.to, [12.0, 120.0]);
}

#[test]
fn empty_favourite_anchor_matches_the_grid_position_after_it_appears() {
    let tabs = tabs(1);
    let mut state = TabDragState::default();
    state.set_empty_favourites_anchor([12.0, 150.0, 196.0, 300.0]);
    assert_eq!(state.favourite_grid, Some([12.0, 150.0, 196.0, 0.0]));
    let target = state.target_rect(
        &tabs,
        TabDropTarget {
            section: TabSection::Favourite,
            index: 0,
        },
        TabSection::Regular,
    );
    assert_eq!(target, [12.0, 150.0, 196.0, 48.0]);
}

#[test]
fn empty_favourite_preview_remains_valid_when_the_grid_becomes_visible() {
    let tabs = tabs(1);
    let mut state = TabDragState::default();
    state.set_empty_favourites_anchor([12.0, 150.0, 196.0, 300.0]);
    state.set_section_rect(TabSection::Regular, [12.0, 150.0, 196.0, 100.0], &tabs);
    state.record_tab_rect(
        TabSection::Regular,
        tabs[0].id(),
        [12.0, 150.0, 196.0, 34.0],
    );
    state.press(
        tabs[0].id(),
        &tabs,
        [30.0, 167.0],
        [10.0, 17.0],
        [196.0, 34.0],
    );
    assert_eq!(
        state.move_to(&tabs, [30.0, 130.0]),
        MoveResult::PreviewChanged
    );
    assert!(state.empty_favourite_area.is_some());

    state.set_section_rect(TabSection::Favourite, [12.0, 150.0, 196.0, 48.0], &tabs);
    assert_eq!(state.refresh_target(&tabs), MoveResult::NoChange);
    assert_eq!(
        state.gesture.as_ref().and_then(|gesture| match gesture {
            Gesture::Dragging(dragging) => dragging.target,
            Gesture::Pressed(_) => None,
        }),
        Some(TabDropTarget {
            section: TabSection::Favourite,
            index: 0,
        })
    );
}
