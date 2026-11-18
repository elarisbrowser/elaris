// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar's header, which is the window's titlebar: resting marks for the window controls,
//! the sidebar toggle, then back, forward and reload for the active tab. It spans the sidebar,
//! or just its own buttons while the sidebar is hidden.

#[cfg(target_os = "linux")]
use rune::Transition;
#[cfg(target_os = "linux")]
use rune::tok::motion;
use rune::{Chrome, Cx, El, Icon, el, row, spacer};

use crate::app::AppState;
use crate::browser::Browser;
use crate::tab::TabId;
use crate::ui::controls::icon_button;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

/// What a navigation button does to the active tab.
type Navigate = fn(&mut Browser, TabId);

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    #[cfg(target_os = "linux")]
    {
        let mut titlebar = el()
            .key("header-hover-zone")
            .absolute(0.0, 0.0)
            .w_full()
            .h(theme::HEADER_HEIGHT)
            .on_hover_change(|state: &mut AppState, inside, tx| {
                if state.titlebar.set_hovered(inside) {
                    tx.animated();
                }
            });
        if state.titlebar.is_visible() {
            let transition = if cx.is_reduced_motion() {
                Transition::opacity()
            } else {
                Transition::offset(0.0, -8.0)
                    .combined(Transition::opacity())
                    .springs(motion::pair::SNAP)
            };
            titlebar = titlebar.child(
                header_content(state, theme, cx)
                    .fill(theme.window)
                    .transition(transition),
            );
        }
        titlebar
    }
    #[cfg(not(target_os = "linux"))]
    {
        header_content(state, theme, cx)
    }
}

fn header_content(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    let tab = state.browser.active_tab();
    let buttons: [(&str, &'static Icon, bool, Navigate); 3] = [
        (
            "Back",
            &icons::BACK,
            tab.is_some_and(|t| t.can_go_back),
            Browser::go_back,
        ),
        (
            "Forward",
            &icons::FORWARD,
            tab.is_some_and(|t| t.can_go_forward),
            Browser::go_forward,
        ),
        if tab.is_some_and(|tab| tab.loading) {
            ("Stop loading", &icons::CLOSE, true, Browser::stop_loading)
        } else {
            ("Reload", &icons::RELOAD, tab.is_some(), Browser::reload)
        },
    ];
    let collapsed = state.sidebar.collapsed;
    let toggle_label = if collapsed {
        "Show sidebar"
    } else {
        "Hide sidebar"
    };
    let toggle = icon_button(
        "toggle-sidebar",
        toggle_label,
        &icons::SIDEBAR,
        theme::ICON_BUTTON,
        theme,
    )
    .shortcut_hint("cmd+s")
    .on_press_tx(super::toggle);
    // Over the sidebar it spans the sidebar, its buttons at the far end. With the sidebar
    // hidden it spans the window, its buttons following the toggle. It lies in macOS's own
    // titlebar band, which drags the window and zooms it on a double click wherever no button
    // takes the press.
    let header = row()
        .key("header")
        .absolute(0.0, 0.0)
        .h(theme::HEADER_HEIGHT)
        .pr(theme::SIDEBAR_PADDING)
        .gap(theme::NAV_GAP)
        .center_y()
        .child(window_controls(cx.titlebar_inset(), theme))
        .child(toggle);
    let (mut header, gap) = if collapsed {
        (header.w_full(), el().w(theme::HEADER_GROUP_GAP))
    } else {
        (header.w(state.sidebar.width), spacer())
    };
    header = header.child(gap);
    for (label, glyph, enabled, navigate) in buttons {
        header = header.child(nav_button(label, glyph, enabled, navigate, theme));
    }
    if collapsed {
        header = header.child(spacer());
    }
    header
}

/// Plain circles under the native window controls, which stay transparent until the pointer
/// reaches them (`titlebar_controls`). `inset` is where the zoom button ends; with no controls
/// over the content, as in full screen, it is zero and nothing is drawn.
fn window_controls(inset: f32, theme: &Theme) -> El {
    let size = Chrome::TRAFFIC_LIGHT;
    let gap = Chrome::TRAFFIC_PITCH - size;
    let width = 3.0 * size + 2.0 * gap;
    let mut marks = row()
        .key("window-controls")
        .gap(gap)
        .pl((inset - width).max(0.0))
        .a11y_hidden();
    if inset > 0.0 {
        for _ in 0..3 {
            marks = marks.child(
                el().size(size, size)
                    .radius(size / 2.0)
                    .fill(theme.window_control),
            );
        }
    }
    marks
}

fn nav_button(
    label: &str,
    glyph: &'static Icon,
    enabled: bool,
    navigate: Navigate,
    theme: &Theme,
) -> El {
    let key = label.to_ascii_lowercase();
    let button = icon_button(&key, label, glyph, theme::ICON_BUTTON, theme).disabled(!enabled);
    if enabled {
        button.on_press(move |state: &mut AppState| state.with_active_tab(navigate))
    } else {
        button.opacity(theme.disabled_opacity)
    }
}
