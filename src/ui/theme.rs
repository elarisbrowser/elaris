// SPDX-License-Identifier: GPL-3.0-only

//! The interface's colours and measurements, in one place. Every colour the interface draws is
//! a token of [`Theme`], named for what it is for, with a light and a dark value here; sizes
//! build on Rune's spacing and radius scales.

use rune::tok::{motion, radius, space};
use rune::{Anim, Curve, Easing, Spring, SpringPair, Transition};
use rune::{Color, oklch, oklcha};
use rune_kit::{Kit, PaletteFocus, PaletteSeed};

use crate::appearance::AccentColor;

/// The hue every neutral leans toward, a cool grey.
const HUE: f32 = 265.0;

pub(crate) struct Theme {
    /// Whether this palette represents the dark appearance.
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "macos")]
    pub(crate) dark: bool,
    /// The window behind the sidebar and around the page.
    pub(crate) window: Color,
    /// The page's container, seen before a page draws.
    pub(crate) page: Color,
    /// Soft surface behind cohesive settings groups.
    pub(crate) settings_surface: Color,
    pub(crate) text: Color,
    /// Labels that matter less: inactive tabs, the status line, placeholders.
    pub(crate) text_muted: Color,
    pub(crate) icon: Color,
    /// The selected colour for emphasized controls and indicators.
    pub(crate) accent: Color,
    /// The window controls at rest, before the pointer fades the native ones in on macOS.
    #[cfg(not(target_os = "linux"))]
    pub(crate) window_control: Color,
    /// Background of the sidebar address field.
    pub(crate) address_field: Color,
    pub(crate) control_border: Color,
    pub(crate) focus: Color,
    pub(crate) selection: Color,
    /// Overlays on buttons and rows under the pointer, and while pressed.
    pub(crate) hover: Color,
    pub(crate) press: Color,
    /// The active tab, raised above the sidebar.
    pub(crate) tab_active: Color,
    pub(crate) tab_active_shadow: Color,
    /// A tab lifted for a reorder, including inactive tabs.
    pub(crate) tab_drag: Color,
    /// A quiet mark for the current tab drop slot.
    pub(crate) tab_drop_indicator: Color,
    /// The dim layer and raised surface of the omnibox.
    pub(crate) overlay_scrim: Color,
    pub(crate) overlay_panel: Color,
    pub(crate) overlay_border: Color,
    pub(crate) overlay_shadow: Color,
    pub(crate) overlay_selected: Color,
    /// How faint a control that can't be used now is.
    pub(crate) disabled_opacity: f32,
}

impl Theme {
    pub(crate) fn kit(&self, surface: Color) -> Kit {
        let mut kit = Kit::from_palette_focus(
            PaletteSeed {
                bg: surface,
                fg: self.text,
                accent: self.accent,
            },
            PaletteFocus::Custom(self.focus),
        );
        // Kit uses faint text for informational labels and shortcuts as well as disabled items.
        kit.c.text_faint = self.text_muted;
        kit.c.text_muted = self.text_muted;
        kit.c.text_weak = self.text_muted;
        kit.c.icon_muted = self.icon;
        kit.c.icon_faint = self.icon;
        kit.c.hover = self.hover;
        kit.c.pressed = self.press;
        kit
    }
}

/// The palette for a dark or light interface.
pub(crate) fn palette(
    dark: bool,
    glass_enabled: bool,
    tint_opacity: u8,
    accent_choice: AccentColor,
) -> Theme {
    let accent_swatch = accent_color(accent_choice);
    let accent = accent_focus_color(accent_choice, dark);
    let selection = if dark {
        accent_swatch.mix(Color::BLACK, 0.58)
    } else {
        accent_swatch.mix(Color::WHITE, 0.55)
    };
    let alpha = if glass_enabled {
        f32::from(tint_opacity.min(100)) / 100.0
    } else {
        1.0
    };
    // Keep the field and selected tab distinguishable even with the window tint turned off.
    const MIN_CHROME_ALPHA: f32 = 0.25;
    let chrome_alpha = MIN_CHROME_ALPHA + (1.0 - MIN_CHROME_ALPHA) * alpha;
    if dark {
        Theme {
            #[cfg(target_os = "macos")]
            dark: true,
            window: oklch(0.185, 0.004, HUE).alpha(alpha),
            page: oklch(0.232, 0.005, HUE),
            settings_surface: oklch(0.255, 0.006, HUE),
            text: oklch(0.955, 0.003, HUE),
            text_muted: oklch(0.81, 0.008, HUE),
            icon: oklch(0.82, 0.006, HUE),
            accent,
            #[cfg(not(target_os = "linux"))]
            window_control: oklcha(1.0, 0.0, 0.0, 0.16),
            address_field: oklch(0.24, 0.006, HUE).alpha(chrome_alpha),
            control_border: oklch(0.56, 0.015, HUE),
            focus: accent,
            selection,
            hover: oklcha(0.66, 0.008, HUE, 0.08),
            press: oklcha(0.66, 0.008, HUE, 0.13),
            tab_active: oklch(0.36, 0.009, HUE).alpha(chrome_alpha),
            tab_active_shadow: oklcha(0.0, 0.0, 0.0, 0.3),
            tab_drag: oklch(0.32, 0.009, HUE).alpha(chrome_alpha),
            tab_drop_indicator: accent,
            overlay_scrim: oklcha(0.0, 0.0, 0.0, 0.16),
            overlay_panel: oklch(0.255, 0.006, HUE),
            overlay_border: oklcha(1.0, 0.0, 0.0, 0.11),
            overlay_shadow: oklcha(0.0, 0.0, 0.0, 0.55),
            overlay_selected: oklch(0.33, 0.016, HUE),
            disabled_opacity: 0.35,
        }
    } else {
        Theme {
            #[cfg(target_os = "macos")]
            dark: false,
            window: oklch(0.947, 0.008, HUE).alpha(alpha),
            page: oklch(1.0, 0.0, 0.0),
            settings_surface: oklch(0.975, 0.004, HUE),
            text: oklch(0.2, 0.008, HUE),
            text_muted: oklch(0.38, 0.016, HUE),
            icon: oklch(0.32, 0.01, HUE),
            accent,
            #[cfg(not(target_os = "linux"))]
            window_control: oklcha(0.2, 0.02, HUE, 0.16),
            address_field: oklch(0.985, 0.004, HUE).alpha(chrome_alpha),
            control_border: oklch(0.55, 0.015, HUE),
            focus: accent,
            selection,
            hover: oklcha(0.2, 0.02, HUE, 0.06),
            press: oklcha(0.2, 0.02, HUE, 0.1),
            tab_active: oklch(1.0, 0.0, 0.0).alpha(chrome_alpha),
            tab_active_shadow: oklcha(0.2, 0.02, HUE, 0.18),
            tab_drag: oklch(0.968, 0.009, HUE).alpha(chrome_alpha),
            tab_drop_indicator: accent,
            overlay_scrim: oklcha(0.2, 0.02, HUE, 0.12),
            overlay_panel: oklch(0.995, 0.002, HUE),
            overlay_border: oklcha(0.2, 0.02, HUE, 0.12),
            overlay_shadow: oklcha(0.2, 0.02, HUE, 0.2),
            overlay_selected: oklcha(0.2, 0.02, HUE, 0.07),
            disabled_opacity: 0.3,
        }
    }
}

/// Accent swatches and semantic accent tokens share one palette.
pub(crate) fn accent_color(accent: AccentColor) -> Color {
    let (lightness, chroma, hue) = accent_values(accent);
    oklch(lightness, chroma, hue)
}

fn accent_focus_color(accent: AccentColor, dark: bool) -> Color {
    let lightness = if dark { 0.72 } else { 0.55 };
    let (_, chroma, hue) = accent_values(accent);
    oklch(lightness, chroma, hue)
}

fn accent_values(accent: AccentColor) -> (f32, f32, f32) {
    match accent {
        AccentColor::Blue => (0.67, 0.19, 255.0),
        AccentColor::Purple => (0.67, 0.16, 300.0),
        AccentColor::Pink => (0.67, 0.18, 350.0),
        AccentColor::Orange => (0.69, 0.15, 62.0),
        AccentColor::Green => (0.67, 0.16, 150.0),
        AccentColor::Gray => (0.65, 0.015, HUE),
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
pub(crate) const HORIZONTAL_STRIP_HEIGHT: f32 = HEADER_HEIGHT - 4.0;
pub(crate) const HORIZONTAL_FIELD_HEIGHT: f32 = FIELD_HEIGHT - 8.0;
pub(crate) const HORIZONTAL_NAV_HEIGHT: f32 = HORIZONTAL_FIELD_HEIGHT;
pub(crate) const HORIZONTAL_CHROME_HEIGHT: f32 = HORIZONTAL_STRIP_HEIGHT + HORIZONTAL_NAV_HEIGHT;
pub(crate) const HORIZONTAL_TAB_HEIGHT: f32 = ROW_HEIGHT - 8.0;
pub(crate) const HORIZONTAL_COMPACT_TAB: f32 = HORIZONTAL_TAB_HEIGHT;
pub(crate) const HORIZONTAL_TAB_MIN: f32 = 104.0;
pub(crate) const HORIZONTAL_TAB_MAX: f32 = 176.0;
pub(crate) const HORIZONTAL_EMPTY_SECTION: f32 = space::S2;
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
pub(crate) const ICON_BUTTON_RADIUS: f32 = radius::MD;
pub(crate) const LOADING_PERIOD: f64 = 1.0;
pub(crate) const ICON: f32 = 16.0;
pub(crate) const CLOSE_BUTTON: f32 = 22.0;
/// The space between the navigation buttons, and between them and the sidebar toggle while the
/// sidebar is hidden.
pub(crate) const NAV_GAP: f32 = space::S1;
pub(crate) const HEADER_GROUP_GAP: f32 = space::S3;

/// How far buttons and rows shrink while pressed, springing back on release.
pub(crate) const PRESS_SCALE: f32 = 0.97;

pub(super) const HOVER_ENTER: Anim = Anim::new(Curve::tween(0.08, Easing::EASE_OUT));
pub(super) const HOVER_EXIT: Anim = Anim::new(Curve::tween(0.15, Easing::EASE_OUT));

/// Compact overlays settle into place on entry and fade in place on dismissal.
pub(super) fn overlay_reveal(reduced_motion: bool) -> Transition {
    const ENTER_OFFSET: f32 = -4.0;
    const PRESENCE: SpringPair = SpringPair::new(Spring::new(0.25, 0.0), Spring::new(0.15, 0.0));

    if reduced_motion {
        return Transition::none();
    }

    Transition::asymmetric(
        Transition::offset(0.0, ENTER_OFFSET).combined(Transition::opacity()),
        Transition::opacity(),
    )
    .springs(PRESENCE)
}

/// Headings for native pages. Compact labels and body copy use Rune’s type scale.
pub(crate) const PAGE_TITLE: rune::TextStyle = rune::TextStyle::new(26.0, 600.0).lh(32.0);

/// The address field.
pub(crate) const FOCUS_WIDTH: f32 = 2.0;
pub(crate) const FIELD_HEIGHT: f32 = 36.0;
pub(crate) const FIELD_PADDING: f32 = space::S3;
pub(crate) const FIELD_RADIUS: f32 = radius::LG;

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
pub(crate) const OMNIBOX_SEARCH_ICON: f32 = 16.0;
pub(crate) const OMNIBOX_SEARCH_CHEVRON: f32 = 12.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_HEIGHT: f32 = 28.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_PADDING: f32 = 6.0;
pub(crate) const OMNIBOX_SEARCH_BUTTON_GAP: f32 = 4.0;

/// The compact floating Find in Page bar.
pub(crate) const FIND_WIDTH: f32 = 344.0;
pub(crate) const FIND_HEIGHT: f32 = 44.0;
pub(crate) const FIND_INSET: f32 = 16.0;
pub(crate) const FIND_RADIUS: f32 = radius::LG;
pub(crate) const FIND_MOTION: Spring = Spring::new(0.22, 0.0);
pub(crate) const FIND_BUTTON: f32 = 26.0;
pub(crate) const FIND_PADDING: f32 = 6.0;
pub(crate) const FIND_GAP: f32 = 4.0;
pub(crate) const FIND_SHADOW_Y: f32 = 3.0;
pub(crate) const FIND_SHADOW_BLUR: f32 = 12.0;
pub(crate) const FIND_INPUT_TEXT: rune::TextStyle = rune::TextStyle::new(14.0, 400.0);

/// Rows in the tab list, and the new-tab and appearance buttons that share their shape.
pub(crate) const ROW_HEIGHT: f32 = 34.0;
pub(crate) const ROW_RADIUS: f32 = radius::LG;
pub(crate) const ROW_PADDING: f32 = space::S2;
pub(crate) const ROW_GAP: f32 = space::S2;
pub(crate) const TAB_GAP: f32 = space::S1;
/// The raised look of the active tab.
pub(crate) const ACTIVE_SHADOW_Y: f32 = 0.5;
pub(crate) const ACTIVE_SHADOW_BLUR: f32 = 1.5;
/// How the active tab's highlight moves to the tab pressed.
pub(crate) const SELECTION_MOTION: Spring = motion::SNAPPY;
/// The short spring shared by live tab preview movement and the dragged tab's drop.
pub(crate) const TAB_REORDER_MOTION: Spring = Spring::new(0.2, 0.12);
pub(crate) const TAB_DROP_SETTLE_SECONDS: f32 = 0.2;
pub(crate) const TAB_DRAG_SCROLL_EDGE: f32 = 34.0;
pub(crate) const TAB_DRAG_SCROLL_MAX: f32 = 520.0;
pub(crate) const TAB_DRAG_SHADOW_Y: f32 = 3.0;
pub(crate) const TAB_DRAG_SHADOW_BLUR: f32 = 8.0;
pub(crate) const FAVOURITE_TILE_HEIGHT: f32 = 48.0;
pub(crate) const FAVOURITE_TILE_MIN_WIDTH: f32 = 40.0;
pub(crate) const FAVOURITE_TILE_RADIUS: f32 = radius::MD;
pub(crate) const FAVOURITE_GAP: f32 = space::S2;
pub(crate) const FAVOURITE_COLUMNS: usize = 4;
pub(crate) const FAVOURITE_ICON: f32 = 20.0;

/// The web page's container: how far it sits in from the window's edges, and its corners.
pub(crate) const VIEWPORT_INSET: f32 = space::S1 + space::S1 / 2.0;
pub(crate) const VIEWPORT_RADIUS: f32 = radius::LG;
/// The status badge's inset from the web page container's lower-right corner.
#[cfg(not(feature = "ladybird-bridge"))]
pub(crate) const VIEWPORT_BADGE_INSET: f32 = space::S3;

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(foreground: Color, background: Color) -> f32 {
        let foreground = paint(foreground, background).luminance();
        let background = background.luminance();
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    // Rune colours are linear sRGB. Model source-over against an opaque surface.
    fn paint(foreground: Color, background: Color) -> Color {
        background.mix(foreground.alpha(1.0), foreground.a)
    }

    #[test]
    fn sidebar_fills_follow_window_tint() {
        for dark in [false, true] {
            let accent = AccentColor::Blue;
            let clear = palette(dark, true, 0, accent);
            let tinted = palette(dark, true, 50, accent);
            let solid = palette(dark, false, 50, accent);
            for (clear, tinted, solid) in [
                (
                    clear.address_field,
                    tinted.address_field,
                    solid.address_field,
                ),
                (clear.tab_active, tinted.tab_active, solid.tab_active),
                (clear.tab_drag, tinted.tab_drag, solid.tab_drag),
            ] {
                assert!(0.0 < clear.a && clear.a < tinted.a && tinted.a < solid.a);
                assert_eq!(solid.a, 1.0);
            }
            assert_eq!(tinted.text.a, 1.0);
        }
    }

    #[test]
    fn floating_icon_buttons_remain_legible_over_web_content() {
        for dark in [false, true] {
            let theme = palette(dark, true, 100, AccentColor::ALL[0]);
            assert_eq!(theme.overlay_panel.a, 1.0);
            assert!(contrast(theme.control_border, theme.overlay_panel) >= 3.0);
            for state in [Color::TRANSPARENT, theme.hover, theme.press] {
                assert!(contrast(theme.icon, paint(state, theme.overlay_panel)) >= 4.5);
            }
        }
    }

    #[test]
    fn kit_labels_and_focus_are_readable_on_derived_surfaces() {
        for dark in [false, true] {
            for accent in AccentColor::ALL {
                let theme = palette(dark, true, 100, accent);
                for base in [theme.settings_surface, theme.overlay_panel] {
                    let kit = theme.kit(base);
                    for surface in [kit.c.bg_base, kit.c.surface_raised, kit.c.bg_button_neutral] {
                        for state in [Color::TRANSPARENT, kit.c.hover, kit.c.pressed] {
                            let background = paint(state, surface);
                            assert!(contrast(kit.c.text_base, background) >= 4.5);
                            assert!(contrast(kit.c.text_faint, background) >= 4.5);
                            assert!(contrast(kit.c.icon_muted, background) >= 3.0);
                            let ratio = contrast(kit.c.border_focus, background);
                            assert!(
                                ratio >= 3.0,
                                "kit focus for {accent:?} (dark={dark}): {ratio:.2} < 3"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn chrome_contrast_in_both_appearances() {
        for dark in [false, true] {
            for accent in AccentColor::ALL {
                let t = palette(dark, true, 100, accent);
                for (role, base) in [
                    ("sidebar and inactive tabs", t.window),
                    ("active tabs", t.tab_active),
                    ("dragged tabs", t.tab_drag),
                    ("address launcher", t.address_field),
                    ("native pages", t.page),
                    ("settings groups", t.settings_surface),
                    ("overlay input and status", t.overlay_panel),
                    (
                        "selected overlay row",
                        paint(t.overlay_selected, t.overlay_panel),
                    ),
                ] {
                    assert_eq!(base.a, 1.0, "{role} must have a stable backdrop");
                    for treatment in [Color::TRANSPARENT, t.hover, t.press] {
                        let background = paint(treatment, base);
                        for (foreground, minimum) in [
                            (t.text, 4.5),
                            (t.text_muted, 4.5),
                            (t.icon, 3.0),
                            (t.focus, 3.0),
                        ] {
                            let ratio = contrast(foreground, background);
                            assert!(
                                ratio >= minimum,
                                "{role} (accent={accent:?}, dark={dark}): {ratio:.2} < {minimum}"
                            );
                        }
                    }
                }
                assert!(contrast(t.text, t.selection) >= 4.5);
                assert!(contrast(t.tab_drop_indicator, t.window) >= 3.0);
                for background in [t.address_field, t.window, t.settings_surface] {
                    assert!(contrast(t.control_border, background) >= 3.0);
                }
            }
        }
    }
}
