// SPDX-License-Identifier: GPL-3.0-only

//! Site Control Centre for Linux, hosted as a Rune popover below the URL bar.

mod content;
mod menus;

use rune::{Cx, El, KeyEvent, el, icon};

use crate::app::AppState;
use crate::site_controls::SiteControlPresentation;
use crate::ui::icons;
use crate::ui::site_control_style as design;
use crate::ui::theme::{self, Theme};

pub(crate) fn view(
    state: &AppState,
    cx: &Cx<AppState>,
    theme: &Theme,
    layer: &mut Vec<El>,
    reduced_motion: bool,
) {
    if !state.site_permissions_open {
        return;
    }
    let presentation = SiteControlPresentation::from_state(state);
    let anchor = state.site_controls_anchor.unwrap_or([
        state.sidebar.width - 38.0,
        theme::HEADER_HEIGHT + theme::FIELD_HEIGHT,
        22.0,
        22.0,
    ]);
    let center = anchor[0] + anchor[2] / 2.0;
    let x = (center - design::WIDTH / 2.0).max(10.0);
    let x = if state.horizontal_tabs() {
        x.min(
            cx.layout_size("window")
                .map_or(x, |size| (size[0] - design::WIDTH - 10.0).max(10.0)),
        )
    } else {
        x
    };
    let arrow_y = anchor[1] + anchor[3] - 1.0;
    let y = arrow_y + 8.0;

    layer.push(
        el().key("site-controls-dismiss-area")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .on_press(|state: &mut AppState| {
                state.site_permissions_open = false;
                state.permission_menu = None;
                state.autoplay_menu_open = false;
                state.site_overflow_open = false;
            }),
    );
    layer.push(
        icon(&icons::SITE_POPOVER_ARROW, design::WIDTH)
            .key("site-controls-arrow")
            .size(20.0, 10.0)
            .absolute(
                if state.horizontal_tabs() {
                    center - 10.0
                } else {
                    x + design::WIDTH / 2.0 - 10.0
                },
                arrow_y,
            )
            .color(theme.overlay_panel),
    );
    layer.push(content::popover(state, theme, &presentation, reduced_motion).absolute(x, y));
    menus::permission_menu(state, theme, layer);
    menus::autoplay_menu(state, theme, layer, &presentation);
    menus::overflow_menu(state, theme, layer, &presentation);
}

pub(crate) fn escape(state: &mut AppState, event: &KeyEvent) -> bool {
    menus::escape(state, event)
}
