// SPDX-License-Identifier: GPL-3.0-only

//! The browser's compact native settings page.

use std::path::{Path, PathBuf};

use rune::tok::type_;
use rune::{Drag, El, Phase, ScrollSpec, TextStyle, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, SegmentedControl, Select, SelectAppearance};

use crate::address::SearchEngine;
use crate::app::{AppState, ClearRange};
use crate::appearance::{AccentColor, Appearance};
use crate::settings::{DownloadDirectory, StartupMode, TabLayout};
use crate::ui::theme::{self, Theme};

const CONTENT_WIDTH: f32 = 720.0;
const CONTENT_INSET: f32 = 28.0;
const TOP_PADDING: f32 = 48.0;
const SECTION_GAP: f32 = 25.0;
const ROW_GAP: f32 = 16.0;
const CONTROL_WIDTH: f32 = 220.0;
const ACCENT_SWATCH_SIZE: f32 = 36.0;
const ACCENT_DOT_SIZE: f32 = 20.0;
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
                .font(super::theme::PAGE_TITLE)
                .color(theme.text),
        )
        .child(appearance_section(state, theme, &kit, compact))
        .child(page_zoom_section(state, theme, &kit, compact))
        .child(search_section(state, theme, &kit, overlays, compact))
        .child(downloads_section(state, theme, &kit, compact))
        .child(privacy_section(state, theme, &kit, overlays, compact))
        .child(split_view_section(state, theme, &kit, compact))
        .child(performance_section(state, theme, &kit, compact))
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

fn page_zoom_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    const ZOOM_OPTIONS: [u16; 5] = [75, 90, 100, 125, 150];
    let current = state.settings.page_zoom;
    let selected = ZOOM_OPTIONS.iter().position(|&zoom| zoom == current);
    let control = SegmentedControl::new(&["75%", "90%", "100%", "125%", "150%"], selected)
        .width(Some(CONTROL_WIDTH))
        .label("Default page zoom")
        .on_pick(|state: &mut AppState, index, _| state.set_page_zoom(ZOOM_OPTIONS[index]))
        .build(kit);
    section(
        "Pages",
        theme,
        settings_group(
            setting_row(
                "Default page zoom",
                &format!("Current default is {current}%."),
                theme,
                control,
                compact,
            ),
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

    let tab_layout = SegmentedControl::new(
        &TabLayout::ALL.map(TabLayout::label),
        TabLayout::ALL
            .iter()
            .position(|layout| *layout == state.settings.tab_layout),
    )
    .width(Some(CONTROL_WIDTH))
    .label("Tab Layout")
    .on_pick(|state: &mut AppState, index, tx| {
        state.set_tab_layout(TabLayout::ALL[index]);
        tx.animated();
        tx.motion(theme::SIDEBAR_TOGGLE);
    })
    .build(kit);

    // Rune Kit's standard switch uses the strong icon token for its checked track.
    let mut switch_kit = kit.clone();
    switch_kit.c.std_icon_strong = kit.c.bg_accent;
    let glass = rune_kit::Switch::new(state.settings.glass_enabled)
        .standard()
        .label("Liquid glass background")
        .on_toggle(|state: &mut AppState| state.set_glass_enabled(!state.settings.glass_enabled))
        .build(&switch_kit);
    let opacity = tint_opacity_control(state.settings.tint_opacity, theme);
    section(
        "Appearance",
        theme,
        settings_group(
            col()
                .gap(12.0)
                .child(setting_row(
                    "Theme",
                    "Choose how Elaris and webpages look.",
                    theme,
                    control,
                    compact,
                ))
                .child(setting_row(
                    "Tab Layout",
                    "Choose where open tabs appear.",
                    theme,
                    tab_layout,
                    compact,
                ))
                .child(setting_row(
                    "Accent colour",
                    "Choose the colour used for interactive emphasis.",
                    theme,
                    accent_color_control(state.settings.accent_color, theme),
                    compact,
                ))
                .child(setting_row(
                    "Liquid glass",
                    "Show the tinted glass window background.",
                    theme,
                    glass,
                    compact,
                ))
                .child(setting_row(
                    "Tint opacity",
                    "100% keeps the current opaque tint.",
                    theme,
                    opacity,
                    compact,
                )),
            theme,
        ),
    )
}

fn accent_color_control(selected: AccentColor, theme: &Theme) -> El {
    let mut choices = row()
        .key("accent-colour-options")
        .role("radiogroup")
        .label("Accent colour")
        .w(ACCENT_SWATCH_SIZE * AccentColor::ALL.len() as f32 + 5.0 * 5.0)
        .gap(5.0)
        .center_y();

    for (index, accent) in AccentColor::ALL.into_iter().enumerate() {
        let is_selected = selected == accent;
        let mut choice = el()
            .key(format!("accent-colour-{}", accent.config_key()))
            .role("radio")
            .label(accent.label())
            .checked(is_selected)
            .position_in_set(index + 1, AccentColor::ALL.len())
            .size(ACCENT_SWATCH_SIZE, ACCENT_SWATCH_SIZE)
            .radius(ACCENT_SWATCH_SIZE / 2.0)
            .center()
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .hover(theme.hover)
            .press(theme.press)
            .on_press(move |state: &mut AppState| state.set_accent_color(accent))
            .child(
                el().key("swatch")
                    .size(ACCENT_DOT_SIZE, ACCENT_DOT_SIZE)
                    .radius(ACCENT_DOT_SIZE / 2.0)
                    .fill(theme::accent_color(accent))
                    .a11y_hidden(),
            );
        if is_selected {
            choice = choice.border(3.0, theme.accent);
        }
        choices = choices.child(choice);
    }

    choices
}

fn tint_opacity_control(value: u8, theme: &Theme) -> El {
    const WIDTH: f32 = 220.0;
    const KNOB: f32 = 16.0;
    let value = value.min(100);
    let fraction = f32::from(value) / 100.0;
    let travel = WIDTH - KNOB;
    let position = KNOB / 2.0 + fraction * travel;
    row()
        .gap(8.0)
        .center_y()
        .child(
            el().key("tint-opacity-slider")
                .role("slider")
                .label("Tint opacity")
                .range_value(f64::from(value), 0.0, 100.0)
                .range_step(1.0)
                .a11y_value(format!("{value}%"))
                .size(WIDTH, 22.0)
                .on_step(|state: &mut AppState, steps, _| {
                    state.set_tint_opacity(
                        (i16::from(state.settings.tint_opacity) + steps as i16).clamp(0, 100) as u8,
                    );
                })
                .on_drag(
                    move |state: &mut AppState, drag: &mut Drag, _| match drag.phase {
                        Phase::Down => {
                            drag.data = f32::from(state.settings.tint_opacity);
                            state.set_tint_opacity(
                                (((drag.x - KNOB / 2.0) / travel).clamp(0.0, 1.0) * 100.0).round()
                                    as u8,
                            );
                        }
                        Phase::Move => {
                            state.set_tint_opacity(
                                (((drag.x - KNOB / 2.0) / travel).clamp(0.0, 1.0) * 100.0).round()
                                    as u8,
                            );
                        }
                        Phase::Up if drag.cancelled() => {
                            state.set_tint_opacity(drag.data.clamp(0.0, 100.0) as u8)
                        }
                        Phase::Up => {}
                    },
                )
                .child(
                    el().key("track")
                        .absolute(0.0, 9.0)
                        .size(WIDTH, 4.0)
                        .radius(2.0)
                        .fill(theme.control_border),
                )
                .child(
                    el().key("fill")
                        .absolute(0.0, 9.0)
                        .size(position, 4.0)
                        .radius(2.0)
                        .fill(theme.focus),
                )
                .child(
                    el().key("knob")
                        .absolute(position - KNOB / 2.0, 3.0)
                        .size(KNOB, KNOB)
                        .radius(KNOB / 2.0)
                        .fill(theme.icon),
                ),
        )
        .child(
            text(format!("{value}%"))
                .font(type_::LABEL)
                .color(theme.text),
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

fn privacy_section(
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
