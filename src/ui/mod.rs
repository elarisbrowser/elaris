// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a toolbar above the web page. Each part is its own module; colours and
//! measurements live in `theme`.

mod appearance_picker;
mod theme;
mod toolbar;
mod viewport;

use rune::{Cx, El, col};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(dark);
    state.page.note_scale(cx.env().scale);
    state.page.note_dark(dark);

    col()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(toolbar::view(state, &theme, cx))
        .child(viewport::view(state, &theme))
}
