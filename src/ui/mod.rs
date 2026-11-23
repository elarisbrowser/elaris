// SPDX-License-Identifier: GPL-3.0-only

//! The window's interface: a sidebar beside the active tab's page. Each part is its own module;
//! colours and measurements live in `theme`.

mod bookmarks;
pub(crate) mod components;
mod downloads;
mod find;
pub(crate) mod form_picker;
pub(crate) mod horizontal_tabs;
pub(crate) mod icons;
pub(crate) mod internal;
mod mini;
mod omnibox;
mod page_dialog;
pub(crate) mod peek;
pub(crate) mod permission_prompt;
mod sidebar;
mod tab_preview;
mod toast;
mod translate;
pub(crate) use sidebar::hover::{
    dismiss as dismiss_tab_preview, displayed as displayed_tab_preview, poll as poll_tab_preview,
};
pub(crate) mod site_control_style;
pub(crate) mod site_permissions;
pub(crate) mod tab_switcher;
pub(crate) mod theme;
pub(crate) mod viewport;
pub(crate) use viewport::menu::PageMenuState;

pub(crate) use sidebar::drag::{
    DragVisual, MoveResult as TabDragMoveResult, TabDragState, TabDropTarget,
};
pub(crate) use sidebar::header::settle_zoom_chip;
pub(crate) use sidebar::resize::settle as settle_sidebar;
pub(crate) use sidebar::settle_copy_link;
pub(crate) use sidebar::{SidebarState, toggle as toggle_sidebar};
pub(crate) use site_permissions::settle_site_action;
pub(crate) use theme::{TAB_REORDER_MOTION, TRAFFIC_LIGHTS, WINDOW_MIN_SIZE};

use rune::SystemMaterial;
use rune::{Cx, El, col, row};

use crate::app::AppState;
use elaris_core::appearance::GlassStyle;

/// The focus key of the page viewport.
pub(crate) const VIEWPORT: &str = "viewport";
pub(crate) const HISTORY_SEARCH: &str = "history-search";
pub(crate) const HISTORY_SCROLL: &str = "history-scroll";
pub(crate) const TASK_MANAGER_SEARCH: &str = "task-manager-search";
pub(crate) const BOOKMARKS_SEARCH: &str = "bookmarks-search";
pub(crate) const BOOKMARKS_SCROLL: &str = "bookmarks-scroll";
pub(crate) const TASK_MANAGER_SCROLL: &str = "task-manager-scroll";
pub(crate) const FIND_INPUT: &str = "find-input";
#[cfg(target_os = "macos")]
pub(crate) use sidebar::tabs::floating_drag_preview;
pub(crate) use theme::FIND_MOTION;

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    #[cfg(elaris_ui = "macos")]
    crate::platform::macos::menu::refresh(state);
    if state.is_mini() {
        return mini::view(state, cx);
    }
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
    #[cfg(elaris_ui = "macos")]
    cx.set_titlebar(theme::native_titlebar(if horizontal {
        theme::HORIZONTAL_STRIP_HEIGHT
    } else {
        theme::HEADER_HEIGHT
    }));
    let window_size = cx
        .layout_size("window")
        .unwrap_or([WINDOW_MIN_SIZE.0, WINDOW_MIN_SIZE.1]);
    let mut window = window_surface(state, &theme, horizontal, window_size, cx.env().scale);
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
            .children(bookmarks::bar(state, &theme, &mut overlays))
            .child(viewport::view(state, &theme, &mut overlays, reduced_motion));
    } else {
        let sidebar = sidebar::view(state, &theme, &mut overlays, reduced_motion);
        let page = viewport::view(state, &theme, &mut overlays, reduced_motion);
        window = if state.sidebar_on_right() {
            window.child(page).child(sidebar)
        } else {
            window.child(sidebar).child(page)
        }
        .child(sidebar::header::view(state, &theme, cx));
        if !state.sidebar.collapsed {
            let window_width = cx.layout_size("window").map_or(0.0, |size| size[0]);
            window = window.child(sidebar::resize::handle(state, window_width));
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
    sidebar::tabs::context_menu(state, &theme, &mut overlays);
    sidebar::header::history_menu(state, &theme, &mut overlays);
    #[cfg(elaris_ui = "linux")]
    if !horizontal && state.titlebar.reveals_on_hover() {
        overlays.push(sidebar::header::hover_zone());
    }
    let height = cx
        .layout_size("window")
        .map_or(WINDOW_MIN_SIZE.1, |size| size[1]);
    overlays.extend(components::hover_highlight::view(
        state,
        cx,
        &theme,
        reduced_motion,
    ));
    overlays.extend(tab_switcher::view(state, &theme, height, reduced_motion));
    overlays.extend(toast::view(
        &state.toasts,
        &theme,
        window_size[0],
        reduced_motion,
    ));
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

/// The window's background, glass included, laid out as a column or a row.
fn window_surface(
    state: &AppState,
    theme: &theme::Theme,
    vertical: bool,
    window_size: [f32; 2],
    scale: f32,
) -> El {
    let mut window = if vertical { col() } else { row() }
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.window);
    if state.settings.glass_enabled {
        window = window
            .system_material(glass_material(state.settings.glass_style))
            .child(
                rune::el()
                    .key("window-tint")
                    .absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .fill(theme.window),
            );
        if state.settings.grain > 0 {
            window = window.child(components::grain::layer(
                state.settings.grain,
                window_size[0],
                window_size[1],
                scale,
            ));
        }
    }
    window
}

fn glass_material(style: GlassStyle) -> SystemMaterial {
    match style {
        GlassStyle::Frosted => SystemMaterial::Popover,
        GlassStyle::Liquid => SystemMaterial::Glass,
    }
}
