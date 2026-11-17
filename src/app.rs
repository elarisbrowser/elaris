// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`Browser`].

use rune::{TextEdit, Tx};

use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::engine::Engine;
use crate::tab::TabId;
use crate::ui::{ADDRESS, VIEWPORT};

/// The window's title when the page has none.
pub(crate) const APP_NAME: &str = "Elaris";

/// What a new tab opens.
const NEW_TAB_ADDRESS: &str = "about:blank";

pub(crate) struct AppState {
    pub(crate) address: TextEdit,
    /// Why the address last submitted could not be opened, until the next is.
    pub(crate) address_error: Option<String>,
    pub(crate) browser: Browser,
    pub(crate) appearance: Appearance,
    /// The sidebar's width in points, which dragging its edge changes.
    pub(crate) sidebar_width: f32,
    /// The tab and address the address bar shows, so it changes only when the active tab's do.
    shown: Option<(TabId, String)>,
    /// The page title the window shows.
    window_title: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    /// The state for a window with one tab opening `start`, or a blank page.
    fn new(start: Option<&str>) -> Self {
        let mut state = Self {
            address: TextEdit::new(String::new()),
            address_error: None,
            browser: Browser::default(),
            appearance: Appearance::default(),
            sidebar_width: crate::ui::SIDEBAR_WIDTH,
            shown: None,
            window_title: None,
        };
        let opened = start.map(|start| state.browser.open_tab(start));
        state.ensure_a_tab();
        state.show_active_tab();
        if let Some(Err(error)) = opened {
            state.address_error = Some(error.to_string());
        }
        state
    }

    /// Loads what the address bar holds in the active tab.
    pub(crate) fn submit_address(&mut self) {
        let address = self.address.text.clone();
        let result = match self.browser.active_tab().map(|tab| tab.id()) {
            Some(tab) => self.browser.navigate(tab, &address),
            None => self.browser.open_tab(&address).map(drop),
        };
        self.address_error = result.err().map(|error| error.to_string());
        self.show_active_tab();
    }

    /// Opens a blank tab and makes it active.
    pub(crate) fn new_tab(&mut self) {
        if let Ok(tab) = self.browser.open_tab(NEW_TAB_ADDRESS) {
            self.activate(tab);
        }
    }

    pub(crate) fn activate(&mut self, tab: TabId) {
        self.browser.activate_tab(tab);
        self.address_error = None;
        self.show_active_tab();
    }

    /// Closes the tab; closing the last one leaves a blank tab in its place.
    pub(crate) fn close(&mut self, tab: TabId) {
        self.browser.close_tab(tab);
        self.ensure_a_tab();
        self.show_active_tab();
    }

    /// Runs `operation` on the active tab, if there is one.
    pub(crate) fn with_active_tab(&mut self, operation: impl FnOnce(&mut Browser, TabId)) {
        if let Some(tab) = self.browser.active_tab().map(|tab| tab.id()) {
            operation(&mut self.browser, tab);
        }
    }

    /// Why the address or the active page failed, or that the page is loading.
    pub(crate) fn status(&self) -> Option<String> {
        let tab = self.browser.active_tab();
        let error = self
            .address_error
            .as_ref()
            .or_else(|| tab.and_then(|tab| tab.error.as_ref()));
        match error {
            Some(error) => Some(error.clone()),
            None => tab
                .is_some_and(|tab| tab.loading)
                .then(|| "Loading…".to_owned()),
        }
    }

    fn ensure_a_tab(&mut self) {
        if self.browser.tabs().is_empty() {
            self.new_tab();
        }
    }

    /// Shows the active tab's address in the address bar once it, or its address, changes.
    fn show_active_tab(&mut self) {
        let Some(tab) = self.browser.active_tab() else {
            return;
        };
        let now = (tab.id(), tab.url.clone());
        if self.shown.as_ref() != Some(&now) {
            let url = if now.1 == NEW_TAB_ADDRESS { "" } else { &now.1 };
            self.address.replace_text(url.to_owned());
            self.shown = Some(now);
        }
    }

    /// The active tab's title, when it differs from what the window shows.
    fn new_window_title(&mut self) -> Option<String> {
        let title = self.browser.active_tab().map(|tab| tab.title.clone())?;
        (self.window_title.as_ref() != Some(&title)).then(|| {
            self.window_title = Some(title.clone());
            title
        })
    }
}

/// Rune's one-time setup, before the first poll: starts the engine, which uses the waker to run
/// `poll` when it has news. Starting returns at once; Ladybird starts on the engine's thread.
pub(crate) fn setup(state: &mut AppState, waker: rune::Waker) {
    match Engine::start(waker) {
        Ok(engine) => state.browser.connect(engine),
        Err(error) => state.browser.fail(error),
    }
}

/// Rune's per-frame hook: forwards UI changes to the browser and shows what it reported.
pub(crate) fn poll(state: &mut AppState, _clock: f64, tx: &mut Tx) -> bool {
    state
        .browser
        .set_viewport_focus(tx.focused() == Some(VIEWPORT));
    let polled = state.browser.poll();
    // While the page animates, Rune keeps drawing at the display's rate; a frame's pixels alone
    // would only wake it, which can miss every other refresh.
    if polled.animating {
        tx.wake_after(std::time::Duration::ZERO);
    }
    state.show_active_tab();
    if let Some(title) = state.new_window_title() {
        tx.set_title(if title.is_empty() {
            APP_NAME.to_owned()
        } else {
            title
        });
    }
    polled.changed
}

/// After the address bar opens a page, the page takes the keyboard.
pub(crate) fn submit_address(state: &mut AppState, tx: &mut Tx) {
    state.submit_address();
    if state.address_error.is_none() {
        tx.focus(VIEWPORT, false);
    }
}

/// A new tab starts in the address bar, ready to type.
pub(crate) fn new_tab(state: &mut AppState, tx: &mut Tx) {
    state.new_tab();
    tx.focus(ADDRESS, false);
}

pub(crate) fn on_window_focus(state: &mut AppState, focused: bool, _: &mut Tx) {
    state.browser.set_window_focus(focused);
}

pub(crate) fn shut_down(state: &mut AppState, _: &mut Tx) {
    state.browser.shut_down();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_url(state: &AppState) -> &str {
        &state.browser.active_tab().unwrap().url
    }

    #[test]
    fn opens_one_blank_tab_without_an_address() {
        let state = AppState::new(None);
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(active_url(&state), "about:blank");
        assert_eq!(state.address.text, "");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn opens_the_address_it_was_given() {
        let state = AppState::new(Some("example.com"));
        assert_eq!(state.address.text, "https://example.com");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn reports_a_start_address_it_cannot_open() {
        let state = AppState::new(Some("not an address"));
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(state.address.text, "");
        assert!(state.address_error.is_some());
    }

    #[test]
    fn submitting_shows_the_full_url_or_says_what_is_wrong() {
        let mut state = AppState::new(None);
        state.address.replace_text("example.com/a".to_owned());
        state.submit_address();
        assert_eq!(state.address.text, "https://example.com/a");
        assert_eq!(state.address_error, None);

        state.address.replace_text("two words".to_owned());
        state.submit_address();
        assert_eq!(state.address.text, "two words");
        assert!(state.address_error.is_some());
        assert_eq!(active_url(&state), "https://example.com/a");
    }

    #[test]
    fn the_address_bar_follows_the_active_tab() {
        let mut state = AppState::new(Some("example.com"));
        let first = state.browser.active_tab().unwrap().id();
        state.new_tab();
        assert_eq!(state.address.text, "");
        state.address.replace_text("example.org".to_owned());
        state.submit_address();

        state.activate(first);
        assert_eq!(state.address.text, "https://example.com");
        assert_eq!(state.browser.tabs().len(), 2);
    }

    #[test]
    fn the_status_says_what_went_wrong() {
        let mut state = AppState::new(None);
        assert_eq!(state.status(), None);
        state.address.replace_text("two words".to_owned());
        state.submit_address();
        assert!(state.status().is_some());
        state.new_tab();
        assert_eq!(state.status(), None);
    }

    #[test]
    fn closing_the_last_tab_leaves_a_blank_one() {
        let mut state = AppState::new(Some("example.com"));
        let only = state.browser.active_tab().unwrap().id();
        state.close(only);
        assert_eq!(state.browser.tabs().len(), 1);
        assert_ne!(state.browser.active_tab().unwrap().id(), only);
        assert_eq!(state.address.text, "");
    }
}
