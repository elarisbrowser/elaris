// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`crate::browser::Browser`].

mod construction;
mod history;
mod idle;
mod linux_frame;
mod registry;
mod shared;
mod state;
mod window_sessions;

// The test modules glob-import this module, so these names stay in scope for them.
#[cfg(test)]
use crate::storage::{self, Session, Storage};
#[cfg(test)]
use crate::tab::{Tab, TabId, TabSection};
#[cfg(test)]
use crate::ui::VIEWPORT;
#[cfg(test)]
use elaris_core::settings::StartupMode;
pub(crate) use history::HistoryConfirm;
#[cfg(elaris_ui = "linux")]
pub(crate) use linux_frame::{LinuxDecorations, LinuxTitlebar};
pub(crate) use mini::{MiniStart, is_mini_window, open_external_url, open_in_browser};
use registry::WINDOW_REGISTRY;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use registry::native_window;
pub(crate) use registry::{
    activate_window, is_private_window, open_window_count, private_window_count, register_window,
    reopen_or_create_window, unregister_window,
};
#[cfg(test)]
use rune::{Key, KeyEvent, Tx};
pub(crate) use shared::persist_download_history;
use shared::{publish_permissions, publish_settings};
pub(crate) use state::{APP_NAME, AppState};
use state::{RECENTLY_CLOSED_LIMIT, RecentlyClosedTab};
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use window_sessions::send_session_registry_snapshot;
use window_sessions::{
    SESSION_REGISTRY, remove_window_session, save_closed_final_session, store_window_session,
};

mod clear_data;
pub(crate) use clear_data::{ClearDataDialog, ClearRange};
mod bookmarks;
pub(crate) mod cross_window_drag;
pub(crate) mod detach;
mod find;
mod keyboard;
mod lifecycle;
mod memory_saver;
pub(crate) use memory_saver::freed as memory_saver_freed;
mod mini;
mod omnibox_actions;
mod search_engines;
mod session;
mod settings_actions;
mod tabs;
mod task_manager;
pub(crate) mod termination;
mod transfer;

#[cfg(test)]
mod tests;

pub(crate) use bookmarks::open_bookmarks;
pub(crate) use lifecycle::{
    new_tab, omnibox_input_key, on_reopen, on_window_focus, open_from_launcher, open_history,
    open_privacy_notice, open_settings, poll, search_bookmarks, setup_headless, shut_down,
    start_workers, submit_omnibox, window_closed,
};
pub(crate) use task_manager::open_task_manager;
