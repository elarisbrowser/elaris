// SPDX-License-Identifier: GPL-3.0-only

//! Browsing data and content blocking preferences.

use rune::tok::{space, type_};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, SegmentedControl};

use crate::app::{AppState, ClearRange};
use crate::settings::ContentBlockList;
use crate::ui::internal::PageContext;
use crate::ui::internal::widgets::{Category, setting, switch_kit};
use crate::ui::theme::{self, Theme};

use super::section;

const DIALOG_WIDTH: f32 = 440.0;

pub(super) fn category(cx: &mut PageContext) -> Category {
    Category::new(
        "settings-privacy",
        "Privacy & security",
        vec![browsing_data(cx), content_blocking(cx)],
    )
}

fn browsing_data(cx: &mut PageContext) -> El {
    if let Some(dialog) = cx.state.clear_data {
        clear_data_dialog(dialog, cx.theme, cx.overlays);
    }
    let open = Button::new("Clear…")
        .variant(ButtonVariant::Neutral)
        .on_press(AppState::open_clear_data)
        .build(&cx.kit)
        .key("settings-clear-data");
    let description = cx
        .state
        .clear_data_status
        .as_deref()
        .unwrap_or("Remove history, cookies and site data, cached files and download history.");
    section(
        "Browsing data",
        cx.theme,
        [setting(cx, "Clear browsing data", description, open)],
    )
}

fn content_blocking(cx: &mut PageContext) -> El {
    let switch_kit = switch_kit(&cx.kit);
    let blocking = &cx.state.settings.content_blocking;
    let master = rune_kit::Switch::new(blocking.enabled)
        .standard()
        .label("Block ads and trackers")
        .hide_label()
        .on_toggle(|state: &mut AppState| {
            state.set_content_blocking(!state.settings.content_blocking.enabled)
        })
        .build(&switch_kit);
    let lists = ContentBlockList::ALL.into_iter().map(|list| {
        let control = rune_kit::Switch::new(blocking.lists.contains(&list))
            .standard()
            .label(list.label())
            .hide_label()
            .on_toggle(move |state: &mut AppState| {
                let enabled = state.settings.content_blocking.lists.contains(&list);
                state.set_content_blocking_list(list, !enabled)
            })
            .build(&switch_kit);
        setting(cx, list.label(), list.description(), control)
    });
    let rows: Vec<El> = std::iter::once(setting(
        cx,
        "Block ads and trackers",
        "Lists download when blocking turns on, then refresh daily.",
        master,
    ))
    .chain(lists)
    .collect();
    section("Content blocking", cx.theme, rows)
}

fn clear_data_dialog(dialog: crate::app::ClearDataDialog, theme: &Theme, overlays: &mut Vec<El>) {
    let kit = theme.kit(theme.overlay_panel);
    let switch_kit = switch_kit(&kit);
    let selected = ClearRange::ALL
        .iter()
        .position(|range| *range == dialog.range);
    let range = SegmentedControl::new(&ClearRange::ALL.map(ClearRange::label), selected)
        .label("Time range")
        .on_pick(|state: &mut AppState, index, _| {
            state.update_clear_data(|dialog| dialog.range = ClearRange::ALL[index]);
        })
        .build(&kit);
    let option = |label: &'static str, on: bool, toggle: fn(&mut crate::app::ClearDataDialog)| {
        row()
            .w_full()
            .center_y()
            .child(text(label).font(type_::BODY).color(theme.text).grow(1.0))
            .child(
                rune_kit::Switch::new(on)
                    .standard()
                    .label(label)
                    .on_toggle(move |state: &mut AppState| state.update_clear_data(toggle))
                    .build(&switch_kit),
            )
    };
    let actions = row()
        .w_full()
        .gap(space::S2)
        .justify_end()
        .child(
            Button::new("Cancel")
                .variant(ButtonVariant::Neutral)
                .on_press(|state: &mut AppState| state.clear_data = None)
                .build(&kit)
                .autofocus(),
        )
        .child(
            Button::new("Clear Data")
                .variant(ButtonVariant::Danger)
                .disabled(!dialog.has_selection())
                .on_press(AppState::confirm_clear_data)
                .build(&kit),
        );
    Dialog::new("clear-data", "Clear browsing data")
        .open(true)
        .fit()
        .width(DIALOG_WIDTH)
        .click_outside(false)
        .on_close(|state: &mut AppState| state.clear_data = None)
        .child(
            col()
                .w_full()
                .px(theme::PAGE_SECTION_GAP)
                .pb(theme::PAGE_SECTION_GAP)
                .gap(theme::PAGE_ROW_GAP)
                .child(range)
                .child(option("Browsing history", dialog.history, |dialog| {
                    dialog.history = !dialog.history;
                }))
                .child(option(
                    "Cookies and site data",
                    dialog.engine.site_data,
                    |dialog| {
                        dialog.engine.site_data = !dialog.engine.site_data;
                    },
                ))
                .child(option("Cached files", dialog.engine.cache, |dialog| {
                    dialog.engine.cache = !dialog.engine.cache;
                }))
                .child(option(
                    "Download history",
                    dialog.engine.download_history,
                    |dialog| {
                        dialog.engine.download_history = !dialog.engine.download_history;
                    },
                ))
                .child(actions),
        )
        .build(&kit, overlays);
}
