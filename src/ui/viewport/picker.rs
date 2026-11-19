// SPDX-License-Identifier: GPL-3.0-only

use rune::{
    El, Key, ScrollSpec, col, row, text, text_input,
    tok::{space, type_},
};

use super::focus_key;
use crate::{
    app::AppState,
    tab::{Tab, TabSection},
    ui::{
        sidebar::tab_icon,
        theme::{self, Theme},
    },
};

pub(super) const SEARCH: &str = "split-tab-search";

fn matches(state: &AppState) -> Vec<&Tab> {
    let query = state.split_search.text.trim().to_lowercase();
    state
        .browser
        .tabs()
        .iter()
        .filter(|tab| {
            !state.browser.visible_tabs().contains(&Some(tab.id()))
                && (tab.title.to_lowercase().contains(&query)
                    || tab.url.to_lowercase().contains(&query))
        })
        .collect()
}

pub(super) fn view(state: &AppState, theme: &Theme, side: usize) -> El {
    let tabs = matches(state);
    let mut list = col()
        .key("split-picker-results")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    let mut previous = None;
    for (index, tab) in tabs.into_iter().enumerate() {
        let section = tab.section();
        if previous != Some(section) {
            previous = Some(section);
            let name = match section {
                TabSection::Favourite => "Favourites",
                TabSection::Pinned => "Pinned Tabs",
                TabSection::Regular => "Tabs",
            };
            list = list.child(text(name).font(type_::LABEL).color(theme.text_muted));
        }
        let id = tab.id();
        let selected = state.split_selection == index;
        list = list.child(
            row()
                .key(format!("split-choice-{}", id.raw()))
                .w_full()
                .px(space::S2)
                .py(space::S1)
                .gap(space::S2)
                .center_y()
                .radius(theme::FIELD_RADIUS)
                .fill(if selected {
                    theme.selection
                } else {
                    theme.page
                })
                .child(tab_icon(tab, selected, theme::FAVOURITE_ICON, theme))
                .child(
                    col()
                        .min_w(0.0)
                        .grow(1.0)
                        .child(
                            text(if tab.title.is_empty() {
                                &tab.url
                            } else {
                                &tab.title
                            })
                            .color(theme.text)
                            .single_line()
                            .truncate(),
                        )
                        .child(
                            text(crate::address::compact_address(&tab.url))
                                .font(type_::LABEL)
                                .color(theme.text_muted)
                                .single_line()
                                .truncate(),
                        ),
                )
                .on_press_tx(move |state: &mut AppState, tx| {
                    state.place_split_tab(id, side);
                    tx.focus(focus_key(&state.browser), false);
                }),
        );
    }
    col()
        .key("split-tab-picker")
        .w_full()
        .h_full()
        .px(space::S3)
        .py(space::S3)
        .gap(space::S2)
        .fill(theme.page)
        .clip_rounded(theme::VIEWPORT_RADIUS)
        .child(text("Choose a tab").font(type_::LABEL).color(theme.text))
        .child(
            text_input(&state.split_search, theme::OMNIBOX_INPUT_TEXT)
                .key(SEARCH)
                .single_line()
                .h(theme::OMNIBOX_INPUT_HEIGHT)
                .px(space::S2)
                .color(theme.text)
                .fill(theme.address_field)
                .radius(theme::FIELD_RADIUS)
                .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
                .caret(theme.text, theme.selection)
                .placeholder("Search tabs", theme.text_muted)
                .label("Search tabs")
                .on_edit(|state: &mut AppState| &mut state.split_search)
                .on_input_change(|state: &mut AppState, _| state.split_selection = 0)
                .on_input_key(|state: &mut AppState, event, tx| {
                    match event.key {
                        Key::Escape => {
                            state.browser.exit_split(None);
                            tx.focus(focus_key(&state.browser), false);
                            tx.animated();
                        }
                        Key::Up | Key::Down => {
                            let tabs = matches(state);
                            if !tabs.is_empty() {
                                let index = if event.key == Key::Up {
                                    state.split_selection.saturating_sub(1)
                                } else {
                                    (state.split_selection + 1).min(tabs.len() - 1)
                                };
                                tx.scroll_into_view(format!(
                                    "split-choice-{}",
                                    tabs[index].id().raw()
                                ));
                                state.split_selection = index;
                            }
                        }
                        _ => return false,
                    }
                    true
                })
                .on_submit(move |state: &mut AppState, tx| {
                    if let Some(id) = matches(state)
                        .get(state.split_selection)
                        .map(|tab| tab.id())
                    {
                        state.place_split_tab(id, side);
                        tx.focus(focus_key(&state.browser), false);
                    }
                }),
        )
        .child(list)
        .child(
            row()
                .gap(space::S3)
                .child(
                    text("New Tab")
                        .color(theme.text)
                        .on_press_tx(crate::app::new_tab),
                )
                .child(text("Cancel").color(theme.text_muted).on_press_tx(
                    |state: &mut AppState, tx| {
                        state.browser.exit_split(None);
                        tx.focus(focus_key(&state.browser), false);
                        tx.animated();
                    },
                )),
        )
}
