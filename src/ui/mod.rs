// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod controls;
mod icons;
mod sidebar;
mod theme;
mod viewport;

pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use theme::{HEADER_HEIGHT, SIDEBAR_TOGGLE, SIDEBAR_WIDTH};

use rune::{Cx, El, row};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
/// The focus key of the address field.
pub(crate) const ADDRESS: &str = "address";

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(dark);
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);

    // The header and the resize handle come last, so they sit over the sidebar and the page.
    let window = row()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.window)
        .child(sidebar::view(state, &theme))
        .child(viewport::view(state, &theme))
        .child(sidebar::header::view(state, &theme, cx));
    if state.sidebar_collapsed {
        window
    } else {
        window.child(sidebar::resize::handle(state.sidebar_width))
    }
}
