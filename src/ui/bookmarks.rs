// SPDX-License-Identifier: GPL-3.0-only

//! Bookmarks in the window's chrome: the star in the address field with its editor, and the
//! bookmarks bar the horizontal layout shows under its toolbar.

use rune::tok::{space, type_};
use rune::{Align, El, Side, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::menu::{Menu, MenuItem, Popover};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::bookmarks::{Bookmark, BookmarkId};

const EDITOR_TITLE: &str = "bookmark-editor-title";

/// The horizontal layout's chrome above the page: its tab strip and toolbar, and the bookmarks
/// bar while it shows.
pub(crate) fn horizontal_chrome_height(state: &AppState) -> f32 {
    theme::HORIZONTAL_CHROME_HEIGHT
        + if bar_shown(state) {
            theme::BOOKMARKS_BAR_HEIGHT
        } else {
            0.0
        }
}

/// The bar shows in the horizontal layout once the Bookmarks Bar holds something.
fn bar_shown(state: &AppState) -> bool {
    state.horizontal_tabs() && !state.bookmarks().bar().is_empty()
}

/// The star in the address field, drawn like the field's other buttons and filled when the active
/// page is bookmarked. It opens the editor.
pub(in crate::ui) fn star(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> Option<El> {
    let tab = state
        .browser
        .active_tab()
        .filter(|tab| !tab.is_internal() && !tab.url.is_empty() && tab.url != "about:blank")?;
    let bookmarked = state.bookmarks().find_url(&tab.url).is_some();
    let frame = controls::icon_button_frame(
        "bookmark-star",
        if bookmarked {
            "Edit bookmark"
        } else {
            "Bookmark this page"
        },
        theme::ICON_BUTTON,
        theme,
    )
    .help(if bookmarked {
        "Edit bookmark"
    } else {
        "Bookmark this page"
    })
    .shortcut_hint("cmd+d")
    .child(
        icon(
            if bookmarked {
                &icons::STAR_FILLED
            } else {
                &icons::STAR
            },
            theme::ICON,
        )
        .color(theme.icon),
    );
    let trigger = crate::ui::components::hover_highlight::attach(
        frame,
        "bookmark-star",
        crate::ui::components::hover_highlight::Group::Controls,
    )
    .on_press_tx(|state: &mut AppState, tx| {
        if state
            .bookmarks
            .editor
            .as_ref()
            .is_some_and(|editor| !editor.on_page)
        {
            state.close_bookmark_editor();
        } else {
            state.bookmark_active_tab();
            tx.focus(EDITOR_TITLE, false);
        }
    });
    let open = state
        .bookmarks
        .editor
        .as_ref()
        .is_some_and(|editor| !editor.on_page);
    let mut popover = Popover::new("bookmark-editor")
        .trigger(trigger)
        .title("Bookmark")
        .open(open)
        .placement(Side::Bottom, Align::End)
        .on_close(|state: &mut AppState| state.save_bookmark_editor());
    if open {
        popover = popover.child(editor(state, theme, overlays));
    }
    Some(popover.build(state, theme, overlays))
}

/// The editor's fields and buttons: the name, a link's address, its folder, Remove and Done.
pub(in crate::ui) fn editor(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let Some(editor) = &state.bookmarks.editor else {
        return el();
    };
    let kit = theme.kit(theme.overlay_panel);
    let id = editor.id;
    let is_link = state
        .bookmarks()
        .get(id)
        .is_some_and(|bookmark| bookmark.url().is_some());
    let mut body = col()
        .w(theme::BOOKMARK_EDITOR_WIDTH)
        .gap(space::S3)
        .child(labelled(
            "Name",
            field(
                &editor.title,
                EDITOR_TITLE,
                "Name",
                |state: &mut AppState| {
                    &mut state
                        .bookmarks
                        .editor
                        .as_mut()
                        .expect("an open editor")
                        .title
                },
                theme,
            ),
            theme,
        ));
    if is_link {
        body = body.child(labelled(
            "Address",
            field(
                &editor.url,
                "bookmark-editor-url",
                "Address",
                |state: &mut AppState| {
                    &mut state.bookmarks.editor.as_mut().expect("an open editor").url
                },
                theme,
            ),
            theme,
        ));
    }
    body.child(labelled(
        "Folder",
        folder_picker(state, editor.folder, id, theme, overlays),
        theme,
    ))
    .children(state.bookmarks.error.as_ref().map(|error| {
        text(error.clone())
            .font(type_::LABEL)
            .color(theme.text)
            .wrap()
            .role("alert")
    }))
    .child(
        row()
            .w_full()
            .gap(space::S2)
            .justify_end()
            .child(
                Button::new("Remove")
                    .variant(ButtonVariant::Ghost)
                    .on_press(move |state: &mut AppState| state.remove_bookmark(id))
                    .build(&kit),
            )
            .child(
                Button::new("Done")
                    .variant(ButtonVariant::Submit)
                    .on_press(|state: &mut AppState| state.save_bookmark_editor())
                    .build(&kit),
            ),
    )
}

fn labelled(label: &str, control: El, theme: &Theme) -> El {
    col()
        .w_full()
        .gap(space::S1)
        .child(
            text(label.to_owned())
                .font(type_::LABEL)
                .color(theme.text_muted)
                .a11y_hidden(),
        )
        .child(control)
}

fn field(
    input: &rune::TextEdit,
    key: &'static str,
    label: &'static str,
    edit: fn(&mut AppState) -> &mut rune::TextEdit,
    theme: &Theme,
) -> El {
    row()
        .w_full()
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .px(theme::FIELD_PADDING)
        .radius(theme::FIELD_RADIUS)
        .fill(theme.address_field)
        .border(1.0, theme.control_border)
        .focus_within_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .child(
            text_input(input, type_::BODY)
                .key(key)
                .single_line()
                .grow(1.0)
                .min_w(0.0)
                .color(theme.text)
                .caret(theme.text, theme.selection)
                .no_focus_ring()
                .label(label)
                .on_edit(edit)
                .on_input_key(|state: &mut AppState, event, _| {
                    if event.key == rune::Key::Enter {
                        state.save_bookmark_editor();
                        true
                    } else {
                        false
                    }
                }),
        )
}

/// A field naming the chosen folder, whose menu lists every folder, indented by depth. A folder
/// being edited can't go inside itself, so it and its subfolders are left out.
fn folder_picker(
    state: &AppState,
    current: BookmarkId,
    editing: BookmarkId,
    theme: &Theme,
    overlays: &mut Vec<El>,
) -> El {
    let bookmarks = state.bookmarks();
    let open = state
        .bookmarks
        .editor
        .as_ref()
        .is_some_and(|editor| editor.folder_menu_open);
    let name = bookmarks
        .get(current)
        .map_or_else(String::new, |folder| folder.label().to_owned());
    let field = row()
        .key("bookmark-editor-folder-field")
        .w_full()
        .h(theme::FIELD_HEIGHT)
        .px(theme::FIELD_PADDING)
        .gap(theme::MENU_ICON_GAP)
        .center_y()
        .radius(theme::FIELD_RADIUS)
        .continuous_corners()
        .border(1.0, theme.control_border)
        .hover(theme.hover)
        .press(theme.press)
        .focusable(true)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .role("combobox")
        .label("Folder")
        .child(glyph(&icons::FOLDER, theme))
        .child(
            text(name)
                .font(type_::BODY)
                .color(theme.text)
                .grow(1.0)
                .min_w(0.0)
                .single_line()
                .truncate()
                .a11y_hidden(),
        )
        .child(glyph(&icons::CHEVRON_DOWN, theme))
        .on_press(|state: &mut AppState| {
            if let Some(editor) = &mut state.bookmarks.editor {
                editor.folder_menu_open = !editor.folder_menu_open;
            }
        });
    let excluded: Vec<BookmarkId> = bookmarks
        .get(editing)
        .filter(|bookmark| bookmark.is_folder())
        .map(|_| {
            bookmarks
                .folders()
                .into_iter()
                .filter(|(_, folder)| {
                    folder.id == editing
                        || bookmarks
                            .path(folder.id)
                            .iter()
                            .any(|ancestor| ancestor.id == editing)
                })
                .map(|(_, folder)| folder.id)
                .collect()
        })
        .unwrap_or_default();
    let mut menu = Menu::new("bookmark-editor-folder")
        .trigger(field)
        .open(open)
        .min_width(theme::BOOKMARK_EDITOR_WIDTH)
        .on_close(|state: &mut AppState| {
            if let Some(editor) = &mut state.bookmarks.editor {
                editor.folder_menu_open = false;
            }
        });
    for (depth, folder) in bookmarks.folders() {
        if excluded.contains(&folder.id) {
            continue;
        }
        let id = folder.id;
        let indent = "    ".repeat(depth);
        menu = menu.item(
            MenuItem::new(format!("{indent}{}", folder.label()))
                .checked(id == current)
                .on_select(move |state: &mut AppState| {
                    if let Some(editor) = &mut state.bookmarks.editor {
                        editor.folder = id;
                        editor.folder_menu_open = false;
                    }
                }),
        );
    }
    drop(bookmarks);
    menu.build(state, theme, overlays)
}

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
    let items: Vec<Bookmark> = state.bookmarks().bar().to_vec();
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
    for item in &items {
        let chip = chip(state, item, theme);
        bar = bar.child(if item.is_folder() {
            folder_menu(state, item.id, chip, theme, overlays)
        } else {
            chip
        });
    }
    Some(bar)
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
        .radius(theme::ROW_RADIUS)
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
    if let Some(folder) = shown.and_then(|id| state.bookmarks().get(id).cloned()) {
        let back = (stack.len() > 1).then(|| {
            state
                .bookmarks()
                .get(stack[stack.len() - 2])
                .map_or_else(String::new, |above| above.label().to_owned())
        });
        menu = folder_items(menu, state, &folder, back, theme);
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

/// The keys for a shortcut as menus show them on this platform.
fn keys(macos: &'static str, elsewhere: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        macos
    } else {
        elsewhere
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
            MenuItem::new("Manage Bookmarks")
                .shortcut(keys("⌥⌘B", "Ctrl+Alt+B"))
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    crate::app::open_bookmarks(state, tx);
                }),
        )
        .separator()
        .item(
            MenuItem::new("Add Bookmark")
                .shortcut(keys("⌘D", "Ctrl+D"))
                .disabled(!has_page)
                .on_select_tx(move |state: &mut AppState, tx| {
                    close(state);
                    state.bookmark_active_tab();
                    tx.focus(EDITOR_TITLE, false);
                }),
        )
        .item(
            MenuItem::new("Bookmark All Tabs…")
                .shortcut(keys("⇧⌘D", "Ctrl+Shift+D"))
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

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::*;
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
        let title = runtime
            .state
            .browser
            .active_tab()
            .unwrap()
            .label()
            .to_owned();
        assert_eq!(
            labels,
            [
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
}
