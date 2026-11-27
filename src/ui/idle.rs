// SPDX-License-Identifier: GPL-3.0-only

//! The single, static screen shown while an inactive browser window is idling.

use rune::tok::{space, type_};
use rune::{El, col, el, text};

use crate::app::AppState;
use crate::ui::theme::Theme;

pub(super) fn view(state: &AppState, theme: &Theme) -> El {
    let (heading, detail) = if state.idling_waking() {
        ("Waking up", "Your browser will be ready in a moment.")
    } else {
        ("Idling", "Click anywhere to resume browsing.")
    };
    let content = col()
        .gap(space::S2)
        .center()
        .child(
            text(heading)
                .key("idle-heading")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text(detail)
                .key("idle-detail")
                .font(type_::BODY)
                .color(theme.text_muted),
        );

    let screen = el()
        .key("idle-screen")
        .w_full()
        .h_full()
        .center()
        .fill(theme.page)
        .focus_ring(crate::ui::theme::FOCUS_WIDTH, 1.0, theme.focus)
        .label("Idling. Click anywhere to resume browsing.")
        .on_press_tx(AppState::request_idle_wake)
        .child(content);

    rune_kit::root(screen, Vec::new())
}
