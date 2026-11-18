// SPDX-License-Identifier: GPL-3.0-only

//! The toolbar above the sidebar and page: the sidebar toggle, navigation buttons, and platform
//! window controls. On Linux it slides into the layout from a narrow top-edge hover target.

#[cfg(not(target_os = "linux"))]
use rune::Chrome;
#[cfg(target_os = "linux")]
use rune::tok::motion;
use rune::{Cx, El, Icon, el, row, spacer};
#[cfg(target_os = "linux")]
use rune::{Spring, Transition};

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
        let visible = state.titlebar.is_visible();
        let reduced_motion = cx.system_reduced_motion() || cx.is_reduced_motion();
        let mut slot = el()
            .key("linux-titlebar-slot")
            .w_full()
            .h(if visible { theme::HEADER_HEIGHT } else { 0.0 })
            .clip_soft(0.0)
            .motion(if reduced_motion {
                Spring::NONE
            } else {
                theme::TITLEBAR_REVEAL
            });
        if visible {
            let transition = if reduced_motion {
                Transition::opacity()
            } else {
                Transition::offset(0.0, -6.0)
                    .combined(Transition::opacity())
                    .springs(motion::pair::SNAP)
            };
            slot = slot.child(
                header_content(state, theme, cx, true)
                    .fill(theme.window)
                    .on_hover_change(|state: &mut AppState, inside, tx| {
                        if state.titlebar.set_toolbar_hovered(inside) {
                            titlebar_visibility_changed(tx);
                        }
                    })
                    .transition(transition),
            );
        }
        slot
    }
    #[cfg(not(target_os = "linux"))]
    {
        header_content(state, theme, cx, false)
    }
}

/// A passive hit target that sits at the window's top edge while the toolbar is closed.
#[cfg(target_os = "linux")]
pub(crate) fn hover_zone() -> El {
    el().key("linux-titlebar-hover-zone")
        .absolute(0.0, 0.0)
        .w_full()
        .h(theme::TITLEBAR_HOT_ZONE)
        .on_hover_change(|state: &mut AppState, inside, tx| {
            if state.titlebar.set_top_hovered(inside) {
                titlebar_visibility_changed(tx);
            }
        })
}

#[cfg(target_os = "linux")]
fn titlebar_visibility_changed(tx: &mut rune::Tx) {
    tx.animated();
    tx.motion(theme::TITLEBAR_REVEAL);
}

fn header_content(state: &AppState, theme: &Theme, _cx: &mut Cx<AppState>, in_flow: bool) -> El {
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
    // Linux draws this row in normal layout and makes its blank area draggable. Other platforms
    // keep the existing overlay aligned with their native titlebar behavior.
    let mut header = row()
        .key("header")
        .h(theme::HEADER_HEIGHT)
        .pr(theme::SIDEBAR_PADDING)
        .gap(theme::NAV_GAP)
        .center_y();
    if in_flow {
        header = header.w_full().window_drag().raise();
    } else {
        header = header.absolute(0.0, 0.0);
    }
    #[cfg(not(target_os = "linux"))]
    {
        header = header.child(window_controls(_cx.titlebar_inset(), theme));
    }
    header = header.child(toggle);

    let (mut header, gap) = if collapsed || in_flow {
        (header.w_full(), el().w(theme::HEADER_GROUP_GAP))
    } else {
        (header.w(state.sidebar.width), spacer())
    };
    header = header.child(gap);
    for (label, glyph, enabled, navigate) in buttons {
        header = header.child(nav_button(label, glyph, enabled, navigate, theme));
    }
    if collapsed || in_flow {
        header = header.child(spacer());
    }
    #[cfg(target_os = "linux")]
    if in_flow {
        if state.titlebar.can_minimize() {
            header = header.child(
                icon_button(
                    "minimize-window",
                    "Minimize window",
                    &icons::MINIMIZE,
                    theme::ICON_BUTTON,
                    theme,
                )
                .on_press(|state: &mut AppState| state.titlebar.minimize()),
            );
        }
        header = header
            .child(
                icon_button(
                    "maximize-window",
                    "Maximize or restore window",
                    &icons::MAXIMIZE,
                    theme::ICON_BUTTON,
                    theme,
                )
                .on_press_tx(|_: &mut AppState, tx| tx.toggle_maximized()),
            )
            .child(
                icon_button(
                    "close-window",
                    "Close window",
                    &icons::CLOSE,
                    theme::ICON_BUTTON,
                    theme,
                )
                .on_press_tx(|_: &mut AppState, tx| tx.close_window()),
            );
    }
    header
}

/// Plain circles under the native window controls, which stay transparent until the pointer
/// reaches them (`titlebar_controls`). `inset` is where the zoom button ends; with no controls
/// over the content, as in full screen, it is zero and nothing is drawn.
#[cfg(not(target_os = "linux"))]
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
