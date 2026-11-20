// SPDX-License-Identifier: GPL-3.0-only

use crate::tab::TabSection;
use crate::ui::theme;

use super::{DRAG_THRESHOLD, TARGET_HYSTERESIS};

pub(crate) fn favourite_columns(content_width: f32) -> usize {
    let step = theme::FAVOURITE_TILE_MIN_WIDTH + theme::FAVOURITE_GAP;
    (((content_width + theme::FAVOURITE_GAP) / step).floor() as usize)
        .clamp(1, theme::FAVOURITE_COLUMNS)
}

pub(crate) fn favourite_row_ranges(count: usize, columns: usize) -> Vec<std::ops::Range<usize>> {
    let columns = columns.clamp(1, theme::FAVOURITE_COLUMNS);
    (0..count)
        .step_by(columns)
        .map(|start| start..(start + columns).min(count))
        .collect()
}

pub(crate) fn favourite_tile_width(content_width: f32, items_in_row: usize) -> f32 {
    if items_in_row == 0 {
        return 0.0;
    }
    let content_width = content_width.max(0.0);
    (content_width - theme::FAVOURITE_GAP * (items_in_row - 1) as f32).max(0.0)
        / items_in_row as f32
}

pub(super) fn section_index(section: TabSection) -> usize {
    match section {
        TabSection::Favourite => 0,
        TabSection::Regular => 1,
    }
}

pub(super) fn favourite_slot_rect(
    grid: [f32; 4],
    index: usize,
    item_count: usize,
    columns: usize,
) -> [f32; 4] {
    let columns = columns.clamp(1, theme::FAVOURITE_COLUMNS);
    let row_index = index / columns;
    let column_index = index % columns;
    let row_start = row_index * columns;
    let row_items = item_count.saturating_sub(row_start).min(columns).max(1);
    let width = favourite_tile_width(grid[2], row_items);
    let x = grid[0] + column_index as f32 * (width + theme::FAVOURITE_GAP);
    let y = grid[1] + row_index as f32 * (theme::FAVOURITE_TILE_HEIGHT + theme::FAVOURITE_GAP);
    [x, y, width, theme::FAVOURITE_TILE_HEIGHT]
}

pub(super) fn valid_rect(rect: [f32; 4]) -> bool {
    rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] > 0.0
}

pub(super) fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[1] >= rect[1]
        && point[0] <= rect[0] + rect[2]
        && point[1] <= rect[1] + rect[3]
}

pub(super) fn rect_before_pointer(
    rect: [f32; 4],
    pointer: [f32; 2],
    moving_down: bool,
    moving_up: bool,
) -> bool {
    let center = [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
    let hysteresis = if moving_down {
        TARGET_HYSTERESIS
    } else if moving_up {
        -TARGET_HYSTERESIS
    } else {
        0.0
    };
    pointer[1] > center[1] + hysteresis
        || ((pointer[1] - center[1]).abs() <= hysteresis.abs()
            && pointer[0] > center[0] + hysteresis)
}

pub(super) fn past_drag_threshold(start: [f32; 2], current: [f32; 2]) -> bool {
    let (dx, dy) = ((current[0] - start[0]).abs(), (current[1] - start[1]).abs());
    dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD
}
