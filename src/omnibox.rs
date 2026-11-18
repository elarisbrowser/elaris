// SPDX-License-Identifier: GPL-3.0-only

//! Omnibox state, typed actions, and deterministic suggestions from local browser data.

use rune::TextEdit;

use crate::address::{self, Destination};
use crate::tab::{Tab, TabId};

const MAX_TAB_SUGGESTIONS: usize = 4;
const MAX_HISTORY_SUGGESTIONS: usize = 3;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Current,
    New,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) open: bool,
    pub(crate) mode: Mode,
    pub(crate) input: TextEdit,
    pub(crate) selected: usize,
    pub(crate) selection_moved: bool,
}

impl State {
    pub(crate) fn open(&mut self, query: &str, select_all: bool) {
        self.selection_moved = false;
        self.open = true;
        self.input.replace_text(query.to_owned());
        if select_all {
            self.input.anchor = 0;
            self.input.focus = self.input.text.len();
        }
        self.selected = 0;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn move_selection(&mut self, count: usize, direction: isize) {
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selection_moved = true;
        self.selected = (self.selected as isize + direction).rem_euclid(count as isize) as usize;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Navigate,
    OpenTabs,
    Recent,
    QuickActions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyph {
    Search,
    Page,
    History,
    NewTab,
    Reload,
    Close,
    Sidebar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Navigate(String),
    Search(String),
    SwitchTab(TabId),
    VisitRecent(String),
    NewTab,
    ReloadPage,
    CloseTab,
    ToggleSidebar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub(crate) section: Section,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) glyph: Glyph,
    pub(crate) action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryEntry {
    pub(crate) title: String,
    pub(crate) url: String,
}

/// The most recent locally observed page visit, deduplicated and kept to a small in-memory list.
pub(crate) fn record_visit(history: &mut Vec<HistoryEntry>, title: &str, url: &str) {
    if url.is_empty() || url == "about:blank" {
        return;
    }
    if let Some(index) = history.iter().position(|entry| entry.url == url) {
        let mut entry = history.remove(index);
        if !title.is_empty() {
            entry.title = title.to_owned();
        }
        history.insert(0, entry);
        return;
    }
    history.insert(
        0,
        HistoryEntry {
            title: if title.is_empty() {
                url.to_owned()
            } else {
                title.to_owned()
            },
            url: url.to_owned(),
        },
    );
    history.truncate(20);
}

pub(crate) fn update_title(history: &mut [HistoryEntry], title: &str, url: &str) {
    if title.is_empty() {
        return;
    }
    if let Some(entry) = history.iter_mut().find(|entry| entry.url == url) {
        entry.title = title.to_owned();
    }
}

/// A short list built only from the input, currently open tabs, and locally recorded visits.
pub(crate) fn suggestions(
    query: &str,
    tabs: &[Tab],
    history: &[HistoryEntry],
    sidebar_collapsed: bool,
) -> Vec<Suggestion> {
    let query = query.trim();
    let mut suggestions = Vec::new();

    if !query.is_empty()
        && let Ok(destination) = address::destination_for_input(query)
    {
        match destination {
            Destination::Url(url) => suggestions.push(Suggestion {
                section: Section::Navigate,
                title: format!("Open {}", query),
                detail: address::host_for_address(query).to_owned(),
                glyph: Glyph::Page,
                action: Action::Navigate(url),
            }),
            Destination::Search(query) => suggestions.push(Suggestion {
                section: Section::Navigate,
                title: query.clone(),
                detail: "Google".to_owned(),
                glyph: Glyph::Search,
                action: Action::Search(query),
            }),
        }
    }

    let mut matching_tabs: Vec<_> = tabs
        .iter()
        .filter(|tab| tab.url != "about:blank")
        .filter_map(|tab| {
            let score = match_score(query, &tab.title, &tab.url);
            score.map(|score| (score, tab))
        })
        .collect();
    matching_tabs.sort_by(|(left_score, left_tab), (right_score, right_tab)| {
        left_score
            .cmp(right_score)
            .then_with(|| right_tab.id().raw().cmp(&left_tab.id().raw()))
    });
    suggestions.extend(
        matching_tabs
            .into_iter()
            .take(MAX_TAB_SUGGESTIONS)
            .map(|(_, tab)| Suggestion {
                section: Section::OpenTabs,
                title: tab.label().to_owned(),
                detail: format!("Open tab · {}", address::host_for_address(&tab.url)),
                glyph: Glyph::Page,
                action: Action::SwitchTab(tab.id()),
            }),
    );

    let mut matching_history: Vec<_> = history
        .iter()
        .filter(|entry| !tabs.iter().any(|tab| tab.url == entry.url))
        .filter_map(|entry| {
            match_score(query, &entry.title, &entry.url).map(|score| (score, entry))
        })
        .collect();
    matching_history.sort_by_key(|(score, _)| *score);
    suggestions.extend(
        matching_history
            .into_iter()
            .take(MAX_HISTORY_SUGGESTIONS)
            .map(|(_, entry)| Suggestion {
                section: Section::Recent,
                title: entry.title.clone(),
                detail: address::host_for_address(&entry.url).to_owned(),
                glyph: Glyph::History,
                action: Action::VisitRecent(entry.url.clone()),
            }),
    );

    let actions = [
        ("New Tab", "Open a blank tab", Glyph::NewTab, Action::NewTab),
        (
            "Reload Page",
            "Reload the active page",
            Glyph::Reload,
            Action::ReloadPage,
        ),
        (
            "Close Tab",
            "Close the active tab",
            Glyph::Close,
            Action::CloseTab,
        ),
        (
            if sidebar_collapsed {
                "Show Sidebar"
            } else {
                "Hide Sidebar"
            },
            "Toggle the sidebar",
            Glyph::Sidebar,
            Action::ToggleSidebar,
        ),
    ];
    suggestions.extend(
        actions
            .into_iter()
            .filter(|(title, _, _, action)| {
                if query.is_empty() {
                    matches!(action, Action::ToggleSidebar)
                } else {
                    action_matches(query, title)
                }
            })
            .map(|(title, detail, glyph, action)| Suggestion {
                section: Section::QuickActions,
                title: title.to_owned(),
                detail: detail.to_owned(),
                glyph,
                action,
            }),
    );
    if query.is_empty() {
        suggestions.sort_by_key(|item| item.section != Section::Recent);
    }
    suggestions.truncate(8);
    suggestions
}

fn match_score(query: &str, title: &str, url: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let query = query.to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    let url_lower = url.to_ascii_lowercase();
    let domain = address::host_for_address(url).to_ascii_lowercase();
    if title == query || domain == query {
        Some(0)
    } else if title.starts_with(&query) || domain.starts_with(&query) {
        Some(1)
    } else if title.contains(&query) || domain.contains(&query) {
        Some(2)
    } else if url_lower.contains(&query) {
        Some(3)
    } else {
        None
    }
}

fn action_matches(query: &str, label: &str) -> bool {
    query.is_empty()
        || query.split_whitespace().any(|word| {
            label
                .to_ascii_lowercase()
                .contains(&word.to_ascii_lowercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u64, title: &str, url: &str) -> Tab {
        let mut tab = Tab::new(TabId::from_raw(id), url.to_owned());
        tab.title = title.to_owned();
        tab
    }

    #[test]
    fn ranks_tab_title_matches_before_weaker_matches() {
        let tabs = [
            tab(1, "Rust GUI toolkit", "https://example.com/rust"),
            tab(
                2,
                "The Rust Programming Language",
                "https://www.rust-lang.org",
            ),
            tab(3, "Rust blog", "https://blog.example.org"),
        ];
        let suggestions = suggestions("rust", &tabs, &[], false);
        let tabs: Vec<_> = suggestions
            .iter()
            .filter(|suggestion| suggestion.section == Section::OpenTabs)
            .collect();
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[0].title, "Rust blog");
        assert_eq!(tabs[1].title, "Rust GUI toolkit");
    }

    #[test]
    fn matches_open_tabs_by_domain_and_url_and_selects_the_existing_tab() {
        let tabs = [tab(7, "Elaris", "https://github.com/theo/elaris/issues")];
        let suggestions = suggestions("github.com", &tabs, &[], false);
        assert!(suggestions.iter().any(|suggestion| {
            suggestion.title == "Elaris"
                && suggestion.action == Action::SwitchTab(TabId::from_raw(7))
        }));
    }

    #[test]
    fn filters_recent_entries_to_real_recorded_visits() {
        let history = vec![
            HistoryEntry {
                title: "Rust Book".into(),
                url: "https://doc.rust-lang.org/book/".into(),
            },
            HistoryEntry {
                title: "Example".into(),
                url: "https://example.com/".into(),
            },
        ];
        let suggestions = suggestions("rust", &[], &history, false);
        let recent: Vec<_> = suggestions
            .iter()
            .filter(|suggestion| suggestion.section == Section::Recent)
            .collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Rust Book");
    }

    #[test]
    fn records_only_real_visits_and_deduplicates_urls() {
        let mut history = Vec::new();
        record_visit(&mut history, "", "about:blank");
        record_visit(&mut history, "Other", "https://other.example/");
        record_visit(&mut history, "", "https://example.com/");
        record_visit(&mut history, "Example", "https://example.com/");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].title, "Example");
        assert_eq!(history[0].url, "https://example.com/");
    }

    #[test]
    fn keyboard_selection_wraps_and_opening_selects_the_address() {
        let mut state = State::default();
        state.open("https://example.com", true);
        assert!(state.open);
        assert_eq!(state.input.anchor, 0);
        assert_eq!(state.input.focus, state.input.text.len());
        state.move_selection(3, -1);
        assert_eq!(state.selected, 2);
        state.move_selection(3, 1);
        assert_eq!(state.selected, 0);
        state.close();
        assert!(!state.open);
    }
}
