// SPDX-License-Identifier: GPL-3.0-only

use rune::Tx;

use super::super::{APP_NAME, AppState};
use crate::storage;
use crate::tab::{Tab, TabId};

/// Rune's per-frame hook: forwards UI changes to the browser and shows what it reported.
pub(crate) fn poll(state: &mut AppState, clock: f64, tx: &mut Tx) -> bool {
    if super::super::termination::take_request() {
        // A SIGTERM quits as the Quit command does, so the session is saved first.
        tx.quit();
    }
    #[cfg(target_os = "macos")]
    let deactivated_drag = super::super::transfer::in_progress()
        && !crate::platform::macos::window::application_active()
        && super::cancel_deactivated_drag(state, tx);
    #[cfg(not(target_os = "macos"))]
    let deactivated_drag = false;
    let incoming_drag = super::super::cross_window_drag::poll(state, tx);
    let transferred = state.advance_tab_transfer(tx);
    #[cfg(target_os = "macos")]
    let native_preview_changed = crate::platform::macos::drag_overlay::poll(state, tx);
    #[cfg(not(target_os = "macos"))]
    let native_preview_changed = false;
    super::super::detach::poll(state, tx);
    let focused = tx.focused() == Some(crate::ui::viewport::page_focus_key(&state.browser));
    let focus_changed = state.browser.set_viewport_focus(focused);
    let settling = crate::ui::settle_sidebar(state, tx);
    let link_copy_settled = crate::ui::settle_copy_link(state, clock);
    let site_action_settled = crate::ui::settle_site_action(state, clock);
    let zoom_chip_settled = crate::ui::settle_zoom_chip(state, clock);
    let toast_settled = state.toasts.settle(clock);
    if toast_settled {
        tx.animated();
    }
    let zoom_chip_shown = match state.browser.take_zoom_changed() {
        Some(id) => {
            state.zoom_chip = Some((id, clock + crate::ui::theme::ZOOM_CHIP_SECONDS));
            tx.wake_after(std::time::Duration::from_secs_f64(
                crate::ui::theme::ZOOM_CHIP_SECONDS,
            ));
            true
        }
        None => false,
    };
    // A poll commit mounts and unmounts at once unless asked: the chip's enter and exit need this.
    if zoom_chip_settled || zoom_chip_shown {
        tx.animated();
    }
    let previous_peek = state.browser.peek.as_ref().map(|peek| peek.page.id());
    let mut polled = state.browser.poll();
    let mini_changed = super::super::mini::poll(state, tx);
    open_popups(state, &mut polled);
    for (tab, path) in std::mem::take(&mut polled.screenshots) {
        state.browser.add_saved_file(tab, path);
        state.downloads_open = true;
    }
    state.poll_tab_close(&polled, tx);
    crate::ui::form_picker::poll(state, &mut polled, tx);
    let peek_changed = update_peek(state, previous_peek, clock, tx);
    close_site_controls_after_navigation(state, &polled);

    let context_menu_changed = crate::ui::components::menu::poll_highlight(state, tx);
    let switcher_changed = crate::ui::tab_switcher::reconcile(state, tx);
    state.sync_find_tab();
    update_find_selection(state, &polled);
    let tab_drag_changed = update_tab_drag(state, clock, tx);
    let configuration_changed = update_configuration(state, &polled);
    let shared_settings_changed = state.sync_shared_settings();
    let shared_permissions_changed = state.sync_shared_permissions();
    for (origin, kind, permission) in std::mem::take(&mut polled.permission_choices) {
        if state.permission_store.state(&origin, kind)
            == elaris_core::permissions::PermissionState::Ask
        {
            state.save_permission_choice(origin, kind, permission);
        }
    }
    let guards_changed = crate::ui::permission_prompt::poll(state, clock);
    if polled.downloads_changed
        && let Some(storage) = &state.storage
    {
        super::super::persist_download_history(storage, &state.browser.all_downloads());
    }
    let accessibility = state.browser.take_accessibility_changes();
    for tree in accessibility.removed {
        tx.remove_accessibility_subtree(tree);
    }
    for update in accessibility.updates {
        tx.update_accessibility_subtree(update);
    }
    crate::ui::horizontal_tabs::reveal_active(state, tx);
    record_history_updates(state, &polled);
    refresh_find_after_navigation(state, &polled);
    state.persist_session_if_changed();
    let (history_page_changed, history_changed) = poll_history(state, &polled, tx);
    state.note_tab_memory(&polled);
    let task_manager_changed = state.poll_task_manager(&mut polled, tx);
    state.poll_memory_saver(tx);
    let bookmarks_changed = state.poll_bookmarks() | state.poll_bookmark_details(&polled, tx);
    apply_page_updates(state, &mut polled, tx);
    let preview_changed = crate::ui::poll_tab_preview(state, clock, tx)
        || crate::ui::displayed_tab_preview(state)
            .is_some_and(|id| polled.thumbnails_ready.contains(&id));
    deactivated_drag
        || mini_changed
        || native_preview_changed
        || incoming_drag
        || transferred
        || polled.changed
        || guards_changed
        || peek_changed
        || preview_changed
        || history_changed
        || history_page_changed
        || task_manager_changed
        || bookmarks_changed
        || settling
        || link_copy_settled
        || site_action_settled
        || zoom_chip_settled
        || zoom_chip_shown
        || toast_settled
        || focus_changed
        || configuration_changed
        || shared_settings_changed
        || shared_permissions_changed
        || context_menu_changed
        || switcher_changed
        || tab_drag_changed
}

/// Gives each popup a mini window. Headless tooling opens no windows, so a popup is a tab there.
fn open_popups(state: &mut AppState, polled: &mut crate::browser::Polled) {
    for (source, request, activate) in std::mem::take(&mut polled.popups) {
        if rune::is_headless() {
            state.browser.insert_popup(source, request, activate);
            polled.changed = true;
        } else {
            crate::open_mini_window(super::super::MiniStart::Popup {
                source,
                request,
                private: state.browser.private,
            });
        }
    }
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
                state.apply_config_load(*load);
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
    if state.browser.private {
        return;
    }
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
        // The menu arrives from the engine rather than from input, so ask Rune to animate it.
        let received = state.page_menu.receive(id, menu);
        if received {
            tx.animated();
        }
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
