// SPDX-License-Identifier: GPL-3.0-only

//! Classifies omnibox input as a URL or a web search.

use std::{fmt::Write as _, net::IpAddr, path::Path};

/// A provider used for text that is not an address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchEngine {
    Google,
    Bing,
    #[default]
    DuckDuckGo,
    Brave,
    /// One of the user's own providers, by its position in the custom engine list.
    Custom(usize),
}

/// Where a search query goes in a custom provider's URL template.
pub const QUERY_PLACEHOLDER: &str = "%s";

/// A provider the user added in `[search.engines]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomSearchEngine {
    key: String,
    name: String,
    template: String,
}

impl CustomSearchEngine {
    /// Checks the key and URL template; `name` falls back to the key when it is empty.
    pub fn new(key: &str, name: &str, template: &str) -> Result<Self, String> {
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("its key may only use letters, digits, - and _".to_owned());
        }
        if SearchEngine::from_built_in_key(key).is_some() {
            return Err("its key is already used by a built-in engine".to_owned());
        }
        let scheme = template.split_once("://").map(|(scheme, _)| scheme);
        if !matches!(scheme, Some(scheme) if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
        {
            return Err("its url must start with https:// or http://".to_owned());
        }
        if !template.contains(QUERY_PLACEHOLDER) {
            return Err(format!(
                "its url must contain {QUERY_PLACEHOLDER} where the query goes"
            ));
        }
        let name = name.trim();
        Ok(Self {
            key: key.to_owned(),
            name: if name.is_empty() { key } else { name }.to_owned(),
            template: template.to_owned(),
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The URL template, with [`QUERY_PLACEHOLDER`] where the query goes.
    pub fn url(&self) -> &str {
        &self.template
    }
}

impl SearchEngine {
    /// The built-in providers, in the order the omnibox menu shows them.
    pub const BUILT_IN: [Self; 4] = [Self::Google, Self::Bing, Self::DuckDuckGo, Self::Brave];

    /// Every provider: the built-in ones, then the user's own in config order.
    pub fn all(custom: &[CustomSearchEngine]) -> impl Iterator<Item = Self> {
        Self::BUILT_IN
            .into_iter()
            .chain((0..custom.len()).map(Self::Custom))
    }

    /// A custom engine whose index is out of range resolves to the default provider, so a stale
    /// index never panics.
    fn resolve(self, custom: &[CustomSearchEngine]) -> Resolved<'_> {
        match self {
            Self::Custom(index) => custom
                .get(index)
                .map_or(Resolved::BuiltIn(Self::default()), Resolved::Custom),
            built_in => Resolved::BuiltIn(built_in),
        }
    }

    pub fn label(self, custom: &[CustomSearchEngine]) -> &str {
        match self.resolve(custom) {
            Resolved::Custom(engine) => engine.name(),
            Resolved::BuiltIn(Self::Google) => "Google",
            Resolved::BuiltIn(Self::Bing) => "Bing",
            Resolved::BuiltIn(Self::Brave) => "Brave Search",
            Resolved::BuiltIn(_) => "DuckDuckGo",
        }
    }

    pub fn config_key(self, custom: &[CustomSearchEngine]) -> &str {
        match self.resolve(custom) {
            Resolved::Custom(engine) => engine.key(),
            Resolved::BuiltIn(Self::Google) => "google",
            Resolved::BuiltIn(Self::Bing) => "bing",
            Resolved::BuiltIn(Self::Brave) => "brave",
            Resolved::BuiltIn(_) => "duckduckgo",
        }
    }

    /// The custom engine this refers to, if any.
    pub fn custom(self, custom: &[CustomSearchEngine]) -> Option<&CustomSearchEngine> {
        match self.resolve(custom) {
            Resolved::Custom(engine) => Some(engine),
            Resolved::BuiltIn(_) => None,
        }
    }

    pub fn from_config_key(key: &str, custom: &[CustomSearchEngine]) -> Option<Self> {
        Self::from_built_in_key(key).or_else(|| {
            custom
                .iter()
                .position(|engine| engine.key == key)
                .map(Self::Custom)
        })
    }

    fn from_built_in_key(key: &str) -> Option<Self> {
        match key {
            "google" => Some(Self::Google),
            "bing" => Some(Self::Bing),
            "duckduckgo" => Some(Self::DuckDuckGo),
            "brave" => Some(Self::Brave),
            _ => None,
        }
    }
}

enum Resolved<'a> {
    BuiltIn(SearchEngine),
    Custom(&'a CustomSearchEngine),
}

/// What the omnibox should do with its input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Url(String),
    Search(String),
}

pub fn destination_for_input(input: &str) -> Result<Destination, &'static str> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a search or address");
    }

    if let Some((scheme, _)) = input.split_once("://") {
        return if supported_scheme(scheme) {
            Ok(Destination::Url(input.to_owned()))
        } else {
            Err("Only http, https, file and elaris addresses are supported")
        };
    }

    let lowercase = input.to_ascii_lowercase();
    if lowercase.starts_with("about:") || lowercase.starts_with("data:") {
        return Ok(Destination::Url(input.to_owned()));
    }
    if input.starts_with('/') {
        return Ok(Destination::Url(file_url(Path::new(input))));
    }

    let host = host_for_address(input);
    if is_local_host(host) {
        Ok(Destination::Url(format!("http://{input}")))
    } else if host.contains('.') {
        Ok(Destination::Url(format!("https://{input}")))
    } else {
        Ok(Destination::Search(input.to_owned()))
    }
}

/// The URL Browser should load for submitted omnibox input.
pub fn url_for_address(input: &str) -> Result<String, &'static str> {
    match destination_for_input(input)? {
        Destination::Url(url) => Ok(url),
        Destination::Search(query) => Ok(search_url(SearchEngine::default(), &[], &query)),
    }
}

/// Builds a URL for a search without sending the query until it is submitted.
pub fn search_url(engine: SearchEngine, custom: &[CustomSearchEngine], query: &str) -> String {
    let mut encoded = String::with_capacity(query.len() * 3);
    encode_url_bytes(&mut encoded, query.as_bytes(), false);
    let prefix = match engine.resolve(custom) {
        Resolved::Custom(engine) => return engine.template.replace(QUERY_PLACEHOLDER, &encoded),
        Resolved::BuiltIn(SearchEngine::Google) => "https://www.google.com/search?q=",
        Resolved::BuiltIn(SearchEngine::Bing) => "https://www.bing.com/search?q=",
        Resolved::BuiltIn(SearchEngine::Brave) => "https://search.brave.com/search?q=",
        Resolved::BuiltIn(_) => "https://duckduckgo.com/?q=",
    };
    prefix.to_owned() + &encoded
}

/// Encodes an absolute local path without treating filename punctuation as URL syntax.
pub fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    encode_url_bytes(&mut url, path.as_os_str().as_encoded_bytes(), true);
    url
}

fn encode_url_bytes(url: &mut String, bytes: &[u8], preserve_slashes: bool) {
    for &byte in bytes {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (preserve_slashes && byte == b'/')
        {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
}

/// A compact sidebar label for web pages. Keep the port and subdomains that identify the site.
pub fn compact_address(url: &str) -> &str {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return url;
    }
    let authority = rest.split(['/', '?', '#', '\\']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    if host.is_empty() {
        return url;
    }
    host.strip_prefix("www.")
        .filter(|name| !name.is_empty())
        .unwrap_or(host)
}

/// The host portion used for local-address detection and tab matching.
pub fn host_for_address(input: &str) -> &str {
    let input = input.split_once("://").map_or(input, |(_, rest)| rest);
    let authority = input.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some(end) = authority
        .strip_prefix('[')
        .and_then(|value| value.find(']'))
    {
        return &authority[1..=end];
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => host,
        _ => authority,
    }
}

fn supported_scheme(scheme: &str) -> bool {
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "file" | "elaris"
    )
}

fn is_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::{
        CustomSearchEngine, Destination, SearchEngine, destination_for_input, file_url, search_url,
        url_for_address,
    };
    use std::path::Path;

    #[test]
    fn local_path_keeps_filename_punctuation_out_of_url_syntax() {
        assert_eq!(
            file_url(Path::new("/tmp/100% # café?.html")),
            "file:///tmp/100%25%20%23%20caf%C3%A9%3F.html"
        );
    }

    #[test]
    fn sidebar_addresses_keep_the_site_and_port() {
        for (url, label) in [
            ("https://www.google.com/search?q=hi#top", "google.com"),
            ("https://accounts.google.com/login", "accounts.google.com"),
            ("http://user@example.org:8080/a", "example.org:8080"),
            ("http://[::1]:8000/path", "[::1]:8000"),
            ("file:///tmp/example.html", "file:///tmp/example.html"),
        ] {
            assert_eq!(super::compact_address(url), label);
        }
    }

    #[test]
    fn classifies_urls_domains_and_searches() {
        assert_eq!(
            destination_for_input("https://example.com/a"),
            Ok(Destination::Url("https://example.com/a".into()))
        );
        assert_eq!(
            destination_for_input("github.com/rust-lang/rust"),
            Ok(Destination::Url("https://github.com/rust-lang/rust".into()))
        );
        assert_eq!(
            destination_for_input("best Rust GUI framework"),
            Ok(Destination::Search("best Rust GUI framework".into()))
        );
        assert_eq!(
            destination_for_input("about:blank"),
            Ok(Destination::Url("about:blank".into()))
        );
    }

    #[test]
    fn uses_http_for_local_addresses() {
        for address in ["localhost:8000", "127.0.0.2:8080", "[::1]:3000"] {
            let Destination::Url(url) = destination_for_input(address).unwrap() else {
                panic!("expected a local URL for {address}");
            };
            assert!(url.starts_with("http://"), "{url}");
        }
    }

    #[test]
    fn encodes_search_queries_as_utf8_url_components() {
        assert_eq!(
            search_url(SearchEngine::Google, &[], "best Rust GUI / framework?"),
            "https://www.google.com/search?q=best%20Rust%20GUI%20%2F%20framework%3F"
        );
        assert_eq!(
            url_for_address("café"),
            Ok("https://duckduckgo.com/?q=caf%C3%A9".into())
        );
    }

    #[test]
    fn preserves_existing_address_behaviour_and_rejects_unsupported_schemes() {
        for url in [
            "http://example.com",
            "HTTPS://Example.com/a?b#c",
            "file:///tmp/page.html",
            "about:blank",
        ] {
            assert_eq!(url_for_address(url).as_deref(), Ok(url));
        }
        assert!(url_for_address("ftp://example.com").is_err());
        assert!(url_for_address("  ").is_err());
    }

    #[test]
    fn adds_https_for_domains_and_preserves_file_paths() {
        assert_eq!(
            url_for_address("  example.com/path ").as_deref(),
            Ok("https://example.com/path")
        );
        assert_eq!(
            url_for_address("/tmp/page.html").as_deref(),
            Ok("file:///tmp/page.html")
        );
    }

    #[test]
    fn custom_engines_fill_every_placeholder_and_resolve_by_key() {
        let custom =
            [
                CustomSearchEngine::new("kagi", "Kagi", "https://kagi.com/search?q=%s&src=%s")
                    .unwrap(),
            ];
        let kagi = SearchEngine::from_config_key("kagi", &custom).unwrap();
        assert_eq!(kagi, SearchEngine::Custom(0));
        assert_eq!(kagi.label(&custom), "Kagi");
        assert_eq!(kagi.config_key(&custom), "kagi");
        assert_eq!(
            search_url(kagi, &custom, "a b"),
            "https://kagi.com/search?q=a%20b&src=a%20b"
        );
        assert_eq!(SearchEngine::all(&custom).count(), 5);
        // A stale index falls back to the default provider rather than panicking.
        assert_eq!(SearchEngine::Custom(9).label(&custom), "DuckDuckGo");
    }

    #[test]
    fn custom_engines_reject_bad_keys_and_templates() {
        let url = "https://example.com/?q=%s";
        assert!(CustomSearchEngine::new("google", "", url).is_err());
        assert!(CustomSearchEngine::new("my engine", "", url).is_err());
        assert!(CustomSearchEngine::new("x", "", "https://example.com/").is_err());
        assert!(CustomSearchEngine::new("x", "", "ftp://example.com/%s").is_err());
        assert_eq!(CustomSearchEngine::new("x", " ", url).unwrap().name(), "x");
    }
}
