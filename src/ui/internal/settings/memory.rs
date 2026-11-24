// SPDX-License-Identifier: GPL-3.0-only

//! Memory Saver's settings: what it has freed, its mode, and the sites it always keeps awake.

use rune::tok::{space, type_};
use rune::{El, Key, col, icon, row, text, text_input};
use rune_kit::Button;

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::widgets::{row_title, setting, setting_with};
use crate::ui::icons;
use crate::ui::internal::PageContext;
use crate::ui::theme;
use elaris_memory_saver::MemorySaver;

const SITE_FIELD: &str = "keep-awake-site";

/// Memory Saver's rows for the Performance section.
pub(super) fn rows(cx: &mut PageContext) -> Vec<El> {
    let mode = cx.state.settings.memory_saver;
    let mut rows = vec![intro(cx)];
    rows.extend(modes(mode, cx));
    // With Memory Saver off no tab sleeps, so there is nothing to keep awake.
    if mode != MemorySaver::Off {
        rows.push(keep_awake(cx));
        for site in &cx.state.settings.keep_awake_sites {
            rows.push(kept_site(site, cx));
        }
    }
    rows
}

/// What Memory Saver does, and what sleeping tabs have freed once any have slept.
fn intro(cx: &PageContext) -> El {
    let theme = cx.theme;
    let mut detail = col().gap(theme::PAGE_ROW_COPY_GAP).child(
        text("Puts background tabs to sleep to free memory. They pick up where they left off when you return.")
            .font(type_::BODY)
            .color(theme.text_muted)
            .wrap(),
    );
    if let Some(freed) = freed() {
        detail = detail.child(
            row()
                .key("memory-freed")
                .gap(space::S1)
                .center_y()
                .child(
                    icon(&icons::BED, theme::ICON)
                        .color(theme.icon)
                        .a11y_hidden(),
                )
                .child(text(freed).font(type_::BODY).color(theme.text).wrap()),
        );
    }
    setting_with(cx, "Memory Saver", detail, row())
}

/// The modes as a list of radio rows, each saying when tabs sleep. The chosen one carries a tick.
fn modes(mode: MemorySaver, cx: &PageContext) -> Vec<El> {
    let theme = cx.theme;
    let count = MemorySaver::ALL.len();
    MemorySaver::ALL
        .into_iter()
        .enumerate()
        .map(|(index, choice)| {
            let selected = choice == mode;
            let tick = rune::canvas(
                rune::Shape::new()
                    .stroke(icons::TICK.to_vec(), theme::TICK_STROKE, theme.accent)
                    .view_box(theme::ICON, theme::ICON),
            )
            .size(theme::ICON, theme::ICON)
            .opacity(if selected { 1.0 } else { 0.0 })
            .a11y_hidden();
            setting(cx, choice.label(), choice.description(), tick)
                .key(format!("memory-saver-{}", choice.label().to_lowercase()))
                .role("radio")
                .label(choice.label())
                .description(choice.description())
                .checked(selected)
                .position_in_set(index + 1, count)
                .focusable(true)
                .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
                .on_press(move |state: &mut AppState| state.set_memory_saver(choice))
        })
        .collect()
}

/// What sleeping tabs have freed since Elaris started, once any have slept.
fn freed() -> Option<String> {
    match crate::app::memory_saver_freed() {
        (_, 0) => None,
        (0, tabs) => Some(format!("{} slept since Elaris started.", tabs_label(tabs))),
        (bytes, tabs) => Some(format!(
            "{} freed by putting {} to sleep since Elaris started.",
            elaris_core::processes::format_memory(bytes),
            tabs_label(tabs)
        )),
    }
}

fn tabs_label(tabs: u64) -> String {
    if tabs == 1 {
        "1 tab".to_owned()
    } else {
        format!("{tabs} tabs")
    }
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
        .child(
            row()
                .gap(space::S2)
                .center_y()
                .min_w(0.0)
                .child(
                    icon(&icons::PAGE, theme::ICON)
                        .color(cx.theme.icon)
                        .a11y_hidden(),
                )
                .child(row_title(site, cx.theme)),
        )
        .child(remove)
}
