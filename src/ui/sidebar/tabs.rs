// SPDX-License-Identifier: GPL-3.0-only

//! The open tabs, scrolling once they outgrow the sidebar, then the new-tab button.

use rune::{El, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, icon};

use crate::app::{self, AppState};
use crate::tab::Tab;
use crate::ui::controls::{icon_button, row_button};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

pub(super) fn view(state: &AppState, theme: &Theme) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let mut list = col().key("tabs").w_full().gap(theme::TAB_GAP).label("Tabs");
    for tab in state.browser.tabs() {
        list = list.child(tab_row(tab, Some(tab.id()) == active, theme));
    }
    list = list.child(
        row_button(
            "new-tab",
            icon(&icons::PLUS, theme::ICON).color(theme.text_muted),
            "New Tab",
            theme.text_muted,
            theme,
        )
        .on_press_tx(app::new_tab),
    );
    col()
        .key("tabs-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
}

fn tab_row(tab: &Tab, active: bool, theme: &Theme) -> El {
    let id = tab.id();
    let glyph = if tab.loading {
        &icons::LOADING
    } else {
        &icons::PAGE
    };
    let color = if active { theme.text } else { theme.text_muted };
    let leading = tab.favicon.as_ref().map_or_else(
        || icon(glyph, theme::ICON).color(color),
        |favicon| el().size(theme::ICON, theme::ICON).center().pixels(favicon),
    );
    let close = icon_button(
        &format!("close-{}", id.raw()),
        "Close tab",
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        theme,
    )
    .on_press(move |state: &mut AppState| state.close(id));
    // Only the active tab shows its close button at rest; the others show it under the pointer.
    let close = if active {
        close
    } else {
        close.group_opacity(0.0, 1.0)
    };
    let row = row_button(
        &format!("tab-{}", id.raw()),
        leading,
        tab.label(),
        color,
        theme,
    )
    .help_when_truncated(tab.label())
    .hover_group()
    .on_press_tx(move |state: &mut AppState, tx| {
        tx.color_motion(theme::SELECTION_MOTION);
        state.activate(id);
    })
    .on_middle_press(move |state: &mut AppState, _| state.close(id))
    .on_secondary_press(move |_: &mut AppState, press, tx: &mut Tx| {
        let menu = PopupMenu::new()
            .item(PopupItem::new("reload", "Reload"))
            .item(PopupItem::new("duplicate", "Duplicate"))
            .separator()
            .item(PopupItem::new("close", "Close Tab"))
            .item(PopupItem::new("close-others", "Close Other Tabs"));
        tx.popup_menu(
            menu,
            press,
            move |state: &mut AppState, picked, _| match picked {
                Some("reload") => state.reload(id),
                Some("duplicate") => state.duplicate(id),
                Some("close") => state.close(id),
                Some("close-others") => state.close_others(id),
                _ => {}
            },
        );
    })
    .child(close);
    if active {
        row.fill(theme.tab_active).shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        )
    } else {
        row
    }
}
