// SPDX-License-Identifier: GPL-3.0-only

//! Tracks which window owns the menu and keeps item titles and availability in step with it.

use std::cell::Cell;

use objc2_05::msg_send;
use objc2_app_kit_02::{NSApplication, NSWindowStyleMask};
use objc2_foundation_02::{MainThreadMarker, NSString};

use super::items::{Action, action_from_id};
use super::state::{MenuCommand, MenuState};
use super::{ACTIVE_WINDOW, APP_HANDLES, FULL_SCREEN_ITEM, MENU_ITEMS, MENU_STATE, QUIT_PENDING};
use crate::app::AppState;
use crate::shortcuts::Shortcut;

pub(crate) fn register_window(id: u64, app: rune::AppHandle<AppState>) {
    APP_HANDLES.with(|handles| {
        handles.borrow_mut().insert(id, app);
    });
}

pub(crate) fn set_active_window(id: u64, active: bool) {
    if active {
        ACTIVE_WINDOW.with(|slot| slot.set(Some(id)));
    } else {
        ACTIVE_WINDOW.with(|slot| {
            if slot.get() == Some(id) {
                slot.set(None);
            }
        });
    }
}

pub(crate) fn unregister_window(id: u64) {
    APP_HANDLES.with(|handles| {
        handles.borrow_mut().remove(&id);
    });
    QUIT_PENDING.with(|pending| {
        if let Some(pending) = pending.borrow_mut().as_mut() {
            pending.remove(&id);
        }
    });
    set_active_window(id, false);
}

/// Refreshes native item availability and labels after Rune builds the current browser state.
#[cfg_attr(elaris_ui = "linux", allow(dead_code))]
pub(crate) fn refresh(state: &AppState) {
    if ACTIVE_WINDOW.with(Cell::get) != Some(state.window_id) {
        return;
    }
    let snapshot = MenuState::from(state);
    // Unchanged state needs no AppKit calls: every frame would otherwise rewrite each item.
    // The menu reapplies its state when it opens, which also covers the full-screen title.
    let previous = MENU_STATE.with(|slot| slot.borrow_mut().replace(snapshot));
    if previous == Some(snapshot) {
        return;
    }
    if let Some(mtm) = MainThreadMarker::new() {
        apply_menu_state(snapshot, mtm);
    }
}

pub(super) fn apply_menu_state(snapshot: MenuState, mtm: MainThreadMarker) {
    MENU_ITEMS.with(|slot| {
        for item in slot.borrow().iter() {
            let tag = unsafe { msg_send![&**item, tag] };
            let enabled = action_from_id(tag)
                .and_then(menu_command)
                .is_some_and(|command| snapshot.enabled(command));
            unsafe { item.setEnabled(enabled) };
            match action_from_id(tag) {
                Some(Action::Favourite) => unsafe {
                    item.setTitle(&NSString::from_str(if snapshot.favourite {
                        "Remove from Favourites"
                    } else {
                        "Move Tab to Favourites"
                    }))
                },
                Some(Action::Shortcut(Shortcut::ToggleSidebar)) => unsafe {
                    item.setState(if snapshot.sidebar_visible { 1 } else { 0 })
                },
                _ => {}
            }
        }
    });
    let full_screen = NSApplication::sharedApplication(mtm)
        .keyWindow()
        .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen));
    FULL_SCREEN_ITEM.with(|slot| {
        if let Some(item) = slot.borrow().as_ref() {
            unsafe {
                item.setTitle(&NSString::from_str(if full_screen {
                    "Exit Full Screen"
                } else {
                    "Enter Full Screen"
                }))
            };
        }
    });
}

fn menu_command(action: Action) -> Option<MenuCommand> {
    Some(match action {
        Action::Back => MenuCommand::Back,
        Action::Forward => MenuCommand::Forward,
        Action::Favourite => MenuCommand::Favourite,
        Action::SplitLeft | Action::SplitRight => MenuCommand::Split,
        Action::ExitSplit => MenuCommand::ExitSplit,
        Action::Edit(_) => MenuCommand::Edit,
        Action::Shortcut(Shortcut::NewTab) => MenuCommand::NewTab,
        Action::Shortcut(Shortcut::NewWindow | Shortcut::NewPrivateWindow) => {
            MenuCommand::NewWindow
        }
        Action::Shortcut(Shortcut::Close) => MenuCommand::CloseTab,
        Action::Shortcut(Shortcut::ReopenClosedTab) => MenuCommand::ReopenClosedTab,
        Action::Shortcut(Shortcut::Find) => MenuCommand::Find,
        Action::Shortcut(Shortcut::Reload) => MenuCommand::Reload,
        Action::Shortcut(
            Shortcut::ReloadBypassingCache
            | Shortcut::ZoomIn
            | Shortcut::ZoomOut
            | Shortcut::ResetZoom,
        ) => MenuCommand::WebPage,
        Action::Shortcut(Shortcut::ToggleSidebar) => MenuCommand::ToggleSidebar,
        Action::Shortcut(Shortcut::OpenHistory) => MenuCommand::History,
        Action::Shortcut(Shortcut::NextTab) => MenuCommand::NextTab,
        Action::Shortcut(Shortcut::PreviousTab) => MenuCommand::PreviousTab,
        Action::Shortcut(Shortcut::OpenSettings | Shortcut::OpenTaskManager) => {
            MenuCommand::Settings
        }
        _ => return None,
    })
}
