// SPDX-License-Identifier: GPL-3.0-only

//! The mark at the leading edge of a tab: its favicon, a static page glyph, or a spinner.

use rune::{El, el, icon};

use crate::app::AppState;
use crate::tab::Tab;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
use elaris_core::internal::{InternalPage, InternalRoute};

pub(in crate::ui) fn tab_icon(
    state: &AppState,
    tab: &Tab,
    active: bool,
    size: f32,
    theme: &Theme,
) -> El {
    let color = if active { theme.text } else { theme.text_muted };
    if let Some(favicon) = &tab.favicon {
        return el().size(size, size).center().pixels(favicon);
    }
    if tab.loading {
        let mark = icon(&icons::LOADING, size).color(color).label("Loading");
        return if state.sidebar.hover.reduced.get() {
            mark
        } else {
            mark.looping(rune::LoopSpec::rotation(theme::LOADING_PERIOD))
        };
    }
    let glyph = match tab.internal {
        Some(InternalRoute::Page(InternalPage::History)) => &icons::HISTORY,
        Some(InternalRoute::Page(InternalPage::TaskManager)) => &icons::ACTIVITY,
        _ => &icons::PAGE,
    };
    icon(glyph, size).color(color)
}
