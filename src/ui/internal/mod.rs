// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.
//!
//! To add a page: declare its route in `src/internal.rs`, add a module here whose `view`
//! takes a [`PageContext`] and returns a [`PageLayout`], and map it in [`renderer`]. The match is
//! exhaustive, so a route without a renderer fails to compile.

mod history;
mod layout;
mod placeholders;
mod settings;
mod widgets;

use rune::El;
use rune_kit::Kit;

use crate::app::AppState;
use crate::internal::{InternalPage, InternalRoute};
use crate::ui::{
    icons,
    theme::{self, Theme},
};

use self::layout::PageLayout;

/// Everything a page renderer reads, built once per frame by [`view`].
pub(super) struct PageContext<'a> {
    pub(super) state: &'a AppState,
    pub(super) theme: &'a Theme,
    /// Rune Kit palette seeded from the page's grouped surface.
    pub(super) kit: Kit,
    /// Dialogs and popovers that render above the page.
    pub(super) overlays: &'a mut Vec<El>,
    pub(super) reduced_motion: bool,
    /// The pane is narrow, so rows stack their controls under their copy.
    pub(super) compact: bool,
}

type Renderer = fn(&mut PageContext) -> PageLayout;

fn renderer(page: InternalPage) -> Renderer {
    match page {
        InternalPage::Settings => settings::view,
        InternalPage::History => history::view,
        InternalPage::Downloads => placeholders::downloads,
        InternalPage::About => placeholders::about,
    }
}

pub(crate) fn view(
    state: &AppState,
    route: &InternalRoute,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
    pane_width: f32,
) -> El {
    let mut cx = PageContext {
        state,
        theme,
        kit: theme.kit(theme.settings_surface),
        overlays,
        reduced_motion,
        compact: pane_width > 0.0 && pane_width < theme::PAGE_COMPACT_BREAKPOINT,
    };
    let page = match route {
        InternalRoute::Page(page) => renderer(*page)(&mut cx),
        InternalRoute::NotFound(address) => not_found(address, theme),
    };
    page.view(route.title(), theme)
}

fn not_found(address: &str, theme: &Theme) -> PageLayout {
    PageLayout::new(
        "internal-not-found-scroll",
        &icons::INFO,
        widgets::message(
            &icons::INFO,
            "Unknown address",
            &format!("Elaris does not have an internal page at {address}."),
            theme,
        ),
    )
}
