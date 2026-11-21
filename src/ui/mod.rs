// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod controls;
mod downloads;
mod find;
pub(crate) mod form_picker;
pub(crate) mod horizontal_tabs;
pub(crate) mod hover_highlight;
mod icons;
pub(crate) mod internal;
mod omnibox;
mod page_dialog;
pub(crate) mod peek;
pub(crate) mod permission_prompt;
mod sidebar;
mod tab_preview;
pub(crate) use sidebar::hover::{
    dismiss as dismiss_tab_preview, displayed as displayed_tab_preview, poll as poll_tab_preview,
};
pub(crate) mod site_control_style;
pub(crate) mod site_permissions;
pub(crate) mod tab_switcher;
pub(crate) mod theme;
pub(crate) mod viewport;
pub(crate) use viewport::menu::PageMenuState;

pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use sidebar::settle_copy_link;
pub(crate) use sidebar::{
    DragVisual, SidebarState, TabDragMoveResult, TabDragState, TabDropTarget,
    toggle as toggle_sidebar,
};
pub(crate) use theme::{TAB_REORDER_MOTION, TRAFFIC_LIGHTS, WINDOW_MIN_SIZE};

use rune::SystemMaterial;
use rune::{Cx, El, col, row};

use crate::app::AppState;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) const HISTORY_SEARCH: &str = "history-search";
pub(crate) const HISTORY_SCROLL: &str = "history-scroll";
pub(crate) const FIND_INPUT: &str = "find-input";
#[cfg(target_os = "macos")]
pub(crate) use sidebar::floating_drag_preview;
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
    if state.settings.reduce_motion {
        cx.reduced_motion(true);
    }
    let reduced_motion = cx.is_reduced_motion();
    state.sidebar.hover.reduced.set(reduced_motion);
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);
    let mut overlays = Vec::new();

    let fullscreen = state.browser.fullscreen_tab().is_some();
    let horizontal = state.horizontal_tabs();
    #[cfg(target_os = "macos")]
    cx.set_titlebar(theme::native_titlebar(if horizontal {
        theme::HORIZONTAL_STRIP_HEIGHT
    } else {
        theme::HEADER_HEIGHT
    }));
    let mut window = if horizontal || cfg!(target_os = "linux") {
        col()
    } else {
        row()
    }
    .key("window")
    .w_full()
    .h_full()
    .fill(theme.window);
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
    if fullscreen {
        window = window.child(viewport::view(state, &theme, &mut overlays, reduced_motion));
    } else if horizontal {
        window = window
            .child(horizontal_tabs::view(
                state,
                cx,
                &theme,
                &mut overlays,
                reduced_motion,
            ))
            .child(viewport::view(state, &theme, &mut overlays, reduced_motion));
    } else {
        #[cfg(target_os = "linux")]
        {
            window = window.child(sidebar::header::view(state, &theme, cx));
        }
        let sidebar = sidebar::view(state, &theme, &mut overlays, reduced_motion);
        let page = viewport::view(state, &theme, &mut overlays, reduced_motion);
        #[cfg(target_os = "linux")]
        {
            let mut content = row()
                .key("window-content")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(sidebar)
                .child(page);
            if !state.sidebar.collapsed {
                content = content.child(sidebar::resize::handle(state.sidebar.width));
            }
            window = window.child(content);
        }
        #[cfg(not(target_os = "linux"))]
        {
            window = window
                .child(sidebar)
                .child(page)
                .child(sidebar::header::view(state, &theme, cx));
            if !state.sidebar.collapsed {
                window = window.child(sidebar::resize::handle(state.sidebar.width));
            }
        }
    }
    if state.browser.peek.is_some() {
        window = window.child(peek::backdrop());
    }
    window = window.child(peek::layer(state, &theme, reduced_motion));
    if state.browser.peek.is_none() && state.tab_switcher.is_none() {
        if let Some(find) = find::view(state, &theme, reduced_motion, viewport::page_origin(state))
        {
            overlays.push(find);
        }
        if !horizontal
            && let Some(overlay) = omnibox::view(
                state,
                &theme,
                cx.layout_size("window")
                    .map_or(WINDOW_MIN_SIZE.1, |size| size[1]),
                reduced_motion,
                &mut overlays,
            )
        {
            window = window.child(overlay);
        }
    }
    overlays.extend(tab_preview::view(state, cx, &theme, reduced_motion));
    viewport::menu::view(state, &theme, &mut overlays);
    sidebar::context_menu(state, &theme, &mut overlays);
    sidebar::header::history_menu(state, &theme, &mut overlays);
    #[cfg(target_os = "linux")]
    if !horizontal {
        overlays.push(sidebar::header::hover_zone());
    }
    let height = cx
        .layout_size("window")
        .map_or(WINDOW_MIN_SIZE.1, |size| size[1]);
    overlays.extend(hover_highlight::view(state, cx, &theme, reduced_motion));
    overlays.extend(hover_highlight::context_menu_view(
        state,
        cx,
        &theme,
        reduced_motion,
    ));
    overlays.extend(tab_switcher::view(state, &theme, height, reduced_motion));
    site_permissions::view(state, cx, &theme, &mut overlays, reduced_motion);
    overlays.extend(permission_prompt::view(
        state,
        &theme,
        viewport::page_origin(state),
    ));
    overlays.extend(permission_prompt::fullscreen_notice(state, &theme));
    form_picker::view(state, &theme, &mut overlays, reduced_motion);
    page_dialog::view(state, &theme, &mut overlays);
    let (hover_enter, hover_exit) = if reduced_motion {
        (rune::Anim::INSTANT, rune::Anim::INSTANT)
    } else {
        (theme::HOVER_ENTER, theme::HOVER_EXIT)
    };
    rune_kit::root(
        window.hover_motion_within(hover_enter, hover_exit),
        overlays,
    )
}
