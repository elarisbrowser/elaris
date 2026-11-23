// SPDX-License-Identifier: GPL-3.0-only

//! Browsing data and content blocking preferences.

use rune::El;
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::ui::internal::PageContext;
use crate::ui::internal::clear_data;
use crate::ui::internal::widgets::{setting, switch_kit};
use elaris_core::settings::ContentBlockList;

use super::section;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    vec![browsing_data(cx), content_blocking(cx)]
}

fn browsing_data(cx: &mut PageContext) -> El {
    if let Some(dialog) = cx.state.clear_data {
        clear_data::dialog(dialog, cx.theme, cx.overlays);
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
