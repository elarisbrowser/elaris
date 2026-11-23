// SPDX-License-Identifier: GPL-3.0-only

//! The shared highlight that follows the pointer between adjacent browser controls.

use rune::{Color, Cx, El, Spring, Tx, el};

use crate::app::AppState;
use crate::ui::theme::{self, Theme};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Group {
    Controls,
    Tabs,
    Favourites,
    HorizontalTabs,
}

pub(crate) struct Target {
    key: String,
    group: Group,
}

/// The shared highlight is the control's background, so the control drops its own press overlay;
/// otherwise the overlay shrinks with `press_scale` inside the highlight and reads as a second one.
pub(crate) fn attach(element: El, key: &str, group: Group) -> El {
    attach_with(element, key, group, |_, _, _| {})
}

/// [`attach`], also running `also` on each hover change, for a control with its own hover state.
pub(crate) fn attach_with(
    element: El,
    key: &str,
    group: Group,
    also: impl Fn(&mut AppState, bool, &mut Tx) + 'static,
) -> El {
    let key = key.to_owned();
    element
        .press(Color::TRANSPARENT)
        .on_hover_change(move |state: &mut AppState, inside, tx| {
            update(state, &key, group, inside, tx);
            also(state, inside, tx);
        })
}

pub(crate) fn update(
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

pub(crate) fn view(
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
            Some(crate::ui::horizontal_tabs::SCROLL),
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
            .press_scale_of(&target.key)
            .radius(radius)
            .continuous_corners()
            .fill(theme.hover)
            .inert()
            .a11y_hidden(),
    )
}

pub(crate) fn selection(
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
