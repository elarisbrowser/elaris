// SPDX-License-Identifier: GPL-3.0-only

//! Startup, default browser and configuration file preferences.

use rune::tok::type_;
use rune::{El, col, text};
use rune_kit::{Button, ButtonVariant, Select, SelectAppearance};

use crate::app::AppState;
use crate::settings::StartupMode;
use crate::ui::internal::PageContext;
use crate::ui::internal::widgets::{Category, actions, row_title, setting};
use crate::ui::theme;

use super::section;

pub(super) fn category(cx: &mut PageContext) -> Category {
    Category::new(
        "settings-system",
        "Browser setup",
        vec![startup(cx), default_browser(cx), configuration(cx)],
    )
}

fn startup(cx: &mut PageContext) -> El {
    const OPTIONS: [StartupMode; 2] = [StartupMode::RestoreSession, StartupMode::BlankWindow];
    let select = Select::new("settings-startup", &OPTIONS.map(StartupMode::label))
        .current(Some(cx.state.settings.startup.label()))
        .label("Startup behavior")
        .appearance(SelectAppearance::Field)
        .width(theme::PAGE_CONTROL_WIDTH)
        .open(cx.state.startup_picker_open)
        .on_toggle(|state: &mut AppState| {
            state.startup_picker_open = !state.startup_picker_open;
        })
        .on_close(|state: &mut AppState| state.startup_picker_open = false)
        .on_choose(|state: &mut AppState, value, _| {
            if let Some(startup) = OPTIONS.into_iter().find(|option| option.label() == value) {
                state.set_startup(startup);
            }
            state.startup_picker_open = false;
        })
        .build(&cx.kit, cx.overlays)
        .border(1.0, cx.theme.control_border);
    section(
        "Startup",
        cx.theme,
        [setting(
            cx,
            "When Elaris opens",
            "Choose what opens when you launch Elaris.",
            select,
        )],
    )
}

fn default_browser(cx: &mut PageContext) -> El {
    let description = match (&cx.state.default_browser_error, cx.state.default_browser) {
        (Some(error), _) => error.clone(),
        (None, Some(true)) => "Elaris opens web links from other apps.".into(),
        (None, _) => "Open web links from other apps in Elaris.".into(),
    };
    let control = if cx.state.default_browser == Some(true) {
        text("Current default")
            .font(type_::LABEL)
            .color(cx.theme.text_muted)
    } else {
        Button::new("Make Default")
            .variant(ButtonVariant::Neutral)
            .on_press(|state: &mut AppState| state.make_default_browser())
            .build(&cx.kit)
    };
    section(
        "Default Browser",
        cx.theme,
        [setting(cx, "Default web browser", &description, control)],
    )
}

fn configuration(cx: &mut PageContext) -> El {
    let open = Button::new("Open File")
        .variant(ButtonVariant::Neutral)
        .on_press(|state: &mut AppState| state.open_config_file())
        .build(&cx.kit);
    let reload = Button::new("Reload")
        .variant(ButtonVariant::GhostMuted)
        .on_press(|state: &mut AppState| state.reload_configuration())
        .build(&cx.kit);
    let file = setting(
        cx,
        "Configuration file",
        "Edit config.toml directly, then reload your changes here.",
        actions([open, reload]),
    );
    let issue = cx.state.configuration_issue.as_ref().map(|issue| {
        col()
            .key("configuration-issue")
            .w_full()
            .gap(theme::PAGE_ROW_COPY_GAP)
            .role("alert")
            .child(row_title("Configuration needs attention", cx.theme))
            .child(
                text(issue.clone())
                    .font(type_::BODY)
                    .color(cx.theme.text_muted)
                    .wrap_anywhere(),
            )
    });
    section(
        "Advanced Configuration",
        cx.theme,
        std::iter::once(file).chain(issue),
    )
}
