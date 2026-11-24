// SPDX-License-Identifier: GPL-3.0-only

use crate::ui::components::menu::{MenuItem, context_menu};
use rune::{El, Key, KeyEvent, PointerPress, Tx, el};

use crate::app::AppState;
use crate::site_controls::SiteControlPresentation;
use crate::site_controls::SiteControlsPage;
use crate::ui::theme::Theme;
use elaris_core::permissions::{AutoplayPolicy, PermissionState};

pub(super) fn permission_menu(
    state: &AppState,
    theme: &Theme,
    layer: &mut Vec<El>,
    presentation: &SiteControlPresentation,
) {
    if !presentation.permissions_ready {
        return;
    }
    let Some(kind) = state.permission_menu else {
        return;
    };
    if presentation.private && !crate::engine::PRIVATE_PERMISSION_KINDS.contains(&kind) {
        return;
    }
    let Some(rect) = state.permission_row_anchors[kind.index()] else {
        return;
    };
    let selected = if presentation.private {
        Some(presentation.permissions[kind.index()].1)
    } else {
        state
            .current_permission_origin()
            .as_deref()
            .map(|origin| state.permission_store.state(origin, kind))
    };
    let point = PointerPress {
        x: rect[0] + rect[2] - 8.0,
        y: rect[1] + rect[3] / 2.0,
    };
    let menu = context_menu("site-permission-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.permission_menu = None);
    let menu = [
        (PermissionState::Ask, "Ask"),
        (PermissionState::Allow, "Allow"),
        (PermissionState::Block, "Block"),
    ]
    .into_iter()
    .fold(menu, |menu, (permission, label)| {
        let label = if Some(permission) == selected {
            format!("{label}  ✓")
        } else {
            label.to_owned()
        };
        let private_site = presentation.private.then(|| presentation.clone());
        menu.item(
            MenuItem::new(label).on_select_tx(move |state: &mut AppState, _| {
                if private_site
                    .as_ref()
                    .is_none_or(|site| site.matches_active_site(state))
                {
                    state.set_site_permission(kind, permission);
                }
                state.permission_menu = None;
            }),
        )
    });
    menu.build(state, theme, layer);
}

pub(super) fn autoplay_menu(
    state: &AppState,
    theme: &Theme,
    layer: &mut Vec<El>,
    presentation: &SiteControlPresentation,
) {
    if presentation.private {
        return;
    }
    let (true, Some(rect)) = (state.autoplay_menu_open, state.autoplay_row_anchor) else {
        return;
    };
    let point = PointerPress {
        x: rect[0] + rect[2] - 8.0,
        y: rect[1] + rect[3] / 2.0,
    };
    let menu = context_menu("site-autoplay-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.autoplay_menu_open = false);
    let default = format!("Default ({})", presentation.autoplay_default.label());
    let options = std::iter::once((None, default)).chain(
        AutoplayPolicy::ALL
            .into_iter()
            .map(|policy| (Some(policy), policy.label().to_owned())),
    );
    let menu = options.fold(menu, |menu, (policy, label)| {
        let label = if policy == presentation.autoplay {
            format!("{label}  ✓")
        } else {
            label
        };
        menu.item(MenuItem::new(label).on_select(move |state: &mut AppState| {
            state.set_site_autoplay(policy);
            state.autoplay_menu_open = false;
        }))
    });
    menu.build(state, theme, layer);
}

pub(super) fn overflow_menu(
    state: &AppState,
    theme: &Theme,
    layer: &mut Vec<El>,
    presentation: &SiteControlPresentation,
) {
    let (true, Some(point)) = (state.site_overflow_open, state.site_overflow_anchor) else {
        return;
    };
    let url = presentation.url.clone();
    let mut menu = context_menu("site-controls-overflow-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.site_overflow_open = false);
    if !presentation.private || presentation.permissions_ready {
        let reset_label = if presentation.private {
            "Reset private permissions for this site"
        } else {
            "Reset settings for this site"
        };
        let private_site = presentation.private.then(|| presentation.clone());
        menu = menu.item(
            MenuItem::new(reset_label).on_select(move |state: &mut AppState| {
                if private_site
                    .as_ref()
                    .is_none_or(|site| site.matches_active_site(state))
                {
                    state.reset_site_permissions();
                }
                state.site_overflow_open = false;
            }),
        );
    }
    let menu = if let Some(url) = url {
        menu.item(MenuItem::new("Copy page address").on_select_tx(
            move |state: &mut AppState, tx: &mut Tx| {
                tx.copy(&url);
                state.site_overflow_open = false;
            },
        ))
    } else {
        menu
    };
    menu.build(state, theme, layer);
}

pub(super) fn permission_label(permission: PermissionState) -> &'static str {
    match permission {
        PermissionState::Ask => "Ask",
        PermissionState::Allow => "Allowed",
        PermissionState::Block => "Blocked for this site",
    }
}

pub(super) fn escape(state: &mut AppState, event: &KeyEvent) -> bool {
    if event.key != Key::Escape {
        return false;
    }
    if state.permission_menu.is_some() || state.autoplay_menu_open {
        state.permission_menu = None;
        state.autoplay_menu_open = false;
        return true;
    }
    if state.site_overflow_open {
        state.site_overflow_open = false;
        return true;
    }
    if state.site_controls_page == SiteControlsPage::Security {
        state.site_controls_page = SiteControlsPage::Main;
        return true;
    }
    if state.site_permissions_open {
        state.site_permissions_open = false;
        return true;
    }
    false
}
