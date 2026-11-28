// SPDX-License-Identifier: GPL-3.0-only

//! A concise, in-browser notice describing Elaris's data practices.

use rune::tok::type_;
use rune::{col, text};

use crate::ui::{icons, theme};

use super::PageContext;
use super::layout::PageLayout;
use super::widgets;

const SECTIONS: &[(&str, &[&str])] = &[
    (
        "Data stored on this device",
        &[
            "Elaris stores browsing history, open tabs, bookmarks, settings, website permission choices, and download records in your local application data. Ladybird keeps website data such as cookies and cache in the browser profile.",
            "History, settings, bookmarks, and completed download records can be removed from the browser. Use Settings → Privacy & security → Clear browsing data to clear browsing data and site data.",
        ],
    ),
    (
        "Data sent to websites and providers",
        &[
            "When you visit a website, Elaris and Ladybird make network requests to that site and its embedded resources. The site receives the request data needed to provide its content, such as your IP address and browser request details.",
            "When you show bookmarks in an ordinary window, Elaris may load bookmarked pages in the background to fill in missing titles or icons. Private windows do not do this.",
            "Search terms entered in the address bar are sent to your selected search provider when you choose a search suggestion or submit the search. Elaris does not send typed queries for remote autocomplete.",
            "When content blocking is enabled, Elaris downloads filter lists and refreshes them daily. Your browsing history is not sent to Elaris.",
        ],
    ),
    (
        "Private windows",
        &[
            "Private windows do not add visits to saved history or save their tabs in session restore. Their cookies and site storage are held in a temporary private session that is cleared when the last private window closes.",
            "Private browsing does not hide your activity from websites, your network, or your internet provider. Files you download are kept on disk, even after the private session closes.",
        ],
    ),
    (
        "Diagnostics and contact",
        &[
            "Elaris does not use telemetry, analytics, or automatic crash reporting. If you report a problem, include only diagnostic details you are comfortable sharing; do not attach history or session files.",
            "This notice describes the current alpha and may change as Elaris develops. Report privacy questions or concerns at github.com/elarisbrowser/elaris/issues.",
        ],
    ),
];

pub(super) fn view(cx: &mut PageContext) -> PageLayout {
    let sections = SECTIONS.iter().map(|(title, paragraphs)| {
        let copy = col()
            .w_full()
            .gap(theme::PAGE_GROUP_PADDING)
            .children(paragraphs.iter().map(|paragraph| {
                text(*paragraph)
                    .font(type_::BODY)
                    .color(cx.theme.text)
                    .wrap()
                    .selectable(true)
            }));
        widgets::section(
            format!("privacy-section-{title}"),
            title,
            cx.theme,
            widgets::rows(cx.theme, [copy]),
        )
    });

    PageLayout::new(
        "privacy-notice-scroll",
        &icons::LOCK,
        col()
            .w_full()
            .gap(theme::PAGE_SECTION_GAP)
            .children(sections),
    )
}
