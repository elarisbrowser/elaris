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

use super::{controls, icons, theme, viewport};

pub(crate) const PAGE: &str = "peek-page";
const INITIAL_SCALE: f32 = 0.035;
const OPEN_MOTION: Spring = Spring::new(0.40, 0.18);
const OPEN_SETTLE: f32 = 0.46;
const ACTION_SPACING: f32 = 4.0;
const ACTION_REVEAL: f32 = 0.10;
const ACTION_FADE: f32 = 0.16;
const CLOSE_MOTION: Spring = Spring::new(0.15, 0.0);

fn card(state: &AppState, theme: &theme::Theme, reduced: bool, peek: &peek::PeekState) -> El {
    let page = &peek.page;
    let id = page.id();
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
    let motion = if reduced {
        Transition::opacity()
    } else {
        Transition::asymmetric(Transition::none(), Transition::scale(0.95))
    }
    .springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION));

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
    if let Some(error) = &page.error {
        content = content
            .center()
            .child(text(error).font(type_::BODY).color(theme.text_muted));
    } else {
        content = content.pixels(&page.surface.pixels);
        if page.loading && page.committed_url == "about:blank" {
            content = content.center().child(
                text("Loading preview…")
                    .font(type_::BODY)
                    .color(theme.text_muted),
            );
        }
        if state.browser.page_focused()
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
    }

    el().key(format!("peek-panel-{}", id.raw()))
        .absolute(panel.x, y)
        .size(panel.width, panel.height)
        .scale(scale)
        .scale_motion(Anim::INSTANT)
        .offset(dx * travel, dy * travel)
        .offset_motion(Anim::INSTANT)
        .fill(theme.page)
        .border(1.0, theme.overlay_border)
        .shadow(
            theme::OMNIBOX_SHADOW_Y,
            theme::OMNIBOX_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .continuous_corners()
        .radius(theme::VIEWPORT_RADIUS)
        .clip_rounded(theme::VIEWPORT_RADIUS)
        .transition(motion)
        .role("dialog")
        .label("Peek")
        .child(content)
}

fn actions(state: &AppState, theme: &theme::Theme, peek: &peek::PeekState) -> El {
    let [_, _, width, height] = state.split_rect;
    let panel = peek::geometry([width, height]);
    let stack_height = 3.0 * peek::ACTION_SIZE + 2.0 * ACTION_SPACING;
    let opacity = ((peek.opening_time - ACTION_REVEAL) / ACTION_FADE).clamp(0.0, 1.0);
    col()
        .key("peek-actions")
        .absolute(
            panel.x + panel.width + peek::ACTION_GAP,
            theme::VIEWPORT_INSET + (height - stack_height) / 2.0,
        )
        .w(peek::ACTION_SIZE)
        .gap(ACTION_SPACING)
        .opacity(opacity)
        .opacity_motion(Anim::INSTANT)
        .transition(Transition::opacity().springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION)))
        .role("group")
        .label("Peek actions")
        .child(
            controls::icon_button(
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
            controls::icon_button(
                "peek-open-tab",
                "Open in New Tab",
                &icons::OPEN_TAB,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| state.promote_peek(Destination::Tab, tx)),
        )
        .child(
            controls::icon_button(
                "peek-open-split",
                "Add to Split View",
                &icons::SPLIT_VIEW,
                peek::ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| state.promote_peek(Destination::Split, tx)),
        )
}

/// Advance the card's spring on Rune's frame clock, keeping the web viewport at its final size.
pub(crate) fn advance(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
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
    if let Some(peek) = &state.browser.peek {
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

pub(crate) fn dismiss(state: &mut AppState, tx: &mut Tx) -> bool {
    if state.browser.dismiss_peek() {
        tx.focus(viewport::focus_key(&state.browser), false);
        tx.animated();
        true
    } else {
        false
    }
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
        window.key(KeyEvent {
            key: rune::Key::Escape,
            mods: Mods::default(),
        });
        window.frame(0.01);
        assert!(window.state.browser.peek.is_none());
        assert!(
            window
                .elements()
                .iter()
                .any(|el| el.exiting && el.key.as_deref() == Some(key.as_str()))
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
