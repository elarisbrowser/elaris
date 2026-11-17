// SPDX-License-Identifier: GPL-3.0-only

use rune::{TextEdit, Tx};

use crate::address::url_for_address;
use crate::page::Page;
use crate::ui::VIEWPORT;

pub(crate) struct AppState {
    pub(crate) address: TextEdit,
    pub(crate) page: Page,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        let start = rune::extra_args().first().map(|a| url_for_address(a));
        let (address, url, error) = match start {
            Some(Ok(url)) => (url.clone(), Some(url), None),
            Some(Err(error)) => (String::new(), None, Some(error.to_owned())),
            None => (String::new(), Some("about:blank".to_owned()), None),
        };
        let mut page = Page::new(url);
        page.error = error;
        Self {
            address: TextEdit::new(address),
            page,
        }
    }
}

impl AppState {
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

/// Rune's one-time setup: the waker lets the engine thread run `poll` when it has news.
pub(crate) fn setup(state: &mut AppState, waker: rune::Waker) {
    state.page.set_notify(waker);
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
            "Elaris".to_owned()
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
