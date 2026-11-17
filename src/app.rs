// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown.

use rune::{TextEdit, Tx};

use crate::address::url_for_address;
use crate::appearance::Appearance;
use crate::engine::Engine;
use crate::page::Page;
use crate::ui::VIEWPORT;

/// The window's title when the page has none.
pub(crate) const APP_NAME: &str = "Elaris";

pub(crate) struct AppState {
    pub(crate) address: TextEdit,
    pub(crate) page: Page,
    pub(crate) appearance: Appearance,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    /// The state for a window opening `start`, or a blank page.
    fn new(start: Option<&str>) -> Self {
        let (address, url, error) = match start.map(url_for_address) {
            Some(Ok(url)) => (url.clone(), Some(url), None),
            Some(Err(error)) => (String::new(), None, Some(error.to_owned())),
            None => (String::new(), Some("about:blank".to_owned()), None),
        };
        let mut page = Page::new(url);
        page.error = error;
        Self {
            address: TextEdit::new(address),
            page,
            appearance: Appearance::default(),
        }
    }

    /// Loads what the address bar holds.
    pub(crate) fn navigate(&mut self) {
        match url_for_address(&self.address.text) {
            Ok(url) => {
                self.address.replace_text(url.clone());
                self.page.load(url);
            }
            Err(error) => self.page.error = Some(error.to_owned()),
        }
    }
}

/// Rune's one-time setup, before the first poll: starts the engine, which uses the waker to run
/// `poll` when it has news. Starting returns at once; Ladybird starts on the engine's thread.
pub(crate) fn setup(state: &mut AppState, waker: rune::Waker) {
    match Engine::start(state.page.pixels.clone(), waker) {
        Ok(engine) => state.page.connect(engine),
        Err(error) => state.page.error = Some(error),
    }
}

/// Rune's per-frame hook: forwards UI changes to the page engine and shows what it reported.
pub(crate) fn poll(state: &mut AppState, _clock: f64, tx: &mut Tx) -> bool {
    state
        .page
        .set_viewport_focus(tx.focused() == Some(VIEWPORT));
    let polled = state.page.poll();
    if let Some(url) = polled.url {
        state.address.replace_text(url);
    }
    // While the page animates, Rune keeps drawing at the display's rate; a frame's pixels alone
    // would only wake it, which can miss every other refresh.
    if polled.animating {
        tx.wake_after(std::time::Duration::ZERO);
    }
    if let Some(title) = polled.title {
        tx.set_title(if title.is_empty() {
            APP_NAME.to_owned()
        } else {
            title
        });
    }
    polled.changed
}

pub(crate) fn on_window_focus(state: &mut AppState, focused: bool, _: &mut Tx) {
    state.page.set_window_focus(focused);
}

pub(crate) fn shut_down(state: &mut AppState, _: &mut Tx) {
    state.page.shut_down();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_blank_page_without_an_address() {
        let state = AppState::new(None);
        assert_eq!(state.address.text, "");
        assert_eq!(state.page.error, None);
    }

    #[test]
    fn opens_the_address_it_was_given() {
        let state = AppState::new(Some("example.com"));
        assert_eq!(state.address.text, "https://example.com");
        assert_eq!(state.page.error, None);
    }

    #[test]
    fn reports_a_start_address_it_cannot_open() {
        let state = AppState::new(Some("not an address"));
        assert_eq!(state.address.text, "");
        assert!(state.page.error.is_some());
    }

    #[test]
    fn navigating_shows_the_full_url_or_says_what_is_wrong() {
        let mut state = AppState::new(None);
        state.address.replace_text("example.com/a".to_owned());
        state.navigate();
        assert_eq!(state.address.text, "https://example.com/a");
        assert_eq!(state.page.error, None);

        state.address.replace_text("two words".to_owned());
        state.navigate();
        assert_eq!(state.address.text, "two words");
        assert!(state.page.error.is_some());
    }
}
