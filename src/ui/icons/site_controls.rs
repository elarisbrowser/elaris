// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

pub(crate) static SITE_CONTROLS: Icon = icon(
    "site-controls",
    r#"<rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.35"/><path d="M4.5 6H11M4.5 10H11" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/><circle cx="7" cy="6" r="1.25" fill="currentColor"/><circle cx="9" cy="10" r="1.25" fill="currentColor"/>"#,
);
