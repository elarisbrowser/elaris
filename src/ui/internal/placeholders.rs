// SPDX-License-Identifier: GPL-3.0-only

//! Routes reserved for pages that have no native content yet.

use crate::ui::icons;

use super::PageContext;
use super::layout::PageLayout;
use super::widgets::message;

pub(super) fn downloads(cx: &mut PageContext) -> PageLayout {
    PageLayout::new(
        "downloads-scroll",
        &icons::DOWNLOAD,
        message(
            &icons::DOWNLOAD,
            "Manage downloads",
            "Use the Downloads button at the bottom of the sidebar or next to the address bar.",
            cx.theme,
        ),
    )
}

pub(super) fn about(cx: &mut PageContext) -> PageLayout {
    PageLayout::new(
        "about-scroll",
        &icons::INFO,
        message(
            &icons::INFO,
            "Coming soon",
            "This page is reserved for a future release.",
            cx.theme,
        ),
    )
}
