// SPDX-License-Identifier: GPL-3.0-only

//! Classifies omnibox input as a URL or a web search.

use std::net::IpAddr;

/// The default search provider. Change this constant to choose another provider.
const SEARCH_URL: &str = "https://www.google.com/search?q=";

/// What the omnibox should do with its input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Url(String),
    Search(String),
}

pub(crate) fn destination_for_input(input: &str) -> Result<Destination, &'static str> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a search or address");
    }

    if let Some((scheme, _)) = input.split_once("://") {
        return if supported_scheme(scheme) {
            Ok(Destination::Url(input.to_owned()))
        } else {
            Err("Only http, https and file addresses are supported")
        };
    }

    let lowercase = input.to_ascii_lowercase();
    if lowercase.starts_with("about:") || lowercase.starts_with("data:") {
        return Ok(Destination::Url(input.to_owned()));
    }
    if input.starts_with('/') {
        return Ok(Destination::Url(format!("file://{input}")));
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
pub(crate) fn url_for_address(input: &str) -> Result<String, &'static str> {
    match destination_for_input(input)? {
        Destination::Url(url) => Ok(url),
        Destination::Search(query) => Ok(search_url(&query)),
    }
}

/// Builds a URL for a search without sending the query until it is submitted.
pub(crate) fn search_url(query: &str) -> String {
    let mut url = String::with_capacity(SEARCH_URL.len() + query.len() * 3);
    url.push_str(SEARCH_URL);
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            url.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url
}

/// The host portion used for local-address detection and tab matching.
pub(crate) fn host_for_address(input: &str) -> &str {
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
        "http" | "https" | "file"
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
    use super::{Destination, destination_for_input, search_url, url_for_address};

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
            search_url("best Rust GUI / framework?"),
            "https://www.google.com/search?q=best%20Rust%20GUI%20%2F%20framework%3F"
        );
        assert_eq!(
            url_for_address("café"),
            Ok("https://www.google.com/search?q=caf%C3%A9".into())
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
}
