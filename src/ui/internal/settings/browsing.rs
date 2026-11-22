// SPDX-License-Identifier: GPL-3.0-only

//! Page defaults, search and downloads.

use std::path::{Path, PathBuf};

use rune::tok::type_;
use rune::{El, text};
use rune_kit::{Button, ButtonVariant, SegmentedControl, Select, SelectAppearance};

use crate::address::SearchEngine;
use crate::app::AppState;
use crate::permissions::AutoplayPolicy;
use crate::settings::DownloadDirectory;
use crate::ui::internal::PageContext;
use crate::ui::internal::widgets::{actions, setting, setting_with};
use crate::ui::theme;

use super::section;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    vec![pages(cx), search(cx), downloads(cx)]
}

fn pages(cx: &mut PageContext) -> El {
    const ZOOM_OPTIONS: [u16; 5] = [75, 90, 100, 125, 150];
    let current = cx.state.settings.page_zoom;
    let selected = ZOOM_OPTIONS.iter().position(|&zoom| zoom == current);
    let zoom = SegmentedControl::new(&["75%", "90%", "100%", "125%", "150%"], selected)
        .fit()
        .label("Default page zoom")
        .on_pick(|state: &mut AppState, index, _| state.set_page_zoom(ZOOM_OPTIONS[index]))
        .build(&cx.kit);
    let autoplay = SegmentedControl::new(
        &AutoplayPolicy::ALL.map(AutoplayPolicy::label),
        AutoplayPolicy::ALL
            .iter()
            .position(|policy| *policy == cx.state.settings.autoplay),
    )
    .fit()
    .label("Autoplay")
    .on_pick(|state: &mut AppState, index, _| {
        state.set_autoplay_default(AutoplayPolicy::ALL[index]);
    })
    .build(&cx.kit);
    section(
        "Pages",
        cx.theme,
        [
            setting(
                cx,
                "Default page zoom",
                &format!("Current default is {current}%."),
                zoom,
            ),
            setting(
                cx,
                "Autoplay",
                "Whether pages may start media on their own. Site controls can override it.",
                autoplay,
            ),
        ],
    )
}

fn search(cx: &mut PageContext) -> El {
    const OPTIONS: [SearchEngine; 4] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Brave,
    ];
    let select = Select::new("settings-search-engine", &OPTIONS.map(SearchEngine::label))
        .current(Some(cx.state.settings.search_engine.label()))
        .label("Search engine")
        .appearance(SelectAppearance::Field)
        .width(theme::PAGE_CONTROL_WIDTH)
        .open(cx.state.settings_picker_open)
        .on_toggle(|state: &mut AppState| {
            state.settings_picker_open = !state.settings_picker_open;
        })
        .on_close(|state: &mut AppState| state.settings_picker_open = false)
        .on_choose(|state: &mut AppState, value, _| {
            if let Some(engine) = OPTIONS.into_iter().find(|option| option.label() == value) {
                state.set_search_engine(engine);
            }
            state.settings_picker_open = false;
        })
        .build(&cx.kit, cx.overlays)
        .border(1.0, cx.theme.control_border);
    section(
        "Search",
        cx.theme,
        [setting(
            cx,
            "Search engine",
            "Used for searches from the address bar.",
            select,
        )],
    )
}

fn downloads(cx: &mut PageContext) -> El {
    let directory = cx.state.download_directory();
    let full_path = directory.display().to_string();
    let path = text(display_path(&directory))
        .font(type_::BODY)
        .color(cx.theme.text_muted)
        .truncate()
        .help_when_truncated(full_path);
    let change = Button::new("Change…")
        .variant(ButtonVariant::Neutral)
        .on_press_tx(move |_: &mut AppState, tx| {
            let options = rune::FileDialogOptions {
                title: "Choose Download Folder".into(),
                directory: Some(directory.clone()),
                ..Default::default()
            };
            tx.open_folder(options, |state: &mut AppState, result, _| match result {
                Ok(Some(path)) => match DownloadDirectory::custom(path) {
                    Ok(directory) => state.set_download_directory(directory),
                    Err(error) => state.configuration_issue = Some(error),
                },
                Ok(None) => {}
                Err(error) => state.configuration_issue = Some(error.to_string()),
            });
        })
        .build(&cx.kit);
    let restore = Button::new("Restore Default")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.set_download_directory(DownloadDirectory::Default))
        .build(&cx.kit);
    section(
        "Downloads",
        cx.theme,
        [setting_with(
            cx,
            "Download location",
            path,
            actions([change, restore]),
        )],
    )
}

/// Shortens paths under the home directory to `~/…`.
fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(relative) = path.strip_prefix(home)
    {
        if relative.as_os_str().is_empty() {
            "~".to_owned()
        } else {
            format!("~/{}", relative.display())
        }
    } else {
        path.display().to_string()
    }
}
