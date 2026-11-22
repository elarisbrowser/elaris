// SPDX-License-Identifier: GPL-3.0-only

//! The browser's native settings page, one module per category.

mod appearance;
mod browsing;
mod privacy;
mod system;

use rune::El;

use crate::ui::icons;
use crate::ui::theme::Theme;

use super::PageContext;
use super::layout::PageLayout;
use super::widgets;

const SCROLL: &str = "settings-scroll";

pub(super) fn view(cx: &mut PageContext) -> PageLayout {
    let categories = vec![
        appearance::category(cx),
        browsing::category(cx),
        privacy::category(cx),
        system::category(cx),
    ];
    let toolbar = widgets::jump_bar(
        "settings-navigation",
        "Jump to a settings category",
        SCROLL,
        &categories,
        cx,
    );
    PageLayout::new(
        SCROLL,
        &icons::SITE_CONTROLS,
        widgets::categories("settings-content", categories, cx.theme),
    )
    .toolbar(toolbar)
}

/// A settings section, keyed by its title so tests and focus can find it.
fn section(title: &str, theme: &Theme, rows: impl IntoIterator<Item = El>) -> El {
    widgets::section(
        format!("settings-section-{title}"),
        title,
        theme,
        widgets::rows(theme, rows),
    )
}
