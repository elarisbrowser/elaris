// SPDX-License-Identifier: GPL-3.0-only

//! Bookmarks in the window's chrome: the star in the address field with its editor, and the
//! bookmarks bar the horizontal layout shows under its toolbar.

use rune::El;

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::bookmarks::Bookmark;

mod bar;
mod editor;

pub(in crate::ui) use bar::{bar, mark, sidebar_button};
pub(in crate::ui) use editor::{editor, star};

/// The horizontal layout's chrome above the page: its tab strip and toolbar, and the bookmarks
/// bar while it shows.
pub(crate) fn horizontal_chrome_height(state: &AppState) -> f32 {
    let chrome = if state.compact_tabs() || !super::horizontal_tabs::tab_bar_visible(state) {
        theme::HORIZONTAL_STRIP_HEIGHT
    } else {
        theme::HORIZONTAL_CHROME_HEIGHT
    };
    chrome
        + if bar_shown(state) {
            theme::BOOKMARKS_BAR_HEIGHT
        } else {
            0.0
        }
}

/// How far the chrome above the page reaches down: the horizontal chrome, or the header row that
/// holds the vertical layout's toolbar.
pub(crate) fn top_chrome_height(state: &AppState) -> f32 {
    if state.horizontal_tabs() {
        horizontal_chrome_height(state)
    } else {
        theme::HEADER_HEIGHT
    }
}

/// The bar shows in the horizontal layout while it isn't collapsed and the Bookmarks Bar holds a
/// bookmark, itself or in a folder; empty folders alone don't show it.
fn bar_shown(state: &AppState) -> bool {
    state.settings.bookmarks_bar && bar_has_bookmarks(state)
}

fn bar_has_bookmarks(state: &AppState) -> bool {
    fn holds_link(bookmark: &Bookmark) -> bool {
        !bookmark.is_folder() || bookmark.children().iter().any(holds_link)
    }
    state.horizontal_tabs() && state.bookmarks().bar().iter().any(holds_link)
}

/// The toolbar's button that brings back a collapsed bookmarks bar; none while it shows, or
/// with nothing to show.
pub(in crate::ui) fn expand_button(state: &AppState, theme: &Theme) -> Option<El> {
    (!state.settings.bookmarks_bar && bar_has_bookmarks(state)).then(|| {
        bar_toggle(
            "bookmarks-bar-show",
            "Show bookmarks bar",
            &icons::CHEVRON_DOWN,
            theme,
        )
    })
}

fn bar_toggle(key: &str, label: &str, glyph: &'static rune::Icon, theme: &Theme) -> El {
    crate::ui::components::hover_highlight::attach(
        controls::icon_button_style(key, label, glyph, theme::ICON_BUTTON, theme),
        key,
        crate::ui::components::hover_highlight::Group::Controls,
    )
    .help(label)
    .shortcut_hint("cmd+shift+b")
    .on_press_tx(|state: &mut AppState, tx| {
        state.toggle_bookmarks_bar();
        tx.animated();
        tx.motion(theme::SIDEBAR_TOGGLE);
    })
}

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::editor::EDITOR_TITLE;
    use super::*;
    use elaris_core::bookmarks::BookmarkId;
    use elaris_core::settings::TabLayout;

    fn runtime(layout: TabLayout, bookmarked: bool) -> Runtime<AppState> {
        let mut state = AppState::new(Some("https://example.com/"));
        state.set_tab_layout(layout);
        if bookmarked {
            state.bookmark_active_tab();
            state.close_bookmark_editor();
        }
        let mut runtime = Runtime::new(state, super::super::view, (1000.0, 640.0), Env::default());
        runtime.poll = Some(crate::app::poll);
        runtime.frame(0.0);
        runtime
    }

    #[test]
    fn the_bar_shows_in_the_horizontal_layout_once_it_holds_a_bookmark() {
        let empty = runtime(TabLayout::Horizontal, false);
        assert!(empty.presented("bookmarks-bar").is_none());
        let plain_origin = crate::ui::viewport::page_origin(&empty.state);

        let horizontal = runtime(TabLayout::Horizontal, true);
        assert!(horizontal.presented("bookmarks-bar").is_some());
        let id = horizontal.state.bookmarks().bar()[0].id;
        assert!(
            horizontal
                .presented(&format!("bookmark-{}", id.get()))
                .is_some()
        );
        let origin = crate::ui::viewport::page_origin(&horizontal.state);
        assert_eq!(origin[1], plain_origin[1] + theme::BOOKMARKS_BAR_HEIGHT);

        let vertical = runtime(TabLayout::Vertical, true);
        assert!(vertical.presented("bookmarks-bar").is_none());
    }

    #[test]
    fn the_star_opens_the_editor_in_either_layout() {
        for layout in TabLayout::ALL {
            let mut runtime = runtime(layout, false);
            // In the vertical sidebar the address field's buttons show under the pointer.
            runtime.update(|state, _| state.sidebar.pointer_inside = true);
            runtime.frame(0.01);
            let ([x, y, w, h], _) = runtime
                .presented("bookmark-star")
                .expect("the star shows for a web page");
            let (x, y) = (x + w / 2.0, y + h / 2.0);
            runtime.pointer_down(x, y);
            runtime.pointer_up(x, y);
            runtime.frame(0.016);
            runtime.frame(0.5);
            assert!(runtime.state.bookmarks.editor.is_some(), "{layout:?}");
            assert!(runtime.presented(EDITOR_TITLE).is_some(), "{layout:?}");
        }
    }

    #[test]
    fn the_bookmarks_page_lists_the_open_folder() {
        let mut runtime = runtime(TabLayout::Vertical, true);
        let id = runtime.state.bookmarks().bar()[0].id;
        runtime.update(crate::app::open_bookmarks);
        runtime.frame(0.016);
        runtime.frame(0.5);
        assert!(
            runtime
                .presented(&format!("bookmarks-row-{}", id.get()))
                .is_some()
        );
        assert!(
            runtime
                .presented(&format!("bookmarks-folder-{}", BookmarkId::OTHER.get()))
                .is_some()
        );
    }

    fn menu_labels(runtime: &Runtime<AppState>) -> Vec<String> {
        runtime
            .elements()
            .into_iter()
            .filter(|element| {
                element.role == "menuitem"
                    && !element.exiting
                    && element
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("bookmarks-sidebar-menu"))
            })
            .filter_map(|element| element.label)
            .collect()
    }

    #[test]
    fn the_sidebar_star_lists_actions_then_bookmarks_and_opens_folders_within() {
        let mut runtime = runtime(TabLayout::Vertical, true);
        runtime.update(|state, _| {
            let mut bookmarks = state.bookmarks();
            let folder = bookmarks
                .add_folder(BookmarkId::OTHER, "Reading", 0)
                .unwrap();
            bookmarks
                .add_link(folder, "Docs", "https://docs.rs/", 0)
                .unwrap();
        });
        let ([x, y, w, h], _) = runtime
            .presented("bookmarks-button")
            .expect("the vertical sidebar has a bookmarks button");
        let (x, y) = (x + w / 2.0, y + h / 2.0);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.016);
        runtime.frame(0.5);
        let labels = menu_labels(&runtime);
        let title = runtime.state.bookmarks().bar()[0].label().to_owned();
        assert_eq!(title, "example.com");
        assert_eq!(
            labels,
            [
                "Search Bookmarks",
                "Manage Bookmarks",
                "Add Bookmark",
                "Bookmark All Tabs…",
                title.as_str(),
                "Other Bookmarks"
            ]
        );

        runtime.update(|state, _| state.bookmarks.open_folders.push(BookmarkId::OTHER));
        runtime.frame(0.6);
        runtime.frame(1.2);
        assert_eq!(menu_labels(&runtime), ["Bookmarks", "Reading"]);

        let horizontal = super::tests::runtime(TabLayout::Horizontal, true);
        assert!(horizontal.presented("bookmarks-button").is_none());
    }

    #[test]
    fn the_sidebar_menu_slides_its_highlight_to_the_hovered_row() {
        let mut runtime = runtime(TabLayout::Vertical, true);
        runtime.update(|state, _| state.bookmarks.sidebar_menu = true);
        runtime.frame(0.016);
        runtime.frame(0.5);
        let row = runtime
            .elements()
            .into_iter()
            .find(|element| {
                element.role == "menuitem"
                    && element
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("bookmarks-sidebar-menu-item"))
            })
            .and_then(|element| element.key)
            .unwrap();
        let ([x, y, w, h], _) = runtime.presented(&row).unwrap();
        runtime.pointer_move(x + w / 2.0, y + h / 2.0);
        runtime.frame(0.6);
        runtime.frame(0.7);
        assert_eq!(
            runtime.state.context_menu_highlight.as_deref(),
            Some(row.as_str())
        );
    }

    #[test]
    fn the_bar_needs_a_bookmark_and_collapses_to_a_toolbar_button() {
        let mut runtime = runtime(TabLayout::Horizontal, false);
        runtime.update(|state, _| {
            state
                .bookmarks()
                .add_folder(BookmarkId::BAR, "Empty", 0)
                .unwrap();
        });
        runtime.frame(0.016);
        assert!(
            runtime.presented("bookmarks-bar").is_none(),
            "an empty folder alone"
        );

        runtime.update(|state, _| state.bookmark_tab(state.browser.tabs()[0].id(), false));
        runtime.frame(0.032);
        runtime.frame(1.0);
        assert!(runtime.presented("bookmarks-bar").is_some());
        assert!(runtime.presented("bookmarks-bar-show").is_none());

        let ([x, y, w, h], _) = runtime.presented("bookmarks-bar-hide").unwrap();
        let (x, y) = (x + w / 2.0, y + h / 2.0);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.05);
        runtime.frame(2.0);
        assert!(!runtime.state.settings.bookmarks_bar);
        assert!(runtime.presented("bookmarks-bar").is_none());
        assert!(runtime.presented("bookmarks-bar-show").is_some());
        assert_eq!(
            horizontal_chrome_height(&runtime.state),
            theme::HORIZONTAL_CHROME_HEIGHT
        );
    }
}
