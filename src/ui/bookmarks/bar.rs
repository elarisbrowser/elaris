// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::space;
use rune::tok::type_;
use rune::{Align, El, PointerPress, Side, el, icon, row, text};

use crate::app::AppState;
use crate::shortcuts::Shortcut;
use crate::ui::components::animated_icon;
use crate::ui::components::controls;
use crate::ui::components::menu::{Menu, MenuItem};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::bookmarks::{Bookmark, BookmarkId, Bookmarks};
use elaris_core::settings::TabLayout;

use super::{bar_shown, bar_toggle, editor::EDITOR_TITLE};

fn glyph(glyph: &'static rune::Icon, theme: &Theme) -> El {
    icon(glyph, theme::ICON).color(theme.icon).a11y_hidden()
}

/// A bookmark's mark: its saved icon, the favicon of an open tab at its address, a page, or a
/// folder.
pub(in crate::ui) fn mark(state: &AppState, bookmark: &Bookmark, theme: &Theme) -> El {
    if bookmark.is_folder() {
        return glyph(&icons::FOLDER, theme);
    }
    if let Some(pixels) = state.bookmark_icon(bookmark) {
        return el()
            .size(theme::ICON, theme::ICON)
            .pixels(&pixels)
            .a11y_hidden();
    }
    let favicon = state
        .browser
        .tabs()
        .iter()
        .find(|tab| Some(tab.url.as_str()) == bookmark.url())
        .and_then(|tab| tab.favicon.as_ref());
    match favicon {
        Some(pixels) => el()
            .size(theme::ICON, theme::ICON)
            .pixels(pixels)
            .a11y_hidden(),
        None => glyph(&icons::PAGE, theme),
    }
}

/// The bookmarks bar: the Bookmarks Bar's links and folders. A folder opens as a menu, and a
/// subfolder opens within that menu, with a row leading back.
pub(in crate::ui) fn bar(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> Option<El> {
    if !bar_shown(state) {
        return None;
    }
    let bookmarks = state.bookmarks();
    let mut bar = row()
        .key("bookmarks-bar")
        .w_full()
        .h(theme::BOOKMARKS_BAR_HEIGHT)
        .px(theme::BOOKMARKS_BAR_PADDING)
        .gap(theme::BOOKMARKS_BAR_GAP)
        .center_y()
        .clip_soft(0.0)
        .role("toolbar")
        .label("Bookmarks bar");
    for item in bookmarks.bar() {
        let chip = chip(state, item, theme);
        bar = bar.child(if item.is_folder() {
            folder_menu(state, &bookmarks, item.id, chip, theme, overlays)
        } else {
            chip
        });
    }
    drop(bookmarks);
    if let Some((bookmark, point)) = state.bookmarks.bar_menu {
        if let Some(id) = bookmark {
            let is_folder = state.bookmarks().get(id).is_some_and(Bookmark::is_folder);
            let mut menu = crate::ui::components::menu::context_menu(
                "bookmark-bar-context",
                el(),
                Some(point),
            )
            .on_close(|state: &mut AppState| state.bookmarks.bar_menu = None);
            if is_folder {
                menu = menu.item(MenuItem::new("Open All").on_select(
                    move |state: &mut AppState| {
                        state.bookmarks.bar_menu = None;
                        state.open_bookmark_folder(id);
                    },
                ));
            } else {
                menu = menu
                    .item(
                        MenuItem::new("Open").on_select_tx(move |state: &mut AppState, tx| {
                            state.bookmarks.bar_menu = None;
                            state.open_bookmark(id, false, tx);
                        }),
                    )
                    .item(MenuItem::new("Open in New Tab").on_select_tx(
                        move |state: &mut AppState, tx| {
                            state.bookmarks.bar_menu = None;
                            state.open_bookmark(id, true, tx);
                        },
                    ));
            }
            menu = menu
                .separator()
                .item(
                    MenuItem::new("Edit").on_select(move |state: &mut AppState| {
                        state.bookmarks.bar_menu = None;
                        state.edit_bookmark(id, false);
                    }),
                )
                .item(
                    MenuItem::new("Remove").on_select_tx(move |state: &mut AppState, tx| {
                        state.bookmarks.bar_menu = None;
                        state.remove_bookmark_with_toast(id, tx);
                    }),
                );
            menu.build(state, theme, overlays);
        } else {
            crate::ui::components::menu::context_menu("bookmark-bar-context", el(), Some(point))
                .on_close(|state: &mut AppState| state.bookmarks.bar_menu = None)
                .item(MenuItem::new("Bookmark This Page").on_select_tx(
                    |state: &mut AppState, tx| {
                        state.bookmarks.bar_menu = None;
                        state.bookmark_active_tab_with_toast(tx);
                    },
                ))
                .item(
                    MenuItem::new("New Folder").on_select(|state: &mut AppState| {
                        state.bookmarks.bar_menu = None;
                        state.add_bookmark_folder(BookmarkId::BAR);
                    }),
                )
                .separator()
                .item(
                    MenuItem::new("New Tab")
                        .animated(&animated_icon::PLUS)
                        .on_select_tx(|state: &mut AppState, tx| {
                            state.bookmarks.bar_menu = None;
                            crate::app::new_tab(state, tx);
                        }),
                )
                .item(MenuItem::new("Show Tabs Vertically").on_select_tx(
                    |state: &mut AppState, tx| {
                        state.bookmarks.bar_menu = None;
                        state.set_tab_layout(TabLayout::Vertical);
                        tx.animated();
                        tx.motion(theme::SIDEBAR_TOGGLE);
                    },
                ))
                .separator()
                .item(
                    MenuItem::new("Open File…")
                        .animated(&animated_icon::FOLDER)
                        .on_select_tx(|state: &mut AppState, tx| {
                            state.bookmarks.bar_menu = None;
                            crate::shortcuts::open_file(tx);
                        }),
                )
                .item(
                    MenuItem::new("Settings")
                        .animated(&animated_icon::SETTINGS)
                        .on_select_tx(|state: &mut AppState, tx| {
                            state.bookmarks.bar_menu = None;
                            crate::app::open_settings(state, tx);
                        }),
                )
                .build(state, theme, overlays);
        }
    }
    Some(
        bar.child(el().grow(1.0).on_secondary_press(
            |state: &mut AppState, point: PointerPress, _| {
                state.bookmarks.bar_menu = Some((None, point));
                state.bookmarks.open_folders.clear();
                state.dismiss_context_menu();
            },
        ))
        .child(bar_toggle(
            "bookmarks-bar-hide",
            "Hide bookmarks bar",
            &icons::CHEVRON_UP,
            theme,
        )),
    )
}

fn chip(state: &AppState, bookmark: &Bookmark, theme: &Theme) -> El {
    let id = bookmark.id;
    let chip = row()
        .key(format!("bookmark-{}", id.get()))
        .max_w(theme::BOOKMARKS_BAR_ITEM_MAX)
        .h(theme::BOOKMARKS_BAR_ITEM_HEIGHT)
        .px(theme::ROW_PADDING / 2.0)
        .gap(space::S1)
        .center_y()
        .radius(theme::BOOKMARKS_BAR_ITEM_RADIUS)
        .continuous_corners()
        .hover(theme.hover)
        .press(theme.press)
        .focusable(true)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .role("button")
        .label(bookmark.label())
        .help(bookmark.url().unwrap_or(bookmark.label()))
        .child(mark(state, bookmark, theme))
        .child(
            text(bookmark.label().to_owned())
                .font(type_::LABEL)
                .color(theme.text)
                .single_line()
                .truncate()
                .shrink(),
        );
    let chip = chip.on_secondary_press(move |state: &mut AppState, point: PointerPress, _| {
        state.bookmarks.bar_menu = Some((Some(id), point));
        state.bookmarks.open_folders.clear();
        state.dismiss_context_menu();
    });
    if bookmark.is_folder() {
        chip.on_press(move |state: &mut AppState| {
            let open = &mut state.bookmarks.open_folders;
            let was_open = open.first() == Some(&id);
            open.clear();
            if !was_open {
                open.push(id);
            }
        })
    } else {
        chip.on_press_tx(move |state: &mut AppState, tx| state.open_bookmark(id, false, tx))
            .on_middle_press(move |state: &mut AppState, tx| state.open_bookmark(id, true, tx))
    }
}

/// A bar folder's menu, anchored to its chip. It lists the deepest folder opened from it: its
/// links and subfolders, a row back to the folder above, and Open All.
fn folder_menu(
    state: &AppState,
    bookmarks: &Bookmarks,
    root: BookmarkId,
    trigger: El,
    theme: &Theme,
    layer: &mut Vec<El>,
) -> El {
    let open = state.bookmarks.open_folders.first() == Some(&root);
    let mut menu = Menu::new(format!("bookmark-folder-{}", root.get()))
        .trigger(trigger)
        .open(open)
        .placement(Side::Bottom, Align::Start)
        .min_width(theme::BOOKMARKS_MENU_WIDTH)
        .on_close(|state: &mut AppState| state.bookmarks.open_folders.clear());
    let stack = &state.bookmarks.open_folders;
    let shown = stack.last().copied().filter(|_| open);
    if let Some(folder) = shown.and_then(|id| bookmarks.get(id)) {
        let back = (stack.len() > 1).then(|| {
            bookmarks
                .get(stack[stack.len() - 2])
                .map_or_else(String::new, |above| above.label().to_owned())
        });
        menu = folder_items(menu, state, folder, back, theme);
    }
    menu.build(state, theme, layer)
}

/// A folder's links and subfolders as menu rows: a subfolder opens in the same menu, and `back`
/// names the folder a leading row returns to. Open All follows when the folder holds links.
fn folder_items(
    mut menu: Menu,
    state: &AppState,
    folder: &Bookmark,
    back: Option<String>,
    theme: &Theme,
) -> Menu {
    let id = folder.id;
    if let Some(back) = back {
        menu = menu
            .item(
                MenuItem::new(back)
                    .icon(&icons::BACK)
                    .on_select(|state: &mut AppState| {
                        state.bookmarks.open_folders.pop();
                    }),
            )
            .separator();
    }
    if folder.children().is_empty() {
        menu = menu.item(MenuItem::new("Empty").disabled(true));
    }
    for child in folder.children() {
        menu = menu.item(entry(state, child, theme));
    }
    if folder.children().iter().any(|child| !child.is_folder()) {
        menu = menu.separator().item(
            MenuItem::new("Open All in New Tabs")
                .on_select(move |state: &mut AppState| state.open_bookmark_folder(id)),
        );
    }
    menu
}

/// One bookmark as a menu row: a link opens, a folder opens within the menu.
fn entry(state: &AppState, bookmark: &Bookmark, theme: &Theme) -> MenuItem {
    let id = bookmark.id;
    let item = MenuItem::new(bookmark.label().to_owned()).lead(mark(state, bookmark, theme));
    if bookmark.is_folder() {
        item.on_select(move |state: &mut AppState| state.bookmarks.open_folders.push(id))
    } else {
        item.on_select_tx(move |state: &mut AppState, tx| state.open_bookmark(id, false, tx))
    }
}

/// The vertical sidebar's bookmarks button, beside Downloads. Its menu manages and adds
/// bookmarks, and lists the Bookmarks Bar with Other Bookmarks, whose folders open within it.
pub(in crate::ui) fn sidebar_button(state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
    let open = state.bookmarks.sidebar_menu;
    let trigger = crate::ui::components::hover_highlight::attach(
        controls::icon_button_style(
            "bookmarks-button",
            "Bookmarks",
            &icons::STAR,
            theme::ICON_BUTTON,
            theme,
        ),
        "bookmarks-button",
        crate::ui::components::hover_highlight::Group::Controls,
    )
    .help("Bookmarks")
    .on_press(|state: &mut AppState| {
        state.bookmarks.sidebar_menu = !state.bookmarks.sidebar_menu;
        state.bookmarks.open_folders.clear();
    });
    let mut menu = Menu::new("bookmarks-sidebar-menu")
        .trigger(trigger)
        .open(open)
        .placement(Side::Top, Align::Start)
        .min_width(theme::BOOKMARKS_MENU_WIDTH)
        .on_close(|state: &mut AppState| {
            state.bookmarks.sidebar_menu = false;
            state.bookmarks.open_folders.clear();
        });
    if !open {
        return menu.build(state, theme, layer);
    }
    let stack = &state.bookmarks.open_folders;
    if let Some(folder) = stack
        .last()
        .and_then(|id| state.bookmarks().get(*id).cloned())
    {
        let back = if stack.len() > 1 {
            state
                .bookmarks()
                .get(stack[stack.len() - 2])
                .map_or_else(String::new, |above| above.label().to_owned())
        } else {
            "Bookmarks".to_owned()
        };
        return folder_items(menu, state, &folder, Some(back), theme).build(state, theme, layer);
    }
    let has_page = state
        .browser
        .active_tab()
        .is_some_and(|tab| !tab.is_internal() && !tab.url.is_empty() && tab.url != "about:blank");
    let close = |state: &mut AppState| state.bookmarks.sidebar_menu = false;
    menu = menu
        .item(
            MenuItem::new("Search Bookmarks")
                .shortcut_of(Shortcut::SearchBookmarks)
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    crate::app::search_bookmarks(state, tx);
                }),
        )
        .item(
            MenuItem::new("Manage Bookmarks")
                .shortcut_of(Shortcut::OpenBookmarks)
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    crate::app::open_bookmarks(state, tx);
                }),
        )
        .separator()
        .item(
            MenuItem::new("Add Bookmark")
                .shortcut_of(Shortcut::BookmarkPage)
                .disabled(!has_page)
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    state.bookmark_active_tab_with_toast(tx);
                    tx.focus(EDITOR_TITLE, false);
                }),
        )
        .item(
            MenuItem::new("Bookmark All Tabs…")
                .shortcut_of(Shortcut::BookmarkAllTabs)
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    state.bookmark_all_tabs(tx);
                }),
        );
    let bookmarks = state.bookmarks();
    let [bar, other] = bookmarks.roots();
    if !bar.children().is_empty() || !other.children().is_empty() {
        menu = menu.separator();
    }
    for item in bar.children() {
        menu = menu.item(entry(state, item, theme));
    }
    if !other.children().is_empty() {
        menu = menu.item(entry(state, other, theme));
    }
    drop(bookmarks);
    menu.build(state, theme, layer)
}
