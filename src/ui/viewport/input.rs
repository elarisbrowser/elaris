// SPDX-License-Identifier: GPL-3.0-only

//! Keyboard, IME and clipboard input for the page, and the native edit menu actions.

use super::panes::page_focus_key;
use crate::app::AppState;
use crate::engine::ViewCommand;
use crate::tab::TabId;
use elaris::ladybird::{Edit, MouseKind};
use rune::{Ime, Key, KeyEvent, Phase, Tx};

pub(in crate::ui) fn mouse_kind(phase: Phase, cancelled: bool) -> MouseKind {
    match phase {
        Phase::Down => MouseKind::Down,
        Phase::Move => MouseKind::Move,
        Phase::Up if cancelled => MouseKind::Cancel,
        Phase::Up => MouseKind::Up,
    }
}

pub(super) fn paste(id: TabId, tx: &mut Tx) {
    tx.read_clipboard(move |s: &mut AppState, content, tx| {
        if !s.omnibox.open
            && tx.focused() == Some(page_focus_key(&s.browser))
            && s.browser.input_page() == Some(id)
            && !content.text().is_empty()
        {
            s.browser.send(
                id,
                ViewCommand::Edit(Edit::Paste(content.text().to_owned())),
            );
        }
    });
}

#[cfg(target_os = "macos")]
pub(crate) enum PageEdit {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Applies a native edit menu action to the focused Ladybird page, if it owns focus.
#[cfg(target_os = "macos")]
pub(crate) fn menu_edit(state: &mut AppState, edit: PageEdit, tx: &mut Tx) -> bool {
    let Some(id) = state.browser.input_page().filter(|id| {
        tx.focused() == Some(page_focus_key(&state.browser))
            && !state.omnibox.open
            && state
                .browser
                .page(*id)
                .is_some_and(|page| !page.is_internal())
    }) else {
        return false;
    };
    match edit {
        PageEdit::Cut => state.browser.send(id, ViewCommand::Edit(Edit::Cut)),
        PageEdit::Copy => state.browser.send(id, ViewCommand::Edit(Edit::Copy)),
        PageEdit::Paste => paste(id, tx),
        PageEdit::SelectAll => state.browser.send(id, ViewCommand::Edit(Edit::SelectAll)),
    }
    true
}

pub(in crate::ui) fn page_key(state: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if key.key == Key::Escape && state.browser.exit_web_fullscreen() {
        return true;
    }
    if key.key == Key::Escape && crate::ui::peek::dismiss(state, tx) {
        return true;
    }
    if key.key == Key::Escape && (state.tab_drag.is_dragging() || state.has_split_picker()) {
        return crate::shortcuts::on_key(state, key, tx);
    }
    if crate::ui::tab_switcher::cycle_key(state, key, tx) {
        return true;
    }
    let Some(id) = state.browser.input_page().filter(|id| {
        state
            .browser
            .page(*id)
            .is_some_and(|page| !page.is_internal())
    }) else {
        return false;
    };
    if state.omnibox.open || state.page_menu.open() {
        return false;
    }
    if key.mods.cmd
        && !key.mods.shift
        && !key.mods.alt
        && let Key::Command(c) = key.key
    {
        let edit = match c {
            'a' => Edit::SelectAll,
            'x' => Edit::Cut,
            'c' => Edit::Copy,
            'v' => {
                paste(id, tx);
                return true;
            }
            _ => return false,
        };
        state.browser.send(id, ViewCommand::Edit(edit));
        return true;
    }
    if let Key::Text(s) = &key.key
        && s.chars().count() > 1
    {
        state
            .browser
            .send(id, ViewCommand::Edit(Edit::Commit(s.clone())));
        return true;
    }
    state
        .browser
        .key(key, true, tx.key_repeat(), tx.physical_key())
}

pub(crate) fn ime(state: &mut AppState, event: &Ime, tx: &mut Tx) -> bool {
    if tx.focused() != Some(page_focus_key(&state.browser))
        || state.omnibox.open
        || state.page_menu.open()
    {
        return false;
    }
    let Some(tab) = state
        .browser
        .input_page()
        .and_then(|id| state.browser.page(id))
        .filter(|tab| tab.ime_caret.is_some())
    else {
        return false;
    };
    let edit = match event {
        Ime::Preedit(text, ..) => Edit::Mark(text.clone()),
        Ime::Commit(text) => Edit::Commit(text.clone()),
        Ime::Disabled => Edit::Mark(String::new()),
        _ => return false,
    };
    state.browser.send(tab.id(), ViewCommand::Edit(edit));
    true
}
