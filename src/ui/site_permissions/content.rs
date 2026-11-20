// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::type_;
use rune::{El, PointerPress, Spring, Transition, Tx, el, icon, row, text};

use crate::app::{self, AppState};
use crate::permissions::{PermissionKind, PermissionState};
use crate::site_controls::{SiteControlAction, SiteControlPresentation, SiteControlsPage};
use crate::ui::icons;
use crate::ui::site_control_style as design;
use crate::ui::theme::{self, Theme};

use super::menus::permission_label;

pub(super) fn popover(
    state: &AppState,
    theme: &Theme,
    presentation: &SiteControlPresentation,
    reduced_motion: bool,
) -> El {
    let shell = rune::col()
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
        .transition(theme::overlay_reveal(reduced_motion))
        .token_motion(if reduced_motion {
            Spring::NONE
        } else {
            theme::OMNIBOX_MOTION
        });

    if state.site_controls_page == SiteControlsPage::Security {
        let details = security_page(theme, presentation)
            .key("site-controls-security-page")
            .transition(if reduced_motion {
                Transition::none()
            } else {
                Transition::opacity()
            });
        return shell.child(details);
    }

    let mut main = rune::col()
        .key("site-controls-main-page")
        .w_full()
        .gap(0.0)
        .transition(if reduced_motion {
            Transition::none()
        } else {
            Transition::opacity()
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
            main = main.child(permission_row(theme, kind, permission).mt(design::ROW_GAP));
        }
        main = main
            .child(separator(theme).mt(10.0))
            .child(footer(theme, presentation).mt(8.0));
    } else {
        let message = if presentation.url.is_some() {
            "Elaris pages do not use website permissions."
        } else {
            "Open a website to manage its permissions."
        };
        main = main.child(
            text(message)
                .font(type_::BODY)
                .color(theme.text_muted)
                .py(6.0)
                .mt(4.0),
        );
    }
    shell.child(main)
}

fn security_page(theme: &Theme, presentation: &SiteControlPresentation) -> El {
    let hostname = presentation
        .origin
        .as_deref()
        .and_then(|origin| origin.rsplit("//").next())
        .unwrap_or("This site");
    let (status, description, glyph) = if presentation.https_scheme {
        (
            "Connection is secure",
            "Your information, such as passwords and payment details, is encrypted when sent to this site.",
            &icons::LOCK,
        )
    } else {
        (
            "Connection is not secure",
            "Avoid sending passwords or payment information to this site.",
            &icons::INFO,
        )
    };
    let header = row()
        .key("site-security-header")
        .w_full()
        .h(40.0)
        .gap(8.0)
        .center_y()
        .child(
            crate::ui::controls::icon_button(
                "site-security-back",
                "Back to site settings",
                &icons::BACK,
                32.0,
                theme,
            )
            .on_press(|state: &mut AppState| {
                state.site_controls_page = SiteControlsPage::Main;
            }),
        )
        .child(
            rune::col()
                .grow(1.0)
                .gap(1.0)
                .child(
                    text("Security")
                        .font(rune::TextStyle::new(14.0, 600.0))
                        .color(theme.text),
                )
                .child(
                    text(hostname)
                        .font(rune::TextStyle::new(11.5, 400.0))
                        .color(theme.text_muted)
                        .single_line()
                        .truncate(),
                ),
        )
        .child(
            crate::ui::controls::icon_button(
                "site-security-close",
                "Close site controls",
                &icons::CLOSE,
                28.0,
                theme,
            )
            .on_press(|state: &mut AppState| {
                state.site_permissions_open = false;
                state.site_controls_page = SiteControlsPage::Main;
            }),
        );

    rune::col()
        .w_full()
        .gap(10.0)
        .child(header)
        .child(separator(theme).mt(4.0))
        .child(
            row()
                .key("site-security-status")
                .w_full()
                .gap(12.0)
                .px(8.0)
                .py(12.0)
                .center_y()
                .child(icon(glyph, 20.0).color(theme.text))
                .child(
                    rune::col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(4.0)
                        .child(
                            text(status)
                                .font(rune::TextStyle::new(13.0, 600.0))
                                .color(theme.text),
                        )
                        .child(
                            text(description)
                                .font(rune::TextStyle::new(11.5, 400.0).lh(16.0))
                                .color(theme.text_muted)
                                .wrap(),
                        ),
                ),
        )
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
        let url = presentation.url.clone();
        let tab = presentation.tab;
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
                    if let Some(url) = url.as_deref() {
                        tx.copy(url);
                    }
                }
                SiteControlAction::Reload => {
                    if let Some(tab) = tab {
                        state.browser.reload(tab);
                    }
                }
                SiteControlAction::DuplicateTab => {
                    if let Some(tab) = tab {
                        state.duplicate(tab);
                    }
                }
                SiteControlAction::Settings => app::open_settings(state, tx),
            });
        actions = actions.child(tile);
    }
    actions
}

fn permission_row(theme: &Theme, kind: PermissionKind, permission: PermissionState) -> El {
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
    row()
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
        })
}

fn footer(theme: &Theme, presentation: &SiteControlPresentation) -> El {
    let (label, glyph) = if presentation.https_scheme {
        ("Connection is secure", &icons::LOCK)
    } else {
        ("Connection is not secure", &icons::INFO)
    };
    row()
        .key("site-controls-footer")
        .w_full()
        .h(design::FOOTER_HEIGHT)
        .gap(4.0)
        .center_y()
        .child(
            row()
                .key("site-security-chip")
                .h(32.0)
                .grow(1.0)
                .px(9.0)
                .gap(8.0)
                .center_y()
                .radius(10.0)
                .fill(theme.overlay_selected)
                .hover(theme.hover)
                .press(theme.press)
                .press_scale(theme::PRESS_SCALE)
                .label(label)
                .help("Show connection security details")
                .role("button")
                .focusable(true)
                .child(icon(glyph, 16.0).color(theme.text))
                .child(
                    text(label)
                        .font(rune::TextStyle::new(11.5, 500.0))
                        .color(theme.text)
                        .single_line()
                        .truncate()
                        .grow(1.0),
                )
                .child(icon(&icons::CHEVRON_RIGHT, 12.0).color(theme.text_muted))
                .on_press(|state: &mut AppState| {
                    state.site_controls_page = SiteControlsPage::Security;
                    state.site_overflow_open = false;
                }),
        )
        .child(
            crate::ui::controls::icon_button(
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

fn separator(theme: &Theme) -> El {
    el().key("site-controls-separator")
        .w_full()
        .h(1.0)
        .fill(theme.overlay_border)
}
