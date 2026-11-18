// SPDX-License-Identifier: GPL-3.0-only

//! The open tabs, scrolling once they outgrow the sidebar, then the new-tab button.

use rune::{El, ScrollSpec, Tx, col, el, icon};
use rune_kit::{Kit, MenuItem, PaletteSeed, context_menu as menu_at};

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
        .on_secondary_press(|state: &mut AppState, press, _| {
            state.open_context_menu(None, press);
        })
        .child(list)
}

fn tab_row(tab: &Tab, active: bool, theme: &Theme) -> El {
    let id = tab.id();
    let color = if active { theme.text } else { theme.text_muted };
    let leading = if tab.loading {
        icon(&icons::LOADING, theme::ICON)
            .color(color)
            .label("Loading")
            .looping(rune::LoopSpec::rotation(theme::LOADING_PERIOD))
    } else if let Some(favicon) = &tab.favicon {
        el().size(theme::ICON, theme::ICON).center().pixels(favicon)
    } else {
        icon(&icons::PAGE, theme::ICON).color(color)
    };
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
    .on_secondary_press(move |state: &mut AppState, press, _| {
        state.open_context_menu(Some(id), press);
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

pub(crate) fn context_menu(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some((tab, point)) = state.context_menu() else {
        return;
    };
    let mut menu = menu_at("sidebar-context-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.dismiss_context_menu());
    if let Some(tab) = tab {
        menu = menu
            .item(
                MenuItem::new("Reload").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.reload(tab);
                }),
            )
            .item(
                MenuItem::new("Duplicate").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.duplicate(tab);
                }),
            )
            .separator()
            .item(
                MenuItem::new("Close Tab").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.close(tab);
                }),
            )
            .item(
                MenuItem::new("Close Other Tabs").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.close_others(tab);
                }),
            );
    } else {
        menu = menu.item(MenuItem::new("New Tab").on_select_tx(
            |state: &mut AppState, tx: &mut Tx| {
                state.dismiss_context_menu();
                app::new_tab(state, tx);
            },
        ));
    }
    let kit = Kit::from_palette(PaletteSeed {
        bg: theme.window,
        fg: theme.text,
        accent: theme.icon,
    });
    menu.build(&kit, layer);
}
