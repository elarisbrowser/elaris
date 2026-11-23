// SPDX-License-Identifier: GPL-3.0-only

//! Mini windows: compact single-page windows for links other apps open and for page popups. They
//! have no tab strip, address bar or saved session, and close with their last page.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use rune::Tx;

use super::AppState;
use crate::tab::TabId;
use crate::ui::viewport::focus_key;

/// What a mini window opens once it exists.
pub(crate) enum MiniStart {
    /// A web address another app asked to open.
    Url(String),
    /// A popup a page opened. It waits for the engine, then takes over the pending view.
    Popup {
        source: TabId,
        request: u64,
        private: bool,
    },
}

static MINI_WINDOWS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

fn registry() -> std::sync::MutexGuard<'static, HashSet<u64>> {
    MINI_WINDOWS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn is_mini_window(id: u64) -> bool {
    MINI_WINDOWS.get().is_some_and(|windows| {
        windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&id)
    })
}

pub(super) fn forget(id: u64) {
    if let Some(windows) = MINI_WINDOWS.get() {
        windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }
}

impl AppState {
    pub(crate) fn new_mini_window(start: MiniStart) -> Self {
        let mut state = Self::build(None, false);
        state.omnibox.close();
        state.mini = true;
        // The page sits under the header, as with a hidden sidebar; nothing can show it again.
        state.sidebar.collapsed = true;
        if matches!(start, MiniStart::Popup { private: true, .. }) {
            state.make_private();
        }
        registry().insert(state.window_id);
        state.mini_start = Some(start);
        state
    }

    pub(crate) fn is_mini(&self) -> bool {
        self.mini
    }
}

/// Opens the pending page once possible, and closes the window when its last page goes.
pub(super) fn poll(state: &mut AppState, tx: &mut Tx) -> bool {
    if !state.mini {
        return false;
    }
    match state.mini_start.take() {
        Some(MiniStart::Url(url)) => {
            if state.browser.open_tab(&url).is_ok() {
                tx.focus(focus_key(&state.browser), false);
            } else {
                tx.close_window();
            }
            true
        }
        Some(MiniStart::Popup {
            source, request, ..
        }) if state.browser.engine_ready() => {
            state.browser.insert_popup(source, request, true);
            tx.focus(focus_key(&state.browser), false);
            true
        }
        pending @ Some(_) => {
            state.mini_start = pending;
            false
        }
        None => {
            if state.browser.tabs().is_empty() {
                tx.close_window();
            }
            false
        }
    }
}

/// Rune's hook for a link another app opened with Elaris.
pub(crate) fn open_external_url(_: &mut AppState, url: &str, _: &mut Tx) {
    if let Some(url) = web_address(url) {
        crate::open_mini_window(MiniStart::Url(url));
    }
}

/// Moves the mini window's page into a full browser window.
pub(crate) fn open_in_browser(state: &mut AppState, tx: &mut Tx) {
    let Some(url) = state
        .browser
        .active_tab()
        .map(|tab| tab.url.clone())
        .filter(|url| url != "about:blank")
    else {
        return;
    };
    crate::open_browser_window_with_url(url);
    tx.close_window();
}

/// The link as an http or https address, or None for anything else. It comes from outside the
/// app, so nothing but a plain web address gets through.
fn web_address(link: &str) -> Option<String> {
    let link = link.trim();
    let (scheme, rest) = link.split_once("://")?;
    let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    (web && !host.is_empty() && !link.chars().any(|c| c.is_control() || c.is_whitespace()))
        .then(|| link.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{MiniStart, web_address};
    use crate::app::{AppState, is_mini_window};
    use crate::omnibox::Mode;

    #[test]
    fn mini_window_has_no_sidebar_address_bar_or_saved_session() {
        let mut mini = AppState::new_mini_window(MiniStart::Url("https://example.com/".into()));
        assert!(mini.is_mini() && mini.sidebar.collapsed && !mini.omnibox.open);
        assert!(is_mini_window(mini.window_id));
        mini.open_omnibox(Mode::New);
        assert!(!mini.omnibox.open);
        assert!(!AppState::new_window().is_mini());
        crate::app::unregister_window(mini.window_id);
        assert!(!is_mini_window(mini.window_id));
    }

    #[test]
    fn external_links_must_be_plain_web_addresses() {
        assert_eq!(
            web_address(" https://example.com/a?b#c "),
            Some("https://example.com/a?b#c".into())
        );
        assert!(web_address("HTTP://example.com").is_some());
        for rejected in [
            "file:///etc/passwd",
            "elaris://settings",
            "javascript:alert(1)",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "https://exa\0mple.com",
            "example.com",
        ] {
            assert_eq!(web_address(rejected), None, "{rejected}");
        }
    }
}
