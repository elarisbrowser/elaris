// SPDX-License-Identifier: GPL-3.0-only

use rune::{El, Tx, el};
use rune_kit::{MenuItem, context_menu as menu_at};

use crate::app::{self, AppState};
use crate::tab::{Tab, TabSection};
use crate::ui::theme::{self, Theme};

pub(crate) fn context_menu(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some((tab, point)) = state.context_menu() else {
        return;
    };
    let mut menu = menu_at("sidebar-context-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.dismiss_context_menu());
    if let Some(tab) = tab {
        let section = state
            .browser
            .tab(tab)
            .map_or(TabSection::Regular, Tab::section);
        let section_action = match section {
            TabSection::Favourite => ("Remove from Favourites", TabSection::Regular),
            TabSection::Regular => ("Add to Favourites", TabSection::Favourite),
        };
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
            .item(MenuItem::new(section_action.0).on_select_tx(
                move |state: &mut AppState, tx: &mut Tx| {
                    state.dismiss_context_menu();
                    state.move_tab_to_section_animated(tab, section_action.1, tx);
                },
            ));
        for (side, label) in ["Split Left", "Split Right"].into_iter().enumerate() {
            menu = menu.item(
                MenuItem::new(label).on_select_tx(move |state: &mut AppState, tx| {
                    state.dismiss_context_menu();
                    state.dismiss_find();
                    if state.browser.split.is_some_and(|split| {
                        split
                            .panes(state.browser.split_view_limit)
                            .iter()
                            .all(Option::is_some)
                    }) {
                        state.place_split_tab(tab, side);
                        tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                    } else {
                        state.begin_split_picker(tab, side);
                        tx.focus(crate::ui::viewport::PICKER, false);
                    }
                    tx.animated();
                    tx.motion(theme::TAB_REORDER_MOTION);
                }),
            );
        }
        if let Some(split) = state.browser.split {
            menu = menu.item(MenuItem::new("Swap Panes").on_select_tx(
                |state: &mut AppState, tx| {
                    state.dismiss_context_menu();
                    state.browser.swap_split();
                    tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                    tx.animated();
                },
            ));
            if split.panes.contains(&Some(tab))
                && let Some(other) = split
                    .panes(state.browser.split_view_limit)
                    .iter()
                    .flatten()
                    .copied()
                    .find(|id| *id != tab)
            {
                menu = menu.item(MenuItem::new("Remove Pane").on_select_tx(
                    move |state: &mut AppState, tx| {
                        state.dismiss_context_menu();
                        state.browser.exit_split(Some(other));
                        state.activate(other);
                        tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                        tx.animated();
                    },
                ));
            }
            menu = menu.item(MenuItem::new("Exit Split View").on_select_tx(
                |state: &mut AppState, tx| {
                    state.dismiss_context_menu();
                    state.browser.exit_split(None);
                    tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                    tx.animated();
                },
            ));
        }
        menu = menu
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
    menu = menu
        .separator()
        .item(
            MenuItem::new("Open File…").on_select_tx(|state: &mut AppState, tx: &mut Tx| {
                state.dismiss_context_menu();
                crate::shortcuts::open_file(tx);
            }),
        )
        .item(
            MenuItem::new("Settings").on_select_tx(|state: &mut AppState, tx: &mut Tx| {
                state.dismiss_context_menu();
                app::open_settings(state, tx);
            }),
        );
    crate::ui::hover_highlight::build_context_menu(state, menu, theme, layer);
}
