// SPDX-License-Identifier: GPL-3.0-only

//! Omnibox state, typed actions, and deterministic suggestions from local browser data.

use rune::TextEdit;

use crate::tab::{Tab, TabId};
use elaris_core::address::{self, Destination};
use elaris_core::history::Entry as HistoryEntry;

const MAX_TAB_SUGGESTIONS: usize = 4;
const MAX_HISTORY_SUGGESTIONS: usize = 3;
const MAX_BOOKMARK_SUGGESTIONS: usize = 3;
/// How many bookmarks the bookmarks scope lists at once.
const MAX_BOOKMARK_SCOPE_RESULTS: usize = 50;
/// How much must be typed before a bookmarked site becomes the top hit, so one letter never
/// takes over Enter.
const MIN_TOP_HIT_LENGTH: usize = 2;

/// A bookmark as the omnibox matches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BookmarkHint {
    pub(crate) title: String,
    pub(crate) url: String,
    /// The folders it sits in, such as "Bookmarks Bar News", which a query can name.
    pub(crate) folders: String,
}

/// How well a bookmark matches, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BookmarkMatch {
    /// Its site starts with the whole query: the top hit.
    Site,
    /// Its title starts with the query.
    Title,
    /// Every word starts a word of its title, site, address or folders.
    Words,
    /// The query appears somewhere in its address.
    Address,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Current,
    New,
}

/// What the omnibox searches: everything, or only bookmarks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    All,
    Bookmarks,
}

impl Scope {
    pub(crate) fn toggled(self) -> Scope {
        match self {
            Scope::All => Scope::Bookmarks,
            Scope::Bookmarks => Scope::All,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) open: bool,
    pub(crate) mode: Mode,
    pub(crate) scope: Scope,
    pub(crate) input: TextEdit,
    pub(crate) selected: usize,
    pub(crate) selection_moved: bool,
    pub(crate) search_engine_menu_open: bool,
}

impl State {
    pub(crate) fn open(&mut self, query: &str, select_all: bool) {
        self.selection_moved = false;
        self.open = true;
        self.scope = Scope::All;
        self.input.replace_text(query.to_owned());
        if select_all {
            self.input.anchor = 0;
            self.input.focus = self.input.text.len();
        }
        self.selected = 0;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.search_engine_menu_open = false;
    }

    /// Switches between searching everything and searching bookmarks, from the top result.
    pub(crate) fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
        self.selected = 0;
        self.selection_moved = false;
        self.search_engine_menu_open = false;
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
    Bookmarks,
    Recent,
    QuickActions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyph {
    Search,
    Page,
    History,
    Bookmark,
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

/// A short list built only from the input, currently open tabs, bookmarks, and locally recorded
/// visits.
pub(crate) fn suggestions(
    query: &str,
    tabs: &[Tab],
    bookmarks: &[BookmarkHint],
    history: &[HistoryEntry],
    sidebar_collapsed: bool,
    search_engine: &str,
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
                detail: search_engine.to_owned(),
                glyph: Glyph::Search,
                action: Action::Search(query),
            }),
        }
    }

    let folded = query.to_ascii_lowercase();
    let mut matching_tabs: Vec<_> = tabs
        .iter()
        .filter(|tab| tab.url != "about:blank")
        .filter_map(|tab| {
            let score = match_score(&folded, &tab.title, &tab.url);
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
                detail: site_detail(tab.label(), &tab.url),
                glyph: Glyph::Page,
                action: Action::SwitchTab(tab.id()),
            }),
    );

    // Bookmarks come only for something typed, ahead of plain visits. A bookmark whose site
    // starts with what was typed is the top hit, so Enter opens it.
    let mut matching_bookmarks: Vec<_> = bookmarks
        .iter()
        .filter(|_| !query.is_empty())
        .filter(|bookmark| !tabs.iter().any(|tab| tab.url == bookmark.url))
        .filter_map(|bookmark| {
            let visits = history
                .iter()
                .find(|entry| entry.url == bookmark.url)
                .map_or(0, |entry| entry.visit_count);
            bookmark_score(&folded, bookmark).map(|score| (score, visits, bookmark))
        })
        .collect();
    matching_bookmarks.sort_by(|(left, left_visits, _), (right, right_visits, _)| {
        left.cmp(right).then_with(|| right_visits.cmp(left_visits))
    });
    matching_bookmarks.truncate(MAX_BOOKMARK_SUGGESTIONS);
    let shown_bookmarks: Vec<&String> = matching_bookmarks
        .iter()
        .map(|(_, _, bookmark)| &bookmark.url)
        .collect();
    let suggestion = |bookmark: &BookmarkHint, section| Suggestion {
        section,
        title: if bookmark.title.is_empty() {
            bookmark.url.clone()
        } else {
            bookmark.title.clone()
        },
        detail: site_detail(&bookmark.title, &bookmark.url),
        glyph: Glyph::Bookmark,
        action: Action::VisitRecent(bookmark.url.clone()),
    };
    let mut rest = matching_bookmarks.iter();
    if let Some((score, _, bookmark)) = matching_bookmarks.first()
        && *score == BookmarkMatch::Site
        && folded.chars().count() >= MIN_TOP_HIT_LENGTH
    {
        rest.next();
        let top = suggestion(bookmark, Section::Navigate);
        // A typed address that leads to the same page needs no second row.
        suggestions.retain(|existing| {
            !matches!(&existing.action, Action::Navigate(url) if same_page(url, &bookmark.url))
        });
        suggestions.insert(0, top);
    }
    suggestions.extend(rest.map(|(_, _, bookmark)| suggestion(bookmark, Section::Bookmarks)));

    let mut matching_history: Vec<_> = history
        .iter()
        .filter(|entry| !tabs.iter().any(|tab| tab.url == entry.url))
        .filter(|entry| !shown_bookmarks.contains(&&entry.url))
        .filter_map(|entry| {
            match_score(&folded, &entry.title, &entry.url).map(|score| (score, entry))
        })
        .collect();
    matching_history.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| right.visit_count.cmp(&left.visit_count))
            .then_with(|| right.last_visited.cmp(&left.last_visited))
    });
    suggestions.extend(
        matching_history
            .into_iter()
            .take(MAX_HISTORY_SUGGESTIONS)
            .map(|(_, entry)| Suggestion {
                section: Section::Recent,
                title: entry.title.clone(),
                detail: site_detail(&entry.title, &entry.url),
                glyph: Glyph::History,
                action: Action::VisitRecent(entry.url.clone()),
            }),
    );

    let actions = [
        (
            "New Tab",
            "Search or type a URL",
            Glyph::NewTab,
            Action::NewTab,
        ),
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
                !query.is_empty()
                    && action_matches(&folded, title)
                    && (!tabs.is_empty()
                        || !matches!(action, Action::CloseTab | Action::ReloadPage))
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

/// Bookmarks alone, for the bookmarks scope: every one when nothing is typed, most visited
/// first, otherwise the best matches. An open bookmark switches to its tab.
pub(crate) fn bookmark_suggestions(
    query: &str,
    tabs: &[Tab],
    bookmarks: &[BookmarkHint],
    history: &[HistoryEntry],
) -> Vec<Suggestion> {
    let folded = query.trim().to_ascii_lowercase();
    let mut matching: Vec<_> = bookmarks
        .iter()
        .filter_map(|bookmark| {
            let score = if folded.is_empty() {
                BookmarkMatch::Site
            } else {
                bookmark_score(&folded, bookmark)?
            };
            let visits = history
                .iter()
                .find(|entry| entry.url == bookmark.url)
                .map_or(0, |entry| entry.visit_count);
            Some((score, visits, bookmark))
        })
        .collect();
    matching.sort_by(|(left, left_visits, a), (right, right_visits, b)| {
        left.cmp(right)
            .then_with(|| right_visits.cmp(left_visits))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    matching
        .into_iter()
        .take(MAX_BOOKMARK_SCOPE_RESULTS)
        .map(|(_, _, bookmark)| {
            let host = address::host_for_address(&bookmark.url);
            let detail = match bookmark.folders.as_str() {
                "" => host.to_owned(),
                folders => format!("{folders} · {host}"),
            };
            let action = tabs.iter().find(|tab| tab.url == bookmark.url).map_or_else(
                || Action::VisitRecent(bookmark.url.clone()),
                |tab| Action::SwitchTab(tab.id()),
            );
            Suggestion {
                section: Section::Bookmarks,
                title: if bookmark.title.is_empty() {
                    bookmark.url.clone()
                } else {
                    bookmark.title.clone()
                },
                detail,
                glyph: Glyph::Bookmark,
                action,
            }
        })
        .collect()
}

fn site_detail(title: &str, url: &str) -> String {
    let host = address::host_for_address(url);
    if title.eq_ignore_ascii_case(host) {
        String::new()
    } else {
        host.to_owned()
    }
}

fn match_score(query: &str, title: &str, url: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let title = title.to_ascii_lowercase();
    let url_lower = url.to_ascii_lowercase();
    let domain = address::host_for_address(&url_lower);
    if title == query || domain == query {
        Some(0)
    } else if title.starts_with(query) || domain.starts_with(query) {
        Some(1)
    } else if title.contains(query) || domain.contains(query) {
        Some(2)
    } else if url_lower.contains(query) {
        Some(3)
    } else {
        None
    }
}

/// Whether two addresses differ only by scheme, a leading "www." or a trailing slash.
fn same_page(left: &str, right: &str) -> bool {
    let page = |url: &str| {
        let url = url.to_ascii_lowercase();
        let rest = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        rest.trim_end_matches('/').to_owned()
    };
    page(left) == page(right)
}

/// How a bookmark matches the lower-cased query, if it does.
fn bookmark_score(query: &str, bookmark: &BookmarkHint) -> Option<BookmarkMatch> {
    let url = bookmark.url.to_ascii_lowercase();
    let host = address::host_for_address(&url);
    let site = host.strip_prefix("www.").unwrap_or(host);
    let title = bookmark.title.to_lowercase();
    if !query.contains(' ') && (site.starts_with(query) || host.starts_with(query)) {
        return Some(BookmarkMatch::Site);
    }
    if title.starts_with(query) {
        return Some(BookmarkMatch::Title);
    }
    let text = [title.as_str(), &bookmark.folders.to_lowercase(), &url].join(" ");
    let words: Vec<&str> = text
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if query
        .split_whitespace()
        .all(|part| words.iter().any(|word| word.starts_with(part)))
    {
        return Some(BookmarkMatch::Words);
    }
    url.contains(query).then_some(BookmarkMatch::Address)
}

fn action_matches(query: &str, label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    query
        .split_whitespace()
        .all(|part| label.split_whitespace().any(|word| word.starts_with(part)))
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
        let suggestions = suggestions("rust", &tabs, &[], &[], false, "DuckDuckGo");
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
        let suggestions = suggestions("github.com", &tabs, &[], &[], false, "DuckDuckGo");
        assert!(suggestions.iter().any(|suggestion| {
            suggestion.title == "Elaris"
                && suggestion.action == Action::SwitchTab(TabId::from_raw(7))
        }));
    }

    #[test]
    fn filters_recent_entries_to_real_recorded_visits() {
        let history = vec![
            HistoryEntry {
                url: "https://doc.rust-lang.org/book/".into(),
                title: "Rust Book".into(),
                last_visited: 2,
                visit_count: 1,
            },
            HistoryEntry {
                url: "https://example.com/".into(),
                title: "Example".into(),
                last_visited: 1,
                visit_count: 1,
            },
        ];
        let suggestions = suggestions("rust", &[], &[], &history, false, "DuckDuckGo");
        let recent: Vec<_> = suggestions
            .iter()
            .filter(|suggestion| suggestion.section == Section::Recent)
            .collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "Rust Book");
    }

    fn hint(title: &str, url: &str, folders: &str) -> BookmarkHint {
        BookmarkHint {
            title: title.into(),
            url: url.into(),
            folders: folders.into(),
        }
    }

    fn visit(title: &str, url: &str, visit_count: u64) -> HistoryEntry {
        HistoryEntry {
            url: url.into(),
            title: title.into(),
            last_visited: 1,
            visit_count,
        }
    }

    fn find(query: &str, bookmarks: &[BookmarkHint], history: &[HistoryEntry]) -> Vec<Suggestion> {
        suggestions(query, &[], bookmarks, history, false, "DuckDuckGo")
    }

    #[test]
    fn the_bookmarks_scope_lists_only_bookmarks_and_filters_them() {
        let bookmarks = [
            hint("Rust Book", "https://doc.rust-lang.org/book/", "Reading"),
            hint("Example", "https://example.com/", ""),
        ];
        let history = [visit("Example", "https://example.com/", 5)];
        let all = bookmark_suggestions("", &[], &bookmarks, &history);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "Example");
        assert!(all.iter().all(|item| item.section == Section::Bookmarks));
        let found = bookmark_suggestions("reading", &[], &bookmarks, &history);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].detail, "Reading · doc.rust-lang.org");
        let tabs = [tab(3, "Example", "https://example.com/")];
        let open = bookmark_suggestions("exa", &tabs, &bookmarks, &[]);
        assert_eq!(open[0].action, Action::SwitchTab(TabId::from_raw(3)));
    }

    #[test]
    fn bookmarks_lead_history_for_typed_queries_and_hide_their_visits() {
        let bookmarks = [hint("Rust Book", "https://doc.rust-lang.org/book/", "")];
        let history = [
            visit("Rust Book", "https://doc.rust-lang.org/book/", 9),
            visit("Rust", "https://rust-lang.org/", 1),
        ];
        let found = find("rust", &bookmarks, &history);
        let sections: Vec<_> = found
            .iter()
            .filter(|s| matches!(s.section, Section::Bookmarks | Section::Recent))
            .map(|s| (s.section, s.title.as_str()))
            .collect();
        assert_eq!(
            sections,
            [(Section::Bookmarks, "Rust Book"), (Section::Recent, "Rust")]
        );
        assert!(
            find("", &bookmarks, &[])
                .iter()
                .all(|s| s.glyph != Glyph::Bookmark)
        );
    }

    #[test]
    fn a_bookmarked_site_that_starts_with_the_query_is_the_top_hit() {
        let bookmarks = [
            hint("GitHub", "https://www.github.com/", ""),
            hint("BBC News", "https://bbc.co.uk/news", ""),
        ];
        let top = &find("git", &bookmarks, &[])[0];
        assert_eq!(
            (top.section, top.glyph, &top.action),
            (
                Section::Navigate,
                Glyph::Bookmark,
                &Action::VisitRecent("https://www.github.com/".into())
            )
        );
        // A title word is a suggestion, not the top hit, so the search stays first.
        let news = find("news", &bookmarks, &[]);
        assert!(matches!(news[0].action, Action::Search(_)));
        assert!(
            news.iter()
                .any(|s| s.section == Section::Bookmarks && s.title == "BBC News")
        );
        // One letter never takes over Enter.
        assert!(matches!(
            find("g", &bookmarks, &[])[0].action,
            Action::Search(_)
        ));
        // A typed address that is the bookmark is listed once.
        let typed = find("github.com", &bookmarks, &[]);
        assert_eq!(typed[0].glyph, Glyph::Bookmark);
        assert!(
            !typed
                .iter()
                .any(|s| matches!(&s.action, Action::Navigate(url) if url.contains("github")))
        );
    }

    #[test]
    fn every_word_can_name_the_title_site_address_or_folders() {
        let bookmarks = [
            hint("Front page", "https://bbc.co.uk/", "Bookmarks Bar News"),
            hint("Recipes", "https://cooking.example/", "Other Bookmarks"),
        ];
        let found = find("news bbc", &bookmarks, &[]);
        let titles: Vec<_> = found
            .iter()
            .filter(|s| s.glyph == Glyph::Bookmark)
            .map(|s| s.title.as_str())
            .collect();
        assert_eq!(titles, ["Front page"]);
        assert!(
            find("news cooking", &bookmarks, &[])
                .iter()
                .all(|s| s.glyph != Glyph::Bookmark)
        );
    }

    #[test]
    fn equally_good_bookmarks_rank_by_visits() {
        let bookmarks = [
            hint("Docs one", "https://one.example/", "Docs"),
            hint("Docs two", "https://two.example/", "Docs"),
        ];
        let history = [visit("Docs two", "https://two.example/", 7)];
        let titles: Vec<_> = find("docs", &bookmarks, &history)
            .into_iter()
            .filter(|s| s.glyph == Glyph::Bookmark)
            .map(|s| s.title)
            .collect();
        assert_eq!(titles, ["Docs two", "Docs one"]);
    }

    #[test]
    fn history_search_matches_titles_domains_and_urls_with_title_matches_first() {
        let history = vec![
            HistoryEntry {
                url: "https://docs.example.org/book".into(),
                title: "Rust Book".into(),
                last_visited: 1,
                visit_count: 1,
            },
            HistoryEntry {
                url: "https://github.com/rust-lang/rust".into(),
                title: "Source repository".into(),
                last_visited: 2,
                visit_count: 1,
            },
        ];

        let rust = suggestions("rust", &[], &[], &history, false, "DuckDuckGo");
        let recent: Vec<_> = rust
            .iter()
            .filter(|suggestion| suggestion.section == Section::Recent)
            .collect();
        assert_eq!(recent[0].title, "Rust Book");
        assert_eq!(recent[1].title, "Source repository");

        let github = suggestions("github", &[], &[], &history, false, "DuckDuckGo");
        assert!(github.iter().any(|suggestion| {
            suggestion.section == Section::Recent
                && suggestion.action
                    == Action::VisitRecent("https://github.com/rust-lang/rust".into())
        }));
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
