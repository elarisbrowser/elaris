// SPDX-License-Identifier: GPL-3.0-only

//! The web page: its latest frame in a rounded container inset from the window's edges, and the
//! pointer, wheel and key input it takes.

use elaris::ladybird::MouseKind;
use rune::{El, Phase, el};

use super::VIEWPORT;
use super::theme::{self, Theme};
use crate::app::AppState;
use crate::input;

/// The container's padding holds the page in from the window's edges; the page's own element
/// clips to rounded corners and takes input in the page's coordinates.
pub(crate) fn view(state: &AppState, theme: &Theme) -> El {
    el().key("viewport-container")
        .w_full()
        .grow(1.0)
        .px(theme::VIEWPORT_INSET)
        .pb(theme::VIEWPORT_INSET)
        .child(page(state, theme))
}

/// The page's latest frame, drawn one device pixel per pixel from the top-left. During a live
/// resize the newest frame keeps its own size rather than stretching to the window's, until
/// Ladybird lays the page out at the new size.
fn page(state: &AppState, theme: &Theme) -> El {
    let [width, height] = state.page.frame_size;
    let frame = el()
        .key("page-frame")
        .w(width)
        .h(height)
        .pixels(&state.page.pixels);

    el().key(VIEWPORT)
        .w_full()
        .h_full()
        .fill(theme.card)
        .continuous_corners()
        .clip_rounded(theme::VIEWPORT_RADIUS)
        .focusable(true)
        .no_focus_ring()
        .label("Web page")
        .on_layout(|state: &mut AppState, [_, _, w, h]| state.page.set_viewport([w, h]))
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
                .page
                .pointer(kind, [drag.x, drag.y], held, drag.clicks);
        })
        .on_pointer_move_local(|state: &mut AppState, pointer, _| {
            state
                .page
                .pointer(MouseKind::Move, [pointer.x, pointer.y], false, 0);
        })
        .on_hover_change(|state: &mut AppState, inside, _| {
            if !inside {
                state.page.pointer(MouseKind::Leave, [0.0, 0.0], false, 0);
            }
        })
        .on_wheel(|state: &mut AppState, wheel, _| {
            let phase = input::wheel_phase(wheel.precise, wheel.phase, wheel.momentum);
            state.page.wheel(
                [wheel.x, wheel.y],
                [wheel.dx, wheel.dy],
                wheel.precise,
                phase,
            );
            true
        })
        .on_key(|state: &mut AppState, key, _| state.page.key(key))
        .child(frame)
}
