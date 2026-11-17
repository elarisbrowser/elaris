// SPDX-License-Identifier: GPL-3.0-only

//! The interface's colours and measurements, in one place. Colours come from Rune's light and
//! dark token palettes; sizes build on its spacing and radius scales.

use rune::tok::{self, radius, space};

pub(crate) use rune::tok::Theme;

/// The palette for a dark or light interface.
pub(crate) fn palette(dark: bool) -> Theme {
    tok::theme(dark)
}

/// The toolbar above the page.
pub(crate) const TOOLBAR_HEIGHT: f32 = 64.0;
pub(crate) const TOOLBAR_GAP: f32 = space::S3;
pub(crate) const TOOLBAR_PADDING: f32 = space::S3 + space::S1;

/// Fields and buttons in the toolbar.
pub(crate) const CONTROL_HEIGHT: f32 = 40.0;
pub(crate) const CONTROL_RADIUS: f32 = radius::LG - 2.0;
pub(crate) const CONTROL_PADDING: f32 = space::S1;
pub(crate) const FIELD_PADDING: f32 = space::S3;

/// The segments of the appearance picker, inside its control.
pub(crate) const SEGMENT_HEIGHT: f32 = CONTROL_HEIGHT - 2.0 * CONTROL_PADDING;
pub(crate) const SEGMENT_RADIUS: f32 = CONTROL_RADIUS - CONTROL_PADDING;
pub(crate) const SEGMENT_PADDING: f32 = space::S3;

/// The web page's container: how far it sits in from the window's edges, and its corners.
pub(crate) const VIEWPORT_INSET: f32 = space::S1 + space::S1 / 2.0;
pub(crate) const VIEWPORT_RADIUS: f32 = radius::LG;
