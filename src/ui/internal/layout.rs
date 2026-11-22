// SPDX-License-Identifier: GPL-3.0-only

//! Shared frame, section and state styling for native pages.

use rune::tok::{space, type_};
use rune::{El, Icon, ScrollSpec, col, el, icon, row, text};

use crate::ui::theme::{self, Theme};

const CONTENT_WIDTH: f32 = 800.0;
const HEADER_ICON_SIZE: f32 = 32.0;
const CONTENT_INSET: f32 = space::S6;
pub(super) const SECTION_GAP: f32 = space::S6;
pub(super) const COMPACT_BREAKPOINT: f32 = 560.0;

pub(super) struct PageLayout {
    pub(super) scroll_key: &'static str,
    pub(super) icon: &'static Icon,
    pub(super) toolbar: Option<El>,
    pub(super) content: El,
}

impl PageLayout {
    pub(super) fn view(self, title: &str, theme: &Theme) -> El {
        let header = row()
            .w_full()
            .center_y()
            .gap(space::S3)
            .child(
                row()
                    .size(HEADER_ICON_SIZE, HEADER_ICON_SIZE)
                    .center()
                    .radius(theme::PAGE_GROUP_RADIUS)
                    .fill(theme.settings_surface)
                    .child(icon(self.icon, theme::ICON).color(theme.icon).a11y_hidden()),
            )
            .child(
                text(title)
                    .font(theme::PAGE_TITLE)
                    .color(theme.text)
                    .wrap()
                    .role("heading"),
            );
        col()
            .key("internal-page")
            .w_full()
            .h_full()
            .min_h(0.0)
            .child(centered(
                col()
                    .key("internal-page-header")
                    .w_full()
                    .gap(theme::PAGE_GROUP_PADDING)
                    .pt(theme::PAGE_TOP_PADDING)
                    .pb(theme::PAGE_GROUP_PADDING)
                    .child(header)
                    .children(self.toolbar)
                    .child(el().w_full().h(1.0).fill(theme.separator)),
            ))
            .child(
                col()
                    .key(self.scroll_key)
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(centered(
                        col()
                            .w_full()
                            .pt(space::S2)
                            .pb(space::S6 + space::S3)
                            .child(self.content),
                    )),
            )
    }
}

fn centered(content: El) -> El {
    row().w_full().justify_center().child(
        col()
            .w_full()
            .max_w(CONTENT_WIDTH)
            .px(CONTENT_INSET)
            .child(content),
    )
}

/// An inline state inside a page's content column.
pub(super) fn message(title: &str, detail: &str, theme: &Theme) -> El {
    group(theme)
        .gap(space::S2)
        .px(theme::PAGE_GROUP_PADDING)
        .py(theme::PAGE_GROUP_PADDING)
        .child(
            text(title)
                .font(theme::PAGE_SECTION_TITLE)
                .color(theme.text)
                .wrap()
                .role("heading"),
        )
        .child(
            text(detail)
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap(),
        )
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

/// A hairline between rows of a [`group`], inset to line up with the rows' content.
pub(super) fn separator(theme: &Theme) -> El {
    el().w_full()
        .h(1.0)
        .px(theme::PAGE_GROUP_PADDING)
        .child(el().w_full().h_full().fill(theme.separator))
}
