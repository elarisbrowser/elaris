// SPDX-License-Identifier: GPL-3.0-only

//! The floating live page, its action rail and outside-click handling.

use std::time::Duration;

use elaris::ladybird::MouseKind;
use rune::tok::type_;
use rune::{Anim, El, Phase, PointerButton, Spring, SpringPair, Transition, Tx, col, el, text};

use crate::app::AppState;
use crate::browser::{
    peek::{self, Destination},
    split,
};
use crate::tab::{Tab, TabId};

use super::{controls, icons, theme, viewport};

pub(crate) const PAGE: &str = "peek-page";
const INITIAL_SCALE: f32 = 0.035;
const OPEN_MOTION: Spring = Spring::new(0.40, 0.18);
const OPEN_SETTLE: f32 = 0.46;
const ACTION_SPACING: f32 = 4.0;
const ACTION_REVEAL: f32 = 0.10;
const ACTION_FADE: f32 = 0.16;
const CLOSE_MOTION: Spring = Spring::new(0.15, 0.0);
const DISMISS_TIME: f64 = 0.20;
const EXPAND_TIME: f64 = 0.24;

pub(crate) struct PeekAnimation {
    id: TabId,
    from: [f32; 4],
    started_at: Option<f64>,
    progress: f32,
    kind: AnimationKind,
}

enum AnimationKind {
    Dismiss {
        frame: rune::Pixels,
        status: Option<String>,
        target: [f32; 4],
    },
    Expand,
}

impl PeekAnimation {
    fn new(id: TabId, from: [f32; 4], kind: AnimationKind) -> Self {
        Self {
            id,
            from,
            started_at: None,
            progress: 0.0,
            kind,
        }
    }

    fn rect(&self, state: &AppState) -> [f32; 4] {
        let [_, _, width, height] = state.split_rect;
        let (target, t) = match self.kind {
            AnimationKind::Dismiss { target, .. } => (target, self.progress.powi(2)),
            AnimationKind::Expand => (
                [0.0, theme::VIEWPORT_INSET, width, height],
                1.0 - (1.0 - self.progress).powi(3),
            ),
        };
        std::array::from_fn(|i| self.from[i] + (target[i] - self.from[i]) * t)
    }
}

struct CardPose {
    rect: [f32; 4],
    scale: f32,
    offset: [f32; 2],
}

impl CardPose {
    fn drawn_rect(&self) -> [f32; 4] {
        let [x, y, w, h] = self.rect;
        [
            x + self.offset[0] + w * (1.0 - self.scale) / 2.0,
            y + self.offset[1] + h * (1.0 - self.scale) / 2.0,
            w * self.scale,
            h * self.scale,
        ]
    }
}

fn opening_pose(state: &AppState, reduced: bool, peek: &peek::PeekState) -> CardPose {
    let [_, _, width, height] = state.split_rect;
    let panel = peek::geometry([width, height]);
    let y = theme::VIEWPORT_INSET - peek::OUTSET;
    let [dx, dy] = [
        peek.origin[0] - (panel.x + panel.width / 2.0),
        peek.origin[1] - (panel.y + panel.height / 2.0),
    ];
    let progress = if reduced || peek.opening_time >= OPEN_SETTLE {
        1.0
    } else {
        OPEN_MOTION.sample(0.0, 0.0, 1.0, peek.opening_time).0
    };
    let max_scale = (height + 2.0 * theme::VIEWPORT_INSET) / panel.height;
    let scale = (INITIAL_SCALE + (1.0 - INITIAL_SCALE) * progress).min(max_scale);
    let travel = 1.0 - progress.min(1.0);
    CardPose {
        rect: [panel.x, y, panel.width, panel.height],
        scale,
        offset: [dx * travel, dy * travel],
    }
}

fn card(state: &AppState, theme: &theme::Theme, reduced: bool, peek: &peek::PeekState) -> El {
    let page = &peek.page;
    let id = page.id();
    let mut content = el()
        .key(PAGE)
        .w_full()
        .h_full()
        .focusable(true)
        .no_focus_ring()
        .label("Preview webpage")
        .pointer_buttons([PointerButton::Middle])
        .on_drag(move |state: &mut AppState, drag, tx| {
            if drag.phase == Phase::Down {
                tx.focus(PAGE, false);
            }
            state.browser.drag_page(
                id,
                viewport::mouse_kind(drag.phase, drag.cancelled()),
                [drag.x, drag.y],
                tx.button(),
                tx.mods(),
                drag.clicks,
            );
        })
        .on_pointer_move_local(move |state: &mut AppState, pointer, tx| {
            state
                .browser
                .hover_page(id, MouseKind::Move, [pointer.x, pointer.y], tx.mods());
        })
        .on_hover_change(move |state: &mut AppState, inside, tx| {
            if !inside {
                state
                    .browser
                    .hover_page(id, MouseKind::Leave, [0.0; 2], tx.mods());
            }
        })
        .on_wheel(move |state: &mut AppState, wheel, _| {
            state.browser.wheel_page(id, wheel);
            true
        })
        .on_key(viewport::page_key)
        .on_key_up(|state: &mut AppState, key, tx| {
            state.browser.key(key, false, false, tx.physical_key())
        });
    let status = page_status(page);
    content = paint_page(content, &page.surface.pixels, status, theme);
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

    let pose = opening_pose(state, reduced, peek);
    let motion = if reduced {
        Transition::opacity().springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION))
    } else {
        Transition::none()
    };
    panel(id, pose.rect, theme, 1.0)
        .scale(pose.scale)
        .scale_motion(Anim::INSTANT)
        .offset(pose.offset[0], pose.offset[1])
        .offset_motion(Anim::INSTANT)
        .transition(motion)
        .role("dialog")
        .label("Peek")
        .child(content)
}

fn page_status(page: &Tab) -> Option<&str> {
    page.error.as_deref().or_else(|| {
        (page.loading && page.committed_url == "about:blank").then_some("Loading preview…")
    })
}

fn paint_page(content: El, frame: &rune::Pixels, status: Option<&str>, theme: &theme::Theme) -> El {
    if let Some(status) = status {
        content
            .center()
            .child(text(status).font(type_::BODY).color(theme.text_muted))
    } else {
        content.pixels(frame)
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
            1.0 - animation.progress.powi(3),
            1.0,
        ),
        AnimationKind::Expand => {
            let peek = state
                .browser
                .peek
                .as_ref()
                .filter(|peek| peek.page.id() == animation.id)?;
            (
                &peek.page.surface.pixels,
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

fn actions(state: &AppState, theme: &theme::Theme, peek: &peek::PeekState) -> El {
    let [_, _, width, height] = state.split_rect;
    let panel = peek::geometry([width, height]);
    let opacity = ((peek.opening_time - ACTION_REVEAL) / ACTION_FADE).clamp(0.0, 1.0);
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
            .on_press_tx(expand),
        )
        .child(
            controls::floating_icon_button(
                "peek-open-split",
                "Add to Split View",
                &icons::SPLIT_VIEW,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| state.promote_peek(Destination::Split, tx)),
        )
}

/// Advance Peek on Rune's frame clock without resizing the Ladybird view during motion.
pub(crate) fn advance(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    if let Some(animation) = &mut state.peek_animation {
        let start = *animation.started_at.get_or_insert(clock);
        let duration = match animation.kind {
            AnimationKind::Dismiss { .. } => DISMISS_TIME,
            AnimationKind::Expand => EXPAND_TIME,
        };
        animation.progress = ((clock - start) / duration).clamp(0.0, 1.0) as f32;
        if animation.progress >= 1.0 || state.sidebar.hover.reduced.get() {
            let expand = matches!(animation.kind, AnimationKind::Expand)
                && state
                    .browser
                    .peek
                    .as_ref()
                    .is_some_and(|peek| peek.page.id() == animation.id);
            state.peek_animation = None;
            if expand {
                state.promote_peek(Destination::Tab, tx);
                // The regular viewport takes over at identical bounds in this commit.
                tx.instant();
            }
        } else {
            tx.wake_after(Duration::ZERO);
        }
        return true;
    }
    let Some(peek) = &mut state.browser.peek else {
        return false;
    };
    if peek.opening_time >= OPEN_SETTLE {
        return false;
    }
    let start = *peek.opened_at.get_or_insert(clock);
    peek.opening_time = if state.sidebar.hover.reduced.get() {
        OPEN_SETTLE
    } else {
        ((clock - start) as f32).clamp(0.0, OPEN_SETTLE)
    };
    if peek.opening_time < OPEN_SETTLE {
        tx.wake_after(Duration::ZERO);
    }
    true
}

/// Keep the card and its detached controls inside the content region.
pub(super) fn layer(state: &AppState, theme: &theme::Theme, reduced: bool) -> El {
    let [x, y, width, height] = state.split_rect;
    let mut layer = el()
        .key("peek-layer")
        .absolute(x, y - theme::VIEWPORT_INSET)
        .size(
            width + theme::VIEWPORT_INSET,
            height + 2.0 * theme::VIEWPORT_INSET,
        )
        .clip_rounded(0.0);
    if let Some(animation) = &state.peek_animation {
        layer = layer.children(transition_card(state, theme, animation));
    } else if let Some(peek) = &state.browser.peek {
        layer = layer.child(card(state, theme, reduced, peek));
        if peek.opening_time >= ACTION_REVEAL || reduced {
            layer = layer.child(actions(state, theme, peek));
        }
    }
    layer
}

/// A transparent hit target under the panel catches dismissals without altering sidebar layout.
pub(super) fn backdrop() -> El {
    el().key("peek-backdrop")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden()
        .pointer_buttons([PointerButton::Middle])
        .on_secondary_press(|state: &mut AppState, _, tx| {
            dismiss(state, tx);
        })
        .on_wheel(|_: &mut AppState, _, _| true)
        .on_drag(|state: &mut AppState, drag, tx| {
            let point = drag.window_point();
            if drag.phase == Phase::Down {
                if tx.button() == PointerButton::Primary
                    && peek::shift_only(tx.mods())
                    && let Some(side) = pane_at(state, point)
                {
                    drag.data = side as f32 + 1.0;
                } else {
                    dismiss(state, tx);
                    return;
                }
            }
            if drag.data == 0.0 {
                return;
            }
            let side = drag.data as usize - 1;
            if let Some(id) = state.browser.visible_tabs()[side] {
                let [x, y, width, _] = state.split_rect;
                let offset = state
                    .browser
                    .split
                    .filter(|_| side == 1)
                    .map_or(0.0, |split| {
                        split::widths(width, split.ratio)[0] + split::DIVIDER
                    });
                state.browser.drag_page(
                    id,
                    viewport::mouse_kind(drag.phase, drag.cancelled()),
                    [point[0] - x - offset, point[1] - y],
                    tx.button(),
                    tx.mods(),
                    drag.clicks,
                );
            }
            if drag.phase == Phase::Up {
                dismiss(state, tx);
            }
        })
}

fn pane_at(state: &AppState, point: [f32; 2]) -> Option<usize> {
    let width = state.split_rect[2];
    let boundary = state
        .browser
        .split
        .map_or(width, |split| split::widths(width, split.ratio)[0]);
    let side = split::target(state.split_rect, point, boundary)?;
    if side == 1 && point[0] < state.split_rect[0] + boundary + split::DIVIDER {
        return None;
    }
    state.browser.visible_tabs()[side]
        .and_then(|id| state.browser.tab(id))
        .filter(|tab| !tab.is_internal())
        .map(|_| side)
}

fn expand(state: &mut AppState, tx: &mut Tx) {
    if state.sidebar.hover.reduced.get() {
        state.promote_peek(Destination::Tab, tx);
    } else if let Some(peek) = &state.browser.peek {
        let from = opening_pose(state, false, peek).drawn_rect();
        state.peek_animation = Some(PeekAnimation::new(
            peek.page.id(),
            from,
            AnimationKind::Expand,
        ));
        tx.wake_after(Duration::ZERO);
        tx.animated();
    }
}

pub(crate) fn dismiss(state: &mut AppState, tx: &mut Tx) -> bool {
    let Some(peek) = &state.browser.peek else {
        return false;
    };
    let reduced = state.sidebar.hover.reduced.get();
    let animation = (!reduced).then(|| {
        let pose = opening_pose(state, false, peek);
        let from = state
            .peek_animation
            .as_ref()
            .map_or_else(|| pose.drawn_rect(), |animation| animation.rect(state));
        let [x, y] = peek.origin;
        let [_, _, width, height] = pose.rect;
        let [w, h] = [width * INITIAL_SCALE, height * INITIAL_SCALE];
        let target = [x - w / 2.0, y + theme::VIEWPORT_INSET - h / 2.0, w, h];
        let frame = peek.page.surface.pixels.clone();
        PeekAnimation::new(
            peek.page.id(),
            from,
            AnimationKind::Dismiss {
                frame,
                status: page_status(&peek.page).map(str::to_owned),
                target,
            },
        )
    });
    state.browser.dismiss_peek();
    state.peek_animation = animation;
    tx.focus(viewport::focus_key(&state.browser), false);
    if state.peek_animation.is_some() {
        tx.wake_after(Duration::ZERO);
    }
    tx.animated();
    true
}

#[cfg(test)]
mod tests {
    use rune::{Env, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::engine::{Engine, FakeEngine, Update};
    use crate::tab::TabId;

    const TARGET: &str = "https://preview.example/";

    fn window(reduced: bool) -> (Runtime<AppState>, FakeEngine, TabId) {
        let mut state = AppState::new(Some("source.example"));
        let source = state.browser.active_tab().unwrap().id();
        let (engine, fake) = Engine::fake();
        state.browser.connect(engine);
        let mut window = Runtime::new(
            state,
            crate::ui::view,
            (1080.0, 720.0),
            Env::new(false, 1.0),
        );
        window.on_key = Some(crate::shortcuts::on_key);
        window.set_system_reduced_motion(reduced);
        window.frame(0.0);
        window.frame(0.016);
        (window, fake, source)
    }

    fn open(window: &mut Runtime<AppState>, fake: &FakeEngine, source: TabId, time: f64) -> String {
        fake.updates
            .send(Update::Peek(source, TARGET.into(), 1))
            .unwrap();
        window.update_if(|state, tx| crate::app::poll(state, time, tx));
        window.frame(time);
        let id = window.state.browser.peek.as_ref().unwrap().page.id();
        format!("peek-panel-{}", id.raw())
    }

    #[test]
    fn panel_tracks_content_and_escape_restores_page_focus() {
        let (mut window, fake, source) = window(false);
        let key = open(&mut window, &fake, source, 0.01);
        let opening = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        assert!(opening.rect[2] < 0.05 * opening.target[2]);
        assert!(opening.rect[3] < 0.05 * opening.target[3]);
        window.update_if(|state, tx| crate::app::poll(state, 0.09, tx));
        window.frame(0.016);
        let expanding = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        window.update_if(|state, tx| crate::app::poll(state, 0.36, tx));
        window.frame(0.016);
        let rebound = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
        window.frame(1.0);
        let settled = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        let [x, y, w, h] = window.state.split_rect;
        assert!((settled.rect[0] + settled.rect[2] / 2.0 - (x + w / 2.0)).abs() < 0.05);
        assert!(expanding.rect[2] > opening.rect[2] && expanding.rect[2] < settled.rect[2]);
        assert!(expanding.rect[3] > opening.rect[3] && expanding.rect[3] < settled.rect[3]);
        assert!(rebound.rect[2] > settled.rect[2]);
        assert!(settled.rect[1] < y);
        assert!(settled.rect[2] < w);
        assert!(settled.rect[3] > h);
        assert!(
            window
                .elements()
                .iter()
                .any(|el| el.focused && el.key.as_deref() == Some(PAGE))
        );
        let origin = [80.0, 120.0];
        window.state.browser.peek.as_mut().unwrap().origin = origin;
        window.key(KeyEvent {
            key: rune::Key::Escape,
            mods: Mods::default(),
        });
        window.frame(0.01);
        assert!(window.state.browser.peek.is_none());
        let at_dismissal = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        assert!((at_dismissal.rect[2] - settled.rect[2]).abs() < 0.05);
        window.update_if(|state, tx| crate::app::poll(state, 2.0, tx));
        window.update_if(|state, tx| crate::app::poll(state, 2.18, tx));
        window.frame(0.016);
        let shrinking = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        assert!(shrinking.rect[2] < settled.rect[2] / 3.0);
        let [sx, sy, sw, sh] = shrinking.rect;
        assert!((sx + sw / 2.0 - (x + origin[0])).abs() < 80.0);
        assert!((sy + sh / 2.0 - (y + origin[1])).abs() < 60.0);
        window.update_if(|state, tx| crate::app::poll(state, 2.25, tx));
        window.frame(0.016);
        assert!(
            !window
                .elements()
                .iter()
                .any(|el| el.key.as_deref() == Some(key.as_str()))
        );
        assert!(window.elements().iter().any(|el| el.focused
            && el.key.as_deref() == Some(viewport::focus_key(&window.state.browser))));
    }

    #[test]
    fn reduced_motion_keeps_the_panel_in_place_and_outside_click_dismisses() {
        let (mut window, fake, source) = window(true);
        let key = open(&mut window, &fake, source, 0.01);
        let panel = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(&key))
            .unwrap();
        assert!(
            panel
                .rect
                .into_iter()
                .zip(panel.target)
                .all(|(drawn, target)| (drawn - target).abs() < 0.05)
        );
        assert!(panel.opacity < 1.0);
        window.pointer_down(24.0, 160.0);
        window.pointer_up(24.0, 160.0);
        window.frame(0.05);
        assert!(window.state.browser.peek.is_none());
        assert_eq!(window.state.browser.tabs().len(), 1);
        assert_eq!(window.state.browser.active_tab().unwrap().id(), source);
    }

    #[test]
    fn open_in_tab_expands_into_the_viewport_before_transferring_the_live_page() {
        let (mut window, fake, source) = window(false);
        let key = open(&mut window, &fake, source, 0.01);
        window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
        window.frame(1.0);
        let get_rect = |window: &Runtime<AppState>, key: &str| {
            window
                .elements()
                .into_iter()
                .find(|el| el.key.as_deref() == Some(key))
                .unwrap()
                .rect
        };
        let start = get_rect(&window, &key);
        let id = window.state.browser.peek.as_ref().unwrap().page.id();
        let [x, y, w, h] = get_rect(&window, "peek-open-tab");
        let [x, y] = [x + w / 2.0, y + h / 2.0];
        window.pointer_down(x, y);
        window.pointer_up(x, y);
        window.frame(0.016);
        assert_eq!(window.state.browser.active_tab().unwrap().id(), source);
        assert!((get_rect(&window, &key)[2] - start[2]).abs() < 0.05);
        window.update_if(|state, tx| crate::app::poll(state, 2.0, tx));
        window.update_if(|state, tx| crate::app::poll(state, 2.12, tx));
        window.frame(0.016);
        let midway = get_rect(&window, &key);
        let area = window.state.split_rect;
        assert!(midway[0] < start[0] && midway[0] > area[0]);
        assert!(midway[2] > start[2] && midway[2] < area[2]);
        assert!(midway[1] > start[1] && midway[1] < area[1]);
        assert!(midway[3] < start[3] && midway[3] > area[3]);

        window.update_if(|state, tx| crate::app::poll(state, 2.25, tx));
        window.frame(0.016);
        assert!(window.state.browser.peek.is_none());
        assert_eq!(window.state.browser.active_tab().unwrap().id(), id);
        let viewport = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(viewport::focus_key(&window.state.browser)))
            .unwrap();
        assert!(viewport.focused);
        assert_eq!(viewport.rect, area);
    }

    #[test]
    fn detached_button_promotes_to_the_right_split_pane() {
        let (mut window, fake, source) = window(false);
        let panel_key = open(&mut window, &fake, source, 0.01);
        window.update_if(|state, tx| crate::app::poll(state, 1.0, tx));
        window.frame(1.0);
        let [x, y, w, h] = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("peek-open-split"))
            .unwrap()
            .rect;
        let panel = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(panel_key.as_str()))
            .unwrap();
        assert!(x >= panel.rect[0] + panel.rect[2] + peek::ACTION_GAP - 0.05);
        let close = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("peek-close"))
            .unwrap();
        assert!((close.rect[1] - panel.rect[1]).abs() < 0.05);
        let id = window.state.browser.peek.as_ref().unwrap().page.id();
        let [px, py] = [x + w / 2.0, y + h / 2.0];
        window.pointer_down(px, py);
        window.pointer_up(px, py);
        window.frame(0.01);
        assert!(window.state.browser.peek.is_none());
        assert_eq!(
            window.state.browser.visible_tabs(),
            [Some(source), Some(id)]
        );
    }
}
