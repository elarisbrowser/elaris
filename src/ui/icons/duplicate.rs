// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

pub(crate) static DUPLICATE: Icon = icon(
    "duplicate",
    r#"<rect x="2.75" y="3.75" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/><path d="M6.25 2.75H11.25A1.5 1.5 0 0 1 12.75 4.25V9.25" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><path d="M6.5 12.75H11.25A1.5 1.5 0 0 0 12.75 11.25V6.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
);
