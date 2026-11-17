// SPDX-License-Identifier: GPL-3.0-only

//! Turns what the user types in the address bar into a URL to load.

/// Schemes the address bar loads as typed.
const SCHEMES: [&str; 3] = ["http", "https", "file"];
/// Opaque URLs the address bar loads as typed.
const OPAQUE_PREFIXES: [&str; 2] = ["about:", "data:"];
/// Hosts reached over plain HTTP when typed without a scheme.
const LOCAL_HOSTS: [&str; 2] = ["localhost", "127.0.0.1"];

pub(crate) fn url_for_address(input: &str) -> Result<String, &'static str> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter an address");
    }
    if let Some((scheme, _)) = input.split_once("://") {
        return if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
            Ok(input.to_owned())
        } else {
            Err("Only http, https and file addresses are supported")
        };
    }
    let lowercase = input.to_ascii_lowercase();
    if OPAQUE_PREFIXES.iter().any(|p| lowercase.starts_with(p)) {
        return Ok(input.to_owned());
    }
    if input.starts_with('/') {
        return Ok(format!("file://{input}"));
    }
    if input.contains(char::is_whitespace) {
        return Err("Search is not supported yet; enter an address");
    }
    let host = lowercase
        .split([':', '/', '?', '#'])
        .next()
        .unwrap_or_default();
    if LOCAL_HOSTS.contains(&host) {
        Ok(format!("http://{input}"))
    } else if host.contains('.') {
        Ok(format!("https://{input}"))
    } else {
        Err("Enter an address such as example.com")
    }
}

#[cfg(test)]
mod tests {
    use super::url_for_address;

    #[test]
    fn keeps_supported_urls() {
        for url in [
            "http://example.com",
            "HTTPS://Example.com/a?b#c",
            "file:///tmp/page.html",
            "about:blank",
        ] {
            assert_eq!(url_for_address(url).as_deref(), Ok(url));
        }
    }

    #[test]
    fn adds_a_default_scheme() {
        assert_eq!(
            url_for_address("  example.com/path ").as_deref(),
            Ok("https://example.com/path")
        );
        assert_eq!(
            url_for_address("localhost:8000").as_deref(),
            Ok("http://localhost:8000")
        );
        assert_eq!(
            url_for_address("/tmp/page.html").as_deref(),
            Ok("file:///tmp/page.html")
        );
    }

    #[test]
    fn rejects_what_it_cannot_load() {
        for input in ["", "   ", "ftp://example.com", "hello world", "example"] {
            assert!(url_for_address(input).is_err(), "{input}");
        }
    }
}
