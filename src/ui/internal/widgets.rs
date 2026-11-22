// SPDX-License-Identifier: GPL-3.0-only

//! Building blocks shared by native pages: categories, sections, grouped rows and inline states.
//!
//! Pages compose these instead of styling their own surfaces, so every page keeps the same
//! edges, spacing and type. All measurements come from the `PAGE_*` tokens in the theme.

use rune::tok::{space, type_};
use rune::{Align, El, Icon, col, el, icon, row, text};
use rune_kit::Kit;

use crate::app::AppState;
use crate::ui::theme::{self, Theme};

use super::PageContext;

/// One destination in a page's [`nav`]: a category of sections.
pub(super) struct NavItem {
    /// Unique within the page; stored in [`AppState::page_category`] while selected.
    pub(super) key: &'static str,
    pub(super) title: &'static str,
    pub(super) icon: &'static Icon,
}

/// Category navigation for [`PageLayout::nav`](super::layout::PageLayout::nav): a vertical
/// list beside the content in wide panes and a scrolling row under the title in narrow ones.
/// Choosing an item shows that category and returns `scroll_key` to the top.
pub(super) fn nav(
    label: &str,
    scroll_key: &'static str,
    items: impl ExactSizeIterator<Item = &'static NavItem>,
    selected: &str,
    cx: &PageContext,
) -> El {
    let count = items.len();
    let rows = items.enumerate().map(|(index, item)| {
        let current = item.key == selected;
        let key = item.key;
        row()
            .key(format!("{key}-nav"))
            .h(theme::PAGE_NAV_ROW_HEIGHT)
            .px(theme::PAGE_NAV_ROW_PADDING)
            .gap(space::S2 + space::S1 / 2.0)
            .center_y()
            .radius(theme::PAGE_GROUP_RADIUS - space::S1)
            .role("button")
            .label(item.title)
            .checked(current)
            .position_in_set(index + 1, count)
            .focusable(true)
            .focus_ring(theme::FOCUS_WIDTH, 0.0, cx.theme.focus)
            .fill(if current {
                cx.theme.settings_surface
            } else {
                rune::Color::TRANSPARENT
            })
            .hover(cx.theme.hover)
            .press(cx.theme.press)
            .on_press_tx(move |state: &mut AppState, tx| {
                state.page_category = Some(key);
                tx.scroll_to_instant(scroll_key, 0.0, 0.0);
            })
            .child(
                icon(item.icon, theme::ICON)
                    .color(if current {
                        cx.theme.accent
                    } else {
                        cx.theme.icon
                    })
                    .a11y_hidden(),
            )
            .child(
                text(item.title)
                    // One style for every state, so selecting an item never resizes its label.
                    .font(type_::BODY)
                    .color(if current {
                        cx.theme.text
                    } else {
                        cx.theme.text_muted
                    })
                    .single_line()
                    .a11y_hidden(),
            )
    });
    let list = if cx.side_nav {
        col().w_full().gap(space::S1 / 2.0)
    } else {
        row().w_full().gap(space::S1).scroll_x()
    };
    list.key("page-nav").label(label).children(rows)
}

/// The heading and sections of the category a page's [`nav`] selected.
pub(super) fn category(key: &'static str, title: &str, sections: Vec<El>, theme: &Theme) -> El {
    col()
        .key(key)
        .w_full()
        .gap(theme::PAGE_SECTION_GAP)
        .child(
            text(title)
                .font(theme::PAGE_CATEGORY_TITLE)
                .color(theme.text)
                .wrap()
                .role("heading")
                .px(theme::PAGE_GROUP_PADDING),
        )
        .children(sections)
}

/// A titled block of a native page: a muted heading above its grouped content.
pub(super) fn section(key: impl Into<String>, title: &str, theme: &Theme, content: El) -> El {
    col()
        .key(key)
        .w_full()
        .gap(theme::PAGE_SECTION_TITLE_GAP)
        .child(
            text(title)
                .font(theme::PAGE_SECTION_TITLE)
                .color(theme.text_muted)
                .role("heading")
                .px(theme::PAGE_GROUP_PADDING),
        )
        .child(content)
}

/// The rounded surface that groups a native page's rows.
pub(super) fn group(theme: &Theme) -> El {
    col()
        .w_full()
        .fill(theme.settings_surface)
        .continuous_corners()
        .clip_rounded(theme::PAGE_GROUP_RADIUS)
}

/// A [`group`] of padded rows divided by inset hairlines.
pub(super) fn rows(theme: &Theme, rows: impl IntoIterator<Item = El>) -> El {
    let mut group = group(theme);
    for (index, content) in rows.into_iter().enumerate() {
        if index > 0 {
            group = group.child(separator(theme));
        }
        group = group.child(
            col()
                .w_full()
                .min_h(theme::PAGE_ROW_MIN_HEIGHT)
                .justify_center()
                .px(theme::PAGE_GROUP_PADDING)
                .py(theme::PAGE_ROW_PADDING)
                .child(content),
        );
    }
    group
}

/// A hairline between rows of a [`group`], inset to line up with the rows' content.
pub(super) fn separator(theme: &Theme) -> El {
    el().w_full()
        .h(1.0)
        .px(theme::PAGE_GROUP_PADDING)
        .child(el().w_full().h_full().fill(theme.separator))
}

/// A labelled control: title and wrapping description, with the control trailing.
pub(super) fn setting(cx: &PageContext, title: &str, description: &str, control: El) -> El {
    let detail = text(description)
        .font(type_::BODY)
        .color(cx.theme.text_muted)
        .wrap();
    setting_with(cx, title, detail, control)
}

/// A [`setting`] whose secondary line is custom, such as a truncated path.
///
/// In a compact pane the control drops under the copy, on the same leading edge.
pub(super) fn setting_with(cx: &PageContext, title: &str, detail: El, control: El) -> El {
    let copy = col()
        .key("setting-copy")
        .grow(1.0)
        .min_w(0.0)
        .gap(theme::PAGE_ROW_COPY_GAP)
        .child(row_title(title, cx.theme))
        .child(detail);
    if cx.compact {
        col()
            .w_full()
            .gap(theme::PAGE_ROW_PADDING)
            .child(copy)
            .child(control)
    } else {
        row()
            .w_full()
            .gap(theme::PAGE_ROW_GAP)
            .justify_between()
            .center_y()
            .child(copy)
            .child(control)
    }
}

pub(super) fn row_title(title: &str, theme: &Theme) -> El {
    text(title).font(theme::PAGE_ROW_TITLE).color(theme.text)
}

/// Buttons laid side by side as one trailing control.
pub(super) fn actions(buttons: impl IntoIterator<Item = El>) -> El {
    row().gap(space::S2).center_y().children(buttons)
}

/// A centred inline state inside a page's content column: empty, unavailable or unknown.
/// Append a button with `.child(..)` to offer a way forward.
pub(super) fn message(glyph: &'static Icon, title: &str, detail: &str, theme: &Theme) -> El {
    group(theme)
        .items_center()
        .gap(space::S2)
        .px(theme::PAGE_GROUP_PADDING)
        .py(theme::PAGE_SECTION_GAP)
        .child(
            row()
                .size(theme::PAGE_HEADER_ICON, theme::PAGE_HEADER_ICON)
                .center()
                .child(icon(glyph, theme::ICON).color(theme.icon).a11y_hidden()),
        )
        .child(
            text(title)
                .font(theme::PAGE_SECTION_TITLE)
                .color(theme.text)
                .align(Align::Center)
                .w_full()
                .wrap()
                .role("heading"),
        )
        .child(
            text(detail)
                .font(type_::BODY)
                .color(theme.text_muted)
                .align(Align::Center)
                .w_full()
                .wrap(),
        )
}

/// Rune Kit's standard switch draws its checked track with the strong icon token; page
/// switches use the accent instead.
pub(super) fn switch_kit(kit: &Kit) -> Kit {
    let mut kit = kit.clone();
    kit.c.std_icon_strong = kit.c.bg_accent;
    kit
}
