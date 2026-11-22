// SPDX-License-Identifier: GPL-3.0-only

//! The browser's compact native settings page.

mod appearance;
mod privacy;

use std::path::{Path, PathBuf};

use rune::tok::type_;
use rune::{El, TextStyle, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl, Select, SelectAppearance};

use crate::address::SearchEngine;
use crate::app::AppState;
use crate::permissions::AutoplayPolicy;
use crate::settings::{DownloadDirectory, StartupMode};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

use self::appearance::appearance_section;
use self::privacy::{content_blocking_section, privacy_section};
use super::layout::{self, PageLayout};
const ROW_GAP: f32 = 16.0;
const CONTROL_WIDTH: f32 = 220.0;

pub(super) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    compact: bool,
) -> PageLayout {
    let kit = theme.kit(theme.settings_surface);
    let content = col()
        .key("settings-content")
        .w_full()
        .gap(layout::SECTION_GAP)
        .child(appearance_section(state, theme, &kit, compact))
        .child(page_zoom_section(state, theme, &kit, compact))
        .child(search_section(state, theme, &kit, overlays, compact))
        .child(downloads_section(state, theme, &kit, compact))
        .child(privacy_section(state, theme, &kit, overlays, compact))
        .child(content_blocking_section(state, theme, &kit, compact))
        .child(split_view_section(state, theme, &kit, compact))
        .child(performance_section(state, theme, &kit, compact))
        .child(startup_section(state, theme, &kit, overlays, compact))
        .child(default_browser_section(state, theme, &kit, compact))
        .child(configuration_section(state, theme, &kit, compact));
    PageLayout {
        scroll_key: "settings-scroll",
        icon: &icons::SYSTEM,
        toolbar: None,
        content,
    }
}

fn page_zoom_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    const ZOOM_OPTIONS: [u16; 5] = [75, 90, 100, 125, 150];
    let current = state.settings.page_zoom;
    let selected = ZOOM_OPTIONS.iter().position(|&zoom| zoom == current);
    let control = SegmentedControl::new(&["75%", "90%", "100%", "125%", "150%"], selected)
        .fit()
        .label("Default page zoom")
        .on_pick(|state: &mut AppState, index, _| state.set_page_zoom(ZOOM_OPTIONS[index]))
        .build(kit);
    let autoplay = SegmentedControl::new(
        &AutoplayPolicy::ALL.map(AutoplayPolicy::label),
        AutoplayPolicy::ALL
            .iter()
            .position(|policy| *policy == state.settings.autoplay),
    )
    .fit()
    .label("Autoplay")
    .on_pick(|state: &mut AppState, index, _| {
        state.set_autoplay_default(AutoplayPolicy::ALL[index]);
    })
    .build(kit);
    section(
        "Pages",
        theme,
        settings_group(
            col()
                .gap(12.0)
                .child(setting_row(
                    "Default page zoom",
                    &format!("Current default is {current}%."),
                    theme,
                    control,
                    compact,
                ))
                .child(setting_row(
                    "Autoplay",
                    "Whether pages may start media on their own. Site controls can override it.",
                    theme,
                    autoplay,
                    compact,
                )),
            theme,
        ),
    )
}

fn split_view_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let options = [2usize, 3, 4];
    let selected = options
        .iter()
        .position(|limit| *limit == state.settings.split_view_limit.min(4));
    let control = SegmentedControl::new(&["2", "3", "4"], selected)
        .width(Some(CONTROL_WIDTH))
        .label("Maximum split panes")
        .on_pick(|state: &mut AppState, index, _| {
            state.set_split_view_limit([2, 3, 4][index]);
        })
        .build(kit);
    section(
        "Split View",
        theme,
        settings_group(
            setting_row(
                "Maximum panes",
                "Choose how many pages can appear in the split view grid. Higher limits are available in config.toml.",
                theme,
                control,
                compact,
            ),
            theme,
        ),
    )
}

fn search_section(
    state: &AppState,
    theme: &Theme,
    kit: &Kit,
    overlays: &mut Vec<El>,
    compact: bool,
) -> El {
    const OPTIONS: [SearchEngine; 4] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Brave,
    ];
    let current = state.settings.search_engine.label();
    let select = Select::new("settings-search-engine", &OPTIONS.map(SearchEngine::label))
        .current(Some(current))
        .label("Search engine")
        .appearance(SelectAppearance::Field)
        .width(CONTROL_WIDTH)
        .open(state.settings_picker_open)
        .on_toggle(|state: &mut AppState| {
            state.settings_picker_open = !state.settings_picker_open;
        })
        .on_close(|state: &mut AppState| state.settings_picker_open = false)
        .on_choose(|state: &mut AppState, value, _| {
            if let Some(engine) = OPTIONS.into_iter().find(|option| option.label() == value) {
                state.set_search_engine(engine);
            }
            state.settings_picker_open = false;
        });
    section(
        "Search",
        theme,
        settings_group(
            setting_row(
                "Search engine",
                "Used for searches from the address bar.",
                theme,
                select
                    .build(kit, overlays)
                    .border(1.0, theme.control_border),
                compact,
            ),
            theme,
        ),
    )
}

fn downloads_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let download_directory = state.download_directory();
    let path = download_directory.display().to_string();
    let current_directory = download_directory;
    let change = Button::new("Change…")
        .variant(ButtonVariant::Neutral)
        .on_press_tx(move |_state: &mut AppState, tx| {
            let options = rune::FileDialogOptions {
                title: "Choose Download Folder".into(),
                directory: Some(current_directory.clone()),
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
        .build(kit);
    let restore = Button::new("Restore Default")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.set_download_directory(DownloadDirectory::Default))
        .build(kit);
    let actions = row().gap(8.0).child(change).child(restore);
    let copy = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .child(setting_title("Download location", theme))
        .child(
            text(display_path(&state.download_directory()))
                .font(type_::BODY)
                .color(theme.text_muted)
                .truncate()
                .help_when_truncated(path),
        );
    let row = if compact {
        col()
            .w_full()
            .gap(10.0)
            .child(copy)
            .child(row().w_full().justify_end().child(actions))
    } else {
        row()
            .w_full()
            .min_h(54.0)
            .gap(ROW_GAP)
            .center_y()
            .child(copy)
            .child(actions)
    };
    section("Downloads", theme, settings_group(row, theme))
}

fn startup_section(
    state: &AppState,
    theme: &Theme,
    kit: &Kit,
    overlays: &mut Vec<El>,
    compact: bool,
) -> El {
    const OPTIONS: [StartupMode; 2] = [StartupMode::RestoreSession, StartupMode::BlankWindow];
    let current = state.settings.startup.label();
    let select = Select::new("settings-startup", &OPTIONS.map(StartupMode::label))
        .current(Some(current))
        .label("Startup behavior")
        .appearance(SelectAppearance::Field)
        .width(CONTROL_WIDTH)
        .open(state.startup_picker_open)
        .on_toggle(|state: &mut AppState| {
            state.startup_picker_open = !state.startup_picker_open;
        })
        .on_close(|state: &mut AppState| state.startup_picker_open = false)
        .on_choose(|state: &mut AppState, value, _| {
            if let Some(startup) = OPTIONS.into_iter().find(|option| option.label() == value) {
                state.set_startup(startup);
            }
            state.startup_picker_open = false;
        });
    section(
        "Startup",
        theme,
        settings_group(
            setting_row(
                "When Elaris opens",
                "Choose what opens when you launch Elaris.",
                theme,
                select
                    .build(kit, overlays)
                    .border(1.0, theme.control_border),
                compact,
            ),
            theme,
        ),
    )
}

fn default_browser_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let is_default = state.default_browser == Some(true);
    let description = match (&state.default_browser_error, state.default_browser) {
        (Some(error), _) => error.clone(),
        (None, Some(true)) => "Elaris opens web links from other apps.".into(),
        (None, _) => "Open web links from other apps in Elaris.".into(),
    };
    let control = if is_default {
        text("Current default")
            .font(type_::LABEL)
            .color(theme.text_muted)
    } else {
        Button::new("Make Default")
            .variant(ButtonVariant::Neutral)
            .on_press(|state: &mut AppState| state.make_default_browser())
            .build(kit)
    };
    section(
        "Default Browser",
        theme,
        settings_group(
            setting_row("Default web browser", &description, theme, control, compact),
            theme,
        ),
    )
}

fn performance_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let mut switch_kit = kit.clone();
    switch_kit.c.std_icon_strong = kit.c.bg_accent;
    let reduce_motion = rune_kit::Switch::new(state.settings.reduce_motion)
        .standard()
        .label("Reduce motion")
        .on_toggle(|state: &mut AppState| state.set_reduce_motion(!state.settings.reduce_motion))
        .build(&switch_kit);
    section(
        "Performance",
        theme,
        settings_group(
            setting_row(
                "Reduce motion",
                "Minimise animations and transitions throughout Elaris.",
                theme,
                reduce_motion,
                compact,
            ),
            theme,
        ),
    )
}

fn configuration_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let open = Button::new("Open Configuration File")
        .variant(ButtonVariant::Neutral)
        .on_press(|state: &mut AppState| state.open_config_file())
        .build(kit);
    let reload = Button::new("Reload Configuration")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.reload_configuration())
        .build(kit);
    let description = col().grow(1.0).min_w(0.0).child(
        text("Edit config.toml directly, then reload your changes here.")
            .font(type_::BODY)
            .color(theme.text_muted)
            .wrap(),
    );
    let actions = if compact {
        col()
            .w_full()
            .gap(6.0)
            .child(row().w_full().justify_end().child(open))
            .child(row().w_full().justify_end().child(reload))
    } else {
        row().gap(8.0).child(open).child(reload)
    };
    let mut content = if compact {
        col().w_full().gap(14.0).child(description).child(actions)
    } else {
        row()
            .w_full()
            .gap(ROW_GAP)
            .center_y()
            .child(description)
            .child(actions)
    };
    if let Some(issue) = &state.configuration_issue {
        content = content.child(
            col()
                .key("configuration-issue")
                .gap(4.0)
                .child(
                    text("Configuration needs attention")
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .child(
                    text(issue.clone())
                        .font(type_::BODY)
                        .color(theme.text_muted)
                        .wrap_anywhere(),
                ),
        );
    }
    section(
        "Advanced Configuration",
        theme,
        settings_group(content, theme),
    )
}

fn section(title: &str, theme: &Theme, content: El) -> El {
    layout::section(format!("settings-section-{title}"), title, theme, content)
}

fn settings_group(content: El, theme: &Theme) -> El {
    layout::group(theme)
        .px(theme::PAGE_GROUP_PADDING)
        .py(12.0)
        .child(content)
}

fn setting_row(title: &str, description: &str, theme: &Theme, control: El, compact: bool) -> El {
    let copy = col()
        .key("setting-copy")
        .grow(1.0)
        .min_w(0.0)
        .gap(3.0)
        .child(setting_title(title, theme))
        .child(
            text(description)
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap(),
        );
    if compact {
        col()
            .w_full()
            .gap(10.0)
            .child(copy)
            .child(row().w_full().justify_end().child(control))
    } else {
        row()
            .w_full()
            .min_h(54.0)
            .gap(ROW_GAP)
            .justify_between()
            .center_y()
            .child(copy)
            .child(control)
    }
}

fn setting_title(title: &str, theme: &Theme) -> El {
    text(title)
        .font(TextStyle::new(14.0, 500.0).lh(19.0))
        .color(theme.text)
}

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
