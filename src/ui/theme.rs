// SPDX-License-Identifier: GPL-3.0-only

//! The interface's colours and measurements, in one place. Colours come from Rune's light and
//! dark token palettes; sizes build on its spacing and radius scales.

use rune::tok::{self, radius, space};

pub(crate) use rune::tok::Theme;

/// The palette for a dark or light interface.
pub(crate) fn palette(dark: bool) -> Theme {
    tok::theme(dark)
}

/// The sidebar left of the page.
pub(crate) const SIDEBAR_WIDTH: f32 = 250.0;
pub(crate) const SIDEBAR_PADDING: f32 = space::S2 + space::S1 / 2.0;
pub(crate) const SIDEBAR_GAP: f32 = space::S2;
/// The row level with the window controls, holding the navigation buttons. It is the window's
/// titlebar, so macOS centres the controls in it.
pub(crate) const HEADER_HEIGHT: f32 = 38.0;

/// Square icon buttons: back, forward, reload, and each tab's close button.
pub(crate) const ICON_BUTTON: f32 = 28.0;
pub(crate) const ICON_BUTTON_RADIUS: f32 = radius::SM;
pub(crate) const ICON: f32 = 16.0;
pub(crate) const CLOSE_BUTTON: f32 = 22.0;
/// The space between the navigation buttons.
pub(crate) const NAV_GAP: f32 = space::S1;

/// How far buttons and rows shrink while pressed, springing back on release.
pub(crate) const PRESS_SCALE: f32 = 0.97;

/// The address field.
pub(crate) const FIELD_HEIGHT: f32 = 36.0;
pub(crate) const FIELD_RADIUS: f32 = radius::LG - 2.0;
pub(crate) const FIELD_PADDING: f32 = space::S3;

/// Rows in the tab list, and the new-tab and appearance buttons that share their shape.
pub(crate) const ROW_HEIGHT: f32 = 34.0;
pub(crate) const ROW_RADIUS: f32 = radius::MD;
pub(crate) const ROW_PADDING: f32 = space::S2;
pub(crate) const ROW_GAP: f32 = space::S2;
pub(crate) const TAB_GAP: f32 = 2.0;
/// The raised look of the active tab.
pub(crate) const ACTIVE_SHADOW_Y: f32 = 0.5;
pub(crate) const ACTIVE_SHADOW_BLUR: f32 = 1.5;

/// The web page's container: how far it sits in from the window's edges, and its corners.
pub(crate) const VIEWPORT_INSET: f32 = space::S1 + space::S1 / 2.0;
pub(crate) const VIEWPORT_RADIUS: f32 = radius::LG;
