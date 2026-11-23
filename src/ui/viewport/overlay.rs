// SPDX-License-Identifier: GPL-3.0-only

//! Where the page sits in the window, and the overlays drawn over it.

use crate::app::AppState;
use crate::browser::split;
use crate::ui::theme::{self, Theme};
use rune::{El, el};

pub(super) fn indicator(theme: &Theme, radius: f32) -> El {
    el().w_full()
        .h_full()
        .inert()
        .a11y_hidden()
        .fill(theme.selection)
        .radius(radius)
}

/// Whether the header row sits over the page's column, so the page starts below it: with the
/// sidebar hidden or on the right on macOS, or on Linux while the row is revealed.
pub(super) fn header_above_page(state: &AppState) -> bool {
    #[cfg(elaris_ui = "linux")]
    {
        crate::ui::sidebar::header::header_revealed(state)
    }
    #[cfg(not(elaris_ui = "linux"))]
    {
        state.sidebar.collapsed || state.sidebar_on_right() || state.toolbar_address_bar()
    }
}

/// The webpage's top-left origin in the window, used to anchor its native find overlay.
pub(crate) fn page_origin(state: &AppState) -> [f32; 2] {
    if state.browser.fullscreen_tab().is_some() {
        return [0.0, 0.0];
    }
    let inset = state.page_inset();
    let left = if state.sidebar.collapsed || state.sidebar_on_right() {
        inset
    } else {
        state.sidebar.width
    };
    let top = if header_above_page(state) {
        theme::HEADER_HEIGHT
    } else {
        inset
    };
    let offset = state
        .browser
        .displayed_split()
        .map_or([0.0, 0.0], |split_state| {
            if state.browser.visible_tabs().len() == 2 {
                return [
                    if state.browser.focused_side() == 0 {
                        0.0
                    } else {
                        split::widths(state.split_rect[2], split_state.ratio)[0] + split::DIVIDER
                    },
                    0.0,
                ];
            }
            let rect = split::grid_cell_rect(
                [0.0, 0.0, state.split_rect[2], state.split_rect[3]],
                state.browser.visible_tabs().len(),
                state.browser.focused_side(),
                split_state.ratio,
            );
            [rect[0], rect[1]]
        });
    if state.horizontal_tabs() {
        [
            inset + offset[0],
            crate::ui::bookmarks::horizontal_chrome_height(state) + inset + offset[1],
        ]
    } else {
        [left + offset[0], top + offset[1]]
    }
}
