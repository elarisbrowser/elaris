// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar: the header beside the window controls, the address field and status, the open
//! tabs, and the appearance button.
//!
//! - `header`: the titlebar row and its navigation buttons.
//! - `tabs`: the tab list and the new-tab button.

mod header;
mod tabs;

use rune::tok::type_;
use rune::{Cx, El, col, el, text, text_input};

use super::controls::row_button;
use super::theme::{self, Theme};
use super::{ADDRESS, icons};
use crate::app::{self, AppState};
use crate::appearance::Appearance;

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    col()
        .key("sidebar")
        .w(theme::SIDEBAR_WIDTH)
        .h_full()
        .child(header::view(state, theme, cx))
        .child(body(state, theme))
}

/// Everything below the header, inset from the sidebar's edges.
fn body(state: &AppState, theme: &Theme) -> El {
    let mut body = col()
        .key("sidebar-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(theme::SIDEBAR_PADDING)
        .pb(theme::SIDEBAR_PADDING)
        .gap(theme::SIDEBAR_GAP)
        .child(address(state, theme));
    if let Some(status) = state.status() {
        body = body.child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(theme.text_muted)
                .px(theme::FIELD_PADDING)
                .truncate(),
        );
    }
    body.child(tabs::view(state, theme))
        .child(appearance(state.appearance, theme))
}

fn address(state: &AppState, theme: &Theme) -> El {
    let field = text_input(&state.address, type_::BODY)
        .key(ADDRESS)
        .single_line()
        .w_full()
        .px(theme::FIELD_PADDING)
        .color(theme.text)
        .placeholder("Search or enter address", theme.text_muted)
        .label("Address")
        .on_edit(|state: &mut AppState| &mut state.address)
        .on_submit(app::submit_address);
    el().key("address-container")
        .w_full()
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .radius(theme::FIELD_RADIUS)
        .fill(theme.field)
        .child(field)
}

/// A quiet button at the foot of the sidebar that steps through System, Light and Dark.
fn appearance(current: Appearance, theme: &Theme) -> El {
    let glyph = match current {
        Appearance::System => &icons::SYSTEM,
        Appearance::Light => &icons::LIGHT,
        Appearance::Dark => &icons::DARK,
    };
    let label = format!("Appearance: {}", current.label());
    row_button(
        "appearance",
        glyph,
        current.label(),
        theme.text_muted,
        theme,
    )
    .label(label.clone())
    .help(format!("{label}. Click to change."))
    .on_press(|state: &mut AppState| state.appearance = state.appearance.next())
}
