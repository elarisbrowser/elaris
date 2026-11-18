// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.

mod history;
mod settings;

use rune::tok::type_;
use rune::{El, col, text};

use crate::app::AppState;
use crate::internal::{InternalPage, InternalRoute};
use crate::ui::theme::{self, Theme};

pub(crate) fn view(
    state: &AppState,
    route: &InternalRoute,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    match route {
        InternalRoute::Page(InternalPage::Settings) => settings::view(state, theme, overlays),
        InternalRoute::Page(InternalPage::History) => {
            history::view(state, theme, overlays, reduced_motion)
        }
        InternalRoute::Page(page) => message_page(
            page.title(),
            "This page is reserved for a future release.",
            theme,
        ),
        InternalRoute::NotFound(address) => message_page(
            "Page not found",
            &format!("Elaris does not have an internal page at {address}."),
            theme,
        ),
    }
}

fn message_page(title: &str, message: &str, theme: &Theme) -> El {
    col()
        .key("internal-message-page")
        .w_full()
        .h_full()
        .center()
        .gap(10.0)
        .child(text(title).font(theme::PAGE_TITLE).color(theme.text))
        .child(
            text(message)
                .font(type_::BODY)
                .color(theme.text_muted)
                .wrap(),
        )
}
