// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::tab::TabSection;
use elaris_core::bookmarks::BookmarkId;
use elaris_core::internal::{InternalPage, InternalRoute};
use rune::Tx;

fn bookmark_on(state: &AppState) -> Option<BookmarkId> {
    let url = state.browser.active_tab()?.url.clone();
    state.bookmarks().find_url(&url).map(|bookmark| bookmark.id)
}

#[test]
fn the_star_bookmarks_the_page_in_the_bar_once_and_opens_its_editor() {
    let mut state = AppState::new(Some("https://example.com/"));
    state.bookmark_active_tab();
    let id = bookmark_on(&state).expect("bookmarked");
    assert_eq!(state.bookmarks().bar()[0].id, id);
    let editor = state.bookmarks.editor.as_ref().unwrap();
    assert_eq!(
        (editor.id, editor.folder, editor.on_page),
        (id, BookmarkId::BAR, false)
    );

    // Again, it edits the same bookmark rather than adding another.
    state.close_bookmark_editor();
    state.bookmark_active_tab();
    assert_eq!(state.bookmarks().bar().len(), 1);
    assert_eq!(state.bookmarks.editor.as_ref().unwrap().id, id);
}

#[test]
fn the_editor_renames_readdresses_and_moves_its_bookmark() {
    let mut state = AppState::new(Some("https://example.com/"));
    state.bookmark_active_tab();
    let id = bookmark_on(&state).unwrap();
    {
        let editor = state.bookmarks.editor.as_mut().unwrap();
        editor.title.replace_text("Example".to_owned());
        editor.url.replace_text("https://example.org/".to_owned());
        editor.folder = BookmarkId::OTHER;
    }
    state.save_bookmark_editor();
    assert!(state.bookmarks.editor.is_none());
    let bookmarks = state.bookmarks();
    let bookmark = bookmarks.get(id).unwrap();
    assert_eq!(bookmark.title, "Example");
    assert_eq!(bookmark.url(), Some("https://example.org/"));
    assert_eq!(bookmarks.parent(id), Some((BookmarkId::OTHER, 0)));
}

#[test]
fn removing_a_bookmark_closes_its_editor() {
    let mut state = AppState::new(Some("https://example.com/"));
    state.bookmark_active_tab();
    let id = bookmark_on(&state).unwrap();
    state.remove_bookmark(id);
    assert!(state.bookmarks.editor.is_none());
    assert!(state.bookmarks().bar().is_empty());
}

#[test]
fn a_tab_is_bookmarked_from_its_menu_without_the_editor() {
    let mut state = AppState::new(Some("https://example.com/"));
    let other = state.browser.open_tab("https://example.org/").unwrap();
    state.bookmark_tab(other, false);
    assert!(state.bookmarks.editor.is_none());
    assert!(state.bookmarks().find_url("https://example.org/").is_some());
    // Internal pages have nothing to bookmark.
    state
        .browser
        .navigate_internal(other, InternalRoute::Page(InternalPage::History))
        .unwrap();
    state.bookmark_tab(other, false);
    assert_eq!(state.bookmarks().bar().len(), 1);
}

#[test]
fn bookmarks_open_in_place_or_in_a_new_tab_and_join_the_favourites() {
    let mut state = AppState::new(Some("https://example.com/"));
    let id = {
        let mut bookmarks = state.bookmarks();
        bookmarks
            .add_link(BookmarkId::BAR, "Docs", "https://docs.rs/", 0)
            .unwrap()
    };
    let mut tx = Tx::default();
    state.open_bookmark(id, false, &mut tx);
    assert_eq!(state.browser.tabs().len(), 1);
    assert_eq!(state.browser.active_tab().unwrap().url, "https://docs.rs/");

    state.open_bookmark(id, true, &mut tx);
    assert_eq!(state.browser.tabs().len(), 2);

    state.add_bookmark_to_favourites(id);
    let favourites: Vec<_> = state
        .browser
        .section_tabs(TabSection::Favourite)
        .map(|tab| tab.url.clone())
        .collect();
    assert_eq!(favourites, ["https://docs.rs/"]);
}

#[test]
fn a_new_folder_opens_its_editor_to_be_named() {
    let mut state = AppState::new(Some("https://example.com/"));
    state.add_bookmark_folder(BookmarkId::OTHER);
    let editor = state.bookmarks.editor.as_ref().unwrap();
    assert!(editor.on_page);
    assert_eq!(editor.folder, BookmarkId::OTHER);
    assert!(state.bookmarks().get(editor.id).unwrap().is_folder());
}

#[test]
fn a_change_in_one_window_is_seen_by_the_others() {
    let mut state = AppState::new(Some("https://example.com/"));
    let mut other = AppState::new(Some("https://example.org/"));
    other.poll_bookmarks();
    state.bookmark_active_tab();
    // Other tests change bookmarks in parallel, so only the change being seen is certain.
    assert!(other.poll_bookmarks());
}

#[test]
fn every_web_page_is_saved_to_a_new_folder_named_on_the_bookmarks_page() {
    let mut state = AppState::new(Some("https://example.com/"));
    state.browser.open_tab("https://example.org/").unwrap();
    state.browser.open_tab("elaris://history").unwrap();
    state.bookmark_all_tabs(&mut Tx::default());
    let editor = state.bookmarks.editor.as_ref().expect("naming the folder");
    assert!(editor.on_page);
    let bookmarks = state.bookmarks();
    let folder = bookmarks.get(editor.id).unwrap();
    assert_eq!(folder.title, "Saved Tabs");
    let urls: Vec<_> = folder
        .children()
        .iter()
        .filter_map(|link| link.url())
        .collect();
    assert_eq!(urls, ["https://example.com/", "https://example.org/"]);
    assert_eq!(bookmarks.parent(folder.id).unwrap().0, BookmarkId::OTHER);
    drop(bookmarks);
    assert_eq!(
        state.browser.active_tab().unwrap().internal,
        Some(InternalRoute::Page(InternalPage::Bookmarks))
    );
}

#[test]
fn fetched_pages_and_open_tabs_fill_in_what_bookmarks_lack() {
    let mut state = AppState::new(Some("https://example.com/"));
    let (engine, fake) = crate::engine::Engine::fake();
    state.browser.connect(engine);
    state.set_tab_layout(elaris_core::settings::TabLayout::Horizontal);
    let id = {
        let mut bookmarks = state.bookmarks();
        bookmarks
            .add_link(BookmarkId::BAR, "", "https://docs.rs/", 0)
            .unwrap()
    };
    // On show in the bar and lacking a title, its page is fetched, once.
    let mut tx = Tx::default();
    state.poll_bookmark_details(&crate::browser::Polled::default(), &mut tx);
    let loads = |fake: &crate::engine::FakeEngine| {
        fake.commands
            .try_iter()
            .filter(|command| {
                matches!(command, crate::engine::Command::View(_, crate::engine::ViewCommand::Load(url)) if url == "https://docs.rs/")
            })
            .count()
    };
    assert_eq!(loads(&fake), 1);
    state.poll_bookmark_details(&crate::browser::Polled::default(), &mut tx);
    assert_eq!(loads(&fake), 0);

    let polled = crate::browser::Polled {
        fetched: vec![crate::browser::Fetched {
            url: "https://docs.rs/".into(),
            title: Some("Docs.rs".into()),
            favicon: None,
        }],
        ..Default::default()
    };
    assert!(state.poll_bookmark_details(&polled, &mut tx));
    assert_eq!(state.bookmarks().get(id).unwrap().title, "Docs.rs");

    // An open tab at the page gives it its icon.
    let tab = state.browser.active_tab().unwrap().id();
    state.browser.navigate(tab, "https://docs.rs/").unwrap();
    state.browser.restore_favicon(tab, 1, 1, vec![9, 9, 9, 9]);
    let changed = crate::browser::Polled {
        changed: true,
        ..Default::default()
    };
    state.poll_bookmark_details(&changed, &mut tx);
    assert!(state.bookmarks().get(id).unwrap().favicon.is_some());
}

#[test]
fn the_tab_menu_removes_a_bookmark_it_made() {
    let mut state = AppState::new(Some("https://example.com/"));
    let tab = state.browser.active_tab().unwrap().id();
    state.bookmark_tab(tab, false);
    let id = bookmark_on(&state).unwrap();
    state.remove_bookmark(id);
    assert!(bookmark_on(&state).is_none());
}
