// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver's settings: its mode, what it has freed, and the sites it always keeps awake.

use rune::tok::{space, type_};
use rune::{El, Key, row, text, text_input};
use rune_kit::{Button, SegmentedControl};

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::widgets::{row_title, setting, setting_with};
use crate::ui::icons;
use crate::ui::internal::PageContext;
use crate::ui::theme;
use elaris_memory_saver::MemorySaver;

const SITE_FIELD: &str = "keep-awake-site";

pub(super) fn rows(cx: &mut PageContext) -> Vec<El> {
    let mode = cx.state.settings.memory_saver;
    let control = SegmentedControl::new(
        &MemorySaver::ALL.map(MemorySaver::label),
        MemorySaver::ALL.iter().position(|choice| *choice == mode),
    )
    .width(Some(theme::PAGE_CONTROL_WIDTH))
    .label("Memory Saver")
    .on_pick(|state: &mut AppState, index, _| state.set_memory_saver(MemorySaver::ALL[index]))
    .build(&cx.kit);
    let mut rows = vec![setting(
        cx,
        "Memory Saver",
        &format!(
            "Puts background tabs to sleep to free memory; they pick up where they left off when you return. {}",
            mode.description()
        ),
        control,
    )];
    rows.push(setting(cx, "Memory freed", &freed(), el_empty()));
    rows.push(keep_awake(cx));
    for site in &cx.state.settings.keep_awake_sites {
        rows.push(kept_site(site, cx));
    }
    rows
}

/// What sleeping tabs have freed since Elaris started.
fn freed() -> String {
    match crate::app::memory_saver_freed() {
        (_, 0) => "No tabs have slept since Elaris started.".to_owned(),
        (0, tabs) => format!("{} slept since Elaris started.", tabs_label(tabs)),
        (bytes, tabs) => format!(
            "At least {} freed by putting {} to sleep since Elaris started.",
            elaris_core::processes::format_memory(bytes),
            tabs_label(tabs)
        ),
    }
}

fn tabs_label(tabs: u64) -> String {
    if tabs == 1 {
        "1 tab".to_owned()
    } else {
        format!("{tabs} tabs")
    }
}

fn el_empty() -> El {
    row()
}

fn keep_awake(cx: &mut PageContext) -> El {
    let theme = cx.theme;
    let input = &cx.state.memory_saver.site_input;
    let field = text_input(input, type_::BODY)
        .key(SITE_FIELD)
        .single_line()
        .grow(1.0)
        .min_w(0.0)
        .color(theme.text)
        .caret(theme.text, theme.selection)
        .no_focus_ring()
        .placeholder("mail.example.com", theme.text_muted)
        .label("Site to keep awake")
        .on_edit(|state: &mut AppState| &mut state.memory_saver.site_input)
        .on_input_key(|state: &mut AppState, event, _| {
            if event.key == Key::Enter {
                state.add_keep_awake_site();
                true
            } else {
                false
            }
        });
    let field = row()
        .grow(1.0)
        .min_w(0.0)
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .px(theme::FIELD_PADDING)
        .radius(theme::FIELD_RADIUS)
        .fill(theme.address_field)
        .border(1.0, theme.control_border)
        .focus_within_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .child(field);
    let add = Button::new("Add")
        .disabled(input.text.trim().is_empty())
        .on_press(|state: &mut AppState| state.add_keep_awake_site())
        .build(&cx.kit);
    let control = row()
        .w(theme::PAGE_CONTROL_WIDTH)
        .gap(space::S2)
        .center_y()
        .child(field)
        .child(add);
    let detail = cx.state.memory_saver.site_error.map_or_else(
        || {
            text("Tabs on these sites, and their subdomains, never sleep.")
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap()
        },
        |error| {
            text(error)
                .font(type_::BODY)
                .color(theme.text)
                .wrap()
                .role("alert")
        },
    );
    setting_with(cx, "Always keep these sites awake", detail, control)
}

fn kept_site(site: &str, cx: &PageContext) -> El {
    let removed = site.to_owned();
    let remove = controls::icon_button(
        &format!("keep-awake-remove-{site}"),
        &format!("Stop keeping {site} awake"),
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        cx.theme,
    )
    .focusable(true)
    .on_press(move |state: &mut AppState| state.remove_keep_awake_site(&removed));
    row()
        .key(format!("keep-awake-{site}"))
        .w_full()
        .justify_between()
        .center_y()
        .child(row_title(site, cx.theme))
        .child(remove)
}
