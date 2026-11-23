// SPDX-License-Identifier: GPL-3.0-only

use super::icon;
use rune::Icon;

pub(crate) static LOADING: Icon = icon(
    "loading",
    r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.25" opacity="0.3"/><path d="M8 2.25A5.75 5.75 0 0 1 13.75 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
