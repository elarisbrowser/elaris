// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn active_url(state: &AppState) -> &str {
    &state.browser.active_tab().unwrap().url
}

mod find;
mod history;
mod omnibox;
mod session;
mod settings;
mod sidebar;
mod startup;
mod tabs;
