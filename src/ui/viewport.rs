// SPDX-License-Identifier: GPL-3.0-only

//! Rounded webpage panes and their pointer, wheel, and keyboard input.

pub(crate) mod menu;
mod picker;
pub(crate) use picker::PICKER;

use crate::browser::{Browser, split};
use crate::{engine::ViewCommand, tab::TabId};
use elaris::ladybird::{Edit, MouseKind};
use rune::{El, Phase, el};
use rune::{Ime, Key, KeyEvent, Tx};

use super::VIEWPORT;
use super::theme::{self, Theme};
use crate::app::AppState;
use rune::PointerButton;

/// The container's padding holds the page in from the window's edges, the sidebar being its
/// left; the page's own element clips to rounded corners and takes input in the page's
/// coordinates.
pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    #[cfg(target_os = "linux")]
    let top_padding = theme::VIEWPORT_INSET;
    #[cfg(not(target_os = "linux"))]
    let top_padding = if state.sidebar.collapsed {
        theme::HEADER_HEIGHT
    } else {
        theme::VIEWPORT_INSET
    };
    let horizontal = state.horizontal_tabs();
    let inset = theme::VIEWPORT_INSET;
    let radius = if horizontal {
        theme::HORIZONTAL_VIEWPORT_RADIUS
    } else {
        theme::VIEWPORT_RADIUS
    };
    let viewport = rune::row()
        .key("viewport-container")
        .grow(1.0)
        // Its size comes from the window, never from the frame inside: otherwise the last
        // frame holds it open and a narrower window never reaches the page.
        .min_w(0.0)
        .min_h(0.0)
        // With the sidebar hidden, the page sits below the header and in from the left edge too.
        .pt(if horizontal { inset } else { top_padding })
        .pl(if horizontal || state.sidebar.collapsed {
            inset
        } else {
            0.0
        })
        .pr(inset)
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
            .continuous_corners()
            .clip_rounded(radius);
        if let Some(tab) = id
            .and_then(|id| state.browser.tab(id))
            .filter(|tab| !tab.is_internal())
        {
            // The surface stays on the tab's keyed container when the input region changes sides.
            pane = pane.pixels(&tab.surface.pixels);
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
        return viewport.child(layout);
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
            .grow(1.0)
            .gap(split::DIVIDER);
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
            grid_row = grid_row.child(wrap(id, side, 1.0, child));
        }
        for column in row_panes.len()..columns {
            grid_row = grid_row.child(
                el().key(format!("split-grid-empty-{row_index}-{column}"))
                    .w(0.0)
                    .h_full()
                    .min_h(0.0)
                    .grow(1.0)
                    .inert(),
            );
        }
        grid = grid.child(grid_row);
    }
    viewport.child(layout.child(grid))
}

fn wrapper_key(id: Option<TabId>, side: usize) -> String {
    id.map_or_else(
        || format!("empty-web-pane-{side}"),
        |id| format!("web-pane-{}", id.raw()),
    )
}

pub(crate) fn focus_key(browser: &Browser) -> &'static str {
    pane_key(browser.focused_side())
}

pub(crate) fn page_focus_key(browser: &Browser) -> &'static str {
    if browser.peek.is_some() {
        super::peek::PAGE
    } else {
        focus_key(browser)
    }
}

fn pane_key(side: usize) -> &'static str {
    PANE_KEYS
        .get(side)
        .copied()
        .unwrap_or("right-page-viewport")
}

const PANE_KEYS: [&str; 64] = [
    VIEWPORT,
    "right-page-viewport",
    "split-page-2",
    "split-page-3",
    "split-page-4",
    "split-page-5",
    "split-page-6",
    "split-page-7",
    "split-page-8",
    "split-page-9",
    "split-page-10",
    "split-page-11",
    "split-page-12",
    "split-page-13",
    "split-page-14",
    "split-page-15",
    "split-page-16",
    "split-page-17",
    "split-page-18",
    "split-page-19",
    "split-page-20",
    "split-page-21",
    "split-page-22",
    "split-page-23",
    "split-page-24",
    "split-page-25",
    "split-page-26",
    "split-page-27",
    "split-page-28",
    "split-page-29",
    "split-page-30",
    "split-page-31",
    "split-page-32",
    "split-page-33",
    "split-page-34",
    "split-page-35",
    "split-page-36",
    "split-page-37",
    "split-page-38",
    "split-page-39",
    "split-page-40",
    "split-page-41",
    "split-page-42",
    "split-page-43",
    "split-page-44",
    "split-page-45",
    "split-page-46",
    "split-page-47",
    "split-page-48",
    "split-page-49",
    "split-page-50",
    "split-page-51",
    "split-page-52",
    "split-page-53",
    "split-page-54",
    "split-page-55",
    "split-page-56",
    "split-page-57",
    "split-page-58",
    "split-page-59",
    "split-page-60",
    "split-page-61",
    "split-page-62",
    "split-page-63",
];

pub(crate) fn drop_side(state: &AppState, id: TabId, point: [f32; 2]) -> Option<usize> {
    let panes = state.browser.visible_tabs();
    if panes.len() == 2 {
        let ratio = state
            .browser
            .displayed_split()
            .map_or(0.5, |split| split.ratio);
        let [x, y, width, height] = state.split_rect;
        if point[0] < x || point[0] >= x + width || point[1] < y || point[1] >= y + height {
            return None;
        }
        let side =
            usize::from(point[0] >= x + split::widths(width, ratio)[0] + split::DIVIDER / 2.0);
        return (panes[side] != Some(id)).then_some(side);
    }
    let side = split::target(state.split_rect, point, panes.len())?;
    if side >= panes.len() {
        return None;
    }
    if state.browser.displayed_split().is_some() {
        (panes[side] != Some(id)).then_some(side)
    } else {
        panes[0].map(|_| side)
    }
}

fn divider(live: bool, theme: &Theme) -> El {
    let handle = el()
        .key("split-divider")
        .w(split::DIVIDER)
        .h_full()
        .center()
        .child(
            el().w(1.0)
                .h(24.0)
                .radius(0.5)
                .fill(theme.overlay_border)
                .inert(),
        );

    if !live {
        return handle;
    }
    handle
        .cursor(rune::Cursor::ResizeColumn)
        .label("Resize split panes")
        .on_drag(|state: &mut AppState, drag, tx| {
            if drag.phase == Phase::Down {
                if let Some(split) = state.browser.displayed_split() {
                    drag.data = split::widths(state.browser.content_width(), split.ratio)[0];
                }
            } else if !drag.cancelled() {
                state.browser.resize_split(drag.data + drag.dx);
                tx.instant();
            }
        })
}

fn indicator(theme: &Theme, radius: f32) -> El {
    el().w_full()
        .h_full()
        .inert()
        .a11y_hidden()
        .fill(theme.selection)
        .radius(radius)
}

/// The webpage's top-left origin in the window, used to anchor its native find overlay.
pub(crate) fn page_origin(state: &AppState) -> [f32; 2] {
    let left = if state.sidebar.collapsed {
        theme::VIEWPORT_INSET
    } else {
        state.sidebar.width
    };
    #[cfg(target_os = "linux")]
    let top = theme::HEADER_HEIGHT + theme::VIEWPORT_INSET;
    #[cfg(not(target_os = "linux"))]
    let top = if state.sidebar.collapsed {
        theme::HEADER_HEIGHT
    } else {
        theme::VIEWPORT_INSET
    };
    let offset = state
        .browser
        .displayed_split()
        .map_or([0.0, 0.0], |split_state| {
            if state.browser.visible_tabs().len() == 2 {
                return [
                    split::widths(state.split_rect[2], split_state.ratio)[0] + split::DIVIDER,
                    0.0,
                ];
            }
            let rect = split::grid_cell_rect(
                [0.0, 0.0, state.split_rect[2], state.split_rect[3]],
                state.browser.visible_tabs().len(),
                state.browser.focused_side(),
            );
            [rect[0], rect[1]]
        });
    if state.horizontal_tabs() {
        [
            theme::VIEWPORT_INSET + offset[0],
            theme::HORIZONTAL_CHROME_HEIGHT + theme::VIEWPORT_INSET + offset[1],
        ]
    } else {
        [left + offset[0], top + offset[1]]
    }
}

/// The page's latest frame fills the viewport over its appearance-aware background. During a live
/// resize Rune scales a frame to new bounds.
fn page(
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
            page = page.child(super::internal::view(
                state,
                route,
                theme,
                overlays,
                reduced_motion,
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
    }
    let mut page = page.w_full().h_full();
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
                state.browser.wheel_page(id, wheel);
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

pub(super) fn mouse_kind(phase: Phase, cancelled: bool) -> MouseKind {
    match phase {
        Phase::Down => MouseKind::Down,
        Phase::Move => MouseKind::Move,
        Phase::Up if cancelled => MouseKind::Cancel,
        Phase::Up => MouseKind::Up,
    }
}

fn paste(id: TabId, tx: &mut Tx) {
    tx.read_clipboard(move |s: &mut AppState, content, tx| {
        if !s.omnibox.open
            && tx.focused() == Some(page_focus_key(&s.browser))
            && s.browser.input_page() == Some(id)
            && !content.text().is_empty()
        {
            s.browser.send(
                id,
                ViewCommand::Edit(Edit::Paste(content.text().to_owned())),
            );
        }
    });
}

#[cfg(target_os = "macos")]
pub(crate) enum PageEdit {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Applies a native edit menu action to the focused Ladybird page, if it owns focus.
#[cfg(target_os = "macos")]
pub(crate) fn menu_edit(state: &mut AppState, edit: PageEdit, tx: &mut Tx) -> bool {
    let Some(id) = state.browser.input_page().filter(|id| {
        tx.focused() == Some(page_focus_key(&state.browser))
            && !state.omnibox.open
            && state
                .browser
                .page(*id)
                .is_some_and(|page| !page.is_internal())
    }) else {
        return false;
    };
    match edit {
        PageEdit::Cut => state.browser.send(id, ViewCommand::Edit(Edit::Cut)),
        PageEdit::Copy => state.browser.send(id, ViewCommand::Edit(Edit::Copy)),
        PageEdit::Paste => paste(id, tx),
        PageEdit::SelectAll => state.browser.send(id, ViewCommand::Edit(Edit::SelectAll)),
    }
    true
}

pub(super) fn page_key(state: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if key.key == Key::Escape && super::peek::dismiss(state, tx) {
        return true;
    }
    if key.key == Key::Escape && (state.tab_drag.is_dragging() || state.has_split_picker()) {
        return crate::shortcuts::on_key(state, key, tx);
    }
    if crate::ui::tab_switcher::cycle_key(state, key, tx) {
        return true;
    }
    let Some(id) = state.browser.input_page().filter(|id| {
        state
            .browser
            .page(*id)
            .is_some_and(|page| !page.is_internal())
    }) else {
        return false;
    };
    if state.omnibox.open || state.page_menu.open() {
        return false;
    }
    if key.mods.cmd
        && !key.mods.shift
        && !key.mods.alt
        && let Key::Command(c) = key.key
    {
        let edit = match c {
            'a' => Edit::SelectAll,
            'x' => Edit::Cut,
            'c' => Edit::Copy,
            'v' => {
                paste(id, tx);
                return true;
            }
            _ => return false,
        };
        state.browser.send(id, ViewCommand::Edit(edit));
        return true;
    }
    if let Key::Text(s) = &key.key
        && s.chars().count() > 1
    {
        state
            .browser
            .send(id, ViewCommand::Edit(Edit::Commit(s.clone())));
        return true;
    }
    state
        .browser
        .key(key, true, tx.key_repeat(), tx.physical_key())
}

pub(crate) fn ime(state: &mut AppState, event: &Ime, tx: &mut Tx) -> bool {
    if tx.focused() != Some(page_focus_key(&state.browser))
        || state.omnibox.open
        || state.page_menu.open()
    {
        return false;
    }
    let Some(tab) = state
        .browser
        .input_page()
        .and_then(|id| state.browser.page(id))
        .filter(|tab| tab.ime_caret.is_some())
    else {
        return false;
    };
    let edit = match event {
        Ime::Preedit(text, ..) => Edit::Mark(text.clone()),
        Ime::Commit(text) => Edit::Commit(text.clone()),
        Ime::Disabled => Edit::Mark(String::new()),
        _ => return false,
    };
    state.browser.send(tab.id(), ViewCommand::Edit(edit));
    true
}
