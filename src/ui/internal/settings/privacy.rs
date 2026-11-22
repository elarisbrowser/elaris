// SPDX-License-Identifier: GPL-3.0-only

//! Browsing data and content blocking preferences.

use rune::tok::type_;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, SegmentedControl};

use crate::app::{AppState, ClearRange};
use crate::settings::ContentBlockList;
use crate::ui::internal::layout::SECTION_GAP;
use crate::ui::theme::Theme;

use super::{ROW_GAP, section, setting_row, settings_group};

pub(super) fn privacy_section(
    state: &AppState,
    theme: &Theme,
    kit: &Kit,
    overlays: &mut Vec<El>,
    compact: bool,
) -> El {
    if let Some(dialog) = state.clear_data {
        clear_data_dialog(dialog, theme, overlays);
    }
    let open = Button::new("Clear…")
        .variant(ButtonVariant::Neutral)
        .on_press(AppState::open_clear_data)
        .build(kit)
        .key("settings-clear-data");
    let description = state
        .clear_data_status
        .as_deref()
        .unwrap_or("Remove history, cookies and site data, cached files and download history.");
    section(
        "Privacy",
        theme,
        settings_group(
            setting_row("Clear browsing data", description, theme, open, compact),
            theme,
        ),
    )
}

pub(super) fn content_blocking_section(
    state: &AppState,
    theme: &Theme,
    kit: &Kit,
    compact: bool,
) -> El {
    let mut switch_kit = kit.clone();
    switch_kit.c.std_icon_strong = kit.c.bg_accent;
    let blocking = &state.settings.content_blocking;
    let master = rune_kit::Switch::new(blocking.enabled)
        .standard()
        .label("Block ads and trackers")
        .hide_label()
        .on_toggle(|state: &mut AppState| {
            state.set_content_blocking(!state.settings.content_blocking.enabled)
        })
        .build(&switch_kit);
    let mut rows = col().gap(12.0).child(setting_row(
        "Block ads and trackers",
        "Lists download when blocking turns on, then refresh daily.",
        theme,
        master,
        compact,
    ));
    for list in ContentBlockList::ALL {
        let control = rune_kit::Switch::new(blocking.lists.contains(&list))
            .standard()
            .label(list.label())
            .hide_label()
            .on_toggle(move |state: &mut AppState| {
                let enabled = state.settings.content_blocking.lists.contains(&list);
                state.set_content_blocking_list(list, !enabled)
            })
            .build(&switch_kit);
        rows = rows.child(setting_row(
            list.label(),
            list.description(),
            theme,
            control,
            compact,
        ));
    }
    section("Content blocking", theme, settings_group(rows, theme))
}

fn clear_data_dialog(dialog: crate::app::ClearDataDialog, theme: &Theme, overlays: &mut Vec<El>) {
    let kit = theme.kit(theme.overlay_panel);
    let mut switch_kit = kit.clone();
    switch_kit.c.std_icon_strong = kit.c.bg_accent;
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
        .gap(9.0)
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
        .width(440.0)
        .click_outside(false)
        .on_close(|state: &mut AppState| state.clear_data = None)
        .child(
            col()
                .w_full()
                .px(SECTION_GAP)
                .pb(SECTION_GAP)
                .gap(ROW_GAP)
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
