// SPDX-License-Identifier: GPL-3.0-only

//! Persistent favourites and the scrollable regular tab list.

use rune::{El, GrowthSpec, ScrollSpec, Spring, col};

use crate::app::AppState;
use crate::tab::{Tab, TabSection};
use crate::ui::theme::{self, Theme};
use elaris_core::settings::NewTabPosition;

mod context_menu;
mod drag_handler;
mod icon;
mod overlay;
mod placeholder;
mod row;
mod sections;
mod split;

pub(crate) use context_menu::context_menu;
pub(in crate::ui) use drag_handler::tab_drag_handler;
pub(in crate::ui) use icon::tab_icon;
pub(in crate::ui) use overlay::drag_overlay;
pub(crate) use overlay::floating_drag_preview;
use sections::{divider, favourites, new_tab_button, presence_motion, regular_rows};

pub(super) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let favourite_items = state
        .tab_drag
        .preview_items(state.browser.tabs(), TabSection::Favourite);
    let has_favourites = !favourite_items.is_empty();
    let reorder_motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let new_tab_top = state.settings.new_tab_position == NewTabPosition::Top;
    let mut scroll_content = col()
        .key("tabs-scroll-content")
        .w_full()
        .gap(theme::TAB_GAP)
        .growth(GrowthSpec {
            spring: presence_motion(reduced_motion),
            edge: theme::TAB_PRESENCE_EDGE,
        })
        .motion(reorder_motion);
    if !has_favourites && !state.browser.tabs().is_empty() {
        scroll_content =
            scroll_content.child(divider("regular-tabs-divider", theme).arrives_whole());
    }
    if new_tab_top {
        scroll_content = scroll_content.child(new_tab_button(theme).arrives_whole());
    }
    scroll_content =
        scroll_content.child(regular_rows(state, active, theme, reduced_motion).arrives_whole());
    if !new_tab_top {
        scroll_content = scroll_content.child(new_tab_button(theme).arrives_whole());
    }

    let scroll_live = state.tab_drag.scroll_live();
    let scroll = col()
        .key("tabs-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            state.tab_drag.set_viewport(rect);
            if !has_favourites {
                state.tab_drag.set_empty_favourites_anchor(rect);
            }
            tx.live_only();
        })
        .on_scroll_offset(move |position| {
            scroll_live.set([position.offset[1], position.max[1]]);
        })
        .on_secondary_press(|state: &mut AppState, press, _| {
            state.open_context_menu(None, press);
        })
        .child(scroll_content);

    let mut sidebar_tabs = col().key("tab-sections").w_full().grow(1.0).min_h(0.0);
    if has_favourites {
        sidebar_tabs = sidebar_tabs
            .child(favourites(
                state,
                active,
                &favourite_items,
                theme,
                reduced_motion,
            ))
            .child(divider("favourites-divider", theme))
            .gap(theme::SIDEBAR_GAP);
    }
    sidebar_tabs = sidebar_tabs.child(scroll);

    drag_overlay(state, theme, overlays, reduced_motion);
    sidebar_tabs
}
