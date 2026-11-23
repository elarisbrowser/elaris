// SPDX-License-Identifier: GPL-3.0-only

//! Placeholder slots shown while a tab is dragged: the drop target and the source's vacated slot.

use rune::{El, Spring, el};

use super::drag_handler::tab_drag_handler;
use crate::app::AppState;
use crate::tab::{TabId, TabSection};
use crate::ui::theme::{self, Theme};

#[allow(clippy::too_many_arguments)]
pub(super) fn drop_placeholder(
    tab: TabId,
    same_section: bool,
    section: TabSection,
    index: usize,
    count: usize,
    width: f32,
    motion: Spring,
    theme: &Theme,
) -> El {
    if section == TabSection::Favourite {
        let mut slot = el()
            .key(if same_section {
                format!("favourite-{}", tab.raw())
            } else {
                "favourites-drop-placeholder".to_owned()
            })
            .w(width)
            .h(theme::FAVOURITE_TILE_HEIGHT)
            .radius(theme::FAVOURITE_TILE_RADIUS)
            .continuous_corners()
            .fill(theme.tab_drop_indicator)
            .motion(motion)
            .position_in_set(index + 1, count)
            .on_layout_tx(move |state: &mut AppState, rect, tx| {
                state.tab_drag.record_placeholder(section, rect);
                tx.live_only();
            });
        if same_section {
            slot = tab_drag_handler(slot, tab, vec![tab]);
        }
        slot
    } else {
        let mut slot = el()
            .key(if same_section {
                format!("tab-{}", tab.raw())
            } else {
                "regular-drop-placeholder".to_owned()
            })
            .w_full()
            .h(theme::ROW_HEIGHT)
            .radius(theme::ROW_RADIUS)
            .continuous_corners()
            .fill(theme.tab_drop_indicator)
            .opacity(0.28)
            .motion(motion)
            .position_in_set(index + 1, count)
            .on_layout_tx(move |state: &mut AppState, rect, tx| {
                state.tab_drag.record_placeholder(section, rect);
                tx.live_only();
            });
        if same_section {
            slot = tab_drag_handler(slot, tab, vec![tab]);
        }
        slot
    }
}

pub(super) fn source_placeholder(
    tab: TabId,
    section: TabSection,
    index: usize,
    count: usize,
    width: f32,
) -> El {
    let (key, width, height) = if section == TabSection::Favourite {
        (
            format!("favourite-{}", tab.raw()),
            width,
            theme::FAVOURITE_TILE_HEIGHT,
        )
    } else {
        (format!("tab-{}", tab.raw()), 0.0, theme::ROW_HEIGHT)
    };
    let slot = el().key(key).h(height).position_in_set(index + 1, count);
    let slot = if width > 0.0 {
        slot.w(width)
    } else {
        slot.w_full()
    };
    tab_drag_handler(slot, tab, vec![tab])
}
