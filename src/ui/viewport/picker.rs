// SPDX-License-Identifier: GPL-3.0-only

use rune::{
    El, Key, ScrollSpec, col, row, text,
    tok::{radius, space, type_},
};

use super::focus_key;
use crate::{
    app::AppState,
    tab::{Tab, TabSection},
    ui::{
        tab_switcher::{ROW, tab_entry, tab_panel},
        theme::{self, Theme},
    },
};

pub(crate) const PICKER: &str = "split-tab-picker";

fn candidates(state: &AppState) -> Vec<&Tab> {
    let visible = state.browser.visible_tabs();
    state
        .browser
        .tabs()
        .iter()
        .filter(|tab| !visible.contains(&Some(tab.id())))
        .collect()
}

fn choice_key(tab: &Tab) -> String {
    format!("split-choice-{}", tab.id().raw())
}

pub(super) fn view(state: &AppState, theme: &Theme, side: usize) -> El {
    let tabs = candidates(state);
    let mut list = col()
        .key("split-picker-results")
        .w_full()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    let mut previous = None;
    let mut groups = 0;
    let count = tabs.len();
    for (index, tab) in tabs.into_iter().enumerate() {
        let section = tab.section();
        if previous != Some(section) {
            previous = Some(section);
            groups += 1;
            let name = match section {
                TabSection::Favourite => "Favourites",
                TabSection::Regular => "Tabs",
            };
            list = list.child(
                text(name)
                    .font(type_::LABEL)
                    .color(theme.text_muted)
                    .px(space::S3)
                    .h(theme::ROW_HEIGHT / 2.0),
            );
        }
        let id = tab.id();
        let selected = state.split_selection == index;
        let entry = tab_entry(choice_key(tab), tab, selected, theme)
            .radius(radius::MD)
            .on_pointer_move_local(move |state: &mut AppState, _, _| {
                state.split_selection = index;
            });
        let entry = if selected {
            entry.fill(theme.overlay_selected)
        } else {
            entry
        };
        list = list.child(entry.on_press_tx(move |state: &mut AppState, tx| {
            state.place_split_tab(id, side);
            tx.focus(focus_key(&state.browser), false);
        }));
    }
    let key = if state
        .browser
        .visible_tabs()
        .iter()
        .position(Option::is_none)
        == Some(side)
    {
        PICKER.to_owned()
    } else {
        format!("{PICKER}-{side}")
    };
    let mut panel = tab_panel(&key, theme)
        .gap(space::S2)
        .focusable(true)
        .no_focus_ring()
        .label("Choose a tab for Split View")
        .on_key(move |state: &mut AppState, event, tx| {
            match event.key {
                Key::Escape => {
                    state.browser.exit_split(None);
                    tx.focus(focus_key(&state.browser), false);
                    tx.animated();
                }
                Key::Up | Key::Down => {
                    let tabs = candidates(state);
                    if !tabs.is_empty() {
                        let index = if event.key == Key::Up {
                            state.split_selection.saturating_sub(1).min(tabs.len() - 1)
                        } else {
                            (state.split_selection + 1).min(tabs.len() - 1)
                        };
                        tx.scroll_into_view(choice_key(tabs[index]));
                        state.split_selection = index;
                    }
                }
                Key::Enter => {
                    let tabs = candidates(state);
                    if let Some(tab) = tabs.get(state.split_selection) {
                        state.place_split_tab(tab.id(), side);
                        tx.focus(focus_key(&state.browser), false);
                    } else if tabs.is_empty() {
                        crate::app::new_tab(state, tx);
                    }
                }
                _ => return false,
            }
            true
        });
    if count > 0 {
        let height =
            (count as f32 * ROW + groups as f32 * (theme::ROW_HEIGHT / 2.0)).min(6.0 * ROW);
        panel = panel.child(list.h(height));
    }
    let panel = panel.child(
        row()
            .gap(space::S2)
            .child(
                text("New Tab")
                    .font(type_::LABEL)
                    .color(theme.text)
                    .px(space::S3)
                    .py(space::S2)
                    .radius(radius::MD)
                    .hover(theme.hover)
                    .on_press_tx(crate::app::new_tab),
            )
            .child(
                text("Cancel")
                    .font(type_::LABEL)
                    .color(theme.text_muted)
                    .px(space::S3)
                    .py(space::S2)
                    .radius(radius::MD)
                    .hover(theme.hover)
                    .on_press_tx(|state: &mut AppState, tx| {
                        state.browser.exit_split(None);
                        tx.focus(focus_key(&state.browser), false);
                        tx.animated();
                    }),
            ),
    );
    col()
        .w_full()
        .h_full()
        .min_w(0.0)
        .padding(space::S3)
        .center()
        .child(panel)
}
