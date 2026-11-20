// SPDX-License-Identifier: GPL-3.0-only

//! The shared highlight that follows the pointer between adjacent browser controls.

use rune::{Cx, El, Spring, Tx, el};

use super::theme::{self, Theme};
use crate::app::AppState;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Group {
    Controls,
    Tabs,
    Favourites,
    HorizontalTabs,
}

pub(crate) struct Target {
    key: String,
    group: Group,
}

pub(super) fn attach(element: El, key: &str, group: Group) -> El {
    let key = key.to_owned();
    element.on_hover_change(move |state: &mut AppState, inside, tx| {
        update(state, &key, group, inside, tx);
    })
}

pub(super) fn update(
    state: &mut AppState,
    key: &str,
    group: Group,
    inside: bool,
    tx: &mut Tx,
) -> bool {
    if inside {
        state.hover_highlight = Some(Target {
            key: key.to_owned(),
            group,
        });
    } else if state
        .hover_highlight
        .as_ref()
        .is_some_and(|target| target.key == key)
    {
        state.hover_highlight = None;
    } else {
        return false;
    }
    tx.animated();
    true
}

pub(super) fn view(
    state: &AppState,
    cx: &Cx<AppState>,
    theme: &Theme,
    reduced: bool,
) -> Option<El> {
    let target = state.hover_highlight.as_ref()?;
    if state.tab_drag.has_preview()
        || !state.browser.window_focused()
        || state.tab_switcher.is_some()
        || state.context_menu().is_some()
        || state.page_menu.open()
    {
        return None;
    }
    let [mut x, mut y, mut width, mut height] = cx.window_layout(&target.key)?;
    let (key, clip, radius) = match target.group {
        Group::Controls => {
            let band = [
                "header",
                "horizontal-navigation",
                "horizontal-tab-strip",
                "sidebar-bottom-controls",
                "find-bar",
            ]
            .into_iter()
            .find(|key| {
                cx.window_layout(key).is_some_and(|[left, top, w, h]| {
                    x >= left && x + width <= left + w && y >= top && y + height <= top + h
                })
            })
            .unwrap_or(&target.key);
            (
                format!("controls-hover-{band}"),
                None,
                theme::ICON_BUTTON_RADIUS,
            )
        }
        Group::Tabs => ("tabs-hover".into(), Some("tabs-scroll"), theme::ROW_RADIUS),
        Group::Favourites => (
            "favourite-hover".into(),
            Some("sidebar"),
            theme::FAVOURITE_TILE_RADIUS,
        ),
        Group::HorizontalTabs => (
            "horizontal-tabs-hover".into(),
            Some(super::horizontal_tabs::SCROLL),
            theme::ROW_RADIUS,
        ),
    };
    if let Some(clip) = clip {
        let [left, top, w, h] = cx.window_layout(clip)?;
        let right = (x + width).min(left + w);
        let bottom = (y + height).min(top + h);
        x = x.max(left);
        y = y.max(top);
        width = right - x;
        height = bottom - y;
    }
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let motion = if reduced {
        Spring::NONE
    } else {
        theme::HIGHLIGHT_MOTION
    };
    Some(
        el().key(key)
            .absolute(0.0, 0.0)
            .size(width, height)
            .offset(x, y)
            .offset_motion(motion)
            .motion(motion)
            .radius(radius)
            .continuous_corners()
            .fill(theme.hover)
            .inert()
            .a11y_hidden(),
    )
}

pub(super) fn selection(
    key: &str,
    y: f32,
    height: f32,
    radius: f32,
    theme: &Theme,
    reduced: bool,
) -> El {
    el().key(key)
        .pin(Some(0.0), Some(0.0), Some(0.0), None)
        .h(height)
        .offset(0.0, y)
        .offset_motion(if reduced {
            Spring::NONE
        } else {
            theme::HIGHLIGHT_MOTION
        })
        .radius(radius)
        .fill(theme.overlay_selected)
        .inert()
        .a11y_hidden()
}

// Rune Kit focuses the menu item under the pointer and uses that same focus for arrow keys.
pub(crate) fn poll_context_menu(state: &mut AppState, tx: &mut Tx) -> bool {
    let scope = if state.context_menu().is_some() {
        "sidebar-context-menu-item"
    } else if state.page_menu.open() {
        "page-context-menu-item"
    } else if state.omnibox.search_engine_menu_open {
        "search-engine-menu-item"
    } else {
        ""
    };
    let item = tx
        .focused()
        .filter(|key| !scope.is_empty() && key.starts_with(scope));
    if item == state.context_menu_highlight.as_deref() {
        return false;
    }
    state.context_menu_highlight = item.map(str::to_owned);
    tx.animated();
    true
}

pub(in crate::ui) fn build_context_menu(
    state: &AppState,
    menu: rune_kit::Menu,
    theme: &Theme,
    layer: &mut Vec<El>,
) {
    let _ = build_highlight_menu(state, menu, None, theme, layer);
}

pub(in crate::ui) fn build_highlight_menu(
    state: &AppState,
    menu: rune_kit::Menu,
    surface_key: Option<&str>,
    theme: &Theme,
    layer: &mut Vec<El>,
) -> El {
    let mut kit = theme.kit(theme.overlay_panel);
    if state.context_menu_highlight.is_some() {
        kit.c.hover = rune::Color::TRANSPARENT;
    }
    let before = layer.len();
    let trigger = menu.build(&kit, layer);
    if layer.len() > before
        && let Some(surface) = layer.pop()
    {
        let surface = surface.on_pointer_move_local(|_: &mut AppState, _, tx| {
            tx.wake_after(std::time::Duration::ZERO);
            tx.live_only();
        });
        layer.push(if let Some(key) = surface_key {
            surface.key(key)
        } else {
            surface
        });
    }
    trigger
}

pub(super) fn context_menu_view(
    state: &AppState,
    cx: &Cx<AppState>,
    theme: &Theme,
    reduced: bool,
) -> Option<El> {
    let item = state.context_menu_highlight.as_deref()?;
    let surface = if state.context_menu().is_some() {
        "sidebar-context-menu-menu"
    } else if state.page_menu.open() {
        "page-context-menu-menu"
    } else if state.omnibox.search_engine_menu_open {
        "search-engine-menu-menu"
    } else {
        return None;
    };
    let [left, top, w, h] = cx.window_layout(surface)?;
    let [x, y, width, height] = cx.window_layout(item)?;
    let x0 = x.max(left);
    let y0 = y.max(top);
    let width = (x + width).min(left + w) - x0;
    let height = (y + height).min(top + h) - y0;
    (width > 0.0 && height > 0.0).then(|| {
        el().key("context-menu-hover-highlight")
            .absolute(0.0, 0.0)
            .size(width, height)
            .offset(x0, y0)
            .offset_motion(if reduced {
                Spring::NONE
            } else {
                theme::HIGHLIGHT_MOTION
            })
            .radius(rune::tok::radius::SM)
            .fill(theme.hover)
            .inert()
            .a11y_hidden()
    })
}
