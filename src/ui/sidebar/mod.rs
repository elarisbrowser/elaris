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
use rune::{El, Tx, col, icon, text};
pub(crate) use tabs::context_menu;
#[cfg(target_os = "macos")]
pub(crate) use tabs::floating_drag_preview;

use super::controls::row_button;
use super::icons;
use super::theme::{self, Theme};
use crate::app::{self, AppState};
use crate::appearance::Appearance;
pub(crate) use drag::MoveResult as TabDragMoveResult;
pub(crate) use drag::{DragVisual, TabDragState, TabDropTarget};

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
        }
    }
}

/// Shows or hides the sidebar, springing the page to its new width.
pub(crate) fn toggle(state: &mut AppState, tx: &mut Tx) {
    if state.horizontal_tabs() {
        return;
    }
    state.site_permissions_open = false;
    #[cfg(target_os = "macos")]
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
    #[cfg(target_os = "linux")]
    let header_clearance = 0.0;
    #[cfg(not(target_os = "linux"))]
    let header_clearance = theme::HEADER_HEIGHT;
    let sidebar = col()
        .key("sidebar")
        .w(if collapsed { 0.0 } else { state.sidebar.width })
        .h_full()
        .clip_soft(0.0)
        .pt(header_clearance)
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
        .child(search_launcher(state, theme));
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
                .child(appearance(state.settings.appearance, theme)),
        )
}

fn search_launcher(state: &AppState, theme: &Theme) -> El {
    let address = state
        .browser
        .active_tab()
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty() && *url != "about:blank");
    let label = address
        .map(crate::address::compact_address)
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
    .token_motion(theme::OMNIBOX_MOTION)
    .h(theme::FIELD_HEIGHT)
    .radius(theme::FIELD_RADIUS)
    .continuous_corners()
    .fill(theme.address_field)
    .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
    .label("Search or type a URL")
    .help(address.unwrap_or("Open search and address suggestions"))
    .shortcut_hint("cmd+l")
    .focusable(true)
    .on_press_tx(app::open_from_launcher);
    if state.browser.active_tab().is_some() {
        launcher.child(site_controls(theme))
    } else {
        launcher
    }
}

pub(super) fn site_controls(theme: &Theme) -> El {
    super::controls::icon_button(
        "site-controls",
        "Site permissions and page information",
        &icons::SITE_CONTROLS,
        theme::CLOSE_BUTTON,
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
fn appearance(current: Appearance, theme: &Theme) -> El {
    let glyph = match current {
        Appearance::System => &icons::SYSTEM,
        Appearance::Light => &icons::LIGHT,
        Appearance::Dark => &icons::DARK,
    };
    let label = format!("Appearance: {}", current.label());
    super::controls::icon_button("appearance", &label, glyph, theme::ICON_BUTTON, theme)
        .label(label.clone())
        .help(format!("{label}. Click to change."))
        .on_press(|state: &mut AppState| state.set_appearance(state.settings.appearance.next()))
}
