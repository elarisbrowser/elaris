// SPDX-License-Identifier: GPL-3.0-only

//! Shared dimensions and semantic colours for the Site Control Centre hosts.

#[cfg(any(target_os = "macos", test))]
use rune::Color;

#[cfg(target_os = "macos")]
use super::theme::Theme;

pub(crate) const WIDTH: f32 = 280.0;
pub(crate) const OUTER_RADIUS: f32 = 20.0;
pub(crate) const PADDING: f32 = 8.0;
pub(crate) const ACTION_HEIGHT: f32 = 34.0;
pub(crate) const ACTION_GAP: f32 = 6.0;
pub(crate) const ACTION_RADIUS: f32 = 12.0;
pub(crate) const ROW_HEIGHT: f32 = 40.0;
pub(crate) const ICON_CIRCLE: f32 = 28.0;
pub(crate) const GLYPH: f32 = 12.0;
pub(crate) const ROW_GLYPH: f32 = 11.0;
pub(crate) const ROW_RADIUS: f32 = 8.0;
pub(crate) const ROW_GAP: f32 = 2.0;
pub(crate) const SECTION_GAP: f32 = 12.0;
pub(crate) const FOOTER_HEIGHT: f32 = 28.0;

#[cfg(target_os = "macos")]
pub(crate) const HEADING_HEIGHT: f32 = 15.0;
#[cfg(target_os = "macos")]
pub(crate) const HEADING_TOP: f32 = PADDING + ACTION_HEIGHT + SECTION_GAP;
#[cfg(target_os = "macos")]
pub(crate) const ROWS_TOP: f32 = HEADING_TOP + HEADING_HEIGHT + 4.0;
#[cfg(target_os = "macos")]
pub(crate) const PERMISSION_COUNT: f32 = 4.0;
#[cfg(target_os = "macos")]
pub(crate) const SEPARATOR_TOP: f32 =
    ROWS_TOP + PERMISSION_COUNT * ROW_HEIGHT + (PERMISSION_COUNT - 1.0) * ROW_GAP + 8.0;
#[cfg(target_os = "macos")]
pub(crate) const FOOTER_TOP: f32 = SEPARATOR_TOP + 7.0;
#[cfg(target_os = "macos")]
pub(crate) const HEIGHT: f32 = FOOTER_TOP + FOOTER_HEIGHT + PADDING;
#[cfg(target_os = "macos")]
pub(crate) const INTERNAL_HEIGHT: f32 = HEADING_TOP + HEADING_HEIGHT + 4.0 + 36.0 + PADDING;

#[derive(Clone, Copy)]
#[cfg(target_os = "macos")]
pub(crate) struct Palette {
    pub(crate) panel: Color,
    pub(crate) tile: Color,
    pub(crate) text: Color,
    pub(crate) muted: Color,
    pub(crate) icon: Color,
    pub(crate) hover: Color,
    pub(crate) border: Color,
    pub(crate) dark: bool,
}

#[cfg(target_os = "macos")]
impl Palette {
    pub(crate) fn from_theme(theme: &Theme) -> Self {
        Self {
            panel: theme.overlay_panel,
            tile: theme.overlay_selected,
            text: theme.text,
            muted: theme.text_muted,
            icon: theme.text,
            hover: theme.hover,
            border: theme.overlay_border,
            dark: theme.dark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color) -> f32 {
        let r = rune::color::srgb_encode(color.r);
        let g = rune::color::srgb_encode(color.g);
        let b = rune::color::srgb_encode(color.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn contrast(a: Color, b: Color) -> f32 {
        let (lighter, darker) = if luminance(a) > luminance(b) {
            (luminance(a), luminance(b))
        } else {
            (luminance(b), luminance(a))
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn composite(foreground: Color, background: Color) -> Color {
        Color {
            r: foreground.r * foreground.a + background.r * (1.0 - foreground.a),
            g: foreground.g * foreground.a + background.g * (1.0 - foreground.a),
            b: foreground.b * foreground.a + background.b * (1.0 - foreground.a),
            a: 1.0,
        }
    }

    #[test]
    fn semantic_icons_contrast_with_site_control_surfaces() {
        for dark in [false, true] {
            let theme = super::super::theme::palette(
                dark,
                false,
                100,
                crate::appearance::AccentColor::Blue,
            );
            assert!(contrast(theme.text, theme.overlay_panel) >= 3.0);
            let selected_surface = composite(theme.overlay_selected, theme.overlay_panel);
            assert!(contrast(theme.text_muted, selected_surface) >= 3.0);
        }
    }
}
