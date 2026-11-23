// SPDX-License-Identifier: GPL-3.0-only

//! The toolbar above the sidebar and page: the sidebar toggle, navigation buttons, and platform
//! window controls. On Linux the window controls appear when the pointer reaches the top edge.

use crate::ui::components::menu::{MenuItem, context_menu};
#[cfg(not(elaris_ui = "linux"))]
use rune::Chrome;
#[cfg(elaris_ui = "linux")]
use rune::Transition;
#[cfg(elaris_ui = "linux")]
use rune::tok::motion;
use rune::{Cx, El, Icon, el, row, spacer, text};

use crate::app::AppState;
use crate::browser::Browser;
use crate::engine::ViewCommand;
use crate::tab::TabId;
use crate::ui::components::controls::icon_button;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

mod reload;
mod zoom;

pub(crate) use zoom::settle_zoom_chip;
pub(in crate::ui) use zoom::zoom_indicator;

/// What a navigation button does to the active tab.
type Navigate = fn(&mut Browser, TabId);

pub(crate) fn view(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> El {
    #[cfg(elaris_ui = "linux")]
    {
        // The toggle and navigation always show; revealing the row only adds the window
        // controls and pushes the page down beneath them.
        let visible = header_revealed(state);
        let header = header_content(state, theme, cx, visible);
        if visible {
            header.on_hover_change(|state: &mut AppState, inside, tx| {
                if state.titlebar.set_toolbar_hovered(inside) {
                    titlebar_visibility_changed(tx);
                }
            })
        } else {
            header
        }
    }
    #[cfg(not(elaris_ui = "linux"))]
    {
        header_content(state, theme, cx, false)
    }
}

/// Whether the Linux row shows its window controls and holds the page below it: always with the
/// sidebar closed or on the right, otherwise while the pointer is at the top edge or in the row.
#[cfg(elaris_ui = "linux")]
pub(crate) fn header_revealed(state: &AppState) -> bool {
    state.sidebar.collapsed
        || state.sidebar_on_right()
        || state.titlebar.is_visible()
        || state.zoom_menu.is_some()
}

/// A passive hit target that sits at the window's top edge while the toolbar is closed.
#[cfg(elaris_ui = "linux")]
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

#[cfg(elaris_ui = "linux")]
fn titlebar_visibility_changed(tx: &mut rune::Tx) {
    tx.animated();
    tx.motion(theme::TITLEBAR_REVEAL);
}

/// `revealed` spans the row across the window and adds the Linux window controls.
fn header_content(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>, revealed: bool) -> El {
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
    // The toggle and navigation keep their place whether or not the Linux window controls show:
    // with the sidebar open they span its width, and the controls take the rest of the row.
    let mut lead = row()
        .key("header-lead")
        .h_full()
        .gap(theme::NAV_GAP)
        .center_y();
    #[cfg(not(elaris_ui = "linux"))]
    {
        lead = lead.child(window_controls(cx.titlebar_inset(), theme));
    }
    #[cfg(elaris_ui = "linux")]
    {
        lead = lead.pl(theme::SIDEBAR_PADDING);
    }
    lead = lead.child(toggle);
    if state.browser.private {
        lead = lead.child(
            text("Private")
                .key("private-window")
                .font(rune::tok::type_::LABEL)
                .color(theme.accent)
                .label("Private window"),
        );
    }
    // A right-hand sidebar leaves the window controls over the page, so the row spans the window
    // as it does with the sidebar hidden.
    let spans_window = collapsed || state.sidebar_on_right();
    lead = if spans_window {
        lead.child(el().w(theme::HEADER_GROUP_GAP))
    } else {
        lead.w(state.sidebar.width)
            .pr(theme::SIDEBAR_PADDING)
            .child(spacer())
    };
    lead = lead.children(navigation(state, theme));

    let mut header = row()
        .key("header")
        .h(theme::HEADER_HEIGHT)
        .gap(theme::NAV_GAP)
        .center_y()
        .absolute(0.0, 0.0)
        .window_drag()
        .child(lead);
    if spans_window || revealed {
        header = header.w_full().pr(theme::SIDEBAR_PADDING).child(spacer());
    }
    #[cfg(elaris_ui = "linux")]
    if revealed {
        header = header.children(fading_window_controls(state, theme, cx));
    }
    header
}

#[cfg(elaris_ui = "linux")]
fn fading_window_controls(state: &AppState, theme: &Theme, cx: &mut Cx<AppState>) -> Vec<El> {
    let fade = if cx.system_reduced_motion() || cx.is_reduced_motion() {
        Transition::none()
    } else {
        Transition::opacity().springs(motion::pair::SNAP)
    };
    linux_window_controls(state, theme)
        .into_iter()
        .map(|control| control.transition(fade))
        .collect()
}

pub(in crate::ui) fn navigation(state: &AppState, theme: &Theme) -> impl Iterator<Item = El> {
    let tab = state.browser.active_tab();
    let buttons: [(&str, &'static Icon, bool, Navigate); 2] = [
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
    ];
    let reload = reload::button(state, theme);
    buttons
        .into_iter()
        .enumerate()
        .map(|(index, (label, glyph, enabled, navigate))| {
            let button = nav_button(label, glyph, enabled, navigate, theme);
            if !enabled {
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
        .chain(std::iter::once(reload))
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
    menu.build(state, theme, layer);
}

#[cfg(elaris_ui = "linux")]
pub(in crate::ui) fn linux_window_controls(state: &AppState, theme: &Theme) -> Vec<El> {
    let mut controls = Vec::new();
    // The compositor owns the frame, or the window manager tiles it and wants no controls.
    if crate::app::LinuxDecorations::current() == crate::app::LinuxDecorations::System {
        return controls;
    }
    if state.titlebar.can_minimize() {
        controls.push(
            icon_button(
                "minimize-window",
                "Minimize window",
                &icons::MINIMIZE,
                theme::ICON_BUTTON,
                theme,
            )
            .on_press_tx(|_: &mut AppState, tx| tx.minimize()),
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
/// reaches them (`platform::macos::titlebar`). `inset` is where the zoom button ends; with no controls
/// over the content, as in full screen, it is zero and nothing is drawn.
#[cfg(not(elaris_ui = "linux"))]
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
        crate::ui::components::controls::icon_button_style(
            &key,
            label,
            glyph,
            theme::ICON_BUTTON,
            theme,
        )
        .disabled(true)
        .opacity(theme.disabled_opacity)
    }
}
