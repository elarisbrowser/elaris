// SPDX-License-Identifier: GPL-3.0-only

//! Explicit routes for Elaris-owned native pages.

// Declare each route once so its canonical address, title, and parser stay in sync.
macro_rules! internal_pages {
    ($($page:ident => ($slug:literal, $title:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum InternalPage {
            $($page),+
        }

        impl InternalPage {
            pub fn address(self) -> &'static str {
                match self {
                    $(Self::$page => concat!("elaris://", $slug)),+
                }
            }

            pub fn title(self) -> &'static str {
                match self {
                    $(Self::$page => $title),+
                }
            }

            fn from_slug(slug: &str) -> Option<Self> {
                match slug {
                    $($slug => Some(Self::$page),)+
                    _ => None,
                }
            }
        }
    };
}

internal_pages! {
    Settings => ("settings", "Settings"),
    History => ("history", "History"),
    Bookmarks => ("bookmarks", "Bookmarks"),
    Downloads => ("downloads", "Downloads"),
    TaskManager => ("processes", "Task Manager"),
    About => ("about", "About Elaris"),
    Privacy => ("privacy", "Privacy Notice"),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InternalRoute {
    Page(InternalPage),
    NotFound(String),
}

impl InternalRoute {
    /// Parses every `elaris:` URL as native, but only recognizes registered routes.
    pub fn parse(address: &str) -> Option<Self> {
        let address = address.trim();
        let (scheme, rest) = address.split_once(':')?;
        if !scheme.eq_ignore_ascii_case("elaris") {
            return None;
        }

        let page = rest.strip_prefix("//").and_then(InternalPage::from_slug);
        Some(page.map_or_else(|| Self::NotFound(address.to_owned()), Self::Page))
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Page(page) => page.title(),
            Self::NotFound(_) => "Page Not Found",
        }
    }

    pub fn is_internal_address(address: &str) -> bool {
        Self::parse(address).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{InternalPage, InternalRoute};

    #[test]
    fn recognizes_only_explicit_internal_routes() {
        for (url, page) in [
            ("elaris://settings", InternalPage::Settings),
            ("elaris://history", InternalPage::History),
            ("elaris://downloads", InternalPage::Downloads),
            ("elaris://processes", InternalPage::TaskManager),
            ("elaris://about", InternalPage::About),
            ("elaris://privacy", InternalPage::Privacy),
            ("ELARIS://settings", InternalPage::Settings),
        ] {
            assert_eq!(InternalRoute::parse(url), Some(InternalRoute::Page(page)));
            assert_eq!(
                InternalRoute::parse(page.address()),
                Some(InternalRoute::Page(page))
            );
        }
        assert_eq!(InternalRoute::parse("https://elaris/settings"), None);
    }

    #[test]
    fn malformed_and_unknown_internal_routes_are_not_found() {
        for url in [
            "elaris://unknown",
            "elaris://settings/extra",
            "elaris://settings?debug=true",
            "elaris://",
            "elaris:settings",
        ] {
            assert_eq!(
                InternalRoute::parse(url),
                Some(InternalRoute::NotFound(url.to_owned()))
            );
        }
    }
}
