// SPDX-License-Identifier: GPL-3.0-only

use elaris::ladybird::PageDialogKind;
use rune::tok::{space, type_};
use rune::{El, col, row, text, text_input};
use rune_kit::{Button, ButtonVariant, Dialog};

use crate::app::AppState;
use crate::engine::ViewCommand;
use crate::tab::{Tab, TabId};
use crate::ui::theme::Theme;

pub(super) fn view(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) {
    let Some(tab) = state
        .browser
        .peek
        .as_ref()
        .map(|peek| &peek.page)
        .or_else(|| state.browser.active_tab())
    else {
        return;
    };
    let Some(dialog) = tab.page_dialog.as_ref() else {
        return;
    };
    let id = tab.id();
    let request = &dialog.request;
    let request_id = request.request_id;
    let before_unload = request.kind == PageDialogKind::BeforeUnload;
    let alert = request.kind == PageDialogKind::Alert;
    let prompt = request.kind == PageDialogKind::Prompt;
    let title = if before_unload {
        "Leave this page?".to_owned()
    } else {
        crate::permissions::PermissionStore::origin(&tab.committed_url).map_or_else(
            || "This page says".into(),
            |origin| format!("{origin} says"),
        )
    };
    let kit = theme.kit(theme.overlay_panel);
    let mut body = col()
        .w_full()
        .px(space::S3)
        .pb(space::S3)
        .gap(space::S3)
        .child(
            text(if before_unload {
                "Changes you made may not be saved."
            } else {
                &request.message
            })
            .font(type_::BODY)
            .color(theme.text)
            .wrap(),
        );
    if prompt {
        body = body.child(
            text_input(&dialog.response, type_::BODY)
                .key("page-dialog-response")
                .label("Response")
                .color(theme.text)
                .caret(theme.text, theme.selection)
                .single_line()
                .autofocus()
                .on_edit(move |state: &mut AppState| {
                    &mut dialog_tab(state, id)
                        .expect("displayed dialog tab must exist")
                        .page_dialog
                        .as_mut()
                        .expect("dialog input belongs to the displayed request")
                        .response
                })
                .on_input_key(move |state: &mut AppState, event, _| {
                    if event.key == rune::Key::Enter {
                        answer(state, id, request_id, true);
                        true
                    } else {
                        false
                    }
                }),
        );
    }
    let mut actions = row().w_full().justify_end().gap(space::S2);
    if !alert {
        let cancel = Button::new(if before_unload { "Stay" } else { "Cancel" })
            .on_press(move |state: &mut AppState| answer(state, id, request_id, false))
            .build(&kit);
        actions = actions.child(if prompt { cancel } else { cancel.autofocus() });
    }
    let accept = Button::new(if before_unload { "Leave" } else { "OK" })
        .variant(ButtonVariant::Submit)
        .on_press(move |state: &mut AppState| answer(state, id, request_id, true))
        .build(&kit);
    actions = actions.child(if alert { accept.autofocus() } else { accept });
    Dialog::new("page-dialog", title)
        .open(true)
        .fit()
        .width(440.0)
        .click_outside(false)
        .on_close(move |state: &mut AppState| answer(state, id, request_id, false))
        .child(body.child(actions))
        .build(&kit, overlays);
}

fn dialog_tab(state: &mut AppState, id: TabId) -> Option<&mut Tab> {
    let browser = &mut state.browser;
    if browser
        .peek
        .as_ref()
        .is_some_and(|peek| peek.page.id() == id)
    {
        return browser.peek.as_mut().map(|peek| &mut peek.page);
    }
    browser.tab_mut(id)
}

fn answer(state: &mut AppState, id: TabId, request_id: u64, accepted: bool) {
    let Some(tab) = dialog_tab(state, id) else {
        return;
    };
    if tab
        .page_dialog
        .as_ref()
        .is_none_or(|dialog| dialog.request.request_id != request_id)
    {
        return;
    }
    let response = tab
        .page_dialog
        .take()
        .and_then(|mut dialog| accepted.then(|| std::mem::take(&mut dialog.response.text)));
    state.browser.send(
        id,
        ViewCommand::ReplyToDialog {
            request_id,
            response,
        },
    );
}
