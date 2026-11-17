// SPDX-License-Identifier: GPL-3.0-only

use elaris::ladybird::MouseKind;
use rune::tok::{self, Theme, type_};
use rune::{Cx, El, Phase, col, el, row, text, text_input};

use crate::app::AppState;
use crate::input;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let theme = tok::theme(true);
    state.page.note_scale(cx.env().scale);

    let address = text_input(&state.address, type_::BODY)
        .key("address")
        .single_line()
        .w_full()
        .h(40.0)
        .px(12.0)
        .color(theme.text)
        .placeholder("Search or enter address", theme.text_secondary)
        .label("Address or search")
        .on_edit(|state: &mut AppState| &mut state.address)
        .on_submit(|state: &mut AppState, tx| {
            state.navigate();
            tx.focus(VIEWPORT, false);
        });

    let mut toolbar = row()
        .key("toolbar")
        .w_full()
        .h(64.0)
        .gap(12.0)
        .pl(16.0 + cx.titlebar_inset())
        .pr(16.0)
        .center_y()
        .child(
            el().key("address-container")
                .w_full()
                .h(40.0)
                .px(4.0)
                .center_y()
                .radius(10.0)
                .fill(theme.control_off)
                .child(address),
        );
    if let Some(status) = status(state) {
        toolbar = toolbar.child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .single_line(),
        );
    }

    col()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(toolbar)
        .child(viewport(state, &theme))
}

fn status(state: &AppState) -> Option<String> {
    if let Some(error) = &state.page.error {
        Some(error.clone())
    } else if state.page.loading {
        Some("Loading…".to_owned())
    } else {
        None
    }
}

/// The page's latest frame, drawn one device pixel per pixel from the top-left. A frame from
/// before a resize keeps its own size rather than being stretched.
fn viewport(state: &AppState, theme: &Theme) -> El {
    let [width, height] = state.page.frame_size;
    let frame = el()
        .key("page-frame")
        .w(width)
        .h(height)
        .pixels(&state.page.pixels);

    el().key(VIEWPORT)
        .w_full()
        .grow(1.0)
        .fill(theme.bg)
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
