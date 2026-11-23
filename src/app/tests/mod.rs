// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn active_url(state: &AppState) -> &str {
    &state.browser.active_tab().unwrap().url
}

mod bookmarks;
mod customize;
mod find;
mod history;
mod horizontal_tabs;
mod memory_saver;
mod omnibox;
mod session;
mod settings;
mod sidebar;
mod startup;
mod tabs;
mod task_manager;
