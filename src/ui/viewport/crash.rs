// SPDX-License-Identifier: GPL-3.0-only

//! What a tab shows once its page's process has crashed or been ended for its memory use, in
//! place of the blank page the engine stands in with, until a reload brings the page back.

use rune::{El, el};
use rune_kit::Button;

use crate::app::AppState;
use crate::tab::Tab;
use crate::ui::components::widgets::message;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

pub(super) fn crash_page(tab: &Tab, theme: &Theme) -> Option<El> {
    let crash = tab.crash.as_ref()?;
    let id = tab.id();
    let site = elaris_core::permissions::PermissionStore::origin(&crash.url).map_or_else(
        || crash.url.clone(),
        |origin| origin.split("://").nth(1).unwrap_or(&origin).to_owned(),
    );
    let (glyph, title, detail) = if crash.out_of_memory {
        (
            &icons::ACTIVITY,
            "This page ran out of memory",
            format!(
                "{site} used so much memory that Elaris ended it to keep everything else running. Reloading may hit the same limit."
            ),
        )
    } else {
        (
            &icons::INFO,
            "This page crashed",
            format!("Something went wrong while showing {site}. Reloading usually brings it back."),
        )
    };
    let reload = Button::new("Reload")
        .on_press(move |state: &mut AppState| state.reload(id))
        .build(&theme.kit(theme.page));
    Some(
        el().key(format!("crash-{}", id.raw()))
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .fill(theme.page)
            .center()
            .role("alert")
            .child(
                message(glyph, title, &detail, theme)
                    .w(theme::CRASH_PAGE_WIDTH)
                    .child(reload),
            ),
    )
}
