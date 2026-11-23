// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

/// A page that isn't bookmarked, and the bookmarks page.
pub(crate) static STAR: Icon = icon(
    "star",
    r#"<path d="M8 2.25L9.7 5.8L13.6 6.3L10.75 9L11.45 12.85L8 11L4.55 12.85L5.25 9L2.4 6.3L6.3 5.8Z" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/>"#,
);

/// A bookmarked page.
pub(crate) static STAR_FILLED: Icon = icon(
    "star-filled",
    r#"<path d="M8 2.25L9.7 5.8L13.6 6.3L10.75 9L11.45 12.85L8 11L4.55 12.85L5.25 9L2.4 6.3L6.3 5.8Z" fill="currentColor" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/>"#,
);
