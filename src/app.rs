// SPDX-License-Identifier: GPL-3.0-only

//! Application state and the hooks Rune calls: starting the engine, each frame's poll, window
//! focus and shutdown. What the interface does with tabs goes through the [`Browser`].

use rune::{Key, KeyEvent, PointerPress, Tx};

use crate::address;
use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::engine::Engine;
use crate::omnibox::{self, Action, HistoryEntry, Suggestion};
use crate::tab::{Tab, TabId};
use crate::ui::{SidebarState, VIEWPORT};

/// The window's title when the page has none.
pub(crate) const APP_NAME: &str = "Elaris";

pub(crate) struct AppState {
    pub(crate) omnibox: omnibox::State,
    /// Why the last submitted query could not be opened, until the next is.
    pub(crate) address_error: Option<String>,
    pub(crate) browser: Browser,
    pub(crate) appearance: Appearance,
    pub(crate) sidebar: SidebarState,
    /// The tab and URL last recorded in local recent history.
    shown: Option<(TabId, String)>,
    history: Vec<HistoryEntry>,
    /// The page title the window shows.
    window_title: Option<String>,
    context_menu: Option<(Option<TabId>, PointerPress)>,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    /// Opens the supplied page, or shows the omnibox in an empty window.
    fn new(start: Option<&str>) -> Self {
        let mut state = Self {
            omnibox: omnibox::State::default(),
            address_error: None,
            browser: Browser::default(),
            appearance: Appearance::default(),
            sidebar: SidebarState::default(),
            shown: None,
            history: Vec::new(),
            window_title: None,
            context_menu: None,
        };
        if let Some(start) = start {
            match state.browser.open_tab(start) {
                Ok(_) => state.show_active_tab(),
                Err(error) => {
                    state.open_omnibox(omnibox::Mode::New);
                    state.address_error = Some(error.to_string());
                }
            }
        } else {
            state.open_omnibox(omnibox::Mode::New);
        }
        state
    }

    /// Loads what the omnibox holds in the active tab.
    pub(crate) fn submit_omnibox(&mut self) {
        let address = self.omnibox.input.text.clone();
        let result = self.navigate_destination(&address);
        self.address_error = result.err().map(|error| error.to_string());
        self.show_active_tab();
    }

    pub(crate) fn open_omnibox(&mut self, mode: omnibox::Mode) {
        self.omnibox.mode = mode;
        let use_active_url = mode == omnibox::Mode::Current;
        let query = if use_active_url {
            self.browser
                .active_tab()
                .map(|tab| tab.url.clone())
                .filter(|url| url != "about:blank")
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.omnibox
            .open(&query, use_active_url && !query.is_empty());
        self.address_error = None;
    }

    pub(crate) fn suggestions(&self) -> Vec<Suggestion> {
        omnibox::suggestions(
            &self.omnibox.input.text,
            self.browser.tabs(),
            &self.history,
            self.sidebar.collapsed,
        )
    }

    pub(crate) fn choose_suggestion(&mut self, suggestion: Suggestion, tx: &mut Tx) {
        let result = match suggestion.action {
            Action::Navigate(url) => self.navigate_destination(&url),
            Action::Search(query) => self.navigate_destination(&address::search_url(&query)),
            Action::VisitRecent(url) => self.navigate_destination(&url),
            Action::SwitchTab(tab) => {
                self.activate(tab);
                Ok(())
            }
            Action::NewTab => {
                new_tab(self, tx);
                return;
            }
            Action::ReloadPage => {
                self.with_active_tab(Browser::reload);
                Ok(())
            }
            Action::CloseTab => {
                self.close_active();
                if self.browser.tabs().is_empty() {
                    tx.focus("omnibox-input", false);
                    return;
                }
                Ok(())
            }
            Action::ToggleSidebar => {
                crate::ui::toggle_sidebar(self, tx);
                Ok(())
            }
        };
        self.address_error = result.err().map(|error| error.to_string());
        if self.address_error.is_none() {
            self.omnibox.close();
            tx.focus(VIEWPORT, false);
        }
    }

    fn navigate_destination(&mut self, url: &str) -> Result<(), crate::browser::Error> {
        match (self.omnibox.mode, self.browser.active_tab().map(Tab::id)) {
            (omnibox::Mode::Current, Some(tab)) => self.browser.navigate(tab, url)?,
            _ => {
                let tab = self.browser.open_tab(url)?;
                self.browser.activate_tab(tab);
            }
        }
        self.address_error = None;
        self.show_active_tab();
        Ok(())
    }

    pub(crate) fn activate(&mut self, tab: TabId) {
        self.browser.activate_tab(tab);
        self.address_error = None;
        self.show_active_tab();
    }

    /// Closes the tab and opens the omnibox when the window becomes empty.
    pub(crate) fn close(&mut self, tab: TabId) {
        self.browser.close_tab(tab);
        if self.browser.tabs().is_empty() {
            self.shown = None;
            self.open_omnibox(omnibox::Mode::New);
        } else {
            self.show_active_tab();
        }
    }

    pub(crate) fn duplicate(&mut self, tab: TabId) {
        if let Some(tab) = self.browser.duplicate_tab(tab) {
            self.activate(tab);
        }
    }

    pub(crate) fn close_others(&mut self, tab: TabId) {
        self.browser.close_other_tabs(tab);
        self.show_active_tab();
    }

    pub(crate) fn reload(&mut self, tab: TabId) {
        self.browser.reload(tab);
    }

    pub(crate) fn open_context_menu(&mut self, tab: Option<TabId>, point: PointerPress) {
        self.context_menu = Some((tab, point));
    }

    pub(crate) fn context_menu(&self) -> Option<(Option<TabId>, PointerPress)> {
        self.context_menu
    }

    pub(crate) fn dismiss_context_menu(&mut self) {
        self.context_menu = None;
    }

    pub(crate) fn close_active(&mut self) {
        if let Some(tab) = self.browser.active_tab().map(Tab::id) {
            self.close(tab);
        }
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

    /// Records a real active-page visit when its tab or committed URL changes.
    fn show_active_tab(&mut self) {
        let Some(tab) = self.browser.active_tab() else {
            return;
        };
        let now = (tab.id(), tab.url.clone());
        if self.shown.as_ref() != Some(&now) {
            omnibox::record_visit(&mut self.history, &tab.title, &tab.url);
            self.shown = Some(now);
        } else {
            omnibox::update_title(&mut self.history, &tab.title, &tab.url);
        }
    }

    /// The active tab's title, when it differs from what the window shows.
    fn new_window_title(&mut self) -> Option<String> {
        let title = self
            .browser
            .active_tab()
            .map(|tab| tab.title.clone())
            .unwrap_or_default();
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
    let settling = crate::ui::settle_sidebar(state, tx);
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
    polled.changed || settling
}

/// A query was submitted from the focused omnibox.
pub(crate) fn submit_omnibox(state: &mut AppState, tx: &mut Tx) {
    if !state.omnibox.open
        || (state.omnibox.input.text.trim().is_empty() && !state.omnibox.selection_moved)
    {
        return;
    }
    let suggestions = state.suggestions();
    if let Some(suggestion) = suggestions.get(state.omnibox.selected).cloned() {
        state.choose_suggestion(suggestion, tx);
    } else {
        state.submit_omnibox();
        if state.address_error.is_none() {
            state.omnibox.close();
            tx.focus(VIEWPORT, false);
        }
    }
}

/// The sidebar launcher edits the current address.
pub(crate) fn open_from_launcher(state: &mut AppState, tx: &mut Tx) {
    state.open_omnibox(omnibox::Mode::Current);
    tx.focus("omnibox-input", false);
}

/// A new tab starts in the omnibox, ready to type.
pub(crate) fn new_tab(state: &mut AppState, tx: &mut Tx) {
    state.open_omnibox(omnibox::Mode::New);
    tx.focus("omnibox-input", false);
}

/// Keyboard input the omnibox uses before the text editor handles it.
pub(crate) fn omnibox_input_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    match &event.key {
        Key::Escape => {
            state.omnibox.close();
            tx.focus(VIEWPORT, false);
            true
        }
        Key::Up | Key::Down => {
            let count = state.suggestions().len();
            let direction = if event.key == Key::Up { -1 } else { 1 };
            state.omnibox.move_selection(count, direction);
            if count > 0 {
                tx.scroll_into_view(format!("omnibox-result-{}", state.omnibox.selected));
            }
            true
        }
        _ => false,
    }
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
    use rune::Mods;

    fn active_url(state: &AppState) -> &str {
        &state.browser.active_tab().unwrap().url
    }

    #[test]
    fn starts_with_an_empty_omnibox_and_no_tabs() {
        let state = AppState::new(None);
        assert!(state.browser.tabs().is_empty());
        assert!(state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn reduced_motion_centers_the_resized_omnibox_immediately() {
        let mut runtime = rune::Runtime::new(
            AppState::new(None),
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.set_system_reduced_motion(true);
        runtime.frame(0.0);
        runtime.key(KeyEvent {
            key: Key::Text("x".into()),
            mods: Mods::default(),
        });
        runtime.frame(1.0 / 60.0);
        let panel = runtime
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("omnibox-panel"))
            .unwrap();
        assert_eq!(panel.rect, panel.target);
        let [x, y, width, height] = panel.rect;
        assert_eq!((x + width / 2.0, y + height / 2.0), (480.0, 320.0));
    }

    #[test]
    fn two_suggestions_fit_a_compact_panel() {
        let mut state = AppState::new(None);
        for address in ["example.org", "example.com"] {
            state.browser.open_tab(address).unwrap();
        }
        assert_eq!(state.suggestions().len(), 2);
        let mut runtime = rune::Runtime::new(
            state,
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        let elements = runtime.elements();
        let panel = elements
            .iter()
            .find(|el| el.key.as_deref() == Some("omnibox-panel"))
            .unwrap();
        assert!((190.0..=240.0).contains(&panel.target[3]));
    }

    #[test]
    fn startup_omnibox_accepts_typing_and_opens_the_first_tab_on_enter() {
        let mut runtime = rune::Runtime::new(
            AppState::new(None),
            crate::ui::view,
            (960.0, 640.0),
            rune::Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        runtime.key(KeyEvent {
            key: Key::Text("example.com".into()),
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(runtime.state.browser.tabs().is_empty());
        runtime.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.browser.tabs().len(), 1);
        assert_eq!(active_url(&runtime.state), "https://example.com");
        assert!(!runtime.state.omnibox.open);
    }

    #[test]
    fn opens_a_domain_it_was_given() {
        let state = AppState::new(Some("example.com"));
        assert_eq!(active_url(&state), "https://example.com");
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn treats_an_ordinary_start_argument_as_a_search() {
        let state = AppState::new(Some("not an address"));
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(
            active_url(&state),
            "https://www.google.com/search?q=not%20an%20address"
        );
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn submitting_navigates_the_current_tab_for_urls_and_searches() {
        let mut state = AppState::new(Some("example.org"));
        state.omnibox.input.replace_text("example.com/a".to_owned());
        state.submit_omnibox();
        assert_eq!(active_url(&state), "https://example.com/a");
        assert_eq!(state.address_error, None);

        state
            .omnibox
            .input
            .replace_text("best Rust GUI framework".to_owned());
        state.submit_omnibox();
        assert_eq!(
            active_url(&state),
            "https://www.google.com/search?q=best%20Rust%20GUI%20framework"
        );
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(state.address_error, None);
    }

    #[test]
    fn cmd_l_opens_with_the_current_url_selected_even_when_sidebar_is_collapsed() {
        let mut state = AppState::new(Some("example.com"));
        state.sidebar.collapsed = true;
        let mut tx = Tx::default();
        let event = KeyEvent {
            key: Key::Command('l'),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        };
        assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
        assert!(state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "https://example.com");
        assert_eq!(
            (state.omnibox.input.anchor, state.omnibox.input.focus),
            (0, state.omnibox.input.text.len())
        );
        assert_eq!(active_url(&state), "https://example.com");
    }

    #[test]
    fn escape_closes_without_changing_the_page() {
        let mut state = AppState::new(Some("example.com"));
        state.open_omnibox(omnibox::Mode::Current);
        let mut tx = Tx::default();
        assert!(omnibox_input_key(
            &mut state,
            &KeyEvent {
                key: Key::Escape,
                mods: Mods::default(),
            },
            &mut tx,
        ));
        assert!(!state.omnibox.open);
        assert_eq!(active_url(&state), "https://example.com");
    }

    #[test]
    fn choosing_a_navigation_result_reuses_the_active_tab() {
        let mut state = AppState::new(Some("example.com"));
        let active = state.browser.active_tab().unwrap().id();
        state.open_omnibox(omnibox::Mode::Current);
        state.omnibox.input.replace_text("example.org".to_owned());
        let suggestion = state.suggestions().remove(0);
        state.choose_suggestion(suggestion, &mut Tx::default());
        assert_eq!(active_url(&state), "https://example.org");
        assert_eq!(state.browser.active_tab().unwrap().id(), active);
        assert_eq!(state.browser.tabs().len(), 1);
        assert!(!state.omnibox.open);
    }

    #[test]
    fn new_tab_opens_empty_and_dismisses_without_creating_tabs() {
        let mut state = AppState::new(Some("example.com"));
        let mut tx = Tx::default();
        assert!(crate::shortcuts::on_key(
            &mut state,
            &KeyEvent {
                key: Key::Command('t'),
                mods: Mods {
                    cmd: true,
                    ..Mods::default()
                },
            },
            &mut tx,
        ));
        for _ in 0..3 {
            assert!(state.omnibox.open);
            submit_omnibox(&mut state, &mut tx);
            state.omnibox.close();
            assert_eq!(state.browser.tabs().len(), 1);
            assert_eq!(active_url(&state), "https://example.com");
            new_tab(&mut state, &mut tx);
        }
        for (query, expected, count) in [
            ("example.org", "https://example.org", 2),
            (
                "two words",
                "https://www.google.com/search?q=two%20words",
                3,
            ),
        ] {
            new_tab(&mut state, &mut tx);
            state.omnibox.input.replace_text(query);
            submit_omnibox(&mut state, &mut tx);
            assert_eq!(state.browser.tabs().len(), count);
            assert_eq!(active_url(&state), expected);
        }
    }

    #[test]
    fn the_status_reports_unsupported_schemes_and_new_tabs_clear_them() {
        let mut state = AppState::new(None);
        assert_eq!(state.status(), None);
        state
            .omnibox
            .input
            .replace_text("ftp://example.com".to_owned());
        state.submit_omnibox();
        assert!(state.status().is_some());
        new_tab(&mut state, &mut Tx::default());
        assert_eq!(state.status(), None);
    }

    #[test]
    fn launcher_selects_the_current_address() {
        let mut state = AppState::new(Some("example.com"));
        open_from_launcher(&mut state, &mut Tx::default());
        assert!(state.omnibox.open);
        assert_eq!(state.omnibox.input.text, "https://example.com");
    }

    #[test]
    fn selecting_an_open_tab_activates_it_without_opening_a_duplicate() {
        let mut state = AppState::new(Some("example.com"));
        let first = state.browser.active_tab().unwrap().id();
        let second = state.browser.open_tab("github.com").unwrap();
        state.activate(second);
        state.open_omnibox(omnibox::Mode::New);
        state.omnibox.input.replace_text("example.com");
        let tab_suggestion = state
            .suggestions()
            .into_iter()
            .find(|suggestion| suggestion.action == Action::SwitchTab(first))
            .unwrap();
        state.choose_suggestion(tab_suggestion, &mut Tx::default());
        assert_eq!(state.browser.active_tab().unwrap().id(), first);
        assert_eq!(state.browser.tabs().len(), 2);
        assert_eq!(active_url(&state), "https://example.com");
        assert!(!state.omnibox.open);
    }

    #[test]
    fn omnibox_keys_stay_in_the_input_and_escape_restores_page_focus() {
        use rune::{Env, Runtime};

        let mut runtime = Runtime::new(
            AppState::new(Some("example.com")),
            crate::ui::view,
            (960.0, 640.0),
            Env::new(false, 1.0),
        );
        runtime.frame(0.0);
        runtime.update(|state, tx| {
            state.open_omnibox(omnibox::Mode::New);
            tx.focus("omnibox-input", false);
        });
        runtime.frame(0.0);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|element| element.focused && element.key.as_deref() == Some("omnibox-input"))
        );

        runtime.key(KeyEvent {
            key: Key::Text("x".into()),
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert_eq!(runtime.state.omnibox.input.text, "x");
        assert_eq!(active_url(&runtime.state), "https://example.com");

        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        assert!(!runtime.state.omnibox.open);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|element| element.focused && element.key.as_deref() == Some(VIEWPORT))
        );
    }

    #[test]
    fn closing_the_last_tab_returns_to_the_empty_omnibox() {
        let mut state = AppState::new(Some("example.com"));
        let only = state.browser.active_tab().unwrap().id();
        state.close(only);
        assert!(state.browser.tabs().is_empty());
        assert!(state.omnibox.open);
        assert!(state.omnibox.input.text.is_empty());
    }
}
