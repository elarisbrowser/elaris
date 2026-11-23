// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::{radius, space, type_};
use rune::{Anim, Curve, Cx, El, SurfaceFit, Transition, col, el, icon, row, text};

use super::sidebar::hover::{self, FADE_DURATION};
use super::{icons, theme::Theme};
use crate::app::AppState;
use crate::engine::THUMBNAIL_ASPECT;
use crate::tab::{Tab, TabSection};
use elaris_core::address::host_for_address;
use elaris_core::internal::{InternalPage, InternalRoute};

const WIDTH: f32 = 266.0;
const MIN_WIDTH: f32 = 250.0;
const CORNER: f32 = 14.0;
const LABELS: f32 = 34.0;
const PADDING: f32 = space::S2;
const EDGE: f32 = space::S3;
const GAP: f32 = space::S2;

fn placement(window: [f32; 2], sidebar_right: f32, anchor: [f32; 4]) -> [f32; 4] {
    let [window_w, window_h] = window;
    let [row_x, row_y, row_w, row_h] = anchor;
    let right = sidebar_right.max(row_x + row_w) + GAP;
    let right_width = window_w - EDGE - right;
    let available = if right_width >= MIN_WIDTH {
        right_width
    } else {
        window_w - 2.0 * EDGE
    };
    let width = WIDTH.min(available.max(0.0));
    let aspect = THUMBNAIL_ASPECT.0 as f32 / THUMBNAIL_ASPECT.1 as f32;
    let media = ((width - 2.0 * PADDING).max(0.0) / aspect)
        .min((window_h - 2.0 * EDGE - 2.0 * PADDING - GAP - LABELS).max(0.0));
    let height = media + 2.0 * PADDING + GAP + LABELS;
    let left = row_x - GAP - width;
    let max_x = (window_w - EDGE - width).max(EDGE);
    let max_y = (window_h - EDGE - height).max(EDGE);
    let aligned_y = (row_y + (row_h - height) / 2.0).clamp(EDGE, max_y);
    let (x, y) = if width <= right_width {
        (right, aligned_y)
    } else if left >= EDGE {
        (left, aligned_y)
    } else {
        let below = row_y + row_h + GAP;
        let above = row_y - GAP - height;
        let y = if (EDGE..=max_y).contains(&below) {
            below
        } else if (EDGE..=max_y).contains(&above) {
            above
        } else {
            aligned_y
        };
        (max_x, y)
    };
    [x, y, width, media]
}

fn horizontal_placement(window: [f32; 2], anchor: [f32; 4]) -> [f32; 4] {
    let width = WIDTH.min((window[0] - 2.0 * EDGE).max(0.0));
    let x =
        (anchor[0] + (anchor[2] - width) / 2.0).clamp(EDGE, (window[0] - EDGE - width).max(EDGE));
    let y = anchor[1] + anchor[3] + GAP;
    let aspect = THUMBNAIL_ASPECT.0 as f32 / THUMBNAIL_ASPECT.1 as f32;
    let media = ((width - 2.0 * PADDING).max(0.0) / aspect)
        .min((window[1] - y - EDGE - 2.0 * PADDING - GAP - LABELS).max(0.0));
    [x, y, width, media]
}

fn address(tab: &Tab) -> &str {
    if tab.is_internal() {
        return &tab.url;
    }
    let url = &tab.url;
    let host = url.split_once("://").map_or("", |(_, rest)| {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        host_for_address(
            authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host),
        )
    });
    if host.is_empty() { &tab.url } else { host }
}

fn symbol(tab: &Tab, size: f32, theme: &Theme) -> El {
    let glyph = match tab.internal {
        Some(InternalRoute::Page(InternalPage::Settings)) => &icons::SYSTEM,
        Some(InternalRoute::Page(InternalPage::History)) => &icons::HISTORY,
        Some(InternalRoute::Page(InternalPage::Downloads)) => &icons::DOWNLOAD,
        Some(InternalRoute::Page(InternalPage::TaskManager)) => &icons::ACTIVITY,
        _ => &icons::PAGE,
    };
    tab.favicon.as_ref().map_or_else(
        || icon(glyph, size).color(theme.icon),
        |pixels| el().size(size, size).pixels_fit(pixels, SurfaceFit::Fit),
    )
}

fn reveal(reduced: bool) -> Transition {
    if reduced {
        Transition::none()
    } else {
        Transition::asymmetric(Transition::scale(0.97), Transition::none())
    }
}

pub(super) fn view(
    state: &AppState,
    cx: &mut Cx<AppState>,
    theme: &Theme,
    reduced: bool,
) -> Option<El> {
    let (id, fading) = state.sidebar.hover.display()?;
    if !hover::allowed(state) || !hover::eligible(state, id) {
        return None;
    }
    let tab = state.browser.tab(id)?;
    let window = cx.layout_size("window")?;
    let [x, y, width, media_height] = if state.horizontal_tabs() {
        let anchor = cx.window_layout(&super::horizontal_tabs::key(id))?;
        let clip = cx.window_layout(super::horizontal_tabs::SCROLL)?;
        let left = anchor[0].max(clip[0]);
        let right = (anchor[0] + anchor[2]).min(clip[0] + clip[2]);
        if left >= right {
            return None;
        }
        horizontal_placement(window, [left, anchor[1], right - left, anchor[3]])
    } else {
        let prefix = if tab.section() == TabSection::Favourite {
            "favourite"
        } else {
            "tab"
        };
        let anchor = cx.window_layout(&format!("{prefix}-{}", id.raw()))?;
        let sidebar = cx.window_layout("sidebar")?;
        placement(window, sidebar[0] + sidebar[2], anchor)
    };
    let label = |value: &str, secondary: bool| {
        text(value)
            .font(if secondary { type_::LABEL } else { type_::BODY })
            .color(if secondary {
                theme.text_muted
            } else {
                theme.text
            })
            .single_line()
            .truncate()
    };
    let media = el()
        .w_full()
        .h(media_height)
        .center()
        .fill(theme.address_field)
        .radius(radius::MD)
        .clip_rounded(radius::MD);
    let media = if let Some(pixels) = state.browser.thumbnail(id) {
        media.pixels_fit(&pixels, SurfaceFit::Fill)
    } else {
        media.child(
            col()
                .w_full()
                .px(PADDING)
                .center()
                .gap(GAP)
                .child(symbol(tab, 30.0, theme))
                .child(label(tab.label(), true)),
        )
    };
    Some(
        el().key("tab-preview-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .inert()
            .transition(Transition::none())
            .a11y_hidden()
            .child(
                el().key("tab-preview")
                    .absolute(x, y)
                    .w(width)
                    .radius(CORNER)
                    .continuous_corners()
                    .fill(theme.overlay_panel)
                    .border(1.0, theme.overlay_border)
                    .shadow(8.0, 24.0, theme.overlay_shadow)
                    .opacity(if fading { 0.0 } else { 1.0 })
                    .opacity_motion(if reduced {
                        Anim::INSTANT
                    } else {
                        Anim::new(Curve::tween(FADE_DURATION, rune::Easing::EASE_OUT))
                    })
                    .transition(reveal(reduced))
                    .child(
                        col()
                            .key(format!("tab-preview-content-{}", id.raw()))
                            .transition(if reduced {
                                Transition::none()
                            } else {
                                Transition::asymmetric(Transition::opacity(), Transition::none())
                            })
                            .w_full()
                            .padding(PADDING)
                            .gap(GAP)
                            .child(media)
                            .child(
                                row()
                                    .w_full()
                                    .h(LABELS)
                                    .items_center()
                                    .gap(GAP)
                                    .child(symbol(tab, 16.0, theme))
                                    .child(
                                        col()
                                            .min_w(0.0)
                                            .grow(1.0)
                                            .child(label(tab.label(), false))
                                            .child(label(address(tab), true)),
                                    ),
                            ),
                    ),
            ),
    )
}

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::*;
    use crate::tab::TabId;

    fn runtime(reduced: bool) -> (Runtime<AppState>, [TabId; 3]) {
        let mut state = AppState::new(Some("one.example"));
        let regular = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("two.example").unwrap();
        let favourite = state.browser.open_tab("elaris://settings").unwrap();
        state
            .browser
            .move_tab_to(favourite, TabSection::Favourite, 0)
            .unwrap();
        let mut runtime = Runtime::new(state, super::super::view, (1000.0, 640.0), Env::default());
        runtime.poll = Some(crate::app::poll);
        runtime.set_system_reduced_motion(reduced);
        runtime.frame(0.0);
        (runtime, [regular, second, favourite])
    }

    fn point(runtime: &Runtime<AppState>, id: TabId, prefix: &str) -> [f32; 2] {
        let ([x, y, w, h], _) = runtime
            .presented(&format!("{prefix}-{}", id.raw()))
            .unwrap();
        [x + w / 2.0, y + h / 2.0]
    }

    fn hover(runtime: &mut Runtime<AppState>, id: TabId, prefix: &str, time: f64) {
        let [x, y] = point(runtime, id, prefix);
        runtime.pointer_move(x, y);
        runtime.frame(time);
    }

    fn preview(runtime: &Runtime<AppState>) -> Option<([f32; 4], f32)> {
        runtime.presented("tab-preview")
    }

    #[test]
    fn horizontal_preview_anchors_below_a_tab_and_honours_reduced_motion() {
        let (mut runtime, [_, second, _]) = runtime(true);
        runtime.update(|state, _| {
            state.set_tab_layout(elaris_core::settings::TabLayout::Horizontal);
        });
        runtime.frame(1.0);
        hover(&mut runtime, second, "horizontal-tab", 0.0);
        runtime.frame(0.41);
        let (rect, opacity) = preview(&runtime).unwrap();
        let (tab, _) = runtime
            .presented(&super::super::horizontal_tabs::key(second))
            .unwrap();
        assert!(rect[1] >= tab[1] + tab[3]);
        assert!(rect[0] >= EDGE && rect[0] + rect[2] <= 1000.0 - EDGE);
        assert_eq!(opacity, 1.0);
    }

    #[test]
    fn hover_waits_cancels_and_switches_between_sections() {
        let (mut runtime, [regular, second, favourite]) = runtime(false);
        hover(&mut runtime, second, "tab", 0.0);
        runtime.frame(0.25);
        assert!(preview(&runtime).is_none());
        runtime.pointer_move(700.0, 300.0);
        runtime.frame(0.0);
        runtime.frame(0.30);
        assert!(preview(&runtime).is_none());

        hover(&mut runtime, second, "tab", 0.0);
        runtime.frame(0.39);
        assert!(preview(&runtime).is_none());
        runtime.frame(0.02);
        assert_eq!(hover::displayed(&runtime.state), Some(second));
        let (rect, opacity) = preview(&runtime).unwrap();
        assert!(opacity < 1.0, "the delayed opening fades in");
        let ([sidebar_x, _, sidebar_w, _], _) = runtime.presented("sidebar").unwrap();
        assert!(rect[0] >= sidebar_x + sidebar_w);

        hover(&mut runtime, favourite, "favourite", 0.0);
        runtime.frame(0.07);
        assert_eq!(hover::displayed(&runtime.state), Some(favourite));
        assert!(
            runtime
                .presented(&format!("tab-preview-content-{}", favourite.raw()))
                .is_some()
        );
        let native = runtime.state.browser.tab(favourite).unwrap();
        assert_eq!(
            (native.label(), address(native)),
            ("Settings", "elaris://settings")
        );
        assert!(runtime.state.browser.thumbnail(favourite).is_none());
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), regular);

        runtime.pointer_move(700.0, 300.0);
        runtime.frame(0.0);
        runtime.frame(0.11);
        assert_eq!(
            runtime.state.sidebar.hover.display(),
            Some((favourite, true))
        );
        runtime.frame(0.13);
        assert!(preview(&runtime).is_none());
    }

    #[test]
    fn resized_preview_stays_beside_the_tab_and_reduced_motion_is_instant() {
        let (mut runtime, [_, second, _]) = runtime(true);
        hover(&mut runtime, second, "tab", 0.01);
        runtime.frame(0.42);
        let ([before, _, width, _], opacity) = preview(&runtime).unwrap();
        assert_eq!(width, 266.0);
        assert_eq!(opacity, 1.0);
        runtime.update(|state, _| state.sidebar.width += 50.0);
        runtime.frame(0.01);
        runtime.frame(0.01);
        let ([wider, ..], _) = preview(&runtime).unwrap();
        assert!(wider > before);
        runtime.set_viewport((480.0, 410.0), 1.0);
        runtime.frame(0.01);
        runtime.frame(0.01);
        let ([x, y, w, h], _) = preview(&runtime).unwrap();
        assert!(x >= 0.0 && y >= 0.0 && x + w <= 480.0 && y + h <= 410.0);
        runtime.pointer_move(470.0, 400.0);
        runtime.frame(0.0);
        runtime.frame(0.11);
        assert!(preview(&runtime).is_none());
    }

    #[test]
    fn pressing_and_dragging_clear_the_preview_without_changing_split_focus() {
        let (mut runtime, [regular, second, _]) = runtime(true);
        runtime.update(|state, _| state.browser.place_split(second, 1));
        runtime.frame(0.01);
        let active = runtime.state.browser.active_tab().unwrap().id();
        hover(&mut runtime, regular, "tab", 0.0);
        runtime.frame(0.41);
        assert!(preview(&runtime).is_some());
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), active);
        assert_eq!(
            runtime.state.browser.visible_tabs(),
            [Some(regular), Some(second)]
        );
        let [x, y] = point(&runtime, regular, "tab");
        runtime.pointer_down(x, y);
        runtime.frame(0.01);
        assert!(preview(&runtime).is_none());
        runtime.pointer_move(550.0, 300.0);
        runtime.frame(0.01);
        assert!(runtime.state.tab_drag.is_dragging());
        runtime.frame(0.50);
        assert!(preview(&runtime).is_none());
    }

    #[test]
    fn focused_tab_has_no_preview_in_single_or_split_view() {
        let (mut runtime, [regular, second, _]) = runtime(true);
        hover(&mut runtime, regular, "tab", 0.0);
        runtime.frame(0.45);
        assert!(preview(&runtime).is_none());

        runtime.update(|state, _| state.browser.place_split(second, 1));
        runtime.frame(0.01);
        hover(&mut runtime, second, "tab", 0.0);
        runtime.frame(0.45);
        assert!(preview(&runtime).is_none());

        hover(&mut runtime, regular, "tab", 0.0);
        runtime.frame(0.45);
        assert_eq!(hover::displayed(&runtime.state), Some(regular));
        runtime.update(|state, _| state.browser.activate_tab(regular));
        runtime.frame(0.01);
        assert!(preview(&runtime).is_none());
    }

    #[test]
    fn closing_the_hovered_tab_cancels_its_preview() {
        let (mut runtime, [_, second, _]) = runtime(true);
        hover(&mut runtime, second, "tab", 0.01);
        runtime.frame(0.42);
        assert!(preview(&runtime).is_some());
        runtime.update(|state, _| state.close(second));
        runtime.frame(0.01);
        assert!(preview(&runtime).is_none());
    }
}
