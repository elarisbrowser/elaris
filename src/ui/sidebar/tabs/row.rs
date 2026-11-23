// SPDX-License-Identifier: GPL-3.0-only

//! Tab rows and favourite tiles: the rendered element for one tab in either section.

use rune::{El, Spring, el};

use super::drag_handler::tab_drag_handler;
use super::icon::{dim_if_asleep, tab_icon};
use super::split::{TabRowStyle, members as split_group_members};
use crate::app::AppState;
use crate::tab::{Tab, TabSection};
use crate::ui::components::controls::{icon_button_style, row_button};
use crate::ui::icons;
use crate::ui::sidebar::hover;
use crate::ui::theme::{self, Theme};

#[allow(clippy::too_many_arguments)]
pub(super) fn favourite_tile(
    state: &AppState,
    tab: &Tab,
    active: bool,
    index: usize,
    count: usize,
    width: f32,
    motion: Spring,
    theme: &Theme,
) -> El {
    let id = tab.id();
    let label = tab.label();
    let hidden = state.tab_drag.is_hidden(id);
    let mut tile = el()
        .key(format!("favourite-{}", id.raw()))
        .w(width)
        .h(theme::FAVOURITE_TILE_HEIGHT)
        .center()
        .radius(theme::FAVOURITE_TILE_RADIUS)
        .continuous_corners()
        .motion(motion)
        .press_scale(theme::PRESS_SCALE)
        .label(label)
        .child(
            tab_icon(state, tab, active, theme::FAVOURITE_ICON, theme)
                .key(format!("favourite-icon-{}", id.raw())),
        )
        .position_in_set(index + 1, count)
        .on_press_tx(move |state: &mut AppState, tx| {
            tx.color_motion(theme::SELECTION_MOTION);
            state.activate(id);
            if state.browser.split.is_some() {
                tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            }
        })
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .on_secondary_press(move |state: &mut AppState, press, _| {
            state.open_context_menu(Some(id), press);
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            let target_ready = state
                .tab_drag
                .record_tab_rect(TabSection::Favourite, id, rect);
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        });
    tile = tab_drag_handler(tile, id, vec![id]);
    if active {
        tile = tile.fill(theme.tab_active);
    } else {
        tile = tile.fill(theme.address_field);
    }
    tile = dim_if_asleep(tile, tab);
    if hidden {
        tile = tile.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    hover::attach(tile, id)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tab_row(
    state: &AppState,
    tab: &Tab,
    section: TabSection,
    active: bool,
    index: usize,
    count: usize,
    theme: &Theme,
    style: TabRowStyle,
) -> El {
    let id = tab.id();
    let label = tab.label();
    let hidden = state.tab_drag.is_hidden(id);
    let color = if active { theme.text } else { theme.text_muted };
    let leading = tab_icon(state, tab, active, theme::ICON, theme);
    let close = icon_button_style(
        &format!("close-{}", id.raw()),
        "Close tab",
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        theme,
    )
    .hover(theme.hover)
    .on_press(move |state: &mut AppState| state.close(id));
    let close = if active {
        close
    } else {
        close.group_opacity(0.0, 1.0)
    };
    let mut row = row_button(&format!("tab-{}", id.raw()), leading, label, color, theme)
        .hover_group()
        .child(close)
        .position_in_set(index + 1, count);
    row = row
        .on_press_tx(move |state: &mut AppState, tx| {
            tx.color_motion(theme::SELECTION_MOTION);
            state.activate(id);
            if state.browser.split.is_some() {
                tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            }
        })
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .on_secondary_press(move |state: &mut AppState, press, _| {
            state.open_context_menu(Some(id), press);
        });
    match style {
        TabRowStyle::Standalone if active => row = row.fill(theme.tab_active),
        TabRowStyle::Standalone if state.tab_drag.is_hidden(id) => {
            row = row.fill(theme.tab_drag);
        }
        TabRowStyle::Grouped { selected } => {
            row = row.fill(if selected {
                theme.address_field
            } else {
                theme.tab_drag
            });
        }
        TabRowStyle::Standalone => {}
    }
    if active && matches!(style, TabRowStyle::Standalone) {
        row = row.shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
    row = dim_if_asleep(row, tab);
    if hidden {
        row = row.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    let drag_members = split_group_members(state, section)
        .filter(|pair| pair.contains(&id))
        .map_or_else(|| vec![id], |pair| pair.to_vec());
    row = tab_drag_handler(row, id, drag_members);
    let row = row.on_layout_tx(move |state: &mut AppState, rect, tx| {
        let target_ready = state.tab_drag.record_tab_rect(section, id, rect);
        if target_ready {
            tx.animated();
            tx.motion(theme::TAB_REORDER_MOTION);
            tx.wake_after(std::time::Duration::ZERO);
        } else {
            tx.live_only();
        }
    });
    hover::attach(row, id)
}
