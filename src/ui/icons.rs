// SPDX-License-Identifier: GPL-3.0-only

//! Interface icons: plain strokes and the search providers' coloured brand marks.

use std::sync::{Arc, OnceLock};

use rune::{El, Icon, Image, ImageCache, Pixels, el};

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
pub(crate) static OPEN_TAB: Icon = icon(
    "open-tab",
    r#"<path d="M8 3.5H12.5V8M12.5 3.5L6.75 9.25M11.5 9.5V12.5H3.5V4.5H6.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static SPLIT_VIEW: Icon = icon(
    "split-view",
    r#"<rect x="2.5" y="3.5" width="11" height="9" rx="1.4" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M8 3.75V12.25" stroke="currentColor" stroke-width="1.4"/>"#,
);
#[cfg(target_os = "linux")]
pub(crate) static MINIMIZE: Icon = icon(
    "minimize",
    r#"<path d="M3 8H13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
#[cfg(target_os = "linux")]
pub(crate) static MAXIMIZE: Icon = icon(
    "maximize",
    r#"<rect x="3.25" y="3.25" width="9.5" height="9.5" rx="1.25" fill="none" stroke="currentColor" stroke-width="1.25"/>"#,
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
pub(crate) static CHEVRON_UP: Icon = icon(
    "chevron-up",
    r#"<path d="m4.5 9.5 3.5-3 3.5 3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static SITE_CONTROLS: Icon = icon(
    "site-controls",
    r#"<rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.35"/><path d="M4.5 6H11M4.5 10H11" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round"/><circle cx="7" cy="6" r="1.25" fill="currentColor"/><circle cx="9" cy="10" r="1.25" fill="currentColor"/>"#,
);
pub(crate) static MICROPHONE: Icon = icon(
    "microphone",
    r#"<rect x="6" y="2.25" width="4" height="7.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.3"/><path d="M4 7.5A4 4 0 0 0 12 7.5M8 11.5V14M5.5 14H10.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
);
pub(crate) static CAMERA: Icon = icon(
    "camera",
    r#"<path d="M2.75 5.25H5L6.1 3.75H9.9L11 5.25H13.25V12.5H2.75Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/><circle cx="8" cy="8.75" r="2.25" fill="none" stroke="currentColor" stroke-width="1.3"/>"#,
);
pub(crate) static LOCATION: Icon = icon(
    "location",
    r#"<path d="M12 6.5C12 9.5 8 13.5 8 13.5S4 9.5 4 6.5A4 4 0 1 1 12 6.5Z" fill="none" stroke="currentColor" stroke-width="1.3"/><circle cx="8" cy="6.5" r="1.25" fill="none" stroke="currentColor" stroke-width="1.2"/>"#,
);
pub(crate) static NOTIFICATIONS: Icon = icon(
    "notifications",
    r#"<path d="M4 10.75H12L11 9V6.75A3 3 0 0 0 5 6.75V9ZM6.5 12.5A1.5 1.5 0 0 0 9.5 12.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static COPY: Icon = icon(
    "copy",
    r#"<rect x="5" y="4" width="7" height="8" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/><path d="M3.5 9.5V3.75A1.25 1.25 0 0 1 4.75 2.5H9" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
);
pub(crate) static DUPLICATE: Icon = icon(
    "duplicate",
    r#"<rect x="2.75" y="3.75" width="7" height="7" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/><path d="M6.25 2.75H11.25A1.5 1.5 0 0 1 12.75 4.25V9.25" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><path d="M6.5 12.75H11.25A1.5 1.5 0 0 0 12.75 11.25V6.5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#,
);
pub(crate) static INFO: Icon = icon(
    "info",
    r#"<circle cx="8" cy="8" r="5.75" fill="none" stroke="currentColor" stroke-width="1.3"/><path d="M8 7.25V10.5M8 5.25H8.01" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
);
pub(crate) static MORE: Icon = icon(
    "more",
    r#"<circle cx="3.25" cy="8" r="1" fill="currentColor"/><circle cx="8" cy="8" r="1" fill="currentColor"/><circle cx="12.75" cy="8" r="1" fill="currentColor"/>"#,
);
pub(crate) static SITE_POPOVER_ARROW: Icon = icon(
    "site-popover-arrow",
    r#"<path d="M0 10L8.6 1.4Q10 0 11.4 1.4L20 10Z" fill="currentColor"/>"#,
);
static SEARCH_ENGINE_IMAGES: OnceLock<[Image; 4]> = OnceLock::new();

pub(crate) fn search_engine(engine: SearchEngine, size: f32) -> El {
    el().size(size, size)
        .center()
        .pixels(search_engine_pixels(engine))
}

fn search_engine_pixels(engine: SearchEngine) -> &'static Pixels {
    let images = SEARCH_ENGINE_IMAGES.get_or_init(|| {
        let cache = ImageCache::new(Arc::new(|| {}));
        let images = [
            cache.load(include_bytes!("../../assets/search-engines/google.svg").as_slice()),
            cache.load(include_bytes!("../../assets/search-engines/bing.svg").as_slice()),
            cache.load(include_bytes!("../../assets/search-engines/duckduckgo.svg").as_slice()),
            cache.load(include_bytes!("../../assets/search-engines/brave.png").as_slice()),
        ];
        for image in &images {
            image
                .wait()
                .expect("bundled search engine icon must decode");
        }
        images
    });

    let index = match engine {
        SearchEngine::Google => 0,
        SearchEngine::Bing => 1,
        SearchEngine::DuckDuckGo => 2,
        SearchEngine::Brave => 3,
    };
    images[index].pixels()
}
pub(crate) static HISTORY: Icon = icon(
    "history",
    r#"<path d="M3.25 5.25A5.5 5.5 0 1 1 2.5 8" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round"/><path d="M2.25 2.75V5.5H5M8 4.5V8L10.25 9.5" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static DOWNLOAD: Icon = icon(
    "download",
    r#"<path d="M8 2.5V9M5.25 6.5L8 9.25L10.75 6.5M3 12.25H13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static OPEN: Icon = icon(
    "open",
    r#"<path d="M8.5 3H13V7.5M12.75 3.25L7 9M11.5 8.5V12.25C11.5 12.66 11.16 13 10.75 13H3.75C3.34 13 3 12.66 3 12.25V5.25C3 4.84 3.34 4.5 3.75 4.5H7.5" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub(crate) static FOLDER: Icon = icon(
    "folder",
    r#"<path d="M2.5 4.25C2.5 3.84 2.84 3.5 3.25 3.5H6L7.5 5H12.75C13.16 5 13.5 5.34 13.5 5.75V11.75C13.5 12.16 13.16 12.5 12.75 12.5H3.25C2.84 12.5 2.5 12.16 2.5 11.75Z" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/>"#,
);
