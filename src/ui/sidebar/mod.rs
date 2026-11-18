// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar: the header beside the window controls, the search launcher and status, the open
//! tabs, and the appearance button.
//!
//! - `header`: the titlebar row, its sidebar toggle and navigation buttons. The window draws it
//!   over the sidebar, so it stays when the sidebar collapses.
//! - `tabs`: the tab list and the new-tab button.
//! - `resize`: dragging the sidebar's edge.

pub(crate) mod header;
pub(crate) mod resize;
mod tabs;

use rune::tok::type_;
use rune::{El, Tx, col, icon, text};
pub(crate) use tabs::context_menu;

use super::controls::row_button;
use super::icons;
use super::theme::{self, Theme};
use crate::app::{self, AppState};
use crate::appearance::Appearance;

/// How the sidebar is laid out: what the window shows of it, not of the browser.
pub(crate) struct SidebarState {
    /// Its width in points, which dragging its edge changes.
    pub(crate) width: f32,
    /// Released past its width's limits, and yet to spring back.
    pub(crate) settling: bool,
    /// Hidden, leaving the page the window's width.
    pub(crate) collapsed: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            width: theme::SIDEBAR_WIDTH,
            settling: false,
            collapsed: false,
        }
    }
}

/// Shows or hides the sidebar, springing the page to its new width.
pub(crate) fn toggle(state: &mut AppState, tx: &mut Tx) {
    state.sidebar.collapsed = !state.sidebar.collapsed;
    tx.animated();
    tx.motion(theme::SIDEBAR_TOGGLE);
}

/// The sidebar below the header's room. Collapsing springs its width to zero; its contents
/// keep their width meanwhile, clipped, so they slide away rather than reflow.
pub(crate) fn view(state: &AppState, theme: &Theme) -> El {
    let collapsed = state.sidebar.collapsed;
    let sidebar = col()
        .key("sidebar")
        .w(if collapsed { 0.0 } else { state.sidebar.width })
        .h_full()
        .clip_soft(0.0)
        .pt(theme::HEADER_HEIGHT)
        .child(body(state, theme));
    if collapsed {
        sidebar.a11y_hidden()
    } else {
        sidebar
    }
}

/// Everything below the header, inset from the sidebar's edges.
fn body(state: &AppState, theme: &Theme) -> El {
    let mut body = col()
        .key("sidebar-body")
        .w(state.sidebar.width)
        .grow(1.0)
        .min_h(0.0)
        .px(theme::SIDEBAR_PADDING)
        .pb(theme::SIDEBAR_PADDING)
        .gap(theme::SIDEBAR_GAP)
        .child(search_launcher(state, theme));
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

fn search_launcher(state: &AppState, theme: &Theme) -> El {
    let address = state
        .browser
        .active_tab()
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty() && *url != "about:blank");
    let label = address.unwrap_or("Search or type a URL");
    row_button(
        "omnibox-launcher",
        icon(&icons::SEARCH, theme::ICON).color(theme.text_muted),
        label,
        if address.is_some() {
            theme.text
        } else {
            theme.text_muted
        },
        theme,
    )
    .h(theme::FIELD_HEIGHT)
    .radius(theme::FIELD_RADIUS)
    .continuous_corners()
    .fill(theme.address_field)
    .label("Search or type a URL")
    .help(address.unwrap_or("Open search and address suggestions"))
    .shortcut_hint("cmd+l")
    .focusable(true)
    .on_press_tx(app::open_from_launcher)
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
        icon(glyph, theme::ICON).color(theme.text_muted),
        current.label(),
        theme.text_muted,
        theme,
    )
    .label(label.clone())
    .help(format!("{label}. Click to change."))
    .on_press(|state: &mut AppState| state.appearance = state.appearance.next())
}
