// SPDX-License-Identifier: GPL-3.0-only

//! The interface's colours and measurements, in one place. Every colour the interface draws is
//! a token of [`Theme`], named for what it is for, with a light and a dark value here; sizes
//! build on Rune's spacing and radius scales.

use rune::Spring;
use rune::tok::{motion, radius, space};
use rune::{Color, oklch, oklcha};

/// The hue every neutral leans toward, a cool grey.
const HUE: f32 = 265.0;

pub(crate) struct Theme {
    /// The window behind the sidebar and around the page.
    pub(crate) window: Color,
    /// The page's container, seen before a page draws.
    pub(crate) page: Color,
    pub(crate) text: Color,
    /// Labels that matter less: inactive tabs, the status line, placeholders.
    pub(crate) text_muted: Color,
    pub(crate) icon: Color,
    /// The window controls at rest, before the pointer fades the native ones in on macOS.
    #[cfg(not(target_os = "linux"))]
    pub(crate) window_control: Color,
    /// Background of the sidebar address field.
    pub(crate) address_field: Color,
    /// Overlays on buttons and rows under the pointer, and while pressed.
    pub(crate) hover: Color,
    pub(crate) press: Color,
    /// The active tab, raised above the sidebar.
    pub(crate) tab_active: Color,
    pub(crate) tab_active_shadow: Color,
    /// The dim layer and raised surface of the omnibox.
    pub(crate) overlay_scrim: Color,
    pub(crate) overlay_panel: Color,
    pub(crate) overlay_border: Color,
    pub(crate) overlay_shadow: Color,
    pub(crate) overlay_selected: Color,
    /// How faint a control that can't be used now is.
    pub(crate) disabled_opacity: f32,
}

/// The palette for a dark or light interface.
pub(crate) fn palette(dark: bool) -> Theme {
    if dark {
        Theme {
            window: oklch(0.185, 0.004, HUE),
            page: oklch(0.232, 0.005, HUE),
            text: oklch(0.955, 0.003, HUE),
            text_muted: oklch(0.74, 0.008, HUE),
            icon: oklch(0.82, 0.006, HUE),
            #[cfg(not(target_os = "linux"))]
            window_control: oklcha(1.0, 0.0, 0.0, 0.16),
            address_field: oklcha(1.0, 0.0, 0.0, 0.05),
            hover: oklcha(1.0, 0.0, 0.0, 0.06),
            press: oklcha(1.0, 0.0, 0.0, 0.1),
            tab_active: oklcha(1.0, 0.0, 0.0, 0.15),
            tab_active_shadow: oklcha(0.0, 0.0, 0.0, 0.3),
            overlay_scrim: oklcha(0.0, 0.0, 0.0, 0.16),
            overlay_panel: oklch(0.255, 0.006, HUE),
            overlay_border: oklcha(1.0, 0.0, 0.0, 0.11),
            overlay_shadow: oklcha(0.0, 0.0, 0.0, 0.55),
            overlay_selected: oklcha(1.0, 0.0, 0.0, 0.09),
            disabled_opacity: 0.35,
        }
    } else {
        Theme {
            window: oklch(0.935, 0.006, HUE),
            page: oklch(1.0, 0.0, 0.0),
            text: oklch(0.2, 0.008, HUE),
            text_muted: oklch(0.42, 0.012, HUE),
            icon: oklch(0.32, 0.01, HUE),
            #[cfg(not(target_os = "linux"))]
            window_control: oklcha(0.2, 0.02, HUE, 0.16),
            address_field: oklcha(0.2, 0.02, HUE, 0.05),
            hover: oklcha(0.2, 0.02, HUE, 0.06),
            press: oklcha(0.2, 0.02, HUE, 0.1),
            tab_active: oklch(1.0, 0.0, 0.0),
            tab_active_shadow: oklcha(0.2, 0.02, HUE, 0.18),
            overlay_scrim: oklcha(0.2, 0.02, HUE, 0.12),
            overlay_panel: oklch(0.995, 0.002, HUE),
            overlay_border: oklcha(0.2, 0.02, HUE, 0.12),
            overlay_shadow: oklcha(0.2, 0.02, HUE, 0.2),
            overlay_selected: oklcha(0.2, 0.02, HUE, 0.07),
            disabled_opacity: 0.3,
        }
    }
}

/// The sidebar left of the page: its width at launch, and the range dragging its edge keeps it
/// in.
pub(crate) const SIDEBAR_WIDTH: f32 = 260.0;
/// Never narrower than the header, whose buttons must all fit.
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = HEADER_MIN_WIDTH;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 400.0;
/// How the sidebar slides away and back: short, without bounce, so the page settles quickly.
pub(crate) const SIDEBAR_TOGGLE: Spring = Spring::new(0.25, 0.0);
/// The header's narrowest: room for the window controls, the sidebar toggle and the navigation
/// buttons.
pub(crate) const HEADER_MIN_WIDTH: f32 = 220.0;
/// The window's smallest size: the widest sidebar beside a page still worth reading.
pub(crate) const WINDOW_MIN_SIZE: (f32, f32) = (SIDEBAR_MAX_WIDTH + 320.0, 360.0);
/// The share of the theme's window colour layered over the native glass.
pub(crate) const WINDOW_GLASS_TINT_OPACITY: f32 = 0.14;
/// The invisible strip over the sidebar's edge that drags it.
pub(crate) const RESIZE_HANDLE: f32 = 8.0;
/// How far past its limits the edge can be pulled, however hard.
pub(crate) const RUBBER_BAND_REACH: f32 = 44.0;
/// How the edge springs back once released: quick, with a lively overshoot.
pub(crate) const SIDEBAR_SETTLE: Spring = Spring::new(0.32, 0.42);
pub(crate) const SIDEBAR_PADDING: f32 = space::S2 + space::S1 / 2.0;
pub(crate) const SIDEBAR_GAP: f32 = space::S2;
/// The row level with the window controls, holding the navigation buttons: the height of the
/// titlebar band macOS drags, since the controls sit centred in it.
pub(crate) const HEADER_HEIGHT: f32 = 38.0;
/// The top-edge target that reveals the Linux toolbar while it is hidden.
#[cfg(target_os = "linux")]
pub(crate) const TITLEBAR_HOT_ZONE: f32 = 4.0;
/// The Linux toolbar and the content beneath it settle with a short, quiet spring.
#[cfg(target_os = "linux")]
pub(crate) const TITLEBAR_REVEAL: Spring = Spring::new(0.28, 0.0);
/// Where the window controls sit: centred in the header, as far in from the side as from the top.
pub(crate) const TRAFFIC_LIGHTS: (f32, f32) = (
    (HEADER_HEIGHT - rune::Chrome::TRAFFIC_LIGHT) / 2.0,
    (HEADER_HEIGHT - rune::Chrome::TRAFFIC_LIGHT) / 2.0,
);

/// Square icon buttons: back, forward, reload, and each tab's close button.
pub(crate) const ICON_BUTTON: f32 = 28.0;
pub(crate) const ICON_BUTTON_RADIUS: f32 = radius::SM;
pub(crate) const LOADING_PERIOD: f64 = 1.0;
pub(crate) const ICON: f32 = 16.0;
pub(crate) const CLOSE_BUTTON: f32 = 22.0;
/// The space between the navigation buttons, and between them and the sidebar toggle while the
/// sidebar is hidden.
pub(crate) const NAV_GAP: f32 = space::S1;
pub(crate) const HEADER_GROUP_GAP: f32 = space::S3;

/// How far buttons and rows shrink while pressed, springing back on release.
pub(crate) const PRESS_SCALE: f32 = 0.97;

/// The address field.
pub(crate) const FIELD_HEIGHT: f32 = 36.0;
pub(crate) const FIELD_PADDING: f32 = space::S3;
pub(crate) const FIELD_RADIUS: f32 = radius::MD;

/// The floating search palette and its compact result rows.
pub(crate) const OMNIBOX_WIDTH: f32 = 680.0;
pub(crate) const OMNIBOX_MAX_HEIGHT: f32 = 500.0;
pub(crate) const OMNIBOX_MOTION: Spring = Spring::new(0.24, 0.0);
pub(crate) const OMNIBOX_FOOTER_HEIGHT: f32 = 32.0;
pub(crate) const OMNIBOX_LABEL_HEIGHT: f32 = 22.0;
pub(crate) const OMNIBOX_RADIUS: f32 = radius::LG + 4.0;
pub(crate) const OMNIBOX_INPUT_HEIGHT: f32 = 60.0;
pub(crate) const OMNIBOX_ROW_HEIGHT: f32 = 46.0;
pub(crate) const OMNIBOX_INPUT_TEXT: rune::TextStyle = rune::TextStyle::new(18.0, 400.0);
pub(crate) const OMNIBOX_BORDER_WIDTH: f32 = 1.0;
pub(crate) const OMNIBOX_SHADOW_Y: f32 = 16.0;
pub(crate) const OMNIBOX_SHADOW_BLUR: f32 = 22.0;
/// The blur behind modal overlays, in Rune points.
pub(crate) const MODAL_BACKDROP_BLUR: f32 = 16.0;
pub(crate) const OMNIBOX_SEARCH_ICON: f32 = 16.0;
pub(crate) const OMNIBOX_SEARCH_CHEVRON: f32 = 12.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_PADDING: f32 = 6.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_GAP: f32 = 4.0;

/// Rows in the tab list, and the new-tab and appearance buttons that share their shape.
pub(crate) const ROW_HEIGHT: f32 = 34.0;
pub(crate) const ROW_RADIUS: f32 = radius::MD;
pub(crate) const ROW_PADDING: f32 = space::S2;
pub(crate) const ROW_GAP: f32 = space::S2;
pub(crate) const TAB_GAP: f32 = space::S1;
/// The raised look of the active tab.
pub(crate) const ACTIVE_SHADOW_Y: f32 = 0.5;
pub(crate) const ACTIVE_SHADOW_BLUR: f32 = 1.5;
/// How the active tab's highlight moves to the tab pressed.
pub(crate) const SELECTION_MOTION: Spring = motion::SNAPPY;

/// The web page's container: how far it sits in from the window's edges, and its corners.
pub(crate) const VIEWPORT_INSET: f32 = space::S1 + space::S1 / 2.0;
pub(crate) const VIEWPORT_RADIUS: f32 = radius::LG;
