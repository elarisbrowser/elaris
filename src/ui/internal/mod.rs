// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.
//!
//! To add a page: declare its route in `src/internal.rs`, add a module here whose `view`
//! takes a [`PageContext`] and returns a [`PageLayout`], and register it in [`page`]. The match
//! is exhaustive, so a route without a renderer fails to compile. A page can also list
//! [`PageAction`]s, which its context menu offers.

mod menu;

mod clear_data;
mod history;
mod layout;
mod placeholders;
mod settings;
mod widgets;

use rune::El;
use rune_kit::Kit;

use crate::app::AppState;
use crate::ui::{
    icons,
    theme::{self, Theme},
};
use elaris_core::internal::{InternalPage, InternalRoute};

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
    /// The pane is wide enough for category navigation beside the content.
    pub(super) side_nav: bool,
}

/// A command a page offers in its context menu.
pub(super) struct PageAction {
    pub(super) label: &'static str,
    /// Whether it can run now; disabled items stay visible so the menu keeps its shape.
    pub(super) enabled: fn(&AppState) -> bool,
    pub(super) run: fn(&mut AppState, &mut rune::Tx),
}

/// How Elaris renders one internal page.
struct Page {
    view: fn(&mut PageContext) -> PageLayout,
    actions: &'static [PageAction],
}

fn page(page: InternalPage) -> Page {
    match page {
        InternalPage::Settings => Page {
            view: settings::view,
            actions: settings::ACTIONS,
        },
        InternalPage::History => Page {
            view: history::view,
            actions: history::ACTIONS,
        },
        InternalPage::Downloads => Page {
            view: placeholders::downloads,
            actions: &[],
        },
        InternalPage::About => Page {
            view: placeholders::about,
            actions: &[],
        },
    }
}

pub(crate) use self::menu::context_menu;

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
        side_nav: pane_width <= 0.0 || pane_width >= theme::PAGE_NAV_BREAKPOINT,
    };
    let side_nav = cx.side_nav;
    let page = match route {
        InternalRoute::Page(route) => (page(*route).view)(&mut cx),
        InternalRoute::NotFound(address) => not_found(address, theme),
    };
    page.view(route.title(), theme, side_nav)
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
