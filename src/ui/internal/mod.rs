// SPDX-License-Identifier: GPL-3.0-only

//! Native Rune pages for routes owned by Elaris.

mod history;
mod settings;

use rune::tok::type_;
use rune::{El, col, el, text};

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

/// A titled block of a native page: a muted heading above its grouped content.
fn section(key: impl Into<String>, title: &str, theme: &Theme, content: El) -> El {
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
fn group(theme: &Theme) -> El {
    col()
        .w_full()
        .fill(theme.settings_surface)
        .continuous_corners()
        .clip_rounded(theme::PAGE_GROUP_RADIUS)
}

/// A hairline between rows of a [`group`], inset to line up with the rows' content.
fn separator(theme: &Theme) -> El {
    el().w_full()
        .h(1.0)
        .px(theme::PAGE_GROUP_PADDING)
        .child(el().w_full().h_full().fill(theme.separator))
}
