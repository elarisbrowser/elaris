// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar: the header beside the window controls, the search launcher and status, the open
//! tabs, and compact controls at its foot.
//!
//! - `header`: the titlebar row, its sidebar toggle and navigation buttons. The window draws it
//!   over the sidebar, so it stays when the sidebar collapses.
//! - `tabs`: the tab list and the new-tab button.
//! - `resize`: dragging the sidebar's edge.

pub(crate) mod drag;
pub(crate) mod header;
pub(crate) mod hover;
pub(crate) mod resize;
pub(super) mod tabs;

use rune::tok::type_;
use rune::{El, Transition, Tx, col, icon, row, text};

use super::icons;
use super::permission_prompt;
use super::theme::{self, Theme};
use crate::app::{self, AppState};
use crate::platform::haptics::{self, Haptic};
use crate::ui::components::animated_icon;
use crate::ui::components::controls::row_button;
use crate::ui::components::hover_highlight;
use elaris_core::appearance::Appearance;

/// How the sidebar is laid out: what the window shows of it, not of the browser.
pub(crate) struct SidebarState {
    /// Its width in points, which dragging its edge changes.
    pub(crate) width: f32,
    /// Released past its width's limits, and yet to spring back.
    pub(crate) settling: bool,
    /// Hidden, leaving the page the window's width.
    pub(crate) collapsed: bool,
    pub(in crate::ui) revealed_tab: Option<(crate::tab::TabId, crate::tab::TabSection, usize)>,
    pub(in crate::ui) horizontal_strip_width: Option<f32>,
    pub(in crate::ui) overflow_menu_open: bool,
    pub(in crate::ui) preferred_tab_widths:
        std::cell::RefCell<std::collections::HashMap<crate::tab::TabId, f32>>,
    pub(in crate::ui) hover: hover::Hover,
    /// The pointer is over the sidebar, which reveals the omnibox's buttons.
    pub(in crate::ui) pointer_inside: bool,
    /// When the copy-link button stops showing its tick, on the window clock.
    pub(in crate::ui) link_copied_until: Option<f64>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            width: theme::SIDEBAR_WIDTH,
            settling: false,
            collapsed: false,
            revealed_tab: None,
            horizontal_strip_width: None,
            overflow_menu_open: false,
            preferred_tab_widths: Default::default(),
            hover: hover::Hover::default(),
            pointer_inside: false,
            link_copied_until: None,
        }
    }
}

/// Shows or hides the sidebar, springing the page to its new width.
pub(crate) fn toggle(state: &mut AppState, tx: &mut Tx) {
    if state.horizontal_tabs() || state.is_mini() {
        return;
    }
    state.site_permissions_open = false;
    #[cfg(elaris_ui = "macos")]
    if state.cancel_tab_drag() {
        tx.animated();
        tx.motion(theme::TAB_REORDER_MOTION);
    }
    state.sidebar.collapsed = !state.sidebar.collapsed;
    tx.animated();
    tx.motion(theme::SIDEBAR_TOGGLE);
}

/// The sidebar below the header's room. Collapsing springs its width to zero; its contents
/// keep their width meanwhile, clipped, so they slide away rather than reflow.
pub(crate) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    let collapsed = state.sidebar.collapsed;
    let sidebar = col()
        .key("sidebar")
        .w(if collapsed { 0.0 } else { state.sidebar.width })
        .h_full()
        .clip_soft(0.0)
        .pt(theme::HEADER_HEIGHT)
        .on_hover_change(|state: &mut AppState, inside, tx| {
            if state.sidebar.pointer_inside != inside {
                state.sidebar.pointer_inside = inside;
                tx.animated();
            }
        })
        .child(body(state, theme, overlays, reduced_motion));
    if collapsed {
        sidebar.a11y_hidden()
    } else {
        sidebar
    }
}

/// Everything below the header, inset from the sidebar's edges.
fn body(state: &AppState, theme: &Theme, overlays: &mut Vec<El>, reduced_motion: bool) -> El {
    let mut body = col()
        .key("sidebar-body")
        .w(state.sidebar.width)
        .grow(1.0)
        .min_h(0.0)
        .px(theme::SIDEBAR_PADDING)
        .pb(theme::VIEWPORT_INSET)
        .gap(theme::SIDEBAR_GAP)
        .child(search_launcher(state, theme, reduced_motion, overlays));
    if let Some(status) = state.status() {
        body = body.child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(theme.text_muted)
                .px(theme::FIELD_PADDING)
                .truncate(),
        );
    }
    body.child(tabs::view(state, theme, overlays, reduced_motion))
        .child(
            rune::row()
                .key("sidebar-bottom-controls")
                .w_full()
                .center_y()
                .child(super::downloads::trigger(state, theme, overlays))
                .child(rune::el().grow(1.0))
                .child(appearance(state, theme)),
        )
}

fn search_launcher(state: &AppState, theme: &Theme, reduced: bool, overlays: &mut Vec<El>) -> El {
    let mut marks = state
        .browser
        .active_tab()
        .map(|tab| permission_prompt::address_indicators(tab, theme))
        .unwrap_or_default();
    marks.extend(header::zoom_indicator(state, theme, overlays));
    search_field(state, theme, reduced, marks)
}

fn search_field(state: &AppState, theme: &Theme, reduced: bool, marks: Vec<El>) -> El {
    let address = state
        .browser
        .active_tab()
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty() && *url != "about:blank");
    let label = address
        .map(elaris_core::address::compact_address)
        .unwrap_or("Search or type a URL");
    let launcher = row_button(
        "omnibox-launcher",
        address.is_none().then(|| {
            icon(&icons::SEARCH, theme::ICON)
                .key("search-icon")
                .color(theme.text_muted)
        }),
        label,
        if address.is_some() {
            theme.text
        } else {
            theme.text_muted
        },
        theme,
    )
    .token_motion(if reduced {
        rune::Spring::NONE
    } else {
        theme::OMNIBOX_MOTION
    })
    .h(theme::FIELD_HEIGHT)
    .radius(theme::FIELD_RADIUS)
    .continuous_corners()
    .fill(theme.address_field)
    .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
    .label("Search or type a URL")
    .help(address.unwrap_or("Open search and address suggestions"))
    .shortcut_hint("cmd+l")
    .focusable(true)
    .on_press_tx(app::open_from_launcher)
    .children(marks);
    // Keep the buttons while site controls are open, since the popover is anchored to them.
    let revealed = state.sidebar.pointer_inside || state.site_permissions_open;
    if state.browser.active_tab().is_none() || !revealed {
        return launcher;
    }
    let fade = if reduced {
        Transition::none()
    } else {
        Transition::opacity()
    };
    let mut buttons = row().key("omnibox-buttons").center_y().transition(fade);
    if let Some(url) = address {
        buttons = buttons.child(copy_link(state, url, theme, reduced));
    }
    launcher.child(buttons.child(site_controls(theme, theme::ICON_BUTTON)))
}

/// Copies the page's address, then morphs its chain into a tick that draws itself on.
fn copy_link(state: &AppState, url: &str, theme: &Theme, reduced: bool) -> El {
    let copied = state.sidebar.link_copied_until.is_some();
    let size = theme::ICON_BUTTON;
    let inset = (size - theme::ICON) / 2.0;
    let glyph = crate::ui::components::controls::confirm_glyph(
        "copy-link",
        &icons::LINK,
        theme::ICON,
        copied,
        theme,
        reduced,
    );
    let url = url.to_owned();
    crate::ui::components::hover_highlight::attach(
        rune::el()
            .key("copy-link")
            .size(size, size)
            .radius(theme::ICON_BUTTON_RADIUS)
            .press(theme.press)
            .press_scale(theme::PRESS_SCALE)
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .role("button")
            .focusable(true)
            .label("Copy link")
            .help(if copied { "Copied" } else { "Copy link" })
            .on_press_tx(move |state: &mut AppState, tx| {
                tx.copy(url.clone());
                haptics::play(Haptic::Tap);
                state.sidebar.link_copied_until = Some(tx.clock() + theme::COPY_LINK_CONFIRM);
                tx.wake_after(std::time::Duration::from_secs_f64(theme::COPY_LINK_CONFIRM));
                tx.animated();
            })
            .child(glyph.size(theme::ICON, theme::ICON).absolute(inset, inset)),
        "copy-link",
        crate::ui::components::hover_highlight::Group::Controls,
    )
}

/// Returns the copy-link button to its chain once its tick has shown long enough.
pub(crate) fn settle_copy_link(state: &mut AppState, clock: f64) -> bool {
    match state.sidebar.link_copied_until {
        Some(until) if clock >= until => {
            state.sidebar.link_copied_until = None;
            true
        }
        _ => false,
    }
}

pub(super) fn site_controls(theme: &Theme, size: f32) -> El {
    crate::ui::components::controls::icon_button(
        "site-controls",
        "Site permissions and page information",
        &icons::SITE_CONTROLS,
        size,
        theme,
    )
    .on_layout_tx(|state: &mut AppState, rect, tx| {
        state.site_controls_anchor = Some(rect);
        tx.live_only();
    })
    .on_press_tx(move |state: &mut AppState, tx| {
        if state.horizontal_tabs() && state.omnibox.open {
            state.omnibox.close();
            tx.focus(super::viewport::focus_key(&state.browser), false);
        }
        let open = !state.site_permissions_open;
        state.site_permissions_open = open;
        state.site_controls_page = crate::site_controls::SiteControlsPage::Main;
        state.permission_menu = None;
        state.autoplay_menu_open = false;
        state.site_overflow_open = false;
    })
}

/// A quiet button at the foot of the sidebar that steps through System, Light and Dark.
fn appearance(state: &AppState, theme: &Theme) -> El {
    let current = state.settings.appearance;
    let glyph = match current {
        Appearance::System => &animated_icon::SYSTEM,
        Appearance::Light => &animated_icon::LIGHT,
        Appearance::Dark => &animated_icon::DARK,
    };
    let label = format!("Appearance: {}", current.label());
    let frame = crate::ui::components::controls::icon_button_frame(
        "appearance",
        &label,
        theme::ICON_BUTTON,
        theme,
    );
    hover_highlight::attach_with(
        frame,
        "appearance",
        hover_highlight::Group::Controls,
        animated_icon::hover_change("appearance"),
    )
    .child(animated_icon::glyph(
        state,
        glyph,
        "appearance",
        "appearance-glyph",
        theme::ICON,
        theme.icon,
        state.sidebar.hover.reduced.get(),
    ))
    .label(label.clone())
    .help(format!("{label}. Click to change."))
    .on_press(|state: &mut AppState| state.set_appearance(state.settings.appearance.next()))
}
