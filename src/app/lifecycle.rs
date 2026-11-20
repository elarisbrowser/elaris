// SPDX-License-Identifier: GPL-3.0-only

use rune::{Key, KeyEvent, Tx};

use super::AppState;
mod poll;
#[cfg(feature = "ladybird-bridge")]
use crate::engine::Engine;
use crate::internal::InternalPage;
use crate::omnibox;
use crate::ui::viewport::focus_key;
pub(crate) use poll::poll;

/// Connect worker notifications once Rune can schedule a poll.
pub(crate) fn start_workers(state: &mut AppState, waker: rune::Waker) {
    crate::engine::register_window_waker(state.window_id, waker.clone());
    // Rune's real-time replays call both the headless setup and the native ready hook.
    if std::mem::replace(&mut state.workers_started, true) {
        return;
    }
    if let Some(storage) = &state.storage {
        storage.set_notify(std::sync::Arc::new(crate::engine::wake_all_windows));
    }
    #[cfg(feature = "ladybird-bridge")]
    start_engine(state);
}

/// Ladybird starts on its own thread after Rune's first frame.
#[cfg(feature = "ladybird-bridge")]
fn start_engine(state: &mut AppState) {
    let Some(profile_dir) = state.profile_dir.clone() else {
        state.browser.fail(
            state
                .storage_error
                .clone()
                .unwrap_or_else(|| "Cannot locate the browser profile directory".to_owned()),
        );
        return;
    };
    match Engine::start_shared(
        std::sync::Arc::new(crate::engine::wake_all_windows),
        profile_dir,
        state.engine_download_directory(),
        state.display_refresh_rate,
    ) {
        Ok(engine) => state.browser.connect(engine),
        Err(error) => state.browser.fail(error),
    }
}

/// Headless tooling has no native `on_ready`, so connect workers during Rune's setup callback.
pub(crate) fn setup_headless(state: &mut AppState, waker: rune::Waker) {
    if rune::is_headless() {
        start_workers(state, waker);
    }
}

/// A query was submitted from the focused omnibox.
pub(crate) fn submit_omnibox(state: &mut AppState, tx: &mut Tx) {
    if !state.omnibox.open
        || (state.omnibox.input.text.trim().is_empty() && !state.omnibox.selection_moved)
    {
        return;
    }
    let suggestions = state.suggestions();
    if let Some(suggestion) = suggestions.get(state.omnibox.selected).cloned() {
        state.choose_suggestion(suggestion, tx);
    } else {
        state.submit_omnibox();
        if state.address_error.is_none() {
            state.omnibox.close();
            let key = if state.has_split_picker() {
                crate::ui::viewport::PICKER
            } else {
                focus_key(&state.browser)
            };
            tx.focus(key, false);
        }
    }
}

/// The sidebar launcher edits the current address.
pub(crate) fn open_from_launcher(state: &mut AppState, tx: &mut Tx) {
    state.open_omnibox(omnibox::Mode::Current);
    tx.focus("omnibox-input", false);
}

/// A new tab starts in the omnibox, ready to type.
pub(crate) fn new_tab(state: &mut AppState, tx: &mut Tx) {
    if state.cancel_tab_drag() {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
    state.open_omnibox(omnibox::Mode::New);
    tx.focus("omnibox-input", false);
}

/// Opens the existing settings tab, or creates it once, and returns focus to the native page.
pub(crate) fn open_settings(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    state.address_error = None;
    let tab = state.settings_tab();
    state.activate(tab);
    tx.focus(focus_key(&state.browser), false);
}

pub(crate) fn open_history(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    let tab = state
        .active_history_tab()
        .or_else(|| state.browser.internal_tab(InternalPage::History))
        .unwrap_or_else(|| {
            state
                .browser
                .open_tab(InternalPage::History.address())
                .expect("valid built-in History route")
        });
    state.activate(tab);
    tx.focus(crate::ui::HISTORY_SEARCH, false);
}

/// Keyboard input the omnibox uses before the text editor handles it.
pub(crate) fn omnibox_input_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if crate::ui::tab_switcher::cycle_key(state, event, tx) {
        return true;
    }
    match &event.key {
        Key::Escape => {
            state.omnibox.close();
            tx.focus(focus_key(&state.browser), false);
            true
        }
        Key::Up | Key::Down => {
            let count = state.suggestions().len();
            let direction = if event.key == Key::Up { -1 } else { 1 };
            state.omnibox.move_selection(count, direction);
            if count > 0 {
                tx.scroll_into_view(format!("omnibox-result-{}", state.omnibox.selected));
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn on_window_focus(state: &mut AppState, focused: bool, tx: &mut Tx) {
    if focused {
        super::activate_window(state.window_id);
        #[cfg(target_os = "macos")]
        crate::site_popover_macos::activate_window(state.window_id);
    }
    #[cfg(target_os = "macos")]
    crate::menu_macos::set_active_window(state.window_id, focused);
    state.browser.set_window_focus(focused);
    if !focused {
        crate::ui::dismiss_tab_preview(state);
        crate::ui::tab_switcher::finish(state, None, tx);
    }
    if !focused && state.cancel_tab_drag() {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
}

pub(crate) fn shut_down(state: &mut AppState, _: &mut Tx) {
    state.cancel_tab_transfer();
    super::unregister_window(state.window_id);
    crate::engine::unregister_window_waker(state.window_id);
    if let Some(storage) = &state.storage {
        storage.history().forget_window(state.window_id);
    }
    #[cfg(target_os = "macos")]
    crate::menu_macos::unregister_window(state.window_id);
    state.persist_session();
    state.browser.shut_down();
    if let Some(storage) = &state.storage
        && let Err(error) = storage.flush_on_close()
    {
        eprintln!("Elaris: {error}");
    }
}

/// Reopen an existing queued window, or create a fresh one after the final window was closed.
pub(crate) fn on_reopen(_: &mut AppState, has_visible_windows: bool, _: &mut Tx) -> bool {
    if has_visible_windows {
        return false;
    }
    // A deliberately closed final macOS window was removed from Elaris's registries. Create a
    // fresh Rune window on Dock activation; Rune then skips reopening its retained closed host.
    super::reopen_or_create_window();
    true
}

/// Saves the browser before Rune hides the last window but keeps the app available to reopen.
pub(crate) fn window_closed(state: &mut AppState, _: &mut Tx) {
    if rune::quitting() {
        return;
    }
    let others_remain = super::open_window_count() > 1;
    state.cancel_tab_transfer();
    super::unregister_window(state.window_id);
    #[cfg(target_os = "macos")]
    crate::site_popover_macos::detach_window(state.window_id);
    crate::engine::unregister_window_waker(state.window_id);
    if let Some(storage) = &state.storage {
        storage.history().forget_window(state.window_id);
    }
    #[cfg(target_os = "macos")]
    crate::menu_macos::unregister_window(state.window_id);
    state.persist_session();
    state.browser.close_window_views();
    if let Some(storage) = &state.storage {
        if others_remain {
            super::remove_window_session(state.window_id, storage);
        } else {
            super::save_closed_final_session(storage);
        }
    }
    if let Some(storage) = &state.storage
        && let Err(error) = storage.flush_on_close()
    {
        eprintln!("Elaris: {error}");
    }
}
