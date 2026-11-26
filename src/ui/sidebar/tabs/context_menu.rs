// SPDX-License-Identifier: GPL-3.0-only

use crate::ui::components::menu::{Menu, MenuItem, context_menu as menu_at};
use rune::{El, Tx, el};

use crate::app::{self, AppState};
use crate::tab::{Tab, TabSection};
use crate::ui::components::animated_icon;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::settings::TabLayout;

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
                MenuItem::new("Reload")
                    .animated(&animated_icon::RELOAD)
                    .on_select(move |state: &mut AppState| {
                        state.dismiss_context_menu();
                        state.reload(tab);
                    }),
            )
            .item(
                MenuItem::new("Duplicate")
                    .animated(&animated_icon::COPY)
                    .on_select(move |state: &mut AppState| {
                        state.dismiss_context_menu();
                        state.duplicate(tab);
                    }),
            )
            .separator();
        if let Some(sleepable) = state
            .browser
            .tab(tab)
            .filter(|candidate| !candidate.is_internal() && !candidate.asleep)
        {
            // A tab on screen stays awake; it can sleep once the user moves on.
            let shown = state.browser.visible_tabs().contains(&Some(sleepable.id()));
            let bookmarked = state
                .bookmarks()
                .find_url(&sleepable.url)
                .map(|bookmark| bookmark.id);
            menu = menu
                .item(
                    MenuItem::new(if bookmarked.is_some() {
                        "Remove Bookmark"
                    } else {
                        "Bookmark Tab"
                    })
                    .icon(if bookmarked.is_some() {
                        &icons::STAR_FILLED
                    } else {
                        &icons::STAR
                    })
                    .on_select_tx(move |state: &mut AppState, tx| {
                        state.dismiss_context_menu();
                        match bookmarked {
                            Some(id) => state.remove_bookmark_with_toast(id, tx),
                            None => state.bookmark_tab_with_toast(tab, false, tx),
                        }
                    }),
                )
                .item(
                    MenuItem::new("Sleep Tab")
                        .icon(&icons::BED)
                        .disabled(shown)
                        .on_select(move |state: &mut AppState| {
                            state.dismiss_context_menu();
                            state.sleep_now(tab);
                        }),
                )
                .separator();
        }
        menu = menu.item(
            MenuItem::new(section_action.0)
                .animated(&animated_icon::HEART)
                .on_select_tx(move |state: &mut AppState, tx: &mut Tx| {
                    state.dismiss_context_menu();
                    state.move_tab_to_section_animated(tab, section_action.1, tx);
                }),
        );
        for (side, label) in ["Split Left", "Split Right"].into_iter().enumerate() {
            menu = menu.item(
                MenuItem::new(label)
                    .animated(&animated_icon::SPLIT)
                    .on_select_tx(move |state: &mut AppState, tx| {
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
            menu = menu.item(
                MenuItem::new("Swap Panes")
                    .animated(&animated_icon::SPLIT)
                    .on_select_tx(|state: &mut AppState, tx| {
                        state.dismiss_context_menu();
                        state.browser.swap_split();
                        tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                        tx.animated();
                    }),
            );
            if split.panes.contains(&Some(tab))
                && let Some(other) = split
                    .panes(state.browser.split_view_limit)
                    .iter()
                    .flatten()
                    .copied()
                    .find(|id| *id != tab)
            {
                menu = menu.item(
                    MenuItem::new("Remove Pane")
                        .animated(&animated_icon::CLOSE)
                        .on_select_tx(move |state: &mut AppState, tx| {
                            state.dismiss_context_menu();
                            state.browser.exit_split(Some(other));
                            state.activate(other);
                            tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                            tx.animated();
                        }),
                );
            }
            menu = menu.item(
                MenuItem::new("Exit Split View")
                    .animated(&animated_icon::SPLIT)
                    .on_select_tx(|state: &mut AppState, tx| {
                        state.dismiss_context_menu();
                        state.browser.exit_split(None);
                        tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                        tx.animated();
                    }),
            );
        }
        menu = menu
            .separator()
            .item(
                MenuItem::new("Close Tab")
                    .animated(&animated_icon::CLOSE)
                    .on_select(move |state: &mut AppState| {
                        state.dismiss_context_menu();
                        state.close(tab);
                    }),
            )
            .item(
                MenuItem::new("Close Other Tabs")
                    .animated(&animated_icon::CLOSE_OTHERS)
                    .on_select(move |state: &mut AppState| {
                        state.dismiss_context_menu();
                        state.close_others(tab);
                    }),
            );
    } else {
        menu = menu.item(
            MenuItem::new("New Tab")
                .animated(&animated_icon::PLUS)
                .on_select_tx(|state: &mut AppState, tx: &mut Tx| {
                    state.dismiss_context_menu();
                    app::new_tab(state, tx);
                }),
        );
    }
    menu = layout_items(state, menu.separator())
        .separator()
        .item(
            MenuItem::new("Open File…")
                .animated(&animated_icon::FOLDER)
                .on_select_tx(|state: &mut AppState, tx: &mut Tx| {
                    state.dismiss_context_menu();
                    crate::shortcuts::open_file(tx);
                }),
        )
        .item(
            MenuItem::new("Settings")
                .animated(&animated_icon::SETTINGS)
                .on_select_tx(|state: &mut AppState, tx: &mut Tx| {
                    state.dismiss_context_menu();
                    app::open_settings(state, tx);
                }),
        );
    menu.build(state, theme, layer);
}

/// Switches between the sidebar and the tab strip, and moves the sidebar between the window's
/// edges, without a trip to Settings.
fn layout_items(state: &AppState, menu: Menu) -> Menu {
    let (label, layout) = if state.horizontal_tabs() {
        ("Show Tabs Vertically", TabLayout::Vertical)
    } else {
        ("Show Tabs Horizontally", TabLayout::Horizontal)
    };
    let menu = menu.item(MenuItem::new(label).on_select_tx(
        move |state: &mut AppState, tx: &mut Tx| {
            state.dismiss_context_menu();
            state.set_tab_layout(layout);
            tx.animated();
            tx.motion(theme::SIDEBAR_TOGGLE);
        },
    ));
    if state.horizontal_tabs() {
        return menu;
    }
    let side = state.settings.sidebar_side.opposite();
    menu.item(
        MenuItem::new(format!("Move Sidebar {}", side.label())).on_select_tx(
            move |state: &mut AppState, tx: &mut Tx| {
                state.dismiss_context_menu();
                state.set_sidebar_side(side);
                tx.animated();
                tx.motion(theme::SIDEBAR_TOGGLE);
            },
        ),
    )
}
