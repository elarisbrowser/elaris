// SPDX-License-Identifier: GPL-3.0-only

//! Startup, default browser and configuration file preferences.

use rune::tok::type_;
use rune::{El, col, text};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::ui::components::menu::Dropdown;
use crate::ui::components::widgets::{actions, row_title, setting};
use crate::ui::internal::PageContext;
use crate::ui::theme;
use elaris_core::settings::StartupMode;

use super::section;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    let mut sections = vec![startup(cx)];
    #[cfg(elaris_ui = "linux")]
    sections.push(window_frame(cx));
    sections.extend([default_browser(cx), configuration(cx)]);
    sections
}

#[cfg(elaris_ui = "linux")]
fn window_frame(cx: &mut PageContext) -> El {
    use crate::app::LinuxDecorations;
    use elaris_core::settings::WindowDecorations;

    let select = Dropdown::new("settings-window-frame", "Window frame")
        .options(WindowDecorations::ALL.map(|mode| (mode, mode.label())))
        .current(cx.state.settings.window_decorations)
        .open(cx.state.decorations_picker_open)
        .on_toggle(|state| state.decorations_picker_open = !state.decorations_picker_open)
        .on_close(|state| state.decorations_picker_open = false)
        .on_choose(|state, mode| {
            state.set_window_decorations(mode);
            state.decorations_picker_open = false;
        })
        .build(cx.state, cx.theme, cx.overlays);
    let description = match LinuxDecorations::override_setting() {
        Some(mode) => format!(
            "ELARIS_LINUX_DECORATIONS is set, so Elaris uses the {} frame.",
            mode.label().to_lowercase()
        ),
        None => "Automatic uses the system frame on tiling window managers, where Elaris \
                 hides its toolbar and window controls. Changes apply after a restart."
            .to_owned(),
    };
    if cx.state.restart_notice {
        restart_notice(cx);
    }
    section(
        "Window",
        cx.theme,
        [setting(cx, "Window frame", &description, select)],
    )
}

/// Tells the user the new frame needs a restart. Open windows keep the frame they opened with.
#[cfg(elaris_ui = "linux")]
fn restart_notice(cx: &mut PageContext) {
    use rune::tok::space;
    use rune_kit::Dialog;

    let kit = cx.theme.kit(cx.theme.overlay_panel);
    let dismiss = |state: &mut AppState| state.restart_notice = false;
    let ok = Button::new("OK")
        .variant(ButtonVariant::Neutral)
        .on_press(dismiss)
        .build(&kit)
        .key("restart-notice-ok")
        .autofocus();
    Dialog::new("restart-notice", "Restart to apply")
        .open(true)
        .fit()
        .width(theme::PAGE_DIALOG_WIDTH)
        .on_close(dismiss)
        .child(
            col()
                .w_full()
                .px(theme::PAGE_SECTION_GAP)
                .pb(theme::PAGE_SECTION_GAP)
                .gap(theme::PAGE_SECTION_GAP)
                .child(
                    text("Elaris will use the new window frame the next time it opens.")
                        .font(type_::BODY)
                        .color(cx.theme.text_muted)
                        .wrap(),
                )
                .child(rune::row().w_full().gap(space::S2).justify_end().child(ok)),
        )
        .build(&kit, cx.overlays);
}

fn startup(cx: &mut PageContext) -> El {
    const OPTIONS: [StartupMode; 2] = [StartupMode::RestoreSession, StartupMode::BlankWindow];
    let select = Dropdown::new("settings-startup", "Startup behavior")
        .options(OPTIONS.map(|option| (option, option.label())))
        .current(cx.state.settings.startup)
        .open(cx.state.startup_picker_open)
        .on_toggle(|state| state.startup_picker_open = !state.startup_picker_open)
        .on_close(|state| state.startup_picker_open = false)
        .on_choose(|state, startup| {
            state.set_startup(startup);
            state.startup_picker_open = false;
        })
        .build(cx.state, cx.theme, cx.overlays);
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
