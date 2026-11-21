// SPDX-License-Identifier: GPL-3.0-only

//! The toolbar above the sidebar and page: the sidebar toggle, navigation buttons, and platform
//! window controls. On Linux it slides into the layout from a narrow top-edge hover target.

#[cfg(not(target_os = "linux"))]
use rune::Chrome;
#[cfg(target_os = "linux")]
use rune::Spring;
use rune::Transition;
#[cfg(target_os = "linux")]
use rune::tok::motion;
use rune::{Cx, El, Icon, el, row, spacer, text};
use rune_kit::{Menu, MenuItem, context_menu};

use crate::app::AppState;
use crate::browser::Browser;
use crate::engine::ViewCommand;
use crate::tab::TabId;
use crate::ui::controls::icon_button;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

/// What a navigation button does to the active tab.
type Navigate = fn(&mut Browser, TabId);

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    #[cfg(target_os = "linux")]
    {
        let visible = state.titlebar.is_visible() || state.zoom_menu.is_some();
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
                Transition::none()
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
        header = header.absolute(0.0, 0.0).window_drag();
    }
    #[cfg(not(target_os = "linux"))]
    {
        header = header.child(window_controls(_cx.titlebar_inset(), theme));
    }
    header = header.child(toggle);
    if state.browser.private {
        header = header.child(
            text("Private")
                .key("private-window")
                .font(rune::tok::type_::LABEL)
                .color(theme.accent)
                .label("Private window"),
        );
    }

    let (mut header, gap) = if collapsed || in_flow {
        (header.w_full(), el().w(theme::HEADER_GROUP_GAP))
    } else {
        (header.w(state.sidebar.width), spacer())
    };
    header = header.child(gap);
    header = header.children(navigation(state, theme));
    if collapsed || in_flow {
        header = header.child(spacer());
    }
    #[cfg(target_os = "linux")]
    if in_flow {
        header = header.children(linux_window_controls(state, theme));
    }
    header
}

pub(in crate::ui) fn navigation(state: &AppState, theme: &Theme) -> impl Iterator<Item = El> {
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
            (
                "Reload",
                &icons::RELOAD,
                tab.is_some_and(|tab| !tab.is_internal()),
                Browser::reload,
            )
        },
    ];
    buttons
        .into_iter()
        .enumerate()
        .map(|(index, (label, glyph, enabled, navigate))| {
            let button = nav_button(label, glyph, enabled, navigate, theme);
            if !enabled || index > 1 {
                return button;
            }
            let direction = if index == 0 { -1 } else { 1 };
            let help = if direction < 0 {
                "Back (right-click for history)"
            } else {
                "Forward (right-click for history)"
            };
            button
                .help(help)
                .on_secondary_press(move |state: &mut AppState, point, _| {
                    let Some(id) = state.browser.active_tab().map(crate::tab::Tab::id) else {
                        return;
                    };
                    state.dismiss_context_menu();
                    state.zoom_menu = None;
                    state.page_menu = Default::default();
                    if let Some(tab) = state.browser.tab_mut(id) {
                        tab.navigation_items.clear();
                    }
                    state.navigation_menu = Some((id, point));
                    state
                        .browser
                        .send(id, ViewCommand::NavigationItems(direction));
                })
        })
}

/// Hides the zoom chip once its time is up.
pub(crate) fn settle_zoom_chip(state: &mut AppState, clock: f64) -> bool {
    match state.zoom_chip {
        Some((_, until)) if clock >= until => {
            state.zoom_chip = None;
            true
        }
        _ => false,
    }
}

/// The page-zoom chip with its menu. It shows for a moment after the active page's zoom changes,
/// and stays while its menu is open.
pub(in crate::ui) fn zoom_indicator(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
) -> Option<El> {
    let tab = state
        .browser
        .active_tab()
        .filter(|tab| !tab.is_internal())?;
    let id = tab.id();
    let flashing = state.zoom_chip.is_some_and(|(shown, _)| shown == id);
    if !flashing && state.zoom_menu != Some(id) {
        return None;
    }
    let caption = format!("{}%", state.browser.page_zoom(tab));
    let trigger = el()
        .key("zoom-indicator")
        .w(theme::ZOOM_CHIP_WIDTH)
        .h(theme::ICON_BUTTON)
        .transition(Transition::scale(0.9).combined(Transition::opacity()))
        .center()
        .radius(theme::ICON_BUTTON_RADIUS)
        .press(theme.press)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .label(format!("Page zoom {caption}"))
        .help("Page zoom")
        .child(
            text(caption)
                .font(rune::tok::type_::BODY)
                .color(theme.text)
                .single_line(),
        )
        .on_press(move |state: &mut AppState| {
            state.dismiss_context_menu();
            state.navigation_menu = None;
            state.page_menu = Default::default();
            state.zoom_menu = (state.zoom_menu != Some(id)).then_some(id);
        });
    let mut menu = Menu::new("page-zoom")
        .trigger(trigger)
        .open(state.zoom_menu == Some(id))
        .on_close(|state: &mut AppState| state.zoom_menu = None);
    for (label, delta) in [("Zoom In", 10), ("Zoom Out", -10), ("Reset Page Zoom", 0)] {
        menu = menu.item(MenuItem::new(label).on_select(move |state: &mut AppState| {
            state.zoom_menu = None;
            if state.browser.active_tab().is_some_and(|tab| tab.id() == id) {
                state.browser.change_zoom(id, delta);
            }
        }));
    }
    Some(menu.build(&theme.kit(theme.overlay_panel), overlays))
}

pub(in crate::ui) fn history_menu(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some((id, point)) = state.navigation_menu else {
        return;
    };
    let Some(tab) = state.browser.active_tab().filter(|tab| tab.id() == id) else {
        return;
    };
    let mut menu = context_menu("navigation-history-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.navigation_menu = None);
    if tab.navigation_items.is_empty() {
        menu = menu.group_label("No history entries");
    }
    for entry in &tab.navigation_items {
        let step = entry.step;
        menu = menu.item(
            MenuItem::new(&entry.url).on_select(move |state: &mut AppState| {
                state.navigation_menu = None;
                if state.browser.active_tab().is_some_and(|tab| tab.id() == id) {
                    state.browser.traverse_to_step(id, step);
                }
            }),
        );
    }
    crate::ui::hover_highlight::build_context_menu(state, menu, theme, layer);
}

#[cfg(target_os = "linux")]
pub(in crate::ui) fn linux_window_controls(state: &AppState, theme: &Theme) -> Vec<El> {
    let mut controls = Vec::new();
    if state.titlebar.can_minimize() {
        controls.push(
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
    controls.push(
        icon_button(
            "maximize-window",
            "Maximize or restore window",
            &icons::MAXIMIZE,
            theme::ICON_BUTTON,
            theme,
        )
        .on_press_tx(|_: &mut AppState, tx| tx.toggle_maximized()),
    );
    controls.push(
        icon_button(
            "close-window",
            "Close window",
            &icons::CLOSE,
            theme::ICON_BUTTON,
            theme,
        )
        .on_press_tx(AppState::request_window_close),
    );
    controls
}

/// Plain circles under the native window controls, which stay transparent until the pointer
/// reaches them (`titlebar_controls`). `inset` is where the zoom button ends; with no controls
/// over the content, as in full screen, it is zero and nothing is drawn.
#[cfg(not(target_os = "linux"))]
pub(in crate::ui) fn window_controls(inset: f32, theme: &Theme) -> El {
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
    if enabled {
        icon_button(&key, label, glyph, theme::ICON_BUTTON, theme)
            .on_press(move |state: &mut AppState| state.with_active_tab(navigate))
    } else {
        crate::ui::controls::icon_button_style(&key, label, glyph, theme::ICON_BUTTON, theme)
            .disabled(true)
            .opacity(theme.disabled_opacity)
    }
}
