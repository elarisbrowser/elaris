// SPDX-License-Identifier: GPL-3.0-only

//! Which pane each side shows: the focus and page keys, and the drop target for a dragged tab.

use crate::app::AppState;
use crate::browser::{Browser, split};
use crate::tab::TabId;
use crate::ui::VIEWPORT;

pub(crate) fn focus_key(browser: &Browser) -> &'static str {
    pane_key(browser.focused_side())
}

pub(crate) fn page_focus_key(browser: &Browser) -> &'static str {
    if browser.peek.is_some() {
        crate::ui::peek::PAGE
    } else {
        focus_key(browser)
    }
}

pub(super) fn pane_key(side: usize) -> &'static str {
    PANE_KEYS
        .get(side)
        .copied()
        .unwrap_or("right-page-viewport")
}

const PANE_KEYS: [&str; 64] = [
    VIEWPORT,
    "right-page-viewport",
    "split-page-2",
    "split-page-3",
    "split-page-4",
    "split-page-5",
    "split-page-6",
    "split-page-7",
    "split-page-8",
    "split-page-9",
    "split-page-10",
    "split-page-11",
    "split-page-12",
    "split-page-13",
    "split-page-14",
    "split-page-15",
    "split-page-16",
    "split-page-17",
    "split-page-18",
    "split-page-19",
    "split-page-20",
    "split-page-21",
    "split-page-22",
    "split-page-23",
    "split-page-24",
    "split-page-25",
    "split-page-26",
    "split-page-27",
    "split-page-28",
    "split-page-29",
    "split-page-30",
    "split-page-31",
    "split-page-32",
    "split-page-33",
    "split-page-34",
    "split-page-35",
    "split-page-36",
    "split-page-37",
    "split-page-38",
    "split-page-39",
    "split-page-40",
    "split-page-41",
    "split-page-42",
    "split-page-43",
    "split-page-44",
    "split-page-45",
    "split-page-46",
    "split-page-47",
    "split-page-48",
    "split-page-49",
    "split-page-50",
    "split-page-51",
    "split-page-52",
    "split-page-53",
    "split-page-54",
    "split-page-55",
    "split-page-56",
    "split-page-57",
    "split-page-58",
    "split-page-59",
    "split-page-60",
    "split-page-61",
    "split-page-62",
    "split-page-63",
];

pub(crate) fn drop_side(state: &AppState, id: TabId, point: [f32; 2]) -> Option<usize> {
    let panes = state.browser.visible_tabs();
    if let Some(current) = state.browser.displayed_split()
        && panes.len() < state.browser.split_view_limit.min(split::MAX_PANES)
        && !panes.contains(&Some(id))
        && !panes.contains(&None)
        && split::target(state.split_rect, point, panes.len() + 1, current.ratio)
            == Some(panes.len())
    {
        return Some(panes.len());
    }
    if panes.len() == 2 {
        let ratio = state
            .browser
            .displayed_split()
            .map_or(0.5, |split| split.ratio);
        let [x, y, width, height] = state.split_rect;
        if point[0] < x || point[0] >= x + width || point[1] < y || point[1] >= y + height {
            return None;
        }
        let side =
            usize::from(point[0] >= x + split::widths(width, ratio)[0] + split::DIVIDER / 2.0);
        return (panes[side] != Some(id)).then_some(side);
    }
    let side = split::target(
        state.split_rect,
        point,
        panes.len(),
        state.browser.displayed_split()?.ratio,
    )?;
    if side >= panes.len() {
        return None;
    }
    if state.browser.displayed_split().is_some() {
        (panes[side] != Some(id)).then_some(side)
    } else {
        panes[0].map(|_| side)
    }
}
