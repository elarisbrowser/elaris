// SPDX-License-Identifier: GPL-3.0-only

//! The Elaris version and the engine and UI toolkit revisions it was built from.

use rune::tok::type_;
use rune::{El, text};

use crate::ui::components::widgets::setting;
use crate::ui::internal::PageContext;

use super::section;

const ELARIS_VERSION: &str = env!("CARGO_PKG_VERSION");
const ELARIS_COMMIT: &str = env!("ELARIS_COMMIT");
const LADYBIRD_COMMIT: &str = env!("ELARIS_LADYBIRD_COMMIT");
const LADYBIRD_DATE: &str = env!("ELARIS_LADYBIRD_DATE");
const RUNE_COMMIT: &str = env!("ELARIS_RUNE_COMMIT");

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    let elaris = match short(ELARIS_COMMIT) {
        Some(commit) => format!("{ELARIS_VERSION} ({commit})"),
        None => ELARIS_VERSION.to_owned(),
    };
    let ladybird = match (short(LADYBIRD_COMMIT), LADYBIRD_DATE) {
        (Some(commit), "") => commit.to_owned(),
        (Some(commit), date) => format!("{commit} ({date})"),
        (None, _) => "Unknown".to_owned(),
    };
    let rune = short(RUNE_COMMIT).unwrap_or("Unknown").to_owned();
    let rows = [
        (
            "Elaris",
            "The browser version and the commit it was built from.",
            elaris,
        ),
        (
            "Ladybird",
            "The upstream web engine commit Elaris is pinned to.",
            ladybird,
        ),
        ("Rune", "The UI toolkit commit Elaris is pinned to.", rune),
    ]
    .map(|(title, description, value)| {
        let value = text(value)
            .font(type_::LABEL)
            .color(cx.theme.text_muted)
            .selectable(true);
        setting(cx, title, description, value)
    });
    vec![section("Versions", cx.theme, rows)]
}

/// The abbreviated form of a full commit hash, or `None` when it's unknown.
fn short(commit: &str) -> Option<&str> {
    commit
        .get(..10)
        .or(Some(commit))
        .filter(|commit| !commit.is_empty())
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn shortens_commits() {
        assert_eq!(short("fb59d9637ba02998db1c"), Some("fb59d9637b"));
        assert_eq!(short("abc"), Some("abc"));
        assert_eq!(short(""), None);
    }
}
