// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod controls;
mod downloads;
mod find;
mod icons;
pub(crate) mod internal;
mod omnibox;
pub(crate) mod peek;
mod sidebar;
mod tab_preview;
pub(crate) use sidebar::hover::{
    dismiss as dismiss_tab_preview, displayed as displayed_tab_preview, poll as poll_tab_preview,
};
pub(crate) mod site_control_style;
#[cfg(target_os = "linux")]
pub(crate) mod site_permissions;
pub(crate) mod tab_switcher;
mod theme;
pub(crate) mod viewport;
pub(crate) use viewport::menu::PageMenuState;

pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use sidebar::{
    SidebarState, TabDragMoveResult, TabDragState, TabDropTarget, toggle as toggle_sidebar,
};
pub(crate) use theme::{TAB_REORDER_MOTION, TRAFFIC_LIGHTS, WINDOW_MIN_SIZE};

use rune::SystemMaterial;
#[cfg(target_os = "linux")]
use rune::col;
use rune::{Cx, El, row};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) const HISTORY_SEARCH: &str = "history-search";
pub(crate) const HISTORY_SCROLL: &str = "history-scroll";
pub(crate) const FIND_INPUT: &str = "find-input";
pub(crate) use theme::FIND_MOTION;
pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    #[cfg(target_os = "macos")]
    crate::menu_macos::refresh(state);
    let dark = state.settings.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(
        dark,
        state.settings.glass_enabled,
        state.settings.tint_opacity,
        state.settings.accent_color,
    );
    let reduced_motion = cx.system_reduced_motion() || cx.is_reduced_motion();
    state.sidebar.hover.reduced.set(reduced_motion);
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);
    let mut overlays = Vec::new();

    #[cfg(target_os = "linux")]
    let mut window = {
        let mut content = row()
            .key("window-content")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(sidebar::view(state, &theme, &mut overlays, reduced_motion))
            .child(viewport::view(state, &theme, &mut overlays, reduced_motion));
        if !state.sidebar.collapsed {
            content = content.child(sidebar::resize::handle(state.sidebar.width));
        }
        let mut window = col().key("window").w_full().h_full().fill(theme.window);
        if state.settings.glass_enabled {
            window = window.system_material(SystemMaterial::Glass).child(
                rune::el()
                    .key("window-tint")
                    .absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .fill(theme.window),
            );
        }
        window
            .child(sidebar::header::view(state, &theme, cx))
            .child(content)
    };
    #[cfg(not(target_os = "linux"))]
    let mut window = {
        // The header and resize handle sit over the sidebar and page on macOS.
        let mut window = row().key("window").w_full().h_full().fill(theme.window);
        if state.settings.glass_enabled {
            window = window.system_material(SystemMaterial::Glass).child(
                rune::el()
                    .key("window-tint")
                    .absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .fill(theme.window),
            );
        }
        window = window
            .child(sidebar::view(state, &theme, &mut overlays, reduced_motion))
            .child(viewport::view(state, &theme, &mut overlays, reduced_motion))
            .child(sidebar::header::view(state, &theme, cx));
        if !state.sidebar.collapsed {
            window = window.child(sidebar::resize::handle(state.sidebar.width));
        }
        window
    };
    if state.browser.peek.is_some() {
        window = window.child(peek::backdrop());
    }
    window = window.child(peek::layer(state, &theme, reduced_motion));
    if state.browser.peek.is_none() && state.tab_switcher.is_none() {
        if let Some(find) = find::view(state, &theme, reduced_motion, viewport::page_origin(state))
        {
            overlays.push(find);
        }
        if let Some(overlay) = omnibox::view(
            state,
            &theme,
            cx.layout_size("window")
                .map_or(WINDOW_MIN_SIZE.1, |size| size[1]),
            reduced_motion,
            &mut overlays,
        ) {
            window = window.child(overlay);
        }
    }
    overlays.extend(tab_preview::view(state, cx, &theme, reduced_motion));
    viewport::menu::view(state, &theme, &mut overlays);
    sidebar::context_menu(state, &theme, &mut overlays);
    #[cfg(target_os = "linux")]
    site_permissions::view(state, &theme, &mut overlays, reduced_motion);
    #[cfg(target_os = "linux")]
    overlays.push(sidebar::header::hover_zone());
    let height = cx
        .layout_size("window")
        .map_or(WINDOW_MIN_SIZE.1, |size| size[1]);
    overlays.extend(tab_switcher::view(state, &theme, height, reduced_motion));
    rune_kit::root(window, overlays)
}
