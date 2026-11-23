// SPDX-License-Identifier: GPL-3.0-only

//! Section rendering for the sidebar: the favourites grid, the regular tab list, and their dividers and new-tab button.

use rune::{El, GrowthSpec, Spring, col, el, icon, row};

use super::placeholder::{drop_placeholder, source_placeholder};
use super::row::{favourite_tile, tab_row};
use super::split::{TabRowStyle, members as split_group_members, view as split_tab_group};
use crate::app;
use crate::app::AppState;
use crate::tab::{TabId, TabSection};
use crate::ui::components::controls::row_button;
use crate::ui::components::rule::hairline;
use crate::ui::icons;
use crate::ui::sidebar::drag::{
    PreviewItem, favourite_columns, favourite_row_ranges, favourite_tile_width,
};
use crate::ui::theme::{self, Theme};

pub(super) fn favourites(
    state: &AppState,
    active: Option<TabId>,
    items: &[PreviewItem],
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let content_width = (state.sidebar.width - 2.0 * theme::SIDEBAR_PADDING).max(0.0);
    let columns = favourite_columns(content_width);
    let ranges = favourite_row_ranges(items.len(), columns);
    let mut grid = col()
        .key("favourites-grid")
        .w_full()
        .gap(theme::FAVOURITE_GAP)
        .growth(GrowthSpec {
            spring: motion,
            ..GrowthSpec::default()
        })
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            state.tab_drag.set_favourite_grid(rect);
            tx.live_only();
        });
    for (row_index, range) in ranges.iter().enumerate() {
        let row_count = range.len();
        let width = favourite_tile_width(content_width, row_count);
        let mut tile_row = row()
            .key(format!("favourites-row-{row_index}"))
            .w_full()
            .gap(theme::FAVOURITE_GAP)
            .motion(motion)
            .animate_layout();
        for index in range.clone() {
            let item = items[index];
            let tile = match item {
                PreviewItem::Tab(id) => state.browser.tab(id).map_or_else(el, |tab| {
                    favourite_tile(
                        state,
                        tab,
                        Some(id) == active,
                        index,
                        items.len(),
                        width,
                        motion,
                        theme,
                    )
                }),
                PreviewItem::SourceSlot(id) => {
                    source_placeholder(id, TabSection::Favourite, index, items.len(), width)
                }
                PreviewItem::DropSlot(id, same_section) => drop_placeholder(
                    id,
                    same_section,
                    TabSection::Favourite,
                    index,
                    items.len(),
                    width,
                    motion,
                    theme,
                ),
            };
            tile_row = tile_row.child(tile);
        }
        grid = grid.child(tile_row);
    }
    col()
        .key("favourites-section")
        .w_full()
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            let target_ready =
                state
                    .tab_drag
                    .set_section_rect(TabSection::Favourite, rect, state.browser.tabs());
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        })
        .child(grid)
}

/// A hairline between sidebar sections. Its spacing comes from `SIDEBAR_DIVIDER_PADDING` alone.
/// The spring a tab stack's rows open and close their space on.
pub(super) fn presence_motion(reduced_motion: bool) -> Spring {
    if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_PRESENCE_MOTION
    }
}

pub(super) fn divider(key: &'static str, theme: &Theme) -> El {
    col()
        .key(key)
        .w_full()
        .px(theme::SIDEBAR_PADDING)
        .py(theme::SIDEBAR_DIVIDER_PADDING)
        .child(hairline(theme.overlay_border))
}

/// The button that opens a new tab, at the top or bottom of the tab list.
pub(super) fn new_tab_button(theme: &Theme) -> El {
    row_button(
        "new-tab",
        icon(&icons::PLUS, theme::ICON).color(theme.text_muted),
        "New Tab",
        theme.text_muted,
        theme,
    )
    .on_press_tx(app::new_tab)
}

pub(super) fn regular_rows(
    state: &AppState,
    active: Option<TabId>,
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let section = TabSection::Regular;
    let items = state.tab_drag.preview_items(state.browser.tabs(), section);
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let mut rows = col()
        .key("regular-rows")
        .w_full()
        .gap(theme::TAB_GAP)
        .motion(motion)
        .growth(GrowthSpec {
            spring: presence_motion(reduced_motion),
            edge: theme::TAB_PRESENCE_EDGE,
        });
    let split_pair = split_group_members(state, section)
        .filter(|pair| pair.iter().all(|id| items.contains(&PreviewItem::Tab(*id))));
    let mut split_group_added = false;
    for (index, item) in items.iter().copied().enumerate() {
        let split_tab = match item {
            PreviewItem::Tab(id) => split_pair.filter(|pair| pair.contains(&id)),
            PreviewItem::SourceSlot(_) | PreviewItem::DropSlot(_, _) => None,
        };
        if let Some(pair) = split_tab.filter(|_| !split_group_added) {
            rows = rows.child(
                split_tab_group(state, section, pair, index, items.len(), theme).arrives_whole(),
            );
            split_group_added = true;
            continue;
        }
        if split_group_added && split_tab.is_some() {
            continue;
        }
        let element = match item {
            PreviewItem::Tab(id) => state.browser.tab(id).map_or_else(el, |tab| {
                tab_row(
                    state,
                    tab,
                    section,
                    Some(id) == active,
                    index,
                    items.len(),
                    theme,
                    TabRowStyle::Standalone,
                )
            }),
            PreviewItem::SourceSlot(id) => source_placeholder(id, section, index, items.len(), 0.0),
            PreviewItem::DropSlot(id, same_section) => drop_placeholder(
                id,
                same_section,
                section,
                index,
                items.len(),
                0.0,
                motion,
                theme,
            ),
        };
        rows = rows.child(element.arrives_whole());
    }
    col()
        .key("regular-section")
        .w_full()
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            let target_ready = state
                .tab_drag
                .set_section_rect(section, rect, state.browser.tabs());
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        })
        .child(rows)
}
