// SPDX-License-Identifier: GPL-3.0-only

//! The compact site controls popover and its Rune overlay host.

use rune::tok::type_;
use rune::{El, Spring, Transition, Tx, el, icon, row, text};

use crate::app::AppState;
use crate::permissions::{PermissionKind, PermissionState, PermissionStore};
use crate::ui::controls::icon_button;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

const WIDTH: f32 = 356.0;
const RADIUS: f32 = 20.0;

pub(crate) fn view(state: &AppState, theme: &Theme, layer: &mut Vec<El>, reduced_motion: bool) {
    if !state.site_permissions_open {
        return;
    }
    layer.push(
        el().key("site-permissions-dismiss-area")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .on_press(|state: &mut AppState| state.site_permissions_open = false),
    );

    let x = (state.sidebar.width - 76.0).max(12.0);
    let y = theme::HEADER_HEIGHT + theme::FIELD_HEIGHT + theme::SIDEBAR_GAP;
    let panel = panel(state, theme, reduced_motion);
    layer.push(panel.absolute(x, y));
}

fn panel(state: &AppState, theme: &Theme, reduced_motion: bool) -> El {
    let tab = state.browser.active_tab();
    let url = tab
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty());
    let origin = url.and_then(PermissionStore::origin);
    let panel = rune::col()
        .key("site-permissions-popover")
        .w(WIDTH)
        .max_h(510.0)
        .px(12.0)
        .py(12.0)
        .gap(10.0)
        .radius(RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .shadow(10.0, 24.0, theme.overlay_shadow)
        .transition(if reduced_motion {
            Transition::none()
        } else {
            Transition::offset(0.0, -5.0).combined(Transition::opacity())
        })
        .token_motion(if reduced_motion {
            Spring::NONE
        } else {
            theme::OMNIBOX_MOTION
        });

    let copy = url.map(|url| {
        let copy_url = url.to_owned();
        icon_button("copy-page-url", "Copy page URL", &icons::COPY, 34.0, theme).on_press_tx(
            move |state: &mut AppState, tx: &mut Tx| {
                tx.copy(&copy_url);
                state.site_permissions_open = false;
            },
        )
    });
    let actions = row()
        .key("site-popover-actions")
        .w_full()
        .gap(7.0)
        .child(copy.unwrap_or_else(|| {
            icon_button("copy-page-url", "Copy page URL", &icons::COPY, 34.0, theme)
                .disabled(true)
                .opacity(theme.disabled_opacity)
        }));

    let mut content = panel.child(actions).child(
        text(if origin.is_some() {
            "Site permissions"
        } else {
            "This page"
        })
        .key("site-permissions-heading")
        .font(rune::TextStyle::new(15.0, 600.0))
        .color(theme.text)
        .mt(2.0),
    );

    if let Some(origin) = origin.as_deref() {
        for kind in PermissionKind::ALL {
            content = content.child(permission_row(state, theme, origin, kind));
        }
        content = content
            .child(separator(theme))
            .child(page_info(theme, url.unwrap_or_default(), origin))
            .child(
                text_button("Reset permissions for this site", theme)
                    .on_press(|state: &mut AppState| state.reset_site_permissions()),
            );
    } else {
        let message = if tab.is_none() {
            "Open a website to manage its permissions."
        } else {
            "Elaris pages do not use website permissions."
        };
        content = content
            .child(
                text(message)
                    .font(type_::BODY)
                    .color(theme.text_muted)
                    .py(8.0),
            )
            .child(separator(theme))
            .child(page_info(
                theme,
                url.unwrap_or("No page loaded"),
                "Elaris internal page",
            ));
    }
    content
}

fn permission_row(state: &AppState, theme: &Theme, origin: &str, kind: PermissionKind) -> El {
    let glyph = match kind {
        PermissionKind::Microphone => &icons::MICROPHONE,
        PermissionKind::Camera => &icons::CAMERA,
        PermissionKind::Location => &icons::LOCATION,
        PermissionKind::Notifications => &icons::NOTIFICATIONS,
    };
    let current = match state.permission_decision(origin, kind) {
        crate::permissions::PermissionDecision::Prompt => PermissionState::Ask,
        crate::permissions::PermissionDecision::Allow => PermissionState::Allow,
        crate::permissions::PermissionDecision::Block => PermissionState::Block,
    };
    let row = row()
        .key(format!("permission-{}", kind.key()))
        .w_full()
        .h(38.0)
        .gap(8.0)
        .center_y()
        .child(icon(glyph, theme::ICON).color(theme.icon))
        .child(
            text(kind.label())
                .font(type_::BODY)
                .color(theme.text)
                .grow(1.0),
        );
    let mut choices = rune::row();
    for (permission, label) in [
        (PermissionState::Ask, "Ask"),
        (PermissionState::Allow, "Allow"),
        (PermissionState::Block, "Block"),
    ] {
        let selected = current == permission;
        let button = text(label)
            .font(type_::LABEL)
            .color(if selected {
                theme.text
            } else {
                theme.text_muted
            })
            .center()
            .w(47.0)
            .h(27.0)
            .radius(8.0)
            .fill(if selected {
                theme.overlay_selected
            } else {
                theme.overlay_panel
            })
            .hover(theme.hover)
            .press(theme.press)
            .label(format!("{}: {label}", kind.label()))
            .role("button")
            .on_press(move |state: &mut AppState| state.set_site_permission(kind, permission));
        choices = choices.child(button);
    }
    row.child(choices)
}

fn page_info(theme: &Theme, url: &str, origin: &str) -> El {
    let secure = url.starts_with("https://");
    let title = if secure {
        "Secure connection"
    } else {
        "Not secure"
    };
    rune::col()
        .key("site-page-info")
        .w_full()
        .gap(3.0)
        .child(text(title).font(type_::LABEL).color(theme.text))
        .child(
            text(origin)
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate(),
        )
}

fn separator(theme: &Theme) -> El {
    el().key("site-permissions-separator")
        .w_full()
        .h(1.0)
        .fill(theme.overlay_border)
}

fn text_button(label: &'static str, theme: &Theme) -> El {
    text(label)
        .key("reset-site-permissions")
        .font(type_::LABEL)
        .color(theme.text_muted)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .hover(theme.hover)
        .press(theme.press)
        .role("button")
        .label(label)
}
