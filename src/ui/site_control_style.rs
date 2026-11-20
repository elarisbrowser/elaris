// SPDX-License-Identifier: GPL-3.0-only

//! Shared dimensions for the Site Control Centre.

#[cfg(test)]
use rune::Color;

pub(crate) const WIDTH: f32 = 280.0;
pub(crate) const OUTER_RADIUS: f32 = 20.0;
pub(crate) const PADDING: f32 = 8.0;
pub(crate) const ACTION_HEIGHT: f32 = 34.0;
pub(crate) const ACTION_GAP: f32 = 6.0;
pub(crate) const ACTION_RADIUS: f32 = 12.0;
pub(crate) const ROW_HEIGHT: f32 = 40.0;
pub(crate) const ICON_CIRCLE: f32 = 28.0;
pub(crate) const GLYPH: f32 = 16.0;
pub(crate) const ROW_GLYPH: f32 = 11.0;
pub(crate) const ROW_RADIUS: f32 = 8.0;
pub(crate) const ROW_GAP: f32 = 2.0;
pub(crate) const SECTION_GAP: f32 = 12.0;
pub(crate) const FOOTER_HEIGHT: f32 = 28.0;

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
