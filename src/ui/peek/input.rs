// SPDX-License-Identifier: GPL-3.0-only

//! Routes pointer, wheel, and keyboard input for Peek and its backdrop.

use elaris::ladybird::MouseKind;
use rune::{El, Phase, PointerButton, el};

use crate::app::AppState;
use crate::browser::{peek, split};
use crate::tab::TabId;
use crate::ui::viewport;

use super::{PAGE, dismiss};

pub(super) fn webpage(id: TabId) -> El {
    el().key(PAGE)
        .w_full()
        .h_full()
        .focusable(true)
        .no_focus_ring()
        .label("Preview webpage")
        .pointer_buttons([PointerButton::Middle])
        .on_drag(move |state: &mut AppState, drag, tx| {
            if drag.phase == Phase::Down {
                tx.focus(PAGE, false);
            }
            if state.browser.drag_page(
                id,
                viewport::mouse_kind(drag.phase, drag.cancelled()),
                [drag.x, drag.y],
                tx.button(),
                tx.mods(),
                drag.clicks,
            ) {
                // A transformed Peek card's local point is in Ladybird's unscaled coordinates.
                let [x, y] = drag.window_point();
                let [left, top, _, _] = state.split_rect;
                state.browser.record_peek_origin(id, [x - left, y - top]);
            }
        })
        .on_pointer_move_local(move |state: &mut AppState, pointer, tx| {
            state
                .browser
                .hover_page(id, MouseKind::Move, [pointer.x, pointer.y], tx.mods());
        })
        .on_hover_change(move |state: &mut AppState, inside, tx| {
            if !inside {
                state
                    .browser
                    .hover_page(id, MouseKind::Leave, [0.0; 2], tx.mods());
            }
        })
        .on_wheel(move |state: &mut AppState, wheel, _| {
            state.browser.wheel_page(id, wheel);
            true
        })
        .on_magnify(move |state: &mut AppState, magnify, tx| {
            state.browser.pinch_page(id, magnify, tx.mods());
            true
        })
        .on_key(viewport::page_key)
        .on_key_up(|state: &mut AppState, key, tx| {
            state.browser.key(key, false, false, tx.physical_key())
        })
}

/// A transparent hit target under the panel catches dismissals without altering sidebar layout.
pub(in crate::ui) fn backdrop() -> El {
    el().key("peek-backdrop")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden()
        .pointer_buttons([PointerButton::Middle])
        .on_secondary_press(|state: &mut AppState, _, tx| {
            dismiss(state, tx);
        })
        .on_wheel(|_: &mut AppState, _, _| true)
        .on_drag(|state: &mut AppState, drag, tx| {
            let point = drag.window_point();
            if drag.phase == Phase::Down {
                if tx.button() == PointerButton::Primary
                    && peek::shift_only(tx.mods())
                    && let Some(side) = pane_at(state, point)
                {
                    drag.data = side as f32 + 1.0;
                } else {
                    dismiss(state, tx);
                    return;
                }
            }
            if drag.data == 0.0 {
                return;
            }
            let side = drag.data as usize - 1;
            if let Some(id) = state.browser.visible_tabs()[side] {
                let [x, y, _, _] = pane_rects(state)[side];
                state.browser.drag_page(
                    id,
                    viewport::mouse_kind(
                        drag.phase,
                        drag.cancelled() || !peek::shift_only(tx.mods()),
                    ),
                    [point[0] - x, point[1] - y],
                    tx.button(),
                    tx.mods(),
                    drag.clicks,
                );
            }
            if drag.phase == Phase::Up {
                dismiss(state, tx);
            }
        })
}

fn pane_rects(state: &AppState) -> Vec<[f32; 4]> {
    let panes = state.browser.visible_tabs();
    if let Some(split) = state.browser.displayed_split() {
        let ratio = split.ratio;
        if panes.len() == 2 {
            return split::widths(state.split_rect[2], ratio)
                .into_iter()
                .scan(state.split_rect[0], |x, width| {
                    let rect = [*x, state.split_rect[1], width, state.split_rect[3]];
                    *x += width + split::DIVIDER;
                    Some(rect)
                })
                .collect();
        }
        (0..panes.len())
            .map(|index| split::grid_cell_rect(state.split_rect, panes.len(), index, ratio))
            .collect()
    } else {
        vec![state.split_rect]
    }
}

fn pane_at(state: &AppState, [x, y]: [f32; 2]) -> Option<usize> {
    let panes = state.browser.visible_tabs();
    pane_rects(state)
        .into_iter()
        .enumerate()
        .find_map(|(side, [left, top, w, h])| {
            let tab = state.browser.tab(panes[side]?)?;
            (x >= left && x < left + w && y >= top && y < top + h && !tab.is_internal())
                .then_some(side)
        })
}
