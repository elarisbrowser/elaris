// SPDX-License-Identifier: GPL-3.0-only

//! Per-origin website permission overrides and request decisions.

use std::collections::BTreeMap;

/// A permission that a website can request from Elaris.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionKind {
    Microphone,
    Camera,
    Location,
    Notifications,
}

impl PermissionKind {
    pub const ALL: [Self; 4] = [
        Self::Microphone,
        Self::Camera,
        Self::Location,
        Self::Notifications,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Camera => "camera",
            Self::Location => "location",
            Self::Notifications => "notifications",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Microphone => 0,
            Self::Camera => 1,
            Self::Location => 2,
            Self::Notifications => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Microphone => "Microphone",
            Self::Camera => "Camera",
            Self::Location => "Location",
            Self::Notifications => "Notifications",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

/// A saved choice for one permission on one origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionState {
    #[default]
    Ask,
    Allow,
    Block,
}

impl PermissionState {
    #[cfg(feature = "ladybird-bridge")]
    pub fn choice(self) -> Option<bool> {
        match self {
            Self::Ask => None,
            Self::Allow => Some(true),
            Self::Block => Some(false),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
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
pub enum PermissionDecision {
    Prompt,
    Allow,
    Block,
}

/// Whether pages may start media playback on their own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoplayPolicy {
    Allow,
    /// Silent or muted media may still play, matching Ladybird's default.
    #[default]
    BlockAudio,
    Block,
}

impl AutoplayPolicy {
    pub const ALL: [Self; 3] = [Self::Allow, Self::BlockAudio, Self::Block];

    pub fn key(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::BlockAudio => "block-audio",
            Self::Block => "block",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "Allow",
            Self::BlockAudio => "Block audio",
            Self::Block => "Block all",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|policy| policy.key() == key)
    }

    /// The bridge's policy number, in Ladybird's order.
    #[cfg(feature = "ladybird-bridge")]
    pub fn engine_value(self) -> i32 {
        match self {
            Self::Allow => 0,
            Self::BlockAudio => 1,
            Self::Block => 2,
        }
    }
}

/// Persistent site permission overrides keyed by normalized HTTP(S) origin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionStore {
    sites: BTreeMap<String, BTreeMap<PermissionKind, PermissionState>>,
    blocked_by_default: std::collections::BTreeSet<PermissionKind>,
    /// Sites whose autoplay differs from the browser default.
    autoplay: BTreeMap<String, AutoplayPolicy>,
}

impl PermissionStore {
    pub fn autoplay(&self, origin: &str) -> Option<AutoplayPolicy> {
        self.autoplay.get(origin).copied()
    }

    /// Sets a site's autoplay exception; `None` returns it to the browser default.
    pub fn set_autoplay(&mut self, origin: &str, policy: Option<AutoplayPolicy>) {
        match policy {
            Some(policy) => self.autoplay.insert(origin.to_owned(), policy),
            None => self.autoplay.remove(origin),
        };
    }

    pub fn autoplay_entries(&self) -> impl Iterator<Item = (&str, AutoplayPolicy)> {
        self.autoplay
            .iter()
            .map(|(origin, &policy)| (origin.as_str(), policy))
    }

    pub fn default_blocked(&self, kind: PermissionKind) -> bool {
        self.blocked_by_default.contains(&kind)
    }

    pub fn block_by_default(&mut self, kind: PermissionKind) {
        self.blocked_by_default.insert(kind);
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, PermissionKind, PermissionState)> {
        self.sites.iter().flat_map(|(origin, permissions)| {
            permissions
                .iter()
                .map(move |(&kind, &state)| (origin.as_str(), kind, state))
        })
    }

    pub fn origin(url: &str) -> Option<String> {
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

    pub fn state(&self, origin: &str, kind: PermissionKind) -> PermissionState {
        self.sites
            .get(origin)
            .and_then(|permissions| permissions.get(&kind))
            .copied()
            .unwrap_or_default()
    }

    pub fn decision(&self, origin: &str, kind: PermissionKind) -> PermissionDecision {
        match self.state(origin, kind) {
            PermissionState::Ask => PermissionDecision::Prompt,
            PermissionState::Allow => PermissionDecision::Allow,
            PermissionState::Block => PermissionDecision::Block,
        }
    }

    pub fn set(&mut self, origin: &str, kind: PermissionKind, state: PermissionState) {
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

    pub fn reset(&mut self, origin: &str) {
        self.sites.remove(origin);
        self.autoplay.remove(origin);
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
