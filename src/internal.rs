// SPDX-License-Identifier: GPL-3.0-only

//! Explicit routes for Elaris-owned native pages.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InternalPage {
    Settings,
    History,
    Downloads,
    About,
}

impl InternalPage {
    pub(crate) fn address(self) -> &'static str {
        match self {
            Self::Settings => "elaris://settings",
            Self::History => "elaris://history",
            Self::Downloads => "elaris://downloads",
            Self::About => "elaris://about",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::History => "History",
            Self::Downloads => "Downloads",
            Self::About => "About Elaris",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InternalRoute {
    Page(InternalPage),
    NotFound(String),
}

impl InternalRoute {
    /// Parses every `elaris:` URL as native, but only recognizes the four supported routes.
    pub(crate) fn parse(address: &str) -> Option<Self> {
        let address = address.trim();
        let (scheme, rest) = address.split_once(':')?;
        if !scheme.eq_ignore_ascii_case("elaris") {
            return None;
        }

        let route = rest.strip_prefix("//").unwrap_or_default();
        let page = match route {
            "settings" => Some(InternalPage::Settings),
            "history" => Some(InternalPage::History),
            "downloads" => Some(InternalPage::Downloads),
            "about" => Some(InternalPage::About),
            _ => None,
        };
        Some(page.map_or_else(|| Self::NotFound(address.to_owned()), Self::Page))
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Page(page) => page.title(),
            Self::NotFound(_) => "Page Not Found",
        }
    }

    pub(crate) fn is_internal_address(address: &str) -> bool {
        Self::parse(address).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_explicit_internal_routes() {
        for (url, page) in [
            ("elaris://settings", InternalPage::Settings),
            ("elaris://history", InternalPage::History),
            ("elaris://downloads", InternalPage::Downloads),
            ("elaris://about", InternalPage::About),
            ("ELARIS://settings", InternalPage::Settings),
        ] {
            assert_eq!(InternalRoute::parse(url), Some(InternalRoute::Page(page)));
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
