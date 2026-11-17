// SPDX-License-Identifier: GPL-3.0-only

//! The active tab's page in a rounded container inset from the window's edges, and the pointer,
//! wheel and key input it takes.

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
        // With the sidebar hidden, the page sits below the header and in from the left edge too.
        .pt(if state.sidebar.collapsed {
            theme::HEADER_HEIGHT
        } else {
            theme::VIEWPORT_INSET
        })
        .pl(if state.sidebar.collapsed {
            theme::VIEWPORT_INSET
        } else {
            0.0
        })
        .pr(theme::VIEWPORT_INSET)
        .pb(theme::VIEWPORT_INSET)
        .child(page(state, theme))
}

/// The page's latest frame fills the viewport. During a live resize Rune scales it to the new
/// bounds until Ladybird presents a frame laid out at that size.
fn page(state: &AppState, theme: &Theme) -> El {
    let mut page = el().key(VIEWPORT);
    if let Some(tab) = state.browser.active_tab() {
        page = page.pixels(&tab.surface.pixels);
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
