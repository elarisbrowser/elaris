// SPDX-License-Identifier: GPL-3.0-only

//! The active tab's page: its latest frame in a rounded container inset from the window's edges,
//! and the pointer, wheel and key input it takes.

use elaris::ladybird::MouseKind;
use rune::{El, Phase, el};

use super::VIEWPORT;
use super::theme::{self, Theme};
use crate::app::AppState;
use crate::input;

/// The container's padding holds the page in from the window's edges, the sidebar being its
/// left; the page's own element clips to rounded corners and takes input in the page's
/// coordinates.
pub(crate) fn view(state: &AppState, theme: &Theme) -> El {
    el().key("viewport-container")
        .h_full()
        .grow(1.0)
        // Its size comes from the window, never from the frame inside: otherwise the last
        // frame holds it open and a narrower window never reaches the page.
        .min_w(0.0)
        .min_h(0.0)
        .pt(theme::VIEWPORT_INSET)
        .pr(theme::VIEWPORT_INSET)
        .pb(theme::VIEWPORT_INSET)
        .child(page(state, theme))
}

/// The page's latest frame, drawn one device pixel per pixel from the top-left. During a live
/// resize the newest frame keeps its own size rather than stretching to the window's, until
/// Ladybird lays the page out at the new size.
fn page(state: &AppState, theme: &Theme) -> El {
    let mut page = el().key(VIEWPORT);
    // Each tab keeps its own frame, so switching shows the page as it was at once.
    if let Some(tab) = state.browser.active_tab() {
        let [width, height] = tab.frame_size;
        page = page.child(
            el().key("page-frame")
                .w(width)
                .h(height)
                .pixels(&tab.surface.pixels),
        );
    }
    page.w_full()
        .h_full()
        .fill(theme.page)
        .border(theme::VIEWPORT_BORDER, theme.page_border)
        .continuous_corners()
        .clip_rounded(theme::VIEWPORT_RADIUS)
        .focusable(true)
        .no_focus_ring()
        .label("Web page")
        .on_layout(|state: &mut AppState, [_, _, w, h]| state.browser.set_viewport([w, h]))
        .on_drag(|state: &mut AppState, drag, tx| {
            let kind = match drag.phase {
                Phase::Down => {
                    tx.focus(VIEWPORT, false);
                    MouseKind::Down
                }
                Phase::Move => MouseKind::Move,
                Phase::Up => MouseKind::Up,
            };
            let held = drag.phase != Phase::Up;
            state
                .browser
                .pointer(kind, [drag.x, drag.y], held, drag.clicks);
        })
        .on_pointer_move_local(|state: &mut AppState, pointer, _| {
            state
                .browser
                .pointer(MouseKind::Move, [pointer.x, pointer.y], false, 0);
        })
        .on_hover_change(|state: &mut AppState, inside, _| {
            if !inside {
                state
                    .browser
                    .pointer(MouseKind::Leave, [0.0, 0.0], false, 0);
            }
        })
        .on_wheel(|state: &mut AppState, wheel, _| {
            let phase = input::wheel_phase(wheel.precise, wheel.phase, wheel.momentum);
            state.browser.wheel(
                [wheel.x, wheel.y],
                [wheel.dx, wheel.dy],
                wheel.precise,
                phase,
            );
            true
        })
        .on_key(|state: &mut AppState, key, _| state.browser.key(key))
}
