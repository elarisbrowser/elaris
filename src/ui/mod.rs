// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod icons;
mod sidebar;
mod theme;
mod viewport;

pub(crate) use theme::HEADER_HEIGHT;

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

    row()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.window)
        .child(sidebar::view(state, &theme, cx))
        .child(viewport::view(state, &theme))
}
