// SPDX-License-Identifier: GPL-3.0-only

//! Renders the live Peek card and its detached controls.

mod animation;
mod input;
#[cfg(test)]
mod tests;

use rune::tok::type_;
use rune::{Anim, El, Spring, SpringPair, Transition, col, el, text};

use crate::app::AppState;
use crate::browser::peek::{self, Destination};
use crate::engine::Surface;
use crate::tab::{Tab, TabId};

use super::components::controls;
use super::{icons, theme};

use animation::{AnimationKind, PeekAnimation, ease_out, opening_pose, promote};
pub(crate) use animation::{Presentation, advance, dismiss};
pub(super) use input::backdrop;

pub(crate) const PAGE: &str = "peek-page";
const ACTION_SPACING: f32 = 4.0;
const ACTION_REVEAL: f32 = 0.10;
const ACTION_FADE: f32 = 0.16;
const CLOSE_MOTION: Spring = Spring::new(0.15, 0.0);

fn card(state: &AppState, theme: &theme::Theme, reduced: bool, peek: &peek::PeekState) -> El {
    let page = &peek.page;
    let id = page.id();
    let mut content = input::webpage(id);
    let status = page_status(page);
    content = paint_page(content, &page.surface, status, theme);
    if status.is_none() {
        content = super::viewport::page_hover(content, page, theme);
    }
    if status.is_none()
        && state.browser.page_focused()
        && let Some(rect) = page.ime_caret
    {
        let [x, y, w, h] = rect.map(|value| value as f32 / state.browser.scale());
        content = content.child(
            el().key("peek-ime-caret")
                .absolute(x, y)
                .size(w.max(1.0), h.max(1.0))
                .ime_cursor(),
        );
    }

    let pose = opening_pose(
        state.split_rect,
        peek.origin,
        state.peek_presentation.opening_time(),
        reduced,
    );
    let mut card = panel(id, pose.rect, theme, 1.0)
        .scale(pose.scale)
        .scale_motion(Anim::INSTANT)
        .offset(pose.offset[0], pose.offset[1])
        .offset_motion(Anim::INSTANT);
    card = if reduced {
        card.transition(Transition::opacity().springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION)))
    } else {
        card.opacity(pose.opacity)
            .opacity_motion(Anim::INSTANT)
            .transition(Transition::none())
    };
    card.role("dialog").label("Peek").child(content)
}

pub(super) fn page_point(state: &AppState, point: [f32; 2], reduced: bool) -> Option<[f32; 2]> {
    let peek = state.browser.peek.as_ref()?;
    let pose = opening_pose(
        state.split_rect,
        peek.origin,
        state.peek_presentation.opening_time(),
        reduced,
    );
    let [x, y, _, _] = pose.drawn_rect();
    Some([
        state.split_rect[0] + x + point[0] * pose.scale,
        state.split_rect[1] - theme::VIEWPORT_INSET + y + point[1] * pose.scale,
    ])
}

fn page_status(page: &Tab) -> Option<&str> {
    page.error.as_deref().or_else(|| {
        (page.loading && page.committed_url == "about:blank").then_some("Loading preview…")
    })
}

fn paint_page(content: El, frame: &Surface, status: Option<&str>, theme: &theme::Theme) -> El {
    if let Some(status) = status {
        content
            .center()
            .child(text(status).font(type_::BODY).color(theme.text_muted))
    } else {
        super::viewport::page_surface(content, frame, theme::VIEWPORT_RADIUS)
    }
}

fn panel(id: TabId, [x, y, width, height]: [f32; 4], theme: &theme::Theme, chrome: f32) -> El {
    el().key(format!("peek-panel-{}", id.raw()))
        .absolute(x, y)
        .size(width, height)
        .fill(theme.page)
        .border(
            1.0,
            theme.overlay_border.alpha(theme.overlay_border.a * chrome),
        )
        .shadow(
            theme::OMNIBOX_SHADOW_Y,
            theme::OMNIBOX_SHADOW_BLUR,
            theme.overlay_shadow.alpha(theme.overlay_shadow.a * chrome),
        )
        .continuous_corners()
        .radius(theme::VIEWPORT_RADIUS)
        .clip_rounded(theme::VIEWPORT_RADIUS)
}

fn transition_card(
    state: &AppState,
    theme: &theme::Theme,
    animation: &PeekAnimation,
) -> Option<El> {
    let (frame, status, opacity, chrome) = match &animation.kind {
        AnimationKind::Dismiss { frame, status, .. } => (
            frame,
            status.as_deref(),
            1.0 - ease_out(animation.progress),
            1.0,
        ),
        AnimationKind::Promote(_) => {
            let peek = state
                .browser
                .peek
                .as_ref()
                .filter(|peek| peek.page.id() == animation.id)?;
            (
                &peek.page.surface,
                page_status(&peek.page),
                1.0,
                1.0 - animation.progress,
            )
        }
    };
    let content = el().key(PAGE).w_full().h_full();
    Some(
        panel(animation.id, animation.rect(state), theme, chrome)
            .opacity(opacity)
            .instant_changes()
            .transition(Transition::none())
            .inert()
            .a11y_hidden()
            .child(paint_page(content, frame, status, theme)),
    )
}

fn actions(state: &AppState, theme: &theme::Theme, opening_time: f32) -> El {
    let [_, _, width, height] = state.split_rect;
    let panel = peek::geometry([width, height]);
    let opacity = ((opening_time - ACTION_REVEAL) / ACTION_FADE).clamp(0.0, 1.0);
    col()
        .key("peek-actions")
        .absolute(
            panel.x + panel.width + peek::ACTION_GAP,
            theme::VIEWPORT_INSET + panel.y,
        )
        .w(peek::ACTION_SIZE)
        .gap(ACTION_SPACING)
        .opacity(opacity)
        .opacity_motion(Anim::INSTANT)
        .transition(Transition::opacity().springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION)))
        .role("group")
        .label("Peek actions")
        .child(
            controls::floating_icon_button(
                "peek-close",
                "Close Peek",
                &icons::CLOSE,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| {
                dismiss(state, tx);
            }),
        )
        .child(
            controls::floating_icon_button(
                "peek-open-tab",
                "Open in New Tab",
                &icons::OPEN_TAB,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| promote(state, tx, Destination::Tab)),
        )
        .child(
            controls::floating_icon_button(
                "peek-open-split",
                "Add to Split View",
                &icons::SPLIT_VIEW,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| promote(state, tx, Destination::Split)),
        )
}

/// Keep the card and its detached controls inside the content region.
pub(super) fn layer(state: &AppState, theme: &theme::Theme, reduced: bool) -> El {
    state.peek_presentation.note_reduced_motion(reduced);
    let [x, y, width, height] = state.split_rect;
    let mut layer = el()
        .key("peek-layer")
        .absolute(x, y - theme::VIEWPORT_INSET)
        .size(
            width + theme::VIEWPORT_INSET,
            height + 2.0 * theme::VIEWPORT_INSET,
        )
        .clip_rounded(0.0);
    if let Some(animation) = &state.peek_presentation.animation {
        layer = layer.children(transition_card(state, theme, animation));
    } else if let Some(peek) = &state.browser.peek {
        layer = layer.child(card(state, theme, reduced, peek));
        let opening_time = state.peek_presentation.opening_time();
        if opening_time >= ACTION_REVEAL || reduced {
            layer = layer.child(actions(state, theme, opening_time));
        }
    }
    layer
}
