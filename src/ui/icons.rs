// SPDX-License-Identifier: GPL-3.0-only

//! Interface icons: plain strokes and the search providers' coloured brand marks.

use rune::Icon;

use crate::address::SearchEngine;

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
pub(crate) static SIDEBAR: Icon = icon(
    "sidebar",
    r#"<rect x="2.25" y="3.25" width="11.5" height="9.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M6.25 3.25V12.75" fill="none" stroke="currentColor" stroke-width="1.25"/>"#,
);
pub(crate) static SEARCH: Icon = icon(
    "search",
    r#"<circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10.5 10.5L14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
pub(crate) static CHEVRON_DOWN: Icon = icon(
    "chevron-down",
    r#"<path d="m4.5 6.5 3.5 3 3.5-3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static GOOGLE: Icon = Icon {
    name: "google",
    view: 24.0,
    body: r##"<circle cx="12" cy="12" r="12" fill="#fff"/><path fill="#4285F4" d="M23.49 12.27c0-.79-.07-1.54-.2-2.27H12v4.51h6.44c-.28 1.45-1.08 2.68-2.29 3.5v2.91h3.71c2.17-2 3.43-4.96 3.43-8.65z"/><path fill="#34A853" d="M12 24c3.11 0 5.72-1.03 7.63-2.79l-3.71-2.91c-1.03.69-2.35 1.1-3.92 1.1-3.01 0-5.56-2.03-6.47-4.76H1.7v3C3.6 21.02 7.48 24 12 24z"/><path fill="#FBBC05" d="M5.53 14.64A7.2 7.2 0 0 1 5.15 12c0-.92.16-1.8.38-2.64v-3H1.7A12 12 0 0 0 0 12c0 1.94.46 3.77 1.27 5.4l4.26-2.76z"/><path fill="#EA4335" d="M12 4.6c1.7 0 3.23.58 4.43 1.73l3.32-3.32C17.72 1.15 15.11 0 12 0 7.48 0 3.6 2.98 1.7 6.36l3.83 3C6.44 6.63 8.99 4.6 12 4.6z"/>"##,
};
pub(crate) static BING: Icon = Icon {
    name: "bing",
    view: 24.0,
    body: r##"<path fill="#008373" d="M4 2v20l7.2-2.6V6L4 2z"/><path fill="#008373" d="M11.2 8.4v4.1l5.2 2.1-5.2 1.7v4.4l9.6-4.2v-4.2z"/>"##,
};
pub(crate) static DUCKDUCKGO: Icon = Icon {
    name: "duckduckgo",
    view: 24.0,
    body: r##"<circle cx="12" cy="12" r="11" fill="#DE5833"/><path fill="#fff" d="M6.4 14.4c-.9-1.8.2-4.1 2.6-4.7 1.1-2.4 4.1-3.2 6.5-1.8l1.2 2.7 2.1 1.2-1.6 1.3c-.2 2.7-2.6 4.5-5.8 4.5-2.3 0-4.1-1-5-3.2z"/><circle cx="14.5" cy="10.5" r=".65" fill="#DE5833"/><path d="m16.1 11.7 2.7.2-2.1 1.1" fill="none" stroke="#DE5833" stroke-width=".8" stroke-linecap="round" stroke-linejoin="round"/>"##,
};
pub(crate) static BRAVE: Icon = Icon {
    name: "brave-search",
    view: 24.0,
    body: r##"<path fill="#FB542B" d="m5.2 2.6 2.1 1.1 1.1-.8h7.2l1.1.8 2.1-1.1 2.4 5.1-1.3 1.4.7 4.2-1.7 2.3-1.1 4.1L12 22l-5.8-4.3-1.1-4.1-1.7-2.3.7-4.2-1.3-1.4 2.4-5.1z"/><path fill="#fff" d="m7.1 7.2 2.2 1.1 2.7-1.1 2.7 1.1 2.2-1.1-.5 3.1 1.4 1.5-1.1 1.1-1.4-.7-1.7 1-1.6-1-1.7 1-1.6-1-1.4.7-1.1-1.1 1.4-1.5-.5-3.1z"/>"##,
};

pub(crate) fn search_engine(engine: SearchEngine) -> &'static Icon {
    match engine {
        SearchEngine::Google => &GOOGLE,
        SearchEngine::Bing => &BING,
        SearchEngine::DuckDuckGo => &DUCKDUCKGO,
        SearchEngine::Brave => &BRAVE,
    }
}
pub(crate) static HISTORY: Icon = icon(
    "history",
    r#"<path d="M3.25 5.25A5.5 5.5 0 1 1 2.5 8" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/><path d="M2.25 2.75V5.5H5M8 4.5V8L10.25 9.5" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
