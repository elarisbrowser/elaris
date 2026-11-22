// SPDX-License-Identifier: GPL-3.0-only

//! The sliding highlight inside a menu's rows. The pointer entering a row moves it on the next
//! poll, so it glides; keyboard focus moves it only when focus itself moves. It lives in the
//! panel and is placed from the rows' fixed heights, never from a window-level overlay read from
//! last frame's layout, which a native page could leave hidden.

use rune::{El, Spring, Tx, el};

use crate::app::AppState;
use crate::ui::theme::{self, Theme};

/// The row keys of the open menu that draws a sliding highlight, or "" when none is open.
fn scope(state: &AppState) -> &'static str {
    if state.context_menu().is_some() {
        "sidebar-context-menu-item"
    } else if state.page_menu.open() {
        "page-context-menu-item"
    } else if state.omnibox.search_engine_menu_open {
        "search-engine-menu-item"
    } else {
        ""
    }
}

/// Moves the highlight to a row the pointer entered, or to a row focus moved to. Reading focus
/// every poll let anything else that held focus, such as a native page, clear the pointer's
/// highlight a moment after it appeared.
pub(crate) fn poll(state: &mut AppState, tx: &mut Tx) -> bool {
    let scope = scope(state);
    if scope.is_empty() {
        state.context_menu_focus = None;
        state.context_menu_hover = None;
        let cleared = state.context_menu_highlight.take().is_some();
        if cleared {
            tx.animated();
        }
        return cleared;
    }
    // A pointer move commits live, which would land the highlight at once; applying the entered
    // row here lets it glide.
    let entered = state.context_menu_hover.take();
    let focused = tx
        .focused()
        .filter(|key| key.starts_with(scope))
        .map(str::to_owned);
    let next = if entered.is_some() {
        entered
    } else if focused.is_some() && focused != state.context_menu_focus {
        state.context_menu_focus = focused.clone();
        focused
    } else {
        return false;
    };
    if next == state.context_menu_highlight {
        return false;
    }
    state.context_menu_highlight = next;
    tx.animated();
    true
}

/// The pointer entering a row asks the next poll to move the highlight there.
pub(super) fn hover_row(state: &mut AppState, key: &str, inside: bool, tx: &mut Tx) {
    let scope = scope(state);
    if inside
        && !scope.is_empty()
        && key.starts_with(scope)
        && state.context_menu_highlight.as_deref() != Some(key)
    {
        state.context_menu_hover = Some(key.to_owned());
        tx.wake_after(std::time::Duration::ZERO);
    }
}

/// Leaving the panel clears the pointer's highlight, and moving over it wakes the poll.
pub(super) fn track(panel: El) -> El {
    panel
        .on_hover_change(|state: &mut AppState, inside, tx| {
            if !inside && state.context_menu_highlight.take().is_some() {
                tx.animated();
            }
        })
        .on_pointer_move_local(|_: &mut AppState, _, tx| {
            tx.wake_after(std::time::Duration::ZERO);
            tx.live_only();
        })
}

/// The highlight under a row `top` points down the menu keyed `key`.
pub(super) fn view(key: &str, top: f32, reduced: bool, theme: &Theme) -> El {
    el().key(format!("{key}-highlight"))
        .pin(Some(0.0), Some(0.0), Some(0.0), None)
        .h(theme::MENU_ROW_HEIGHT)
        .offset(0.0, top)
        .offset_motion(if reduced {
            Spring::NONE
        } else {
            theme::HIGHLIGHT_MOTION
        })
        .radius(theme::MENU_ITEM_RADIUS)
        .continuous_corners()
        .fill(theme.hover)
        .inert()
        .a11y_hidden()
}

/// The row of the menu keyed `key` that its sliding highlight sits on, if the highlight is in
/// this menu.
pub(super) fn sliding_row(state: &AppState, key: &str) -> Option<String> {
    state
        .context_menu_highlight
        .clone()
        .filter(|row| row.starts_with(&format!("{key}-item")))
}
