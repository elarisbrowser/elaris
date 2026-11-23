// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

pub(crate) static SYSTEM: Icon = icon(
    "system",
    r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M8 2.5A5.5 5.5 0 0 1 8 13.5Z" fill="currentColor"/>"#,
);
