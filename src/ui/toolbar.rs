// SPDX-License-Identifier: GPL-3.0-only

//! The bar above the page: the address field, what the page is doing, and the appearance picker.

use rune::tok::type_;
use rune::{Cx, El, el, row, text, text_input};

use super::theme::{self, Theme};
use super::{VIEWPORT, appearance_picker};
use crate::app::AppState;

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    let mut toolbar = row()
        .key("toolbar")
        .w_full()
        .h(theme::TOOLBAR_HEIGHT)
        .gap(theme::TOOLBAR_GAP)
        .pl(theme::TOOLBAR_PADDING + cx.titlebar_inset())
        .pr(theme::TOOLBAR_PADDING)
        .center_y()
        .child(address(state, theme));
    if let Some(status) = status(state) {
        toolbar = toolbar.child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .single_line(),
        );
    }
    toolbar.child(appearance_picker::view(state.appearance, theme))
}

fn address(state: &AppState, theme: &Theme) -> El {
    let field = text_input(&state.address, type_::BODY)
        .key("address")
        .single_line()
        .w_full()
        .px(theme::FIELD_PADDING)
        .color(theme.text)
        .placeholder("Search or enter address", theme.text_secondary)
        .label("Address or search")
        .on_edit(|state: &mut AppState| &mut state.address)
        .on_submit(|state: &mut AppState, tx| {
            state.navigate();
            tx.focus(VIEWPORT, false);
        });
    // The field takes the toolbar's spare width, leaving room for the status and the picker.
    el().key("address-container")
        .grow(1.0)
        .h(theme::CONTROL_HEIGHT)
        .px(theme::CONTROL_PADDING)
        .center_y()
        .radius(theme::CONTROL_RADIUS)
        .fill(theme.control_off)
        .child(field)
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
