// SPDX-License-Identifier: GPL-3.0-only

//! Runs a native menu action against the shared browser state.

use objc2_05::sel;
use objc2_app_kit_02::NSApplication;
use objc2_foundation_02::MainThreadMarker;
use rune::Tx;

use super::items::{Action, EditAction};
use crate::app::AppState;
use crate::shortcuts;
use crate::tab::{Tab, TabSection};

pub(super) fn perform_action(state: &mut AppState, action: Action, tx: &mut Tx) {
    let tab = state.browser.active_tab().map(Tab::id);
    match action {
        Action::Shortcut(shortcut) => shortcuts::dispatch(state, shortcut, tx),
        Action::Back => {
            if let Some(tab) = tab {
                state.browser.go_back(tab);
            }
        }
        Action::Forward => {
            if let Some(tab) = tab {
                state.browser.go_forward(tab);
            }
        }
        Action::Favourite => {
            if let Some(tab) = tab {
                let section = state.browser.tab(tab).map(Tab::section);
                let destination = if section == Some(TabSection::Favourite) {
                    TabSection::Regular
                } else {
                    TabSection::Favourite
                };
                state.move_tab_to_section_animated(tab, destination, tx);
            }
        }
        Action::SplitLeft | Action::SplitRight => {
            if let Some(tab) = tab {
                state.begin_split_picker(tab, usize::from(matches!(action, Action::SplitRight)));
            }
        }
        Action::ExitSplit => {
            state.browser.exit_split(tab);
            tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            tx.animated();
        }
        Action::Edit(edit) => {
            let page_edit = match edit {
                EditAction::Cut => Some(crate::ui::viewport::PageEdit::Cut),
                EditAction::Copy => Some(crate::ui::viewport::PageEdit::Copy),
                EditAction::Paste => Some(crate::ui::viewport::PageEdit::Paste),
                EditAction::SelectAll => Some(crate::ui::viewport::PageEdit::SelectAll),
                EditAction::Undo | EditAction::Redo => None,
            };
            let handled =
                page_edit.is_some_and(|edit| crate::ui::viewport::menu_edit(state, edit, tx));
            if !handled {
                let selector = match edit {
                    EditAction::Undo => sel!(undo:),
                    EditAction::Redo => sel!(redo:),
                    EditAction::Cut => sel!(cut:),
                    EditAction::Copy => sel!(copy:),
                    EditAction::Paste => sel!(paste:),
                    EditAction::SelectAll => sel!(selectAll:),
                };
                let mtm =
                    MainThreadMarker::new().expect("menu actions run on AppKit's main thread");
                let application = NSApplication::sharedApplication(mtm);
                // SAFETY: AppKit resolves a nil target through the active native responder chain.
                unsafe { application.sendAction_to_from(selector, None, None) };
            }
        }
    }
}
