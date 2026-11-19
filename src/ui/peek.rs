// SPDX-License-Identifier: GPL-3.0-only

//! The floating live page, its action rail and outside-click handling.

use elaris::ladybird::MouseKind;
use rune::tok::type_;
use rune::{El, Phase, PointerButton, Spring, SpringPair, Transition, Tx, col, el, row, text};

use crate::app::AppState;
use crate::browser::{
    peek::{self, Destination},
    split,
};

use super::{controls, icons, theme, viewport};

pub(crate) const PAGE: &str = "peek-page";
const ACTION_SIZE: f32 = 32.0;
const OPEN_MOTION: Spring = Spring::new(0.24, 0.03);
const CLOSE_MOTION: Spring = Spring::new(0.15, 0.0);

fn view(state: &AppState, theme: &theme::Theme, reduced: bool) -> Option<El> {
    let peek = state.browser.peek.as_ref()?;
    let page = &peek.page;
    let id = page.id();
    let [_, _, width, height] = state.split_rect;
    let panel = peek::geometry([width, height]);
    let center = [panel.x + panel.width / 2.0, panel.y + panel.height / 2.0];
    let [dx, dy] = [peek.origin[0] - center[0], peek.origin[1] - center[1]];
    let motion = if reduced {
        Transition::opacity().springs(SpringPair::new(CLOSE_MOTION, CLOSE_MOTION))
    } else {
        let enter = Transition::scale(0.5)
            .allow_small_scale()
            .combined(Transition::offset(dx, dy));
        let exit = Transition::scale(0.95).combined(Transition::offset(10.0, 0.0));
        Transition::asymmetric(enter, exit).springs(SpringPair::new(OPEN_MOTION, CLOSE_MOTION))
    };

    let mut content = el()
        .key(PAGE)
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .fill(theme.page)
        .clip_rounded(theme::VIEWPORT_RADIUS)
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

    let actions = col()
        .key("peek-actions")
        .w(peek::ACTION_RAIL)
        .h_full()
        .pt(4.0)
        .items_center()
        .role("group")
        .label("Peek actions")
        .child(
            controls::icon_button(
                "peek-close",
                "Close Peek",
                &icons::CLOSE,
                ACTION_SIZE,
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
                ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| state.promote_peek(Destination::Tab, tx)),
        )
        .child(
            controls::icon_button(
                "peek-open-split",
                "Add to Split View",
                &icons::SPLIT_VIEW,
                ACTION_SIZE,
                theme,
            )
            .on_press_tx(|state: &mut AppState, tx| state.promote_peek(Destination::Split, tx)),
        );

    Some(
        row()
            .key(format!("peek-panel-{}", id.raw()))
            .absolute(panel.x, 0.0)
            .size(panel.width, panel.height)
            .fill(theme.overlay_panel)
            .border(1.0, theme.overlay_border)
            .shadow(
                theme::OMNIBOX_SHADOW_Y,
                theme::OMNIBOX_SHADOW_BLUR,
                theme.overlay_shadow,
            )
            .continuous_corners()
            .radius(theme::VIEWPORT_RADIUS)
            .transition(motion)
            .role("dialog")
            .label("Peek")
            .on_background_press(|_: &mut AppState| {})
            .child(content)
            .child(actions),
    )
}

/// Keep the animated panel inside the content region, including its available outer padding.
pub(super) fn layer(state: &AppState, theme: &theme::Theme, reduced: bool) -> El {
    let [x, y, width, height] = state.split_rect;
    el().key("peek-layer")
        .absolute(x, y - peek::OUTSET)
        .size(
            width + theme::VIEWPORT_INSET,
            height + peek::OUTSET + theme::VIEWPORT_INSET,
        )
        .clip_rounded(0.0)
        .children(view(state, theme, reduced))
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
        assert!(opening.rect[2] < opening.target[2]);
        window.frame(1.0);
        let settled = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some(key.as_str()))
            .unwrap();
        let [x, y, w, h] = window.state.split_rect;
        assert!(settled.rect[0] > x);
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
    fn action_rail_promotes_to_the_right_split_pane() {
        let (mut window, fake, source) = window(false);
        open(&mut window, &fake, source, 0.01);
        window.frame(1.0);
        let [x, y, w, h] = window
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("peek-open-split"))
            .unwrap()
            .rect;
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
