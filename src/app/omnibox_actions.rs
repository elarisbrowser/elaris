// SPDX-License-Identifier: GPL-3.0-only

use rune::Tx;

use super::new_tab;
use crate::browser::Browser;
use crate::omnibox::{self, Action, Suggestion};
use crate::tab::{Tab, TabSection};
use crate::ui::viewport::focus_key;
use elaris_core::address::{self, Destination};
use elaris_core::internal::InternalRoute;
use elaris_core::settings::NewTabPosition;

impl super::AppState {
    /// Loads what the omnibox holds in the active tab.
    pub(crate) fn submit_omnibox(&mut self) {
        let address = self.omnibox.input.text.clone();
        let result = match address::destination_for_input(&address) {
            Ok(Destination::Url(url)) => self.navigate_destination(&url),
            Ok(Destination::Search(query)) => {
                self.navigate_destination(&address::search_url(self.settings.search_engine, &query))
            }
            Err(error) => Err(crate::browser::Error::Address(error)),
        };
        self.address_error = result.err().map(|error| error.to_string());
    }

    pub(crate) fn open_omnibox(&mut self, mode: omnibox::Mode) {
        // A mini window has no address bar to type into.
        if self.mini {
            return;
        }
        self.dismiss_find();
        self.page_menu = Default::default();
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
        self.request_history_search();
        self.address_error = None;
    }

    pub(crate) fn suggestions(&self) -> Vec<Suggestion> {
        let query = self.omnibox.input.text.trim();
        let history = self.storage.as_ref().map_or_else(Vec::new, |storage| {
            let history = storage.history();
            if query.is_empty() {
                let mut entries = history.recent(6);
                if entries.len() < 3 {
                    for entry in history.frequent(6) {
                        if !entries.iter().any(|recent| recent.url == entry.url) {
                            entries.push(entry);
                        }
                        if entries.len() >= 6 {
                            break;
                        }
                    }
                }
                entries
            } else {
                history.search(query, 100)
            }
        });
        let mut suggestions = omnibox::suggestions(
            query,
            self.browser.tabs(),
            &history,
            self.sidebar.collapsed,
            self.settings.search_engine,
        );
        if self.horizontal_tabs() {
            suggestions.retain(|suggestion| suggestion.action != Action::ToggleSidebar);
        }
        suggestions
    }

    pub(crate) fn request_history_search(&self) {
        if let Some(storage) = &self.storage
            && let Err(error) = storage.history().request_search(&self.omnibox.input.text)
        {
            eprintln!("Elaris: {error}");
        }
    }

    pub(crate) fn choose_suggestion(&mut self, suggestion: Suggestion, tx: &mut Tx) {
        let result = match suggestion.action {
            Action::Navigate(url) => self.navigate_destination(&url),
            Action::Search(query) => {
                self.navigate_destination(&address::search_url(self.settings.search_engine, &query))
            }
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
            tx.focus(focus_key(&self.browser), false);
        }
    }

    fn navigate_destination(&mut self, url: &str) -> Result<(), crate::browser::Error> {
        self.site_permissions_open = false;
        let route = InternalRoute::parse(url);
        match (
            self.omnibox.mode,
            self.browser.active_tab().map(Tab::id),
            route,
        ) {
            (omnibox::Mode::Current, Some(tab), Some(route)) => {
                self.browser.navigate_internal(tab, route)?;
            }
            (omnibox::Mode::Current, Some(tab), None) => self.browser.navigate(tab, url)?,
            (_, _, _) => {
                let tab = self.browser.open_tab(url)?;
                if self.settings.new_tab_position == NewTabPosition::Top {
                    self.browser.move_tab_to(tab, TabSection::Regular, 0)?;
                }
                self.browser.activate_tab(tab);
            }
        }
        self.address_error = None;
        Ok(())
    }
}
