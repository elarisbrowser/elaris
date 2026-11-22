// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.

mod history;
mod layout;
mod settings;

use rune::{El, Icon};

use crate::app::AppState;
use crate::internal::{InternalPage, InternalRoute};
use crate::ui::{icons, theme::Theme};

use self::layout::PageLayout;

pub(crate) fn view(
    state: &AppState,
    route: &InternalRoute,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
    pane_width: f32,
) -> El {
    let compact = pane_width > 0.0 && pane_width < layout::COMPACT_BREAKPOINT;
    let page = match route {
        InternalRoute::Page(InternalPage::Settings) => {
            settings::view(state, theme, overlays, compact)
        }
        InternalRoute::Page(InternalPage::History) => {
            history::view(state, theme, overlays, reduced_motion, compact)
        }
        InternalRoute::Page(InternalPage::Downloads) => placeholder(
            "downloads-scroll",
            &icons::DOWNLOAD,
            "Manage downloads",
            "Use the Downloads button at the bottom of the sidebar or next to the address bar.",
            theme,
        ),
        InternalRoute::Page(InternalPage::About) => placeholder(
            "about-scroll",
            &icons::INFO,
            "Coming soon",
            "This page is reserved for a future release.",
            theme,
        ),
        InternalRoute::NotFound(address) => PageLayout {
            scroll_key: "internal-not-found-scroll",
            icon: &icons::INFO,
            toolbar: None,
            content: layout::message(
                "Unknown address",
                &format!("Elaris does not have an internal page at {address}."),
                theme,
            ),
        },
    };
    page.view(route.title(), theme)
}

fn placeholder(
    scroll_key: &'static str,
    icon: &'static Icon,
    title: &str,
    detail: &str,
    theme: &Theme,
) -> PageLayout {
    PageLayout {
        scroll_key,
        icon,
        toolbar: None,
        content: layout::message(title, detail, theme),
    }
}
