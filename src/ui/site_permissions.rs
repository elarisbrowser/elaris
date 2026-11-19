// SPDX-License-Identifier: GPL-3.0-only

//! Site Control Centre for Linux, hosted as a Rune popover below the URL bar.

use rune::tok::type_;
use rune::{El, Key, KeyEvent, PointerPress, Spring, Transition, Tx, el, icon, row, text};
use rune_kit::{MenuItem, context_menu};

use crate::app::{self, AppState};
use crate::permissions::{PermissionKind, PermissionState};
use crate::site_controls::{SiteControlAction, SiteControlPresentation};
use crate::ui::icons;
use crate::ui::site_control_style as design;
use crate::ui::theme::{self, Theme};

pub(crate) fn view(state: &AppState, theme: &Theme, layer: &mut Vec<El>, reduced_motion: bool) {
    if !state.site_permissions_open {
        return;
    }
    let presentation = SiteControlPresentation::from_state(state);
    let anchor = state.site_controls_anchor.unwrap_or([
        state.sidebar.width - 38.0,
        theme::HEADER_HEIGHT + theme::FIELD_HEIGHT,
        22.0,
        22.0,
    ]);
    let x = (anchor[0] + anchor[2] / 2.0 - design::WIDTH / 2.0).max(10.0);
    let arrow_y = anchor[1] + anchor[3] - 1.0;
    let y = arrow_y + 8.0;

    layer.push(
        el().key("site-controls-dismiss-area")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .on_press(|state: &mut AppState| {
                state.site_permissions_open = false;
                state.permission_menu = None;
                state.site_overflow_open = false;
            }),
    );
    layer.push(
        icon(&icons::SITE_POPOVER_ARROW, design::WIDTH)
            .key("site-controls-arrow")
            .size(20.0, 10.0)
            .absolute(x + design::WIDTH / 2.0 - 10.0, arrow_y)
            .color(theme.overlay_panel),
    );
    layer.push(popover(state, theme, &presentation, reduced_motion).absolute(x, y));
    permission_menu(state, theme, layer);
    overflow_menu(state, theme, layer, &presentation);
}

fn popover(
    state: &AppState,
    theme: &Theme,
    presentation: &SiteControlPresentation,
    reduced_motion: bool,
) -> El {
    let mut content = rune::col()
        .key("site-controls-popover")
        .w(design::WIDTH)
        .px(design::PADDING)
        .pt(design::PADDING)
        .pb(design::PADDING)
        .gap(0.0)
        .radius(design::OUTER_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(1.0, theme.overlay_border)
        .shadow(9.0, 22.0, theme.overlay_shadow)
        .transition(if reduced_motion {
            Transition::none()
        } else {
            Transition::offset(0.0, -4.0).combined(Transition::opacity())
        })
        .token_motion(if reduced_motion {
            Spring::NONE
        } else {
            theme::OMNIBOX_MOTION
        })
        .child(action_tiles(presentation, theme))
        .child(
            text("This site")
                .key("site-controls-heading")
                .font(rune::TextStyle::new(12.0, 600.0))
                .color(theme.text)
                .px(6.0)
                .mt(design::SECTION_GAP)
                .mb(4.0),
        );

    if presentation.origin.is_some() {
        for (kind, permission) in presentation.permissions {
            content =
                content.child(permission_row(state, theme, kind, permission).mt(design::ROW_GAP));
        }
        content = content
            .child(separator(theme).mt(10.0))
            .child(footer(theme, presentation).mt(8.0));
    } else {
        let message = if presentation.url.is_some() {
            "Elaris pages do not use website permissions."
        } else {
            "Open a website to manage its permissions."
        };
        content = content.child(
            text(message)
                .font(type_::BODY)
                .color(theme.text_muted)
                .py(6.0)
                .mt(4.0),
        );
    }
    content
}

fn action_tiles(presentation: &SiteControlPresentation, theme: &Theme) -> El {
    let mut actions = row()
        .key("site-control-actions")
        .w_full()
        .gap(design::ACTION_GAP);
    for action in SiteControlAction::ALL {
        let glyph = match action {
            SiteControlAction::CopyUrl => &icons::COPY,
            SiteControlAction::Reload => &icons::RELOAD,
            SiteControlAction::DuplicateTab => &icons::DUPLICATE,
            SiteControlAction::Settings => &icons::SITE_CONTROLS,
        };
        let data = presentation.clone();
        let tile = el()
            .key(format!("site-action-{}", action as usize))
            .w((design::WIDTH - design::PADDING * 2.0 - design::ACTION_GAP * 3.0) / 4.0)
            .h(design::ACTION_HEIGHT)
            .center()
            .radius(design::ACTION_RADIUS)
            .fill(theme.overlay_selected)
            .hover(theme.hover)
            .press(theme.press)
            .press_scale(theme::PRESS_SCALE)
            .label(action.label())
            .help(action.label())
            .role("button")
            .focusable(true)
            .child(icon(glyph, design::GLYPH).color(theme.text))
            .on_press_tx(move |state: &mut AppState, tx: &mut Tx| match action {
                SiteControlAction::CopyUrl => {
                    if let Some(url) = data.url.as_deref() {
                        tx.copy(url);
                    }
                }
                SiteControlAction::Reload => {
                    if let Some(tab) = data.tab {
                        state.browser.reload(tab);
                    }
                }
                SiteControlAction::DuplicateTab => {
                    if let Some(tab) = data.tab {
                        state.duplicate(tab);
                    }
                }
                SiteControlAction::Settings => app::open_settings(state, tx),
            });
        actions = actions.child(tile);
    }
    actions
}

fn permission_row(
    _state: &AppState,
    theme: &Theme,
    kind: PermissionKind,
    permission: PermissionState,
) -> El {
    let glyph = match kind {
        PermissionKind::Microphone => &icons::MICROPHONE,
        PermissionKind::Camera => &icons::CAMERA,
        PermissionKind::Location => &icons::LOCATION,
        PermissionKind::Notifications => &icons::NOTIFICATIONS,
    };
    let active = permission == PermissionState::Allow;
    let circle = if active {
        theme.text
    } else {
        theme.overlay_selected
    };
    let glyph_color = if active {
        theme.overlay_panel
    } else {
        theme.text_muted
    };
    let label = permission_label(permission);
    let row = row()
        .key(format!("site-permission-{}", kind.key()))
        .w_full()
        .h(design::ROW_HEIGHT)
        .px(6.0)
        .gap(10.0)
        .center_y()
        .radius(design::ROW_RADIUS)
        .hover(theme.hover)
        .press(theme.press)
        .child(
            el().size(design::ICON_CIRCLE, design::ICON_CIRCLE)
                .center()
                .radius(design::ICON_CIRCLE / 2.0)
                .fill(circle)
                .child(icon(glyph, design::ROW_GLYPH).color(glyph_color)),
        )
        .child(
            rune::col()
                .grow(1.0)
                .gap(1.0)
                .child(
                    text(kind.label())
                        .font(rune::TextStyle::new(12.5, 500.0))
                        .color(theme.text),
                )
                .child(
                    text(label)
                        .font(rune::TextStyle::new(11.5, 400.0))
                        .color(theme.text_muted),
                ),
        )
        .role("button")
        .label(format!("{}, {label}", kind.label()))
        .help("Choose Ask, Allow or Block")
        .focusable(true)
        .on_press(move |state: &mut AppState| state.permission_menu = Some(kind))
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            state.permission_row_anchors[kind.index()] = Some(rect);
            tx.live_only();
        });
    row
}

fn permission_menu(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some(kind) = state.permission_menu else {
        return;
    };
    let Some(rect) = state.permission_row_anchors[kind.index()] else {
        return;
    };
    let origin = state.current_permission_origin();
    let selected = origin
        .as_deref()
        .map(|origin| state.permission_store.state(origin, kind));
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
        menu.item(
            MenuItem::new(label).on_select_tx(move |state: &mut AppState, _| {
                state.set_site_permission(kind, permission);
                state.permission_menu = None;
            }),
        )
    });
    menu.build(&theme.kit(theme.overlay_panel), layer);
}

fn footer(theme: &Theme, presentation: &SiteControlPresentation) -> El {
    let secure_label = if presentation.https_scheme {
        "HTTPS"
    } else {
        "HTTP"
    };
    let connection_icon = &icons::INFO;
    let hostname = presentation
        .origin
        .as_deref()
        .and_then(|origin| origin.rsplit("//").next());
    row()
        .key("site-controls-footer")
        .w_full()
        .h(design::FOOTER_HEIGHT)
        .center_y()
        .child(
            row()
                .key("site-security-chip")
                .h(28.0)
                .px(10.0)
                .gap(6.0)
                .center_y()
                .radius(10.0)
                .fill(theme.overlay_selected)
                .child(icon(connection_icon, 10.0).color(theme.text))
                .child(
                    text(secure_label)
                        .font(rune::TextStyle::new(12.0, 500.0))
                        .color(theme.text),
                ),
        )
        .child(if let Some(hostname) = hostname {
            text(hostname)
                .grow(1.0)
                .shrink()
                .single_line()
                .truncate()
                .font(rune::TextStyle::new(11.5, 400.0))
                .color(theme.text_muted)
        } else {
            el().grow(1.0)
        })
        .child(
            super::controls::icon_button(
                "site-controls-overflow",
                "More site actions",
                &icons::MORE,
                28.0,
                theme,
            )
            .on_press(|state: &mut AppState| state.site_overflow_open = true)
            .on_layout_tx(|state: &mut AppState, rect, tx| {
                state.site_overflow_anchor = Some(PointerPress {
                    x: rect[0] + rect[2] / 2.0,
                    y: rect[1] + rect[3] / 2.0,
                });
                tx.live_only();
            }),
        )
}

fn overflow_menu(
    state: &AppState,
    theme: &Theme,
    layer: &mut Vec<El>,
    presentation: &SiteControlPresentation,
) {
    let (true, Some(point)) = (state.site_overflow_open, state.site_overflow_anchor) else {
        return;
    };
    let url = presentation.url.clone();
    let menu = context_menu("site-controls-overflow-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.site_overflow_open = false)
        .item(MenuItem::new("Reset permissions for this site").on_select(
            |state: &mut AppState| {
                state.reset_site_permissions();
                state.site_overflow_open = false;
            },
        ));
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
    menu.build(&theme.kit(theme.overlay_panel), layer);
}

fn separator(theme: &Theme) -> El {
    el().key("site-controls-separator")
        .w_full()
        .h(1.0)
        .fill(theme.overlay_border)
}

fn permission_label(permission: PermissionState) -> &'static str {
    match permission {
        PermissionState::Ask => "Ask",
        PermissionState::Allow => "Allowed",
        PermissionState::Block => "Blocked for this site",
    }
}

pub(crate) fn escape(state: &mut AppState, event: &KeyEvent) -> bool {
    if event.key != Key::Escape {
        return false;
    }
    if state.permission_menu.is_some() {
        state.permission_menu = None;
        return true;
    }
    if state.site_overflow_open {
        state.site_overflow_open = false;
        return true;
    }
    if state.site_permissions_open {
        state.site_permissions_open = false;
        return true;
    }
    false
}
