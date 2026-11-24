// SPDX-License-Identifier: GPL-3.0-only

//! The Elaris version and the engine and UI toolkit revisions it was built from, which
//! `build.rs` records.

const ELARIS_VERSION: &str = env!("CARGO_PKG_VERSION");
const ELARIS_COMMIT: &str = env!("ELARIS_COMMIT");
const LADYBIRD_COMMIT: &str = env!("ELARIS_LADYBIRD_COMMIT");
const LADYBIRD_DATE: &str = env!("ELARIS_LADYBIRD_DATE");
const RUNE_COMMIT: &str = env!("ELARIS_RUNE_COMMIT");

/// One line naming each version, such as
/// `Elaris 0.1.0 (7c51eb7860) · Ladybird fb59d9637b (2026-11-23) · Rune 1df5e57790`.
pub(super) fn summary() -> String {
    let mut parts = vec![with_detail(
        format!("Elaris {ELARIS_VERSION}"),
        short(ELARIS_COMMIT),
    )];
    if let Some(commit) = short(LADYBIRD_COMMIT) {
        let date = Some(LADYBIRD_DATE).filter(|date| !date.is_empty());
        parts.push(with_detail(format!("Ladybird {commit}"), date));
    }
    if let Some(commit) = short(RUNE_COMMIT) {
        parts.push(format!("Rune {commit}"));
    }
    parts.join(" · ")
}

fn with_detail(label: String, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("{label} ({detail})"),
        None => label,
    }
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
