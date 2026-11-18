// SPDX-License-Identifier: GPL-3.0-only

//! The browser's compact native settings page.

use rune::tok::type_;
use rune::{El, ScrollSpec, TextStyle, col, el, row, text};
use rune_kit::{
    Button, ButtonVariant, Kit, PaletteSeed, SegmentedControl, Select, SelectAppearance,
};

use crate::address::SearchEngine;
use crate::app::AppState;
use crate::appearance::Appearance;
use crate::settings::{DownloadDirectory, StartupMode};
use crate::ui::theme::Theme;

const CONTENT_WIDTH: f32 = 720.0;
const CONTENT_INSET: f32 = 32.0;
const SECTION_GAP: f32 = 28.0;
const ROW_GAP: f32 = 14.0;
const CONTROL_WIDTH: f32 = 240.0;

pub(super) fn view(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let kit = Kit::from_palette(PaletteSeed {
        bg: theme.page,
        fg: theme.text,
        accent: theme.icon,
    });
    let content = col()
        .key("settings-content")
        .w_full()
        .max_w(CONTENT_WIDTH)
        .px(CONTENT_INSET)
        .pt(34.0)
        .pb(40.0)
        .gap(SECTION_GAP)
        .child(
            col()
                .key("settings-heading")
                .gap(6.0)
                .child(
                    text("Settings")
                        .font(TextStyle::new(29.0, 600.0).lh(35.0))
                        .color(theme.text),
                )
                .child(
                    text("Make Elaris feel like yours.")
                        .font(type_::BODY)
                        .color(theme.text_muted),
                ),
        )
        .child(appearance_section(state, theme, &kit))
        .child(search_section(state, theme, &kit, overlays))
        .child(downloads_section(state, theme, &kit))
        .child(startup_section(state, theme, &kit, overlays))
        .child(configuration_section(state, theme, &kit));

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

fn appearance_section(state: &AppState, theme: &Theme, kit: &Kit) -> El {
    section("Appearance", theme).child(setting_row(
        "Theme",
        "Choose how Elaris and webpages look.",
        theme,
        SegmentedControl::new(
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
        .build(kit),
    ))
}

fn search_section(state: &AppState, theme: &Theme, kit: &Kit, overlays: &mut Vec<El>) -> El {
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
    section("Search", theme).child(setting_row(
        "Search engine",
        "Used when you search from the address bar.",
        theme,
        select.build(kit, overlays),
    ))
}

fn downloads_section(state: &AppState, theme: &Theme, kit: &Kit) -> El {
    let download_directory = state.download_directory();
    let path = download_directory.display().to_string();
    let current_directory = download_directory;
    let choose = Button::new("Choose…")
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
    let reset = Button::new("Restore default")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.set_download_directory(DownloadDirectory::Default))
        .build(kit);

    section("Downloads", theme).child(
        col()
            .key("downloads-setting")
            .w_full()
            .gap(11.0)
            .child(
                col()
                    .gap(4.0)
                    .child(setting_title("Download folder", theme))
                    .child(
                        text(path)
                            .font(type_::BODY)
                            .color(theme.text_muted)
                            .wrap_anywhere(),
                    ),
            )
            .child(row().gap(8.0).child(choose).child(reset)),
    )
}

fn startup_section(state: &AppState, theme: &Theme, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let options = ["Restore previous session", "Open a new blank tab"];
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
    section("Startup", theme).child(setting_row(
        "When Elaris opens",
        "Restored background tabs keep their existing lazy loading behavior.",
        theme,
        select.build(kit, overlays),
    ))
}

fn configuration_section(state: &AppState, theme: &Theme, kit: &Kit) -> El {
    let open = Button::new("Open config.toml")
        .variant(ButtonVariant::Neutral)
        .on_press(|state: &mut AppState| state.open_config_file())
        .build(kit);
    let reload = Button::new("Reload configuration")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.reload_configuration())
        .build(kit);
    let mut section = section("Configuration", theme)
        .child(
            text(
                "Changes apply as you make them. Edit the TOML file directly for advanced options.",
            )
            .font(type_::BODY)
            .color(theme.text_muted)
            .wrap(),
        )
        .child(row().gap(8.0).child(open).child(reload));
    if let Some(issue) = &state.configuration_issue {
        section = section.child(
            col()
                .key("configuration-issue")
                .gap(5.0)
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
    section
}

fn section(title: &str, theme: &Theme) -> El {
    col()
        .key(format!("settings-section-{title}"))
        .w_full()
        .gap(12.0)
        .child(
            text(title)
                .font(TextStyle::new(16.0, 600.0).lh(21.0))
                .color(theme.text),
        )
        .child(el().w_full().h(1.0).fill(theme.overlay_border))
}

fn setting_row(title: &str, description: &str, theme: &Theme, control: El) -> El {
    row()
        .key(format!("setting-row-{title}"))
        .w_full()
        .gap(ROW_GAP)
        .justify_between()
        .center_y()
        .child(
            col()
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
                ),
        )
        .child(control)
}

fn setting_title(title: &str, theme: &Theme) -> El {
    text(title)
        .font(TextStyle::new(14.0, 500.0).lh(19.0))
        .color(theme.text)
}
