// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod controls;
mod icons;
mod omnibox;
mod sidebar;
mod theme;
pub(crate) mod viewport;
pub(crate) use viewport::menu::PageMenuState;

pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use sidebar::{SidebarState, toggle as toggle_sidebar};
pub(crate) use theme::{TRAFFIC_LIGHTS, WINDOW_MIN_SIZE};

#[cfg(target_os = "linux")]
use rune::col;
use rune::{Cx, El, SystemMaterial, el, row};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(dark);
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);

    #[cfg(target_os = "linux")]
    let mut window = {
        let mut content = row()
            .key("window-content")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(sidebar::view(state, &theme))
            .child(viewport::view(state, &theme));
        if !state.sidebar.collapsed {
            content = content.child(sidebar::resize::handle(state.sidebar.width));
        }
        let mut window = col()
            .key("window")
            .w_full()
            .h_full()
            .fill(theme.window)
            .system_material(SystemMaterial::Glass)
            .child(window_glass_tint(&theme));
        window
            .child(sidebar::header::view(state, &theme, cx))
            .child(content)
    };
    #[cfg(not(target_os = "linux"))]
    let mut window = {
        // The header and resize handle sit over the sidebar and page on macOS.
        let mut window = row()
            .key("window")
            .w_full()
            .h_full()
            .fill(theme.window)
            .system_material(SystemMaterial::Glass)
            .child(window_glass_tint(&theme));
        window = window
            .child(sidebar::view(state, &theme))
            .child(viewport::view(state, &theme))
            .child(sidebar::header::view(state, &theme, cx));
        if !state.sidebar.collapsed {
            window = window.child(sidebar::resize::handle(state.sidebar.width));
        }
        window
    };
    let mut overlays = Vec::new();
    if let Some(overlay) = omnibox::view(
        state,
        &theme,
        cx.layout_size("window")
            .map_or(WINDOW_MIN_SIZE.1, |size| size[1]),
        cx.is_reduced_motion(),
        &mut overlays,
    ) {
        window = window.child(overlay);
    }
    viewport::menu::view(state, &theme, &mut overlays);
    sidebar::context_menu(state, &theme, &mut overlays);
    #[cfg(target_os = "linux")]
    overlays.push(sidebar::header::hover_zone());
    rune_kit::root(window, overlays)
}

fn window_glass_tint(theme: &theme::Theme) -> El {
    el().key("window-glass-tint")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(theme.window_glass_tint)
}
