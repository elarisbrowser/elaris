// SPDX-License-Identifier: GPL-3.0-only

//! Per-origin website permission overrides and request decisions.

use std::collections::BTreeMap;

/// A permission that a website can request from Elaris.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PermissionKind {
    Microphone,
    Camera,
    Location,
    Notifications,
}

impl PermissionKind {
    pub(crate) const ALL: [Self; 4] = [
        Self::Microphone,
        Self::Camera,
        Self::Location,
        Self::Notifications,
    ];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Camera => "camera",
            Self::Location => "location",
            Self::Notifications => "notifications",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Microphone => "Microphone",
            Self::Camera => "Camera",
            Self::Location => "Location",
            Self::Notifications => "Notifications",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

/// A saved choice for one permission on one origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PermissionState {
    #[default]
    Ask,
    Allow,
    Block,
}

impl PermissionState {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "ask" => Some(Self::Ask),
            "allow" => Some(Self::Allow),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

/// The outcome an engine request should apply without making UI decisions itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermissionDecision {
    Prompt,
    Allow,
    Block,
}

/// Persistent site permission overrides keyed by normalized HTTP(S) origin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PermissionStore {
    sites: BTreeMap<String, BTreeMap<PermissionKind, PermissionState>>,
}

impl PermissionStore {
    pub(crate) fn origin(url: &str) -> Option<String> {
        let (scheme, rest) = url.trim().split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let authority = rest.split(['/', '?', '#']).next()?;
        let authority = authority.rsplit('@').next()?;
        if authority.is_empty() {
            return None;
        }
        let authority = authority.to_ascii_lowercase();
        let authority = match (
            scheme.as_str(),
            authority.strip_suffix(":80"),
            authority.strip_suffix(":443"),
        ) {
            ("http", Some(host), _) => host,
            ("https", _, Some(host)) => host,
            _ => authority.as_str(),
        };
        Some(format!("{scheme}://{authority}"))
    }

    pub(crate) fn state(&self, origin: &str, kind: PermissionKind) -> PermissionState {
        self.sites
            .get(origin)
            .and_then(|permissions| permissions.get(&kind))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn decision(&self, origin: &str, kind: PermissionKind) -> PermissionDecision {
        match self.state(origin, kind) {
            PermissionState::Ask => PermissionDecision::Prompt,
            PermissionState::Allow => PermissionDecision::Allow,
            PermissionState::Block => PermissionDecision::Block,
        }
    }

    pub(crate) fn set(&mut self, origin: &str, kind: PermissionKind, state: PermissionState) {
        if state == PermissionState::Ask {
            if let Some(site) = self.sites.get_mut(origin) {
                site.remove(&kind);
                if site.is_empty() {
                    self.sites.remove(origin);
                }
            }
        } else {
            self.sites
                .entry(origin.to_owned())
                .or_default()
                .insert(kind, state);
        }
    }

    pub(crate) fn reset(&mut self, origin: &str) {
        self.sites.remove(origin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_uses_only_http_origins_and_discards_path_and_credentials() {
        assert_eq!(
            PermissionStore::origin("HTTPS://user:pass@Example.COM:443/a?q=1"),
            Some("https://example.com".to_owned())
        );
        assert_eq!(PermissionStore::origin("elaris://settings"), None);
        assert_eq!(PermissionStore::origin("file:///tmp/page"), None);
    }

    #[test]
    fn overrides_default_to_prompt_and_ask_removes_the_override() {
        let mut store = PermissionStore::default();
        let origin = "https://example.com";
        assert_eq!(
            store.decision(origin, PermissionKind::Camera),
            PermissionDecision::Prompt
        );
        store.set(origin, PermissionKind::Camera, PermissionState::Allow);
        assert_eq!(
            store.decision(origin, PermissionKind::Camera),
            PermissionDecision::Allow
        );
        store.set(origin, PermissionKind::Camera, PermissionState::Ask);
        assert_eq!(
            store.decision(origin, PermissionKind::Camera),
            PermissionDecision::Prompt
        );
    }
}
