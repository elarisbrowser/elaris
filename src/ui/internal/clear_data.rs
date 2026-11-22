// SPDX-License-Identifier: GPL-3.0-only

//! The Clear Browsing Data dialog, shared by Settings and the history page.

use rune::tok::{space, type_};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, SegmentedControl};

use crate::app::{AppState, ClearDataDialog, ClearRange};
use crate::ui::internal::widgets::switch_kit;
use crate::ui::theme::{self, Theme};

const DIALOG_WIDTH: f32 = 440.0;

pub(super) fn dialog(dialog: ClearDataDialog, theme: &Theme, overlays: &mut Vec<El>) {
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
    let option = |label: &'static str, on: bool, toggle: fn(&mut ClearDataDialog)| {
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
