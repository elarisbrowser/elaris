// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::{space, type_};
use rune::{Align, El, Side, col, el, icon, row, text, text_input};
use rune_kit::{Button, ButtonVariant};

use crate::app::AppState;
use crate::ui::components::controls;
use crate::ui::components::menu::{Menu, MenuItem, Popover};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::bookmarks::BookmarkId;

pub(super) const EDITOR_TITLE: &str = "bookmark-editor-title";

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
