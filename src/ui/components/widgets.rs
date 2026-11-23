// SPDX-License-Identifier: GPL-3.0-only

//! Building blocks shared by native pages: categories, sections, grouped rows and inline states.
//!
//! Pages compose these instead of styling their own surfaces, so every page keeps the same
//! edges, spacing and type. All measurements come from the `PAGE_*` tokens in the theme.

use rune::tok::{space, type_};
use rune::{Align, El, GrowthSpec, Icon, Spring, col, el, icon, row, text};
use rune_kit::Kit;

use crate::app::AppState;
use crate::ui::theme::{self, Theme};

use crate::ui::components::rule::hairline;
use crate::ui::internal::PageContext;

/// One destination in a page's [`nav`]: a category of sections.
pub(crate) struct NavItem {
    /// Unique within the page; stored in [`AppState::page_category`] while selected.
    pub(crate) key: &'static str,
    pub(crate) title: &'static str,
    /// The label in the narrow, equal-width row under the title.
    pub(crate) short: &'static str,
    pub(crate) icon: &'static Icon,
}

/// Category navigation for `PageLayout::nav`, in a container
/// like the omnibox and tab switcher: a raised highlight slides to the selected item and a
/// hover highlight glides between items under the pointer. It is a vertical list beside the
/// content in wide panes and an equal-width row under the title in narrow ones. Choosing an
/// item shows that category and returns `scroll_key` to the top.
pub(crate) fn nav(
    label: &str,
    scroll_key: &'static str,
    items: &[&'static NavItem],
    selected: &str,
    cx: &PageContext,
) -> El {
    let theme = cx.theme;
    let count = items.len();
    let current = items.iter().position(|item| item.key == selected);
    let motion = if cx.reduced_motion {
        Spring::NONE
    } else {
        theme::HIGHLIGHT_MOTION
    };
    let vertical = cx.side_nav;
    let height = theme::PAGE_NAV_ROW_HEIGHT;
    let pitch = height + theme::PAGE_NAV_GAP;
    // Highlights sit under the rows: in a column they slide by offset, in the row they share
    // the target item's grid cell as Rune Kit's segmented control does.
    let highlight = |key: &str, index: usize| {
        let base = el()
            .key(key)
            .h(height)
            .radius(theme::PAGE_GROUP_RADIUS - theme::PAGE_NAV_INSET)
            .continuous_corners()
            .inert()
            .a11y_hidden();
        if vertical {
            base.pin(Some(0.0), Some(0.0), Some(0.0), None)
                .offset(0.0, index as f32 * pitch)
                .offset_motion(motion)
        } else {
            base.grid_cell(index).token_motion(motion)
        }
    };
    // Over the selected item the hover highlight fades out in place rather than unmounting, so
    // the next item it reaches still glides from where it was.
    let hover = cx
        .state
        .page_nav_hover
        .filter(|index| *index < count)
        .map(|index| {
            highlight("page-nav-hover", index)
                .fill(theme.hover)
                .opacity(if Some(index) == current { 0.0 } else { 1.0 })
        });
    let selection = current.map(|index| {
        highlight("page-nav-selection", index)
            .fill(theme.tab_active)
            .shadow(
                theme::ACTIVE_SHADOW_Y,
                theme::ACTIVE_SHADOW_BLUR,
                theme.tab_active_shadow,
            )
    });
    let rows = items.iter().enumerate().map(|(index, item)| {
        let on = Some(index) == current;
        let key = item.key;
        let mut entry = row()
            .key(format!("{key}-nav"))
            .h(height)
            .min_w(0.0)
            .px(theme::PAGE_NAV_ROW_PADDING)
            .gap(space::S2 + space::S1 / 2.0)
            .center_y()
            .radius(theme::PAGE_GROUP_RADIUS - theme::PAGE_NAV_INSET)
            .role("button")
            .label(item.title)
            .checked(on)
            .position_in_set(index + 1, count)
            .focusable(true)
            .focus_ring(theme::FOCUS_WIDTH, 0.0, theme.focus)
            // Only entering moves the highlight; leaving the whole list clears it, so crossing
            // the gap between items never drops it.
            .on_hover_change(move |state: &mut AppState, inside, tx| {
                if inside && state.page_nav_hover != Some(index) {
                    state.page_nav_hover = Some(index);
                    tx.animated();
                }
            })
            .on_press_tx(move |state: &mut AppState, tx| {
                state.page_category = Some(key);
                tx.scroll_to_instant(scroll_key, 0.0, 0.0);
                tx.animated();
            });
        let ink = if on { theme.text } else { theme.text_muted };
        if vertical {
            entry = entry.w_full().child(
                icon(item.icon, theme::ICON)
                    .color(if on { theme.accent } else { theme.icon })
                    .a11y_hidden(),
            );
        } else {
            entry = entry.grid_cell(index).justify_center();
        }
        entry.child(
            // One style for every state, so selecting an item never resizes its label.
            text(if vertical { item.title } else { item.short })
                .font(type_::BODY)
                .color(ink)
                .single_line()
                .truncate()
                .a11y_hidden(),
        )
    });
    // Highlights pin to this unpadded list, so they line up with the rows exactly.
    let list = if vertical {
        col().w_full()
    } else {
        row().w_full().equal_columns(count)
    }
    .gap(theme::PAGE_NAV_GAP)
    .children(hover)
    .children(selection)
    .children(rows);
    col()
        .key("page-nav")
        .w_full()
        .label(label)
        .padding(theme::PAGE_NAV_INSET)
        .fill(theme.settings_surface)
        .radius(theme::PAGE_GROUP_RADIUS)
        .continuous_corners()
        .on_hover_change(|state: &mut AppState, inside, tx| {
            if !inside && state.page_nav_hover.take().is_some() {
                tx.animated();
            }
        })
        .child(list)
}

/// The heading and sections of the category a page's [`nav`] selected.
pub(crate) fn category(key: &'static str, title: &str, sections: Vec<El>, theme: &Theme) -> El {
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
pub(crate) fn section(key: impl Into<String>, title: &str, theme: &Theme, content: El) -> El {
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
pub(crate) fn group(theme: &Theme) -> El {
    col()
        .w_full()
        .fill(theme.settings_surface)
        .continuous_corners()
        .clip_rounded(theme::PAGE_GROUP_RADIUS)
}

/// A [`group`] of padded rows divided by inset hairlines. The group's height eases as rows come
/// and go, so a row that a setting shows or hides resizes the surface around it.
pub(crate) fn rows(theme: &Theme, rows: impl IntoIterator<Item = El>) -> El {
    let mut group = group(theme).growth(GrowthSpec::default());
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
pub(crate) fn separator(theme: &Theme) -> El {
    el().w_full()
        .h(1.0)
        .px(theme::PAGE_GROUP_PADDING)
        .child(hairline(theme.separator))
}

/// A labelled control: title and wrapping description, with the control trailing.
pub(crate) fn setting(cx: &PageContext, title: &str, description: &str, control: El) -> El {
    let detail = text(description)
        .font(type_::BODY)
        .color(cx.theme.text_muted)
        .wrap();
    setting_with(cx, title, detail, control)
}

/// A [`setting`] whose secondary line is custom, such as a truncated path.
///
/// In a compact pane the control drops under the copy, on the same leading edge.
pub(crate) fn setting_with(cx: &PageContext, title: &str, detail: El, control: El) -> El {
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

/// A [`setting`] led by an icon tile.
pub(crate) fn setting_icon(
    cx: &PageContext,
    glyph: &'static Icon,
    title: &str,
    description: &str,
    control: El,
) -> El {
    let tile = row()
        .size(theme::PAGE_ROW_ICON, theme::PAGE_ROW_ICON)
        .center()
        .radius(theme::PAGE_ROW_ICON_RADIUS)
        .fill(cx.theme.overlay_control)
        .child(icon(glyph, theme::ICON).color(cx.theme.icon).a11y_hidden());
    row()
        .w_full()
        .gap(theme::PAGE_ROW_GAP)
        .center_y()
        .child(tile)
        .child(
            setting(cx, title, description, control)
                .grow(1.0)
                .min_w(0.0),
        )
}

pub(crate) fn row_title(title: &str, theme: &Theme) -> El {
    text(title).font(theme::PAGE_ROW_TITLE).color(theme.text)
}

/// Buttons laid side by side as one trailing control.
pub(crate) fn actions(buttons: impl IntoIterator<Item = El>) -> El {
    row().gap(space::S2).center_y().children(buttons)
}

/// A centred inline state inside a page's content column: empty, unavailable or unknown.
/// Append a button with `.child(..)` to offer a way forward.
pub(crate) fn message(glyph: &'static Icon, title: &str, detail: &str, theme: &Theme) -> El {
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
pub(crate) fn switch_kit(kit: &Kit) -> Kit {
    let mut kit = kit.clone();
    kit.c.std_icon_strong = kit.c.bg_accent;
    kit
}
