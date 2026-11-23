// SPDX-License-Identifier: GPL-3.0-only

mod date;

use std::path::PathBuf;

use crate::ui::menu::{MenuItem, context_menu};
use elaris::ladybird::{FormPicker, SelectRow};
use rune::tok::{radius, space, type_};
use rune::{El, FileDialogOptions, TextEdit, Tx, col, el, row, text, text_input};
use rune_kit::{Button, ButtonVariant, Dialog};

use crate::app::AppState;
use crate::browser::Polled;
use crate::engine::ViewCommand;
use crate::tab::TabId;
use crate::ui::{
    theme::{self, Theme},
    viewport,
};

pub(crate) struct ActivePicker {
    tab: TabId,
    request_id: u64,
    picker: FormPicker,
    input: TextEdit,
    calendar_month: Option<(u32, u32)>,
    files: Vec<PathBuf>,
}

pub(crate) fn poll(state: &mut AppState, polled: &mut Polled, tx: &mut Tx) {
    let active = state.browser.input_page();
    if let Some(picker) = &state.form_picker
        && (Some(picker.tab) != active
            || state
                .browser
                .page(picker.tab)
                .is_none_or(|tab| tab.error.is_some()))
    {
        reply(state, picker.tab, picker.request_id, None);
        polled.changed = true;
    }
    for (tab, request) in polled.form_pickers.drain(..) {
        let Some((request_id, picker)) = request else {
            if state
                .form_picker
                .as_ref()
                .is_some_and(|picker| picker.tab == tab)
            {
                state.form_picker = None;
            }
            continue;
        };
        if Some(tab) != active || state.form_picker.is_some() {
            state.browser.send(
                tab,
                ViewCommand::ReplyToPicker {
                    request_id,
                    response: None,
                },
            );
            continue;
        }
        let mut input = TextEdit::default();
        match &picker {
            FormPicker::Color { value } | FormPicker::Date { value, .. } => {
                input.replace_text(value.clone())
            }
            FormPicker::File {
                extensions,
                multiple: false,
            } => {
                choose_file(tx, tab, request_id, extensions.clone(), false);
            }
            FormPicker::File { multiple: true, .. } => {}
            FormPicker::Select { .. } => {}
        }
        state.form_picker = Some(ActivePicker {
            tab,
            request_id,
            picker,
            input,
            calendar_month: None,
            files: Vec::new(),
        });
    }
}

fn choose_file(tx: &mut Tx, tab: TabId, request_id: u64, extensions: Vec<String>, multiple: bool) {
    let mut options = FileDialogOptions {
        title: "Choose a file to upload".into(),
        ..Default::default()
    };
    if !extensions.is_empty() {
        options.filters.push(("Accepted files".into(), extensions));
    }
    tx.open_file(options, move |state: &mut AppState, result, tx| {
        let path = match result {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Elaris: {error}");
                None
            }
        };
        if multiple {
            if let Some(active) = state.form_picker.as_mut()
                && active.tab == tab
                && active.request_id == request_id
                && let Some(path) = path
                && !active.files.contains(&path)
            {
                active.files.push(path);
            }
        } else {
            let response = path.map(|path| path.as_os_str().as_encoded_bytes().to_vec());
            if reply(state, tab, request_id, response) {
                tx.focus(viewport::focus_key(&state.browser), false);
            }
        }
    });
}

fn reply(state: &mut AppState, tab: TabId, request_id: u64, response: Option<Vec<u8>>) -> bool {
    if state
        .form_picker
        .as_ref()
        .is_none_or(|picker| picker.tab != tab || picker.request_id != request_id)
    {
        return false;
    }
    state.form_picker = None;
    state.browser.send(
        tab,
        ViewCommand::ReplyToPicker {
            request_id,
            response,
        },
    );
    true
}

pub(super) fn view(state: &AppState, theme: &Theme, overlays: &mut Vec<El>, reduced_motion: bool) {
    let Some(active) = &state.form_picker else {
        return;
    };
    let tab = active.tab;
    let request_id = active.request_id;
    let cancel = move |state: &mut AppState| {
        reply(state, tab, request_id, None);
    };
    let kit = theme.kit(theme.overlay_panel);
    if let FormPicker::Select {
        x,
        y,
        minimum_width,
        rows,
    } = &active.picker
    {
        let point = [*x as f32, *y as f32];
        let [anchor_x, anchor_y] = super::peek::page_point(state, point, reduced_motion)
            .unwrap_or_else(|| {
                let [left, top] = viewport::page_origin(state);
                [left + point[0], top + point[1]]
            });
        let mut menu = context_menu("page-select", el(), None)
            .at(anchor_x, anchor_y)
            .min_width((*minimum_width as f32).clamp(120.0, 480.0))
            .open(true)
            .on_close(cancel);
        for item in rows {
            menu = match item {
                SelectRow::Option {
                    id,
                    label,
                    disabled,
                    selected,
                } => {
                    let id = *id;
                    menu.item(
                        MenuItem::new(label.clone())
                            .disabled(*disabled)
                            .checked(*selected)
                            .on_select_tx(move |state: &mut AppState, tx| {
                                if reply(state, tab, request_id, Some(id.to_string().into_bytes()))
                                {
                                    tx.focus(viewport::focus_key(&state.browser), false);
                                }
                            }),
                    )
                }
                SelectRow::Group { label } => menu.group_label(label.clone()),
                SelectRow::Separator => menu.separator(),
            };
        }
        menu.build(state, theme, overlays);
        return;
    }
    if let FormPicker::File {
        extensions,
        multiple: true,
    } = &active.picker
    {
        multiple_files(active, extensions, theme, overlays);
        return;
    }
    let (title, placeholder) = match &active.picker {
        FormPicker::Color { .. } => ("Choose a color", "#RRGGBB"),
        FormPicker::Date { input_type, .. } => match input_type.as_str() {
            "time" => ("Choose a time", "HH:MM"),
            "month" => ("Choose a month", "YYYY-MM"),
            "week" => ("Choose a week", "YYYY-Www"),
            "datetime-local" => ("Choose a date and time", "YYYY-MM-DDTHH:MM"),
            _ => ("Choose a date", "YYYY-MM-DD"),
        },
        _ => return,
    };
    let mut body = col()
        .w_full()
        .px(space::S3)
        .pb(space::S3)
        .gap(space::S3)
        .child(
            text_input(&active.input, type_::BODY)
                .key("page-picker-value")
                .label(title)
                .placeholder(placeholder, theme.text_muted)
                .color(theme.text)
                .caret(theme.text, theme.selection)
                .single_line()
                .autofocus()
                .on_edit(|state: &mut AppState| {
                    &mut state
                        .form_picker
                        .as_mut()
                        .expect("picker input is mounted only for an active picker")
                        .input
                }),
        );
    match &active.picker {
        FormPicker::Color { .. } => {
            body = body.child(color_palette(&active.input.text, theme));
        }
        FormPicker::Date {
            input_type,
            minimum,
            maximum,
            ..
        } if input_type == "date" => {
            body = body.child(date::calendar(active, minimum, maximum, theme));
        }
        _ => {}
    }
    let confirm = move |state: &mut AppState| {
        if let Some(active) = &state.form_picker
            && valid(active)
        {
            let value = active.input.text.trim().as_bytes().to_vec();
            reply(state, tab, request_id, Some(value));
        }
    };
    let mut actions = row().w_full().justify_end().gap(space::S2);
    if matches!(active.picker, FormPicker::Date { .. }) && !active.input.text.is_empty() {
        actions = actions.child(
            Button::new("Clear")
                .on_press(move |state: &mut AppState| {
                    reply(state, tab, request_id, Some(Vec::new()));
                })
                .build(&kit),
        );
    }
    body = body.child(
        actions
            .child(Button::new("Cancel").on_press(cancel).build(&kit))
            .child(
                Button::new("Apply")
                    .variant(ButtonVariant::Submit)
                    .disabled(!valid(active))
                    .on_press(confirm)
                    .build(&kit),
            ),
    );
    Dialog::new("page-form-picker", title)
        .open(true)
        .fit()
        .width(360.0)
        .click_outside(false)
        .on_close(cancel)
        .child(body)
        .build(&kit, overlays);
}

fn multiple_files(
    active: &ActivePicker,
    extensions: &[String],
    theme: &Theme,
    overlays: &mut Vec<El>,
) {
    let tab = active.tab;
    let request_id = active.request_id;
    let extensions = extensions.to_vec();
    let kit = theme.kit(theme.overlay_panel);
    let mut files = col()
        .w_full()
        .gap(space::S1)
        .max_h(180.0)
        .scroll(rune::ScrollSpec {
            stick_bottom: false,
        });
    for (index, file) in active.files.iter().enumerate() {
        let name = file
            .file_name()
            .unwrap_or(file.as_os_str())
            .to_string_lossy();
        files = files.child(
            row()
                .w_full()
                .center_y()
                .gap(space::S2)
                .child(
                    text(name.as_ref())
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate()
                        .color(theme.text),
                )
                .child(
                    Button::new("Remove")
                        .on_press(move |state: &mut AppState| {
                            if let Some(active) = state.form_picker.as_mut()
                                && index < active.files.len()
                            {
                                active.files.remove(index);
                            }
                        })
                        .build(&kit),
                ),
        );
    }
    let cancel = move |state: &mut AppState| {
        reply(state, tab, request_id, None);
    };
    let actions = row()
        .w_full()
        .justify_end()
        .gap(space::S2)
        .child(
            Button::new("Add file…")
                .on_press_tx(move |_: &mut AppState, tx| {
                    choose_file(tx, tab, request_id, extensions.clone(), true);
                })
                .build(&kit)
                .autofocus(),
        )
        .child(Button::new("Cancel").on_press(cancel).build(&kit))
        .child(
            Button::new("Upload")
                .variant(ButtonVariant::Submit)
                .disabled(active.files.is_empty())
                .on_press(move |state: &mut AppState| {
                    if let Some(active) = &state.form_picker
                        && !active.files.is_empty()
                    {
                        let files: Vec<_> = active
                            .files
                            .iter()
                            .map(|path| path.as_os_str().as_encoded_bytes())
                            .collect();
                        reply(state, tab, request_id, Some(files.join(&0)));
                    }
                })
                .build(&kit),
        );
    Dialog::new("page-upload-files", "Choose files to upload")
        .open(true)
        .fit()
        .width(360.0)
        .click_outside(false)
        .on_close(cancel)
        .child(
            col()
                .w_full()
                .px(space::S3)
                .pb(space::S3)
                .gap(space::S3)
                .child(files)
                .child(actions),
        )
        .build(&kit, overlays);
}

fn color_palette(value: &str, theme: &Theme) -> El {
    let color = rune::color::hex(value.trim()).unwrap_or(theme.overlay_panel);
    let preview = el()
        .key("page-picker-swatch")
        .w_full()
        .h(space::S6)
        .radius(radius::SM)
        .border(1.0, theme.overlay_border)
        .fill(color)
        .a11y_hidden();
    let mut swatches = row().w_full().gap(space::S2);
    for hue in (0..360).step_by(30) {
        let color = rune::oklch(0.68, 0.18, hue as f32);
        let hex = color.hex()[..7].to_owned();
        swatches = swatches.child(
            el().w(0.0)
                .grow(1.0)
                .h(space::S6)
                .radius(radius::SM)
                .fill(color)
                .role("button")
                .label(hex.clone())
                .focusable(true)
                .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
                .on_press(move |state: &mut AppState| {
                    if let Some(active) = state.form_picker.as_mut() {
                        active.input.replace_text(hex.clone());
                    }
                }),
        );
    }
    col().w_full().gap(space::S2).child(preview).child(swatches)
}

fn valid(active: &ActivePicker) -> bool {
    let value = active.input.text.trim();
    match &active.picker {
        FormPicker::Color { .. } => value
            .strip_prefix('#')
            .is_some_and(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())),
        FormPicker::Date {
            input_type,
            minimum,
            maximum,
            ..
        } => {
            date::valid_value(input_type, value)
                && date::within_bounds(input_type, value, minimum, maximum)
        }
        _ => false,
    }
}

/// Escape cancels the open picker, as its Cancel button does.
pub(crate) fn cancel(state: &mut AppState) -> bool {
    let Some(active) = state.form_picker.as_ref() else {
        return false;
    };
    let (tab, request_id) = (active.tab, active.request_id);
    reply(state, tab, request_id, None);
    true
}
