// SPDX-License-Identifier: GPL-3.0-only

//! Page defaults, search and downloads.

use std::path::{Path, PathBuf};

use rune::tok::{space, type_};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, SegmentedControl};

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::menu::Dropdown;
use crate::ui::components::widgets::{actions, setting, setting_with, text_field};
use crate::ui::icons;
use crate::ui::internal::PageContext;
use crate::ui::theme;
use elaris_core::address::{QUERY_PLACEHOLDER, SearchEngine};
use elaris_core::permissions::AutoplayPolicy;
use elaris_core::settings::DownloadDirectory;

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
    const BUILT_IN: [SearchEngine; 4] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Brave,
    ];
    let settings = &cx.state.settings;
    let custom = (0..settings.custom_search_engines.len()).map(SearchEngine::Custom);
    let options: Vec<_> = BUILT_IN
        .into_iter()
        .chain(custom)
        .map(|engine| (engine, settings.search_engine_label(engine).to_owned()))
        .collect();
    let select = Dropdown::new("settings-search-engine", "Search engine")
        .options(options)
        .current(cx.state.settings.search_engine)
        .open(cx.state.settings_picker_open)
        .on_toggle(|state| state.settings_picker_open = !state.settings_picker_open)
        .on_close(|state| state.settings_picker_open = false)
        .on_choose(|state, engine| {
            state.set_search_engine(engine);
            state.settings_picker_open = false;
        })
        .build(cx.state, cx.theme, cx.overlays);
    let mut rows = vec![setting(
        cx,
        "Search engine",
        "Used for searches from the address bar.",
        select,
    )];
    for index in 0..cx.state.settings.custom_search_engines.len() {
        rows.push(custom_engine(cx, index));
    }
    rows.push(add_engine(cx));
    section("Search", cx.theme, rows)
}

/// One of the user's engines: its name and address, and a button that removes it.
fn custom_engine(cx: &PageContext, index: usize) -> El {
    let engine = &cx.state.settings.custom_search_engines[index];
    let remove = controls::icon_button(
        &format!("search-engine-remove-{}", engine.key()),
        &format!("Remove {}", engine.name()),
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        cx.theme,
    )
    .focusable(true)
    .on_press(move |state: &mut AppState| state.remove_search_engine(index));
    let address = text(engine.url())
        .font(type_::BODY)
        .color(cx.theme.text_muted)
        .single_line()
        .truncate();
    setting_with(cx, engine.name(), address, remove).key(format!("search-engine-{}", engine.key()))
}

/// The form that adds an engine: a name, an address with %s for the query, and Add.
fn add_engine(cx: &PageContext) -> El {
    let theme = cx.theme;
    let form = &cx.state.search_engine_form;
    let name = text_field(
        theme,
        &form.name,
        "search-engine-name",
        "Search engine name",
        "Name",
        |state| &mut state.search_engine_form.name,
        AppState::add_search_engine,
    );
    let url = text_field(
        theme,
        &form.url,
        "search-engine-url",
        "Search engine address",
        "Address, with %s",
        |state| &mut state.search_engine_form.url,
        AppState::add_search_engine,
    );
    let add = Button::new("Add")
        .disabled(form.name.text.trim().is_empty() || form.url.text.trim().is_empty())
        .on_press(|state: &mut AppState| state.add_search_engine())
        .build(&cx.kit);
    let control = col()
        .w(theme::PAGE_CONTROL_WIDTH)
        .gap(space::S2)
        .child(name)
        .child(url)
        .child(row().w_full().justify_end().child(add));
    let detail = match &form.error {
        Some(error) => text(error.as_str())
            .font(type_::BODY)
            .color(theme.text)
            .wrap()
            .role("alert"),
        None => text(format!(
            "Put {QUERY_PLACEHOLDER} in the address where the search terms go."
        ))
        .font(type_::BODY)
        .color(theme.text_muted)
        .wrap(),
    };
    setting_with(cx, "Add a search engine", detail, control)
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
