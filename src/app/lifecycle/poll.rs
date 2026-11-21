// SPDX-License-Identifier: GPL-3.0-only

use rune::Tx;

use super::super::{APP_NAME, AppState};
use crate::storage;
use crate::tab::{Tab, TabId};

/// Rune's per-frame hook: forwards UI changes to the browser and shows what it reported.
pub(crate) fn poll(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    #[cfg(target_os = "macos")]
    let deactivated_drag = super::super::transfer::in_progress()
        && !crate::window_macos::application_active()
        && super::cancel_deactivated_drag(state, tx);
    #[cfg(not(target_os = "macos"))]
    let deactivated_drag = false;
    let incoming_drag = super::super::cross_window_drag::poll(state, tx);
    let transferred = state.advance_tab_transfer(tx);
    #[cfg(target_os = "macos")]
    let native_preview_changed = crate::drag_overlay_macos::poll(state, tx);
    #[cfg(not(target_os = "macos"))]
    let native_preview_changed = false;
    super::super::detach::poll(state, tx);
    let focused = tx.focused() == Some(crate::ui::viewport::page_focus_key(&state.browser));
    let focus_changed = state.browser.set_viewport_focus(focused);
    let settling = crate::ui::settle_sidebar(state, tx);
    let previous_peek = state.browser.peek.as_ref().map(|peek| peek.page.id());
    let mut polled = state.browser.poll();
    state.poll_tab_close(&polled, tx);
    crate::ui::form_picker::poll(state, &mut polled, tx);
    let peek_changed = update_peek(state, previous_peek, clock, tx);
    close_site_controls_after_navigation(state, &polled);

    let context_menu_changed = crate::ui::hover_highlight::poll_context_menu(state, tx);
    let switcher_changed = crate::ui::tab_switcher::reconcile(state, tx);
    state.sync_find_tab();
    update_find_selection(state, &polled);
    let tab_drag_changed = update_tab_drag(state, clock, tx);
    let configuration_changed = update_configuration(state, &polled);
    let shared_settings_changed = state.sync_shared_settings();
    let shared_permissions_changed = state.sync_shared_permissions();
    for (origin, kind, permission) in std::mem::take(&mut polled.permission_choices) {
        if state.permission_store.state(&origin, kind) == crate::permissions::PermissionState::Ask {
            state.save_permission_choice(origin, kind, permission);
        }
    }
    crate::ui::horizontal_tabs::reveal_active(state, tx);
    record_history_updates(state, &polled);
    refresh_find_after_navigation(state, &polled);
    state.persist_session_if_changed();
    let (history_page_changed, history_changed) = poll_history(state, &polled, tx);
    apply_page_updates(state, &mut polled, tx);
    let preview_changed = crate::ui::poll_tab_preview(state, clock, tx)
        || crate::ui::displayed_tab_preview(state)
            .is_some_and(|id| polled.thumbnails_ready.contains(&id));
    deactivated_drag
        || native_preview_changed
        || incoming_drag
        || transferred
        || polled.changed
        || peek_changed
        || preview_changed
        || history_changed
        || history_page_changed
        || settling
        || focus_changed
        || configuration_changed
        || shared_settings_changed
        || shared_permissions_changed
        || context_menu_changed
        || switcher_changed
        || tab_drag_changed
}

fn update_peek(state: &mut AppState, previous: Option<TabId>, clock: f64, tx: &mut Tx) -> bool {
    if state
        .browser
        .peek
        .as_ref()
        .is_some_and(|peek| Some(peek.page.id()) != previous)
    {
        state.peek_presentation.open(clock);
        state.page_menu = Default::default();
        crate::ui::dismiss_tab_preview(state);
        tx.focus_when_shown(crate::ui::peek::PAGE, false);
        tx.animated();
    }
    crate::ui::peek::advance(state, clock, tx)
}

fn close_site_controls_after_navigation(state: &mut AppState, polled: &crate::browser::Polled) {
    if state.site_permissions_open
        && let Some(active_tab) = state.browser.active_tab().map(Tab::id)
        && polled
            .committed
            .iter()
            .any(|(tab, _, _)| *tab == active_tab)
    {
        state.site_permissions_open = false;
    }
}

fn update_find_selection(state: &mut AppState, polled: &crate::browser::Polled) {
    for (tab, request_id, text) in &polled.find_selections {
        if state.find_open
            && state.find_tab == Some(*tab)
            && state.find_selection_request == *request_id
            && state.find_input.text.is_empty()
            && !text.is_empty()
        {
            state.replace_find_input(text.clone(), true);
            state.find_input_changed();
        }
    }
}

fn update_tab_drag(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if state.tab_drag.is_dragging() {
        match state.tab_drag.refresh_target(state.browser.tabs()) {
            crate::ui::TabDragMoveResult::PreviewChanged => {
                tx.animated();
                tx.motion(crate::ui::TAB_REORDER_MOTION);
                changed = true;
            }
            crate::ui::TabDragMoveResult::Cancelled => {
                state.split_preview = None;
                changed = true;
            }
            crate::ui::TabDragMoveResult::NoChange => {}
        }
        if let Some(y) = state.tab_drag.scroll_target(clock) {
            if state.horizontal_tabs() {
                tx.scroll_to_instant(crate::ui::horizontal_tabs::SCROLL, y, 0.0);
            } else {
                tx.scroll_to_instant("tabs-scroll", 0.0, y);
            }
            tx.wake_after(std::time::Duration::ZERO);
        }
    }
    if state.tab_drag.settle_tick(clock) {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        changed = true;
    } else if let Some(wait) = state.tab_drag.settle_wake() {
        tx.wake_after(wait);
    }
    changed
}

fn update_configuration(state: &mut AppState, polled: &crate::browser::Polled) -> bool {
    if let Some(error) = &polled.download_directory_error {
        state.configuration_issue = Some(error.clone());
    }
    let events = state
        .storage
        .as_ref()
        .map_or_else(Vec::new, |storage| storage.take_config_events());
    let mut changed = polled.download_directory_error.is_some();
    for event in events {
        match event {
            storage::ConfigEvent::Saved(Ok(())) => {}
            storage::ConfigEvent::Saved(Err(error)) => {
                state.configuration_issue = Some(error);
                changed = true;
            }
            storage::ConfigEvent::Reloaded(Ok(load)) => {
                state.apply_config_load(load);
                changed = true;
            }
            storage::ConfigEvent::Reloaded(Err(error)) => {
                state.configuration_issue = Some(error);
                changed = true;
            }
        }
    }
    changed
}

fn record_history_updates(state: &AppState, polled: &crate::browser::Polled) {
    if let Some(storage) = &state.storage {
        for (_, url, title) in &polled.committed {
            if let Err(error) = storage.history().record_visit(url, title) {
                eprintln!("Elaris: {error}");
            }
        }
        for (url, title) in &polled.titles {
            if let Err(error) = storage.history().update_title(url, title) {
                eprintln!("Elaris: {error}");
            }
        }
    }
}

fn refresh_find_after_navigation(state: &mut AppState, polled: &crate::browser::Polled) {
    if state.find_open
        && let Some(id) = state.find_tab
        && polled.committed.iter().any(|(tab, _, _)| *tab == id)
        && !state.find_input.text.is_empty()
    {
        let query = state.find_input.text.clone();
        let _ = state.browser.find(id, &query);
    }
}

fn poll_history(
    state: &mut AppState,
    polled: &crate::browser::Polled,
    tx: &mut Tx,
) -> (bool, bool) {
    let page_changed = state.poll_history_page(
        tx,
        !polled.committed.is_empty() || !polled.titles.is_empty(),
    );
    let history_changed = if let Some(storage) = &state.storage {
        let revision = storage.history().revision();
        let changed = state.history_revision != revision;
        state.history_revision = revision;
        changed
    } else {
        false
    };
    (page_changed, history_changed)
}

fn apply_page_updates(state: &mut AppState, polled: &mut crate::browser::Polled, tx: &mut Tx) {
    if let Some(text) = polled.clipboard.take() {
        tx.copy(text);
    }
    if let Some((id, menu)) = polled.menu.take()
        && !state.omnibox.open
    {
        state.page_menu.receive(id, menu);
    }
    // While the page animates, Rune keeps drawing at the display's rate; a frame's pixels alone
    // would only wake it, which can miss every other refresh.
    if polled.animating {
        tx.wake_after(std::time::Duration::ZERO);
    }
    if let Some(title) = state.new_window_title() {
        tx.set_title(if title.is_empty() {
            APP_NAME.to_owned()
        } else {
            title
        });
    }
}
