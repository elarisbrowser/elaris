// SPDX-License-Identifier: GPL-3.0-only

//! The shared frame around every native page: header, optional toolbar and scrolling content.

use rune::tok::space;
use rune::{El, Icon, ScrollSpec, col, el, icon, row, text};

use crate::ui::theme::{self, Theme};

/// What a page renderer returns; the frame supplies the title, alignment and scrolling.
pub(super) struct PageLayout {
    /// A stable key for the scroll region, so scroll position survives re-renders.
    pub(super) scroll_key: &'static str,
    pub(super) icon: &'static Icon,
    /// Controls that stay pinned under the title while the content scrolls.
    pub(super) toolbar: Option<El>,
    pub(super) content: El,
}

impl PageLayout {
    pub(super) fn new(scroll_key: &'static str, icon: &'static Icon, content: El) -> Self {
        Self {
            scroll_key,
            icon,
            toolbar: None,
            content,
        }
    }

    pub(super) fn toolbar(mut self, toolbar: El) -> Self {
        self.toolbar = Some(toolbar);
        self
    }

    pub(super) fn view(self, title: &str, theme: &Theme) -> El {
        let header = col()
            .key("internal-page-header")
            .w_full()
            .gap(theme::PAGE_GROUP_PADDING)
            .pt(theme::PAGE_TOP_PADDING)
            .pb(theme::PAGE_GROUP_PADDING)
            .child(title_row(self.icon, title, theme))
            .children(self.toolbar)
            .child(el().w_full().h(1.0).fill(theme.separator));
        col()
            .key("internal-page")
            .w_full()
            .h_full()
            .min_h(0.0)
            .child(centered(header))
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
                            .pt(theme::PAGE_SECTION_GAP)
                            .pb(theme::PAGE_CATEGORY_GAP)
                            .child(self.content),
                    )),
            )
    }
}

fn title_row(page_icon: &'static Icon, title: &str, theme: &Theme) -> El {
    row()
        .w_full()
        .center_y()
        .gap(space::S3)
        .child(
            row()
                .size(theme::PAGE_HEADER_ICON, theme::PAGE_HEADER_ICON)
                .center()
                .radius(theme::PAGE_GROUP_RADIUS - space::S1)
                .fill(theme.settings_surface)
                .child(icon(page_icon, theme::ICON).color(theme.icon).a11y_hidden()),
        )
        .child(
            text(title)
                .font(theme::PAGE_TITLE)
                .color(theme.text)
                .wrap()
                .role("heading"),
        )
}

fn centered(content: El) -> El {
    row().w_full().justify_center().child(
        col()
            .w_full()
            .max_w(theme::PAGE_CONTENT_WIDTH)
            .px(theme::PAGE_SECTION_GAP)
            .child(content),
    )
}
