// SPDX-License-Identifier: GPL-3.0-only

//! The presentation data shared by native and Rune site controls.

use crate::app::AppState;
use crate::permissions::{PermissionDecision, PermissionKind, PermissionState, PermissionStore};
use crate::tab::TabId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SiteControlsPage {
    #[default]
    Main,
    Security,
}

/// A useful action available from the site controls action strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SiteControlAction {
    CopyUrl,
    Reload,
    DuplicateTab,
    Settings,
}

impl SiteControlAction {
    pub(crate) const ALL: [Self; 4] = [
        Self::CopyUrl,
        Self::Reload,
        Self::DuplicateTab,
        Self::Settings,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CopyUrl => "Copy page URL",
            Self::Reload => "Reload page",
            Self::DuplicateTab => "Duplicate tab",
            Self::Settings => "Open settings",
        }
    }
}

/// A page and its current saved settings, ready to render in either platform host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteControlPresentation {
    pub(crate) tab: Option<TabId>,
    pub(crate) url: Option<String>,
    pub(crate) origin: Option<String>,
    /// The URL uses HTTPS. This does not represent certificate validation status.
    pub(crate) https_scheme: bool,
    pub(crate) permissions: [(PermissionKind, PermissionState); 4],
}

impl SiteControlPresentation {
    pub(crate) fn from_state(state: &AppState) -> Self {
        let url = state
            .browser
            .active_tab()
            .map(|tab| tab.url.clone())
            .filter(|url| !url.is_empty() && url != "about:blank");
        let origin = url.as_deref().and_then(PermissionStore::origin);
        let https_scheme = url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"));
        let permissions = PermissionKind::ALL.map(|kind| {
            let permission = origin.as_deref().map_or(PermissionState::Ask, |origin| {
                match state.permission_decision(origin, kind) {
                    PermissionDecision::Prompt => PermissionState::Ask,
                    PermissionDecision::Allow => PermissionState::Allow,
                    PermissionDecision::Block => PermissionState::Block,
                }
            });
            (kind, permission)
        });
        Self {
            tab: state.browser.active_tab().map(|tab| tab.id()),
            url,
            origin,
            https_scheme,
            permissions,
        }
    }
}
