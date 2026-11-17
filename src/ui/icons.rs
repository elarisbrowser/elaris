// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar's icons: plain strokes on a 16-point grid, drawn in their element's colour.

use rune::Icon;

const fn icon(name: &'static str, body: &'static str) -> Icon {
    Icon {
        name,
        view: 16.0,
        body,
    }
}

pub(crate) static BACK: Icon = icon(
    "back",
    r#"<path d="M10 3.5L5.5 8L10 12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static FORWARD: Icon = icon(
    "forward",
    r#"<path d="M6 3.5L10.5 8L6 12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static RELOAD: Icon = icon(
    "reload",
    r#"<path d="M12.5 8A4.5 4.5 0 1 1 11.2 4.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M12.5 2.5V5.5H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static PLUS: Icon = icon(
    "plus",
    r#"<path d="M8 3.5V12.5M3.5 8H12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
pub(crate) static CLOSE: Icon = icon(
    "close",
    r#"<path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
pub(crate) static PAGE: Icon = icon(
    "page",
    r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.25"/><ellipse cx="8" cy="8" rx="2.5" ry="5.75" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M2.25 8H13.75" fill="none" stroke="currentColor" stroke-width="1.25"/>"#,
);
pub(crate) static LOADING: Icon = icon(
    "loading",
    r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.25" opacity="0.3"/><path d="M8 2.25A5.75 5.75 0 0 1 13.75 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
pub(crate) static SYSTEM: Icon = icon(
    "system",
    r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M8 2.5A5.5 5.5 0 0 1 8 13.5Z" fill="currentColor"/>"#,
);
pub(crate) static LIGHT: Icon = icon(
    "light",
    r#"<circle cx="8" cy="8" r="2.75" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M8 1.75V3.25M8 12.75V14.25M1.75 8H3.25M12.75 8H14.25M3.6 3.6L4.65 4.65M11.35 11.35L12.4 12.4M3.6 12.4L4.65 11.35M11.35 4.65L12.4 3.6" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/>"#,
);
pub(crate) static DARK: Icon = icon(
    "dark",
    r#"<path d="M13 9.6A5.25 5.25 0 1 1 6.4 3A4.25 4.25 0 0 0 13 9.6Z" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/>"#,
);
