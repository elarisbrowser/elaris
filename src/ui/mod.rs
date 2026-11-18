// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod controls;
mod icons;
mod omnibox;
mod sidebar;
mod theme;
mod viewport;

pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use sidebar::{SidebarState, toggle as toggle_sidebar};
pub(crate) use theme::{TRAFFIC_LIGHTS, WINDOW_MIN_SIZE};

use rune::{Cx, El, row};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(dark);
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);

    // The header and the resize handle come last, so they sit over the sidebar and the page.
    let mut window = row()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.window)
        .child(sidebar::view(state, &theme))
        .child(viewport::view(state, &theme))
        .child(sidebar::header::view(state, &theme, cx));
    if !state.sidebar.collapsed {
        window = window.child(sidebar::resize::handle(state.sidebar.width));
    }
    if let Some(overlay) = omnibox::view(
        state,
        &theme,
        cx.layout_size("window")
            .map_or(WINDOW_MIN_SIZE.1, |size| size[1]),
        cx.is_reduced_motion(),
    ) {
        window = window.child(overlay);
    }
    let mut overlays = Vec::new();
    sidebar::context_menu(state, &theme, &mut overlays);
    rune_kit::root(window, overlays)
}
