// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.
//!
//! To add a page: declare its route in `crates/elaris-core/src/internal.rs`, add a module here whose `view`
//! takes a [`PageContext`] and returns a [`PageLayout`], and register it in [`page`]. The match
//! is exhaustive, so a route without a renderer fails to compile. A page can also list
//! [`PageAction`]s, which its context menu offers.

mod menu;

mod clear_data;
mod history;
mod layout;
mod placeholders;
mod settings;
use crate::ui::components::widgets;

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
pub(crate) struct PageContext<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) theme: &'a Theme,
    /// Rune Kit palette seeded from the page's grouped surface.
    pub(crate) kit: Kit,
    /// Dialogs and popovers that render above the page.
    pub(crate) overlays: &'a mut Vec<El>,
    pub(crate) reduced_motion: bool,
    /// The pane is narrow, so rows stack their controls under their copy.
    pub(crate) compact: bool,
    /// The pane is wide enough for category navigation beside the content.
    pub(crate) side_nav: bool,
}

/// A command a page offers in its context menu.
pub(super) struct PageAction {
    pub(crate) label: &'static str,
    /// Whether it can run now; disabled items stay visible so the menu keeps its shape.
    pub(crate) enabled: fn(&AppState) -> bool,
    pub(crate) run: fn(&mut AppState, &mut rune::Tx),
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
