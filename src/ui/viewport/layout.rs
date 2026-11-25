// SPDX-License-Identifier: GPL-3.0-only

//! Lays out the viewport's panes: one page, two side by side, or a grid, with the add-pane button.

use super::divider::{divider, grid_divider};
use super::overlay::{header_above_page, indicator};
use super::page::{page, page_surface};
use super::picker::{self, PICKER};
use crate::app::AppState;
use crate::browser::split;
use crate::tab::TabId;
use crate::ui::theme::{self, Theme};
use rune::{El, el};

/// The container's padding holds the page in from the window's edges, the sidebar being the
/// edge it sits against; the page's own element clips to rounded corners and takes input in the page's
/// coordinates.
pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    let top_padding = if header_above_page(state) {
        theme::HEADER_HEIGHT
    } else {
        state.page_inset()
    };
    let fullscreen = state.browser.fullscreen_tab().is_some();
    let top_padding = if fullscreen { 0.0 } else { top_padding };
    let horizontal = state.horizontal_tabs();
    let right = state.sidebar_on_right();
    let inset = if fullscreen { 0.0 } else { state.page_inset() };
    let radius = if fullscreen { 0.0 } else { state.page_radius() };
    let viewport = rune::row()
        .key("viewport-container")
        .grow(1.0)
        // Its size comes from the window, never from the frame inside: otherwise the last
        // frame holds it open and a narrower window never reaches the page.
        .min_w(0.0)
        .min_h(0.0)
        // With the sidebar hidden, the page sits below the header and in from the left edge too.
        .pt(if fullscreen {
            0.0
        } else if horizontal {
            state.page_top_inset()
        } else {
            top_padding
        })
        .pl(if horizontal || state.sidebar.collapsed || right {
            inset
        } else {
            0.0
        })
        .pr(if right && !state.sidebar.collapsed {
            0.0
        } else {
            inset
        })
        .pb(inset);
    let viewport = if horizontal {
        viewport.w_full().h(0.0)
    } else {
        viewport.h_full()
    };
    let panes = state.browser.visible_tabs();
    let preview = state.split_preview;
    let ratio = state.browser.displayed_split().map(|split| split.ratio);
    let layout = rune::row()
        .key("page-layout")
        .w_full()
        .h_full()
        .min_w(0.0)
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            if state.split_rect == rect {
                tx.live_only();
            }
            state.split_rect = rect;
            state.browser.set_viewport([rect[2], rect[3]]);
        });
    let wrap = |id: Option<TabId>, side: usize, weight: f32, child: El| {
        let mut pane = el()
            .key(wrapper_key(id, side))
            .w(0.0)
            .grow(weight)
            .min_w(0.0)
            .h_full()
            .min_h(0.0)
            .fill(theme.page)
            // The frame scales with the pane's layout spring; its child surface stays clipped
            // to these same animated bounds while the engine renders the new viewport size.
            .continuous_corners()
            .clip_rounded(radius);
        if let Some(tab) = id
            .and_then(|id| state.browser.tab(id))
            .filter(|tab| !tab.is_internal())
        {
            // The surface stays on the tab's keyed container when the input region changes sides.
            pane = page_surface(pane, &tab.surface);
        }
        pane = pane.child(child);
        if !horizontal
            && ratio.is_some()
            && id.is_some()
            && id == state.browser.active_tab().map(crate::tab::Tab::id)
        {
            pane = pane.child(
                el().key("focused-pane-border")
                    .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                    .radius(radius)
                    .continuous_corners()
                    .border(1.0, theme.overlay_border)
                    .inert()
                    .a11y_hidden(),
            );
        }
        pane
    };
    let preview_pane = || wrap(None, 1, 1.0, indicator(theme, radius));
    if ratio.is_none() {
        let page = wrap(
            panes[0],
            0,
            1.0,
            page(state, theme, overlays, reduced_motion, 0),
        );
        let layout = match preview {
            Some(0) => layout
                .child(preview_pane())
                .child(divider(false, theme))
                .child(page),
            Some(1) => layout
                .child(page)
                .child(divider(false, theme))
                .child(preview_pane()),
            _ => layout.child(page),
        };
        return viewport.child(layout);
    }
    let append_preview = (preview == Some(panes.len())).then(|| {
        let [_, _, width, height] = state.split_rect;
        let [x, y, w, h] = split::grid_cell_rect(
            [0.0, 0.0, width, height],
            panes.len() + 1,
            panes.len(),
            ratio.unwrap_or(0.5),
        );
        indicator(theme, radius)
            .key("split-add-preview")
            .absolute(x, y)
            .size(w, h)
            .border(2.0, theme.focus)
    });
    if panes.len() == 2 {
        let pane_widths = split::widths(state.browser.content_width(), ratio.unwrap_or(0.5));
        let available = pane_widths.iter().sum::<f32>();
        let mut layout = layout;
        for (side, id) in panes.into_iter().enumerate() {
            let child = if id.is_some() {
                page(state, theme, overlays, reduced_motion, side)
            } else {
                picker::view(state, theme, side)
            };
            let weight = if available > 0.0 {
                pane_widths[side] / available
            } else {
                0.5
            };
            if side == 1 {
                layout = layout.child(divider(true, theme));
            }
            layout = layout.child(wrap(id, side, weight, child));
        }
        if let Some(button) = add_split_pane_button(state, theme) {
            layout = layout.child(button);
        }
        return viewport.child(layout.children(append_preview));
    }
    let columns = (panes.len() as f32).sqrt().ceil() as usize;
    let mut grid = rune::col()
        .key("split-grid")
        .w_full()
        .h_full()
        .min_h(0.0)
        .gap(split::DIVIDER);
    for (row_index, row_panes) in panes.chunks(columns.max(1)).enumerate() {
        let mut grid_row = rune::row()
            .key(format!("split-grid-row-{row_index}"))
            .w_full()
            .h(0.0)
            .min_h(0.0)
            .grow(1.0);
        for (column, id) in row_panes.iter().copied().enumerate() {
            let side = row_index * columns + column;
            let child = if id.is_some() {
                page(state, theme, overlays, reduced_motion, side)
            } else {
                picker::view(state, theme, side)
            };
            let child = if preview == Some(side) {
                child.child(indicator(theme, radius).pin(
                    Some(0.0),
                    Some(0.0),
                    Some(0.0),
                    Some(0.0),
                ))
            } else {
                child
            };
            let count = row_panes.len();
            let first = split::grid_first_share(count, ratio.unwrap_or(0.5));
            let weight = match (count, column) {
                (1, _) => 1.0,
                (_, 0) => first,
                _ => (1.0 - first) / (count - 1) as f32,
            };
            if column == 1 {
                grid_row = grid_row.child(grid_divider(row_index, count, theme));
            } else if column > 1 {
                grid_row = grid_row.child(el().w(split::DIVIDER).inert());
            }
            grid_row = grid_row.child(wrap(id, side, weight, child));
        }
        grid = grid.child(grid_row);
    }
    let mut layout = layout.child(grid);
    if let Some(button) = add_split_pane_button(state, theme) {
        layout = layout.child(button);
    }
    viewport.child(layout.children(append_preview))
}

fn add_split_pane_button(state: &AppState, theme: &Theme) -> Option<El> {
    let split = state.browser.displayed_split()?;
    (split.pane_count < state.browser.split_view_limit.min(split::MAX_PANES)).then(|| {
        crate::ui::components::controls::floating_icon_button(
            "add-split-pane",
            "Add Pane",
            &crate::ui::icons::PLUS,
            theme::ICON_BUTTON,
            theme,
        )
        .pin(
            None,
            Some(theme::VIEWPORT_INSET),
            Some(theme::VIEWPORT_INSET),
            None,
        )
        .on_press_tx(|state: &mut AppState, tx| {
            if state.browser.add_split_pane() {
                state.split_selection = 0;
                tx.focus(PICKER, false);
                tx.animated();
            }
        })
    })
}

fn wrapper_key(id: Option<TabId>, side: usize) -> String {
    id.map_or_else(
        || format!("empty-web-pane-{side}"),
        |id| format!("web-pane-{}", id.raw()),
    )
}
