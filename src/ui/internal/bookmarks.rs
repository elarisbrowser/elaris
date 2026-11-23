// SPDX-License-Identifier: GPL-3.0-only

//! The bookmarks page: the folder tree beside the open folder's bookmarks, a search over every
//! link, and editing in a dialog.

use rune::tok::{space, type_};
use rune::{El, Key, col, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant, Dialog};

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::menu::{Menu, MenuItem};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use crate::ui::{BOOKMARKS_SCROLL, BOOKMARKS_SEARCH};
use elaris_core::bookmarks::{Bookmark, BookmarkId, Bookmarks};

use super::layout::PageLayout;
use super::widgets;
use super::{PageAction, PageContext};

const SEARCH_WIDTH: f32 = 420.0;

pub(super) const ACTIONS: &[PageAction] = &[];

pub(super) fn view(cx: &mut PageContext) -> PageLayout {
    let (state, theme) = (cx.state, cx.theme);
    let kit = cx.kit.clone();
    if state
        .bookmarks
        .editor
        .as_ref()
        .is_some_and(|editor| editor.on_page)
    {
        edit_dialog(state, theme, cx.overlays);
    }
    let bookmarks = state.bookmarks().clone();
    let page = &state.bookmarks.page;
    let folder = bookmarks
        .get(page.folder)
        .filter(|folder| folder.is_folder())
        .unwrap_or(bookmarks.roots()[0]);
    let query = page.search.text.trim().to_owned();
    let new_folder_in = folder.id;
    let toolbar = row()
        .key("bookmarks-header")
        .w_full()
        .gap(theme::PAGE_GROUP_PADDING)
        .center_y()
        .justify_between()
        .child(search(state, theme))
        .child(
            Button::new("New Folder")
                .variant(ButtonVariant::Ghost)
                .on_press(move |state: &mut AppState| state.add_bookmark_folder(new_folder_in))
                .build(&kit),
        );
    let mut content = col()
        .key("bookmarks-content")
        .w_full()
        .gap(theme::PAGE_SECTION_GAP);
    if let Some(error) = &state.bookmarks.error {
        content = content.child(
            text(error.clone())
                .font(type_::BODY)
                .color(theme.text)
                .wrap()
                .role("alert"),
        );
    }
    let (title, items): (String, Vec<&Bookmark>) = if query.is_empty() {
        (
            bookmarks
                .path(folder.id)
                .iter()
                .chain(std::iter::once(&folder))
                .map(|folder| folder.label())
                .collect::<Vec<_>>()
                .join(" › "),
            folder.children().iter().collect(),
        )
    } else {
        ("Search results".to_owned(), bookmarks.search(&query))
    };
    if items.is_empty() {
        content = content.child(if query.is_empty() {
            widgets::message(
                &icons::STAR,
                "No bookmarks here",
                "Bookmark a page with the star in the address field, or press Command-D.",
                theme,
            )
        } else {
            widgets::message(
                &icons::SEARCH,
                "No matching bookmarks",
                "Try another search.",
                theme,
            )
        });
    } else {
        let rows: Vec<El> = items
            .iter()
            .map(|item| {
                bookmark_row(
                    state,
                    &bookmarks,
                    item,
                    !query.is_empty(),
                    theme,
                    cx.overlays,
                )
            })
            .collect();
        content = content.child(widgets::section(
            "bookmarks-list",
            &title,
            theme,
            widgets::rows(theme, rows),
        ));
    }
    PageLayout::new(BOOKMARKS_SCROLL, &icons::STAR, content)
        .toolbar(toolbar)
        .nav(folder_tree(&bookmarks, folder.id, theme))
}

fn search(state: &AppState, theme: &Theme) -> El {
    let page = &state.bookmarks.page;
    let field = text_input(&page.search, type_::BODY)
        .key(BOOKMARKS_SEARCH)
        .single_line()
        .grow(1.0)
        .min_w(0.0)
        .color(theme.text)
        .caret(theme.text, theme.selection)
        .no_focus_ring()
        .placeholder("Search bookmarks", theme.text_muted)
        .label("Search bookmarks")
        .on_edit(|state: &mut AppState| &mut state.bookmarks.page.search)
        .on_input_key(|_: &mut AppState, event, tx| {
            if event.key == Key::Escape {
                tx.focus(crate::ui::VIEWPORT, false);
                true
            } else {
                false
            }
        });
    let clear = (!page.search.text.is_empty()).then(|| {
        controls::icon_button(
            "bookmarks-search-clear",
            "Clear search",
            &icons::CLOSE,
            theme::CLOSE_BUTTON,
            theme,
        )
        .focusable(true)
        .on_press(|state: &mut AppState| state.bookmarks.page.search = Default::default())
    });
    row()
        .w_full()
        .max_w(SEARCH_WIDTH)
        .shrink()
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .px(theme::FIELD_PADDING)
        .gap(space::S2)
        .radius(theme::FIELD_RADIUS)
        .fill(theme.address_field)
        .border(1.0, theme.control_border)
        .focus_within_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .child(
            icon(&icons::SEARCH, theme::ICON)
                .color(theme.icon)
                .a11y_hidden(),
        )
        .child(field)
        .children(clear)
}

/// Every folder, indented by depth, the open one highlighted.
fn folder_tree(bookmarks: &Bookmarks, open: BookmarkId, theme: &Theme) -> El {
    let mut tree = col()
        .key("bookmarks-folders")
        .w_full()
        .gap(theme::PAGE_NAV_GAP)
        .role("tree")
        .label("Bookmark folders");
    for (depth, folder) in bookmarks.folders() {
        let id = folder.id;
        let selected = id == open;
        let mut entry = row()
            .key(format!("bookmarks-folder-{}", id.get()))
            .w_full()
            .h(theme::PAGE_NAV_ROW_HEIGHT)
            .pl(theme::PAGE_NAV_ROW_PADDING + depth as f32 * theme::BOOKMARKS_TREE_INDENT)
            .pr(theme::PAGE_NAV_ROW_PADDING)
            .gap(space::S2)
            .center_y()
            .radius(theme::PAGE_GROUP_RADIUS - theme::PAGE_NAV_INSET)
            .continuous_corners()
            .role("treeitem")
            .label(folder.label())
            .checked(selected)
            .focusable(true)
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .hover(theme.hover)
            .on_press(move |state: &mut AppState| {
                state.bookmarks.page.folder = id;
                state.bookmarks.page.search = Default::default();
            })
            .child(
                icon(&icons::FOLDER, theme::ICON)
                    .color(theme.icon)
                    .a11y_hidden(),
            )
            .child(
                text(folder.label().to_owned())
                    .font(type_::BODY)
                    .color(theme.text)
                    .single_line()
                    .truncate()
                    .grow(1.0)
                    .min_w(0.0),
            );
        if selected {
            entry = entry.fill(theme.tab_active);
        }
        tree = tree.child(entry);
    }
    tree
}

/// One bookmark: a link opens on press, a folder opens in the list. Its menu edits, moves and
/// removes it.
fn bookmark_row(
    state: &AppState,
    bookmarks: &Bookmarks,
    bookmark: &Bookmark,
    searching: bool,
    theme: &Theme,
    overlays: &mut Vec<El>,
) -> El {
    let id = bookmark.id;
    let detail = match bookmark.url() {
        Some(url) if searching => {
            bookmarks
                .path(id)
                .iter()
                .map(|folder| folder.label())
                .collect::<Vec<_>>()
                .join(" › ")
                + " · "
                + url
        }
        Some(url) => url.to_owned(),
        None => {
            let count = bookmark.children().len();
            if count == 1 {
                "1 item".to_owned()
            } else {
                format!("{count} items")
            }
        }
    };
    let name = row()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::S2)
        .center_y()
        .child(crate::ui::bookmarks::mark(state, bookmark, theme))
        .child(
            text(bookmark.label().to_owned())
                .font(theme::PAGE_ROW_TITLE)
                .color(theme.text)
                .single_line()
                .truncate()
                .shrink(),
        )
        .child(
            text(detail)
                .font(type_::BODY)
                .color(theme.text_muted)
                .single_line()
                .truncate()
                .shrink(),
        );
    let folder = bookmark.is_folder();
    let row = row()
        .key(format!("bookmarks-row-{}", id.get()))
        .w_full()
        .h(theme::TASK_MANAGER_ROW_HEIGHT)
        .center_y()
        .gap(space::S2)
        .hover(theme.hover)
        .focusable(true)
        .focus_ring(theme::FOCUS_WIDTH, -1.0, theme.focus)
        .role("button")
        .label(bookmark.label())
        .child(name)
        .child(row_menu(state, bookmarks, bookmark, theme, overlays));
    if folder {
        row.on_press(move |state: &mut AppState| {
            state.bookmarks.page.folder = id;
            state.bookmarks.page.search = Default::default();
        })
    } else {
        row.on_press_tx(move |state: &mut AppState, tx| state.open_bookmark(id, false, tx))
            .on_middle_press(move |state: &mut AppState, tx| state.open_bookmark(id, true, tx))
    }
}

/// A row's More menu: open, edit, add to Favourites, move up or down, and delete.
fn row_menu(
    state: &AppState,
    bookmarks: &Bookmarks,
    bookmark: &Bookmark,
    theme: &Theme,
    overlays: &mut Vec<El>,
) -> El {
    let id = bookmark.id;
    let open = state.bookmarks.row_menu == Some(id);
    let trigger = controls::icon_button(
        &format!("bookmarks-more-{}", id.get()),
        &format!("More for {}", bookmark.label()),
        &icons::MORE,
        theme::CLOSE_BUTTON,
        theme,
    )
    .focusable(true)
    .on_press(move |state: &mut AppState| {
        state.bookmarks.row_menu = (state.bookmarks.row_menu != Some(id)).then_some(id);
    });
    let mut menu = Menu::new(format!("bookmarks-menu-{}", id.get()))
        .trigger(trigger)
        .open(open)
        .on_close(|state: &mut AppState| state.bookmarks.row_menu = None);
    if !open {
        return menu.build(state, theme, overlays);
    }
    let close = |state: &mut AppState| state.bookmarks.row_menu = None;
    if bookmark.is_folder() {
        menu = menu.item(MenuItem::new("Open All in New Tabs").on_select(
            move |state: &mut AppState| {
                close(state);
                state.open_bookmark_folder(id);
            },
        ));
    } else {
        menu = menu
            .item(
                MenuItem::new("Open in New Tab")
                    .icon(&icons::OPEN_TAB)
                    .on_select_tx(move |state: &mut AppState, tx| {
                        close(state);
                        state.open_bookmark(id, true, tx);
                    }),
            )
            .item(
                MenuItem::new("Add to Favourites").on_select(move |state: &mut AppState| {
                    close(state);
                    state.add_bookmark_to_favourites(id);
                }),
            );
    }
    let (parent, index) = bookmarks.parent(id).unwrap_or((BookmarkId::BAR, 0));
    let count = bookmarks
        .get(parent)
        .map_or(0, |folder| folder.children().len());
    menu = menu
        .separator()
        .item(
            MenuItem::new("Edit…").on_select(move |state: &mut AppState| {
                close(state);
                state.edit_bookmark(id, true);
            }),
        )
        .item(MenuItem::new("Move Up").disabled(index == 0).on_select(
            move |state: &mut AppState| {
                close(state);
                state.move_bookmark(id, parent, index.saturating_sub(1));
            },
        ))
        .item(
            MenuItem::new("Move Down")
                .disabled(index + 1 >= count)
                .on_select(move |state: &mut AppState| {
                    close(state);
                    state.move_bookmark(id, parent, index + 1);
                }),
        )
        .separator()
        .item(
            MenuItem::new("Delete")
                .animated(&crate::ui::components::animated_icon::CLOSE)
                .on_select(move |state: &mut AppState| {
                    close(state);
                    state.remove_bookmark(id);
                }),
        );
    menu.build(state, theme, overlays)
}

fn edit_dialog(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) {
    let kit = theme.kit(theme.overlay_panel);
    let mut body_layer = Vec::new();
    let body = crate::ui::bookmarks::editor(state, theme, &mut body_layer);
    Dialog::new("bookmarks-edit", "Edit Bookmark")
        .open(true)
        .fit()
        .on_close(|state: &mut AppState| state.close_bookmark_editor())
        .child(col().px(space::S3).pb(space::S3).child(body))
        .build(&kit, overlays);
    overlays.extend(body_layer);
}
