// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar: navigation beside the window controls, the address field, the open tabs, and
//! the new-tab and appearance buttons.

use rune::tok::type_;
use rune::{Color, Cx, El, Icon, ScrollSpec, col, el, icon, row, spacer, text, text_input};

use super::theme::{self, Theme};
use super::{ADDRESS, icons};
use crate::app::{self, AppState};
use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::tab::Tab;

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(theme::SIDEBAR_WIDTH)
        .h_full()
        .px(theme::SIDEBAR_PADDING)
        .pb(theme::SIDEBAR_PADDING)
        .gap(theme::SIDEBAR_GAP)
        .child(header(state, theme, cx))
        .child(address(state, theme));
    if let Some(status) = status(state) {
        sidebar = sidebar.child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(theme.text_muted)
                .px(theme::FIELD_PADDING)
                .truncate(),
        );
    }
    sidebar
        .child(tabs(state, theme))
        .child(appearance(state.appearance, theme))
}

/// Level with the window controls: back, forward and reload for the active tab.
fn header(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    let tab = state.browser.active_tab();
    let can_go_back = tab.is_some_and(|tab| tab.can_go_back);
    let can_go_forward = tab.is_some_and(|tab| tab.can_go_forward);
    row()
        .key("header")
        .w_full()
        .h(theme::HEADER_HEIGHT)
        .pl(cx.titlebar_inset())
        .window_drag()
        .gap(theme::NAV_GAP)
        .center_y()
        .child(spacer())
        .child(nav_button(
            "back",
            "Back",
            &icons::BACK,
            can_go_back,
            theme,
            Browser::go_back,
        ))
        .child(nav_button(
            "forward",
            "Forward",
            &icons::FORWARD,
            can_go_forward,
            theme,
            Browser::go_forward,
        ))
        .child(nav_button(
            "reload",
            "Reload",
            &icons::RELOAD,
            tab.is_some(),
            theme,
            Browser::reload,
        ))
}

fn nav_button(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    enabled: bool,
    theme: &Theme,
    operation: fn(&mut Browser, crate::tab::TabId),
) -> El {
    let button = icon_button(key, label, glyph, theme::ICON_BUTTON, theme).disabled(!enabled);
    if enabled {
        button.on_press(move |state: &mut AppState| state.with_active_tab(operation))
    } else {
        button.opacity(theme.disabled_opacity)
    }
}

fn icon_button(key: &str, label: &str, glyph: &'static Icon, size: f32, theme: &Theme) -> El {
    el().key(key)
        .size(size, size)
        .center()
        .radius(theme::ICON_BUTTON_RADIUS)
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .label(label)
        .help(label)
        .child(icon(glyph, theme::ICON).color(theme.icon))
}

fn address(state: &AppState, theme: &Theme) -> El {
    let field = text_input(&state.address, type_::BODY)
        .key(ADDRESS)
        .single_line()
        .w_full()
        .px(theme::FIELD_PADDING)
        .color(theme.text)
        .placeholder("Search or enter address", theme.text_muted)
        .label("Address")
        .on_edit(|state: &mut AppState| &mut state.address)
        .on_submit(app::submit_address);
    el().key("address-container")
        .w_full()
        .h(theme::FIELD_HEIGHT)
        .center_y()
        .radius(theme::FIELD_RADIUS)
        .fill(theme.field)
        .child(field)
}

/// Why the address or the active page failed, or that the page is loading.
fn status(state: &AppState) -> Option<String> {
    let tab = state.browser.active_tab();
    if let Some(error) = state
        .address_error
        .as_ref()
        .or_else(|| tab.and_then(|tab| tab.error.as_ref()))
    {
        Some(error.clone())
    } else if tab.is_some_and(|tab| tab.loading) {
        Some("Loading…".to_owned())
    } else {
        None
    }
}

/// The open tabs, scrolling once they outgrow the sidebar, then the new-tab button.
fn tabs(state: &AppState, theme: &Theme) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let mut list = col().key("tabs").w_full().gap(theme::TAB_GAP).label("Tabs");
    for tab in state.browser.tabs() {
        list = list.child(tab_row(tab, Some(tab.id()) == active, theme));
    }
    list = list.child(
        row_shell("new-tab", theme)
            .label("New tab")
            .on_press_tx(app::new_tab)
            .child(icon(&icons::PLUS, theme::ICON).color(theme.text_muted))
            .child(row_text("New Tab", theme.text_muted)),
    );
    col()
        .key("tabs-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
}

fn tab_row(tab: &Tab, active: bool, theme: &Theme) -> El {
    let id = tab.id();
    let glyph = if tab.loading {
        &icons::LOADING
    } else {
        &icons::PAGE
    };
    let close = icon_button(
        &format!("close-{}", id.raw()),
        "Close tab",
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        theme,
    )
    .on_press(move |state: &mut AppState| state.close(id));
    let color = if active { theme.text } else { theme.text_muted };
    let row = row_shell(&format!("tab-{}", id.raw()), theme)
        .label(tab.label())
        .help_when_truncated(tab.label())
        .on_press(move |state: &mut AppState| state.activate(id))
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .child(icon(glyph, theme::ICON).color(color))
        .child(row_text(tab.label(), color))
        .child(close);
    if active {
        row.fill(theme.tab_active).shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        )
    } else {
        row
    }
}

/// A full-width row: a tab, or a button shaped like one.
fn row_shell(key: &str, theme: &Theme) -> El {
    row()
        .key(key)
        .w_full()
        .h(theme::ROW_HEIGHT)
        .pl(theme::ROW_PADDING)
        .pr(theme::ROW_PADDING / 2.0)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
}

fn row_text(content: &str, color: Color) -> El {
    text(content)
        .font(type_::BODY)
        .color(color)
        .single_line()
        .truncate()
        .grow(1.0)
}

/// A quiet button at the foot of the sidebar that steps through System, Light and Dark.
fn appearance(current: Appearance, theme: &Theme) -> El {
    let glyph = match current {
        Appearance::System => &icons::SYSTEM,
        Appearance::Light => &icons::LIGHT,
        Appearance::Dark => &icons::DARK,
    };
    let label = format!("Appearance: {}", current.label());
    row_shell("appearance", theme)
        .label(label.clone())
        .help(format!("{label}. Click to change."))
        .on_press(|state: &mut AppState| state.appearance = state.appearance.next())
        .child(icon(glyph, theme::ICON).color(theme.text_muted))
        .child(row_text(current.label(), theme.text_muted))
}
