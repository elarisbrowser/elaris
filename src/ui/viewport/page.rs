// SPDX-License-Identifier: GPL-3.0-only

//! The page surface: the web page element with its pointer, wheel and key handlers, hover link, and swipe arrow.
use crate::platform::haptics::{self, Haptic};

use super::input::{mouse_kind, page_key};
use super::panes::pane_key;
use crate::app::AppState;
use crate::browser::swipe::{Direction, SwipeView};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris::ladybird::MouseKind;
use rune::{El, Phase, PointerButton, el, icon};

/// The round arrow that slides in from the edge a history swipe pulls from, filling with the
/// accent once releasing will navigate.
fn swipe_indicator(swipe: SwipeView, theme: &Theme) -> El {
    let size = theme::SWIPE_INDICATOR;
    // From fully hidden past the edge to resting just inside it.
    let offset = -size + (size + theme::SWIPE_INDICATOR_INSET) * swipe.progress;
    let (left, right, glyph) = match swipe.direction {
        Direction::Back => (Some(offset), None, &icons::BACK),
        Direction::Forward => (None, Some(offset), &icons::FORWARD),
    };
    let (fill, color) = if swipe.armed {
        (theme.accent, theme.page)
    } else {
        (theme.overlay_panel, theme.icon)
    };
    let min = theme::SWIPE_INDICATOR_MIN_OPACITY;
    // A full-height track keeps the circle centred vertically on the page.
    el().key("swipe-indicator")
        .pin(left, Some(0.0), right, Some(0.0))
        .w(size)
        .center()
        .inert()
        .a11y_hidden()
        .child(
            el().size(size, size)
                .radius(size / 2.0)
                .fill(fill)
                .scale(if swipe.armed {
                    1.0
                } else {
                    theme::SWIPE_INDICATOR_REST_SCALE
                })
                .scale_motion(theme::SWIPE_INDICATOR_SETTLE)
                .border(1.0, theme.overlay_border)
                .shadow(4.0, 16.0, theme.overlay_shadow)
                .opacity(min + (1.0 - min) * swipe.progress)
                .center()
                .child(icon(glyph, theme::SWIPE_INDICATOR_ICON).color(color)),
        )
}

pub(in crate::ui) fn page_hover(page: El, tab: &crate::tab::Tab, theme: &Theme) -> El {
    use rune::tok::{radius, space, type_};

    let page = page.cursor(tab.page_cursor).help(tab.tooltip.as_str());
    if tab.hovered_link.is_empty() {
        return page;
    }
    page.child(
        rune::row()
            .key("page-hovered-link")
            .pin(Some(space::S2), None, Some(space::S2), Some(space::S2))
            .inert()
            .a11y_hidden()
            .child(
                rune::text(elaris_core::address::compact_address(&tab.hovered_link))
                    .min_w(0.0)
                    .truncate()
                    .font(type_::LABEL)
                    .color(theme.text)
                    .fill(theme.overlay_panel)
                    .radius(radius::SM)
                    .px(space::S2)
                    .py(space::S1)
                    .help(tab.hovered_link.as_str()),
            ),
    )
}

/// The page's latest frame fills the viewport over its appearance-aware background. During a live
/// resize Rune scales a frame to new bounds.
pub(super) fn page(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
    side: usize,
) -> El {
    let mut page = el().key(pane_key(side));
    let id = state.browser.visible_tabs()[side];
    let focused = state.browser.focused_side() == side;
    let mut internal = false;
    if let Some(tab) = id.and_then(|id| state.browser.tab(id)) {
        if let Some(route) = &tab.internal {
            internal = true;
            page = page.child(crate::ui::internal::view(
                state,
                route,
                theme,
                overlays,
                reduced_motion,
                state.browser.pane_width(side),
            ));
        }
        if focused
            && !internal
            && !state.omnibox.open
            && !state.page_menu.open()
            && state.browser.page_focused()
            && let Some(rect) = tab.ime_caret
        {
            let [x, y, w, h] = rect.map(|v| v as f32 / state.browser.scale());
            page = page.child(
                el().key("page-ime-caret")
                    .absolute(x, y)
                    .size(w.max(1.0), h.max(1.0))
                    .ime_cursor(),
            );
        }
        if !internal {
            page = page_hover(page, tab, theme);
        }
        if let Some(crash) = super::crash::crash_page(tab, theme) {
            page = page.child(crash);
        }
        if let Some(swipe) = state.browser.swipe().filter(|swipe| swipe.tab == tab.id()) {
            page = page.child(swipe_indicator(swipe, theme));
        }
    }
    let mut page = page.w_full().h_full();
    if !internal && let Some(tree) = id.and_then(|id| state.browser.accessibility_tree(id)) {
        page = page.accessibility_graft(tree);
    }
    page = page
        .focusable(true)
        .no_focus_ring()
        .label("Web page")
        .pointer_buttons([PointerButton::Middle])
        .on_secondary_press_local(move |state: &mut AppState, p, tx| {
            let Some(id) = id else { return };
            state.activate(id);
            state.dismiss_find();
            tx.focus(pane_key(side), false);
            if internal {
                state.page_menu.open_internal(id, p.window());
                return;
            }
            state.page_menu.anchor = Some((id, p.window()));
            for kind in [MouseKind::Down, MouseKind::Up] {
                state.browser.drag_pointer(
                    kind,
                    [p.x, p.y],
                    PointerButton::Secondary,
                    tx.mods(),
                    1,
                );
            }
        })
        .on_drag(move |state: &mut AppState, drag, tx| {
            if drag.phase == Phase::Down {
                if let Some(id) = id
                    && state.browser.active_tab().map(crate::tab::Tab::id) != Some(id)
                {
                    state.activate(id);
                }
                tx.focus(pane_key(side), false);
            }
            if !internal {
                state.browser.drag_pointer(
                    mouse_kind(drag.phase, drag.cancelled()),
                    [drag.x, drag.y],
                    tx.button(),
                    tx.mods(),
                    drag.clicks,
                );
            }
        })
        .on_pointer_move_local(move |state: &mut AppState, pointer, tx| {
            if let Some(id) = id.filter(|_| !internal) {
                state
                    .browser
                    .hover_page(id, MouseKind::Move, [pointer.x, pointer.y], tx.mods());
                tx.live_only();
            }
        })
        .on_hover_change(move |state: &mut AppState, inside, tx| {
            if let Some(id) = id.filter(|_| !inside && !internal) {
                state
                    .browser
                    .hover_page(id, MouseKind::Leave, [0.0; 2], tx.mods());
            }
        })
        .on_wheel(move |state: &mut AppState, wheel, _| {
            if let Some(id) = id.filter(|_| !internal) {
                if state.browser.wheel_page(id, wheel) {
                    haptics::play(Haptic::Snap);
                }
                true
            } else {
                false
            }
        })
        .on_magnify(move |state: &mut AppState, magnify, tx| {
            if let Some(id) = id.filter(|_| !internal) {
                state.browser.pinch_page(id, magnify, tx.mods());
                true
            } else {
                false
            }
        })
        .on_key(move |state: &mut AppState, key, tx| {
            if state.browser.focused_side() == side {
                page_key(state, key, tx)
            } else {
                false
            }
        })
        .on_key_up(move |state: &mut AppState, key, tx| {
            state.browser.focused_side() == side
                && !internal
                && state.browser.key(key, false, false, tx.physical_key())
        });

    #[cfg(not(feature = "ladybird-bridge"))]
    let page = page.child(ui_only_badge(theme));

    page
}

#[cfg(not(feature = "ladybird-bridge"))]
fn ui_only_badge(theme: &Theme) -> El {
    use rune::tok::type_;
    use rune::{row, text};

    row()
        .key("ui-only-mode")
        .pin(
            None,
            None,
            Some(theme::VIEWPORT_BADGE_INSET),
            Some(theme::VIEWPORT_BADGE_INSET),
        )
        .px(theme::ROW_PADDING)
        .py(theme::TAB_GAP)
        .radius(theme::FIELD_RADIUS)
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .child(
            text("UI Only Mode")
                .font(type_::LABEL)
                .color(theme.text_muted),
        )
}

/// Shows the page's frames in `pane`: Rune's GPU samples Ladybird's own surface where it can,
/// and otherwise the engine copies each frame into pixels.
pub(in crate::ui) fn page_surface(pane: El, surface: &crate::engine::Surface, radius: f32) -> El {
    #[cfg(target_os = "macos")]
    if let Some(gpu) = &surface.gpu {
        gpu.set_corner_radius(radius);
    }

    #[cfg(any(elaris_ui = "macos", elaris_ui = "linux"))]
    let page_surface = surface.gpu.as_ref().map(|gpu| {
        el().key("page-native-surface")
            .w_full()
            .h_full()
            .native_surface(gpu.native(), rune::SurfaceFit::Fill)
    });
    #[cfg(any(elaris_ui = "macos", elaris_ui = "linux"))]
    let page_surface = page_surface.unwrap_or_else(|| {
        el().key("page-pixels-surface")
            .w_full()
            .h_full()
            .pixels(&surface.pixels)
    });
    #[cfg(not(any(elaris_ui = "macos", elaris_ui = "linux")))]
    let page_surface = el()
        .key("page-pixels-surface")
        .w_full()
        .h_full()
        .pixels(&surface.pixels);

    // Give the streamed frame its own rounded clip. Native surfaces are composed separately from
    // ordinary fills, so the page mask must be an ancestor of the surface draw itself.
    pane.child(
        el().key("page-surface-clip")
            .w_full()
            .h_full()
            .radius(radius)
            .continuous_corners()
            .clip_rounded(radius)
            .child(page_surface),
    )
}
