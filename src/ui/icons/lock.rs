// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

pub(crate) static LOCK: Icon = icon(
    "lock",
    r#"<rect x="3.25" y="7" width="9.5" height="7" rx="1.25" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M5.25 7V5.5A2.75 2.75 0 0 1 10.75 5.5V7M8 9.5V11.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
