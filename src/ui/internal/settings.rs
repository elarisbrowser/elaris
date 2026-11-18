// SPDX-License-Identifier: GPL-3.0-only

//! The browser's compact native settings page.

use std::path::{Path, PathBuf};

use rune::tok::type_;
use rune::{El, ScrollSpec, TextStyle, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl, Select, SelectAppearance};

use crate::address::SearchEngine;
use crate::app::AppState;
use crate::appearance::Appearance;
use crate::settings::{DownloadDirectory, StartupMode};
use crate::ui::theme::{self, Theme};

const CONTENT_WIDTH: f32 = 720.0;
const CONTENT_INSET: f32 = 28.0;
const TOP_PADDING: f32 = 48.0;
const SECTION_GAP: f32 = 25.0;
const ROW_GAP: f32 = 16.0;
const CONTROL_WIDTH: f32 = 220.0;
const COMPACT_BREAKPOINT: f32 = 560.0;

pub(super) fn view(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let kit = theme.kit(theme.settings_surface);
    let viewport_width = state.browser.viewport_width();
    let compact = viewport_width > 0.0 && viewport_width < COMPACT_BREAKPOINT;
    let content = col()
        .key("settings-content")
        .w_full()
        .max_w(CONTENT_WIDTH)
        .px(CONTENT_INSET)
        .pt(TOP_PADDING)
        .pb(40.0)
        .gap(SECTION_GAP)
        .child(
            text("Settings")
                .font(TextStyle::new(26.0, 600.0).lh(32.0))
                .color(theme.text),
        )
        .child(appearance_section(state, theme, &kit, compact))
        .child(search_section(state, theme, &kit, overlays, compact))
        .child(downloads_section(state, theme, &kit, compact))
        .child(startup_section(state, theme, &kit, overlays, compact))
        .child(configuration_section(state, theme, &kit, compact));

    col()
        .key("settings-scroll")
        .w_full()
        .h_full()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            row()
                .key("settings-center")
                .w_full()
                .justify_center()
                .child(content),
        )
}

fn appearance_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let control = SegmentedControl::new(
        &["System", "Light", "Dark"],
        Some(match state.settings.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        }),
    )
    .width(Some(CONTROL_WIDTH))
    .label("Appearance")
    .on_pick(|state: &mut AppState, index, _| {
        let appearance = match index {
            0 => Appearance::System,
            1 => Appearance::Light,
            _ => Appearance::Dark,
        };
        state.set_appearance(appearance);
    })
    .build(kit);

    section(
        "Appearance",
        theme,
        settings_group(
            setting_row(
                "Theme",
                "Choose how Elaris and webpages look.",
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
    let options = ["DuckDuckGo", "Google", "Bing", "Brave Search"];
    let current = state.settings.search_engine.label();
    let select = Select::new("settings-search-engine", &options)
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
            let engine = match value {
                "DuckDuckGo" => Some(SearchEngine::DuckDuckGo),
                "Google" => Some(SearchEngine::Google),
                "Bing" => Some(SearchEngine::Bing),
                "Brave Search" => Some(SearchEngine::Brave),
                _ => None,
            };
            if let Some(engine) = engine {
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
                select.build(kit, overlays),
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
    let options = ["Restore previous session", "Open a blank window"];
    let current = state.settings.startup.label();
    let select = Select::new("settings-startup", &options)
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
            let startup = match value {
                "Restore previous session" => Some(StartupMode::RestoreSession),
                "Open a blank window" => Some(StartupMode::BlankWindow),
                _ => None,
            };
            if let Some(startup) = startup {
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
                select.build(kit, overlays),
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
    col()
        .key(format!("settings-section-{title}"))
        .w_full()
        .gap(9.0)
        .child(
            text(title)
                .font(TextStyle::new(14.0, 600.0).lh(19.0))
                .color(theme.text_muted),
        )
        .child(content)
}

fn settings_group(content: El, theme: &Theme) -> El {
    col()
        .w_full()
        .px(16.0)
        .py(12.0)
        .fill(theme.settings_surface)
        .continuous_corners()
        .clip_rounded(theme::FIELD_RADIUS)
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
