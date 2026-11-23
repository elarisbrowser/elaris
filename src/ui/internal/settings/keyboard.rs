// SPDX-License-Identifier: GPL-3.0-only

//! Keyboard shortcuts: every rebindable action with its keys. Clicking the keys listens for new
//! ones; the changes are saved to `[keybindings]` in `config.toml`.

use rune::tok::{space, type_};
use rune::{El, row, text};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::shortcuts::{self, Shortcut};
use crate::ui::components::controls;
use crate::ui::components::widgets::{row_title, rows, setting_with};
use crate::ui::icons;
use crate::ui::internal::PageContext;
use crate::ui::theme;

use super::section;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    let mut sections = vec![overview(cx)];
    let list = shortcuts::settings_list();
    let mut groups: Vec<&str> = Vec::new();
    for (group, _, _) in &list {
        if !groups.contains(group) {
            groups.push(group);
        }
    }
    for group in groups {
        let rows = list
            .iter()
            .filter(|(listed, _, _)| *listed == group)
            .map(|(_, shortcut, title)| action(cx, *shortcut, title));
        sections.push(section(group, cx.theme, rows.collect::<Vec<_>>()));
    }
    sections
}

/// How to change a shortcut, or what the last change did, beside Reset All.
fn overview(cx: &PageContext) -> El {
    let theme = cx.theme;
    let detail = match &cx.state.shortcut_recorder.notice {
        Some(notice) => text(notice.as_str())
            .font(type_::BODY)
            .color(theme.text)
            .wrap()
            .role("status"),
        None => {
            text("Click a shortcut, then press the new keys. Backspace removes it; Escape cancels.")
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap()
        }
    };
    let reset = Button::new("Reset All")
        .variant(ButtonVariant::Neutral)
        .disabled(cx.state.settings.keybindings.is_empty())
        .on_press(|state: &mut AppState| state.reset_all_shortcuts())
        .build(&cx.kit);
    // The page title already says what this is, so the overview needs no heading of its own.
    rows(
        theme,
        [setting_with(cx, "Change a shortcut", detail, reset)],
    )
}

/// One action: its title, a reset button while it has its own keys, and its keys.
fn action(cx: &PageContext, shortcut: Shortcut, title: &str) -> El {
    let name = shortcut.name().unwrap_or_default();
    let reset = cx.state.shortcut_customized(shortcut).then(|| {
        controls::icon_button(
            &format!("shortcut-reset-{name}"),
            &format!("Reset {title} to its default keys"),
            &icons::RELOAD,
            theme::CLOSE_BUTTON,
            cx.theme,
        )
        .focusable(true)
        .on_press(move |state: &mut AppState| state.reset_shortcut(shortcut))
    });
    row()
        .key(format!("shortcut-{name}"))
        .w_full()
        .gap(theme::PAGE_ROW_GAP)
        .justify_between()
        .center_y()
        .child(row_title(title, cx.theme).grow(1.0).min_w(0.0))
        .child(
            row()
                .gap(space::S2)
                .center_y()
                .children(reset)
                .child(recorder(cx, shortcut, title, name)),
        )
}

/// The action's keys as a field. Pressing it listens for new ones: the field holds focus as a
/// modal until a chord arrives, Escape or Backspace, or a click outside.
fn recorder(cx: &PageContext, shortcut: Shortcut, title: &str, name: &str) -> El {
    let theme = cx.theme;
    let recording = cx.state.shortcut_recorder.recording == Some(shortcut);
    let hint = shortcut.hint();
    let (shown, color) = match (&hint, recording) {
        (_, true) => ("Press keys…", theme.text_muted),
        (Some(hint), false) => (hint.as_str(), theme.text),
        (None, false) => ("None", theme.text_muted),
    };
    let field = row()
        .key(format!("shortcut-keys-{name}"))
        .w(theme::SHORTCUT_FIELD_WIDTH)
        .h(theme::SHORTCUT_FIELD_HEIGHT)
        .px(theme::FIELD_PADDING)
        .center()
        .radius(theme::FIELD_RADIUS)
        .continuous_corners()
        .fill(theme.address_field)
        .border(
            1.0,
            if recording {
                theme.focus
            } else {
                theme.control_border
            },
        )
        .hover(theme.hover)
        .press(theme.press)
        .focusable(true)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .role("button")
        .label(format!("{title}: {shown}"))
        .help("Click, then press the new keys")
        .on_press(move |state: &mut AppState| state.toggle_recording(shortcut))
        .child(
            text(shown)
                .font(type_::BODY)
                .color(color)
                .single_line()
                .a11y_hidden(),
        );
    if recording {
        field
            .on_key(|state: &mut AppState, event, _| state.record_key(event))
            .modal(true, |state: &mut AppState| state.stop_recording())
    } else {
        field
    }
}
