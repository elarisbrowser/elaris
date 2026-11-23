// SPDX-License-Identifier: GPL-3.0-only

//! The interface's colours and measurements, in one place. Every colour the interface draws is
//! a token of [`Theme`], named for what it is for, with a light and a dark value here; sizes
//! build on Rune's spacing and radius scales.

use rune::tok::{motion, radius, space};
use rune::{Anim, Curve, Easing, Spring, SpringPair, Transition};
use rune::{Color, oklch, oklcha};
use rune_kit::{Kit, PaletteFocus, PaletteSeed};

use elaris_core::appearance::AccentColor;

/// The hue every neutral leans toward, a cool grey.
const HUE: f32 = 265.0;

pub(crate) struct Theme {
    /// The window behind the sidebar and around the page.
    pub(crate) window: Color,
    /// The page's container, seen before a page draws.
    pub(crate) page: Color,
    /// Soft surface behind cohesive settings groups.
    pub(crate) settings_surface: Color,
    /// Hairlines between rows inside a grouped surface.
    pub(crate) separator: Color,
    pub(crate) text: Color,
    /// Labels that matter less: inactive tabs, the status line, placeholders.
    pub(crate) text_muted: Color,
    pub(crate) icon: Color,
    /// The selected colour for emphasized controls and indicators.
    pub(crate) accent: Color,
    /// The window controls at rest, before the pointer fades the native ones in on macOS.
    #[cfg(not(elaris_ui = "linux"))]
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
    /// The band behind a menu's header, and the buttons in the body below it.
    pub(crate) overlay_header: Color,
    pub(crate) overlay_control: Color,
    pub(crate) overlay_control_hover: Color,
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
            window: oklch(0.185, 0.004, HUE).alpha(alpha),
            page: oklch(0.232, 0.005, HUE),
            settings_surface: oklch(0.255, 0.006, HUE),
            separator: oklcha(1.0, 0.0, 0.0, 0.07),
            text: oklch(0.955, 0.003, HUE),
            text_muted: oklch(0.81, 0.008, HUE),
            icon: oklch(0.82, 0.006, HUE),
            accent,
            #[cfg(not(elaris_ui = "linux"))]
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
            overlay_header: oklch(0.215, 0.005, HUE),
            overlay_control: oklcha(1.0, 0.0, 0.0, 0.055),
            overlay_control_hover: oklcha(1.0, 0.0, 0.0, 0.1),
            disabled_opacity: 0.35,
        }
    } else {
        Theme {
            window: oklch(0.947, 0.008, HUE).alpha(alpha),
            page: oklch(1.0, 0.0, 0.0),
            settings_surface: oklch(0.975, 0.004, HUE),
            separator: oklcha(0.2, 0.02, HUE, 0.08),
            text: oklch(0.2, 0.008, HUE),
            text_muted: oklch(0.38, 0.016, HUE),
            icon: oklch(0.32, 0.01, HUE),
            accent,
            #[cfg(not(elaris_ui = "linux"))]
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
            overlay_header: oklch(0.955, 0.005, HUE),
            overlay_control: oklcha(0.2, 0.02, HUE, 0.05),
            overlay_control_hover: oklcha(0.2, 0.02, HUE, 0.09),
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
/// A new browser window's size.
pub(crate) const WINDOW_SIZE: (f32, f32) = (960.0, 640.0);
/// A mini window opens tall and narrow, like a phone-sized reader, and shrinks to a sliver of one.
pub(crate) const MINI_WINDOW_SIZE: (f32, f32) = (480.0, 640.0);
pub(crate) const MINI_WINDOW_MIN_SIZE: (f32, f32) = (320.0, 240.0);
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
/// Space above and below a sidebar divider, so the divider sits close to the sections it separates.
pub(crate) const SIDEBAR_DIVIDER_PADDING: f32 = space::S1;
/// The "Clear" button at the end of the regular tabs' divider.
pub(crate) const SIDEBAR_CLEAR_GAP: f32 = space::S1;
pub(crate) const SIDEBAR_CLEAR_ICON: f32 = 12.0;
/// The row level with the window controls, holding the navigation buttons: the height of the
/// titlebar band macOS drags, since the controls sit centred in it.
pub(crate) const HORIZONTAL_STRIP_HEIGHT: f32 = HEADER_HEIGHT - 4.0;
pub(crate) const HORIZONTAL_FIELD_HEIGHT: f32 = FIELD_HEIGHT - 8.0;
pub(crate) const HORIZONTAL_NAV_HEIGHT: f32 = HORIZONTAL_FIELD_HEIGHT;
pub(crate) const HORIZONTAL_CHROME_HEIGHT: f32 = HORIZONTAL_STRIP_HEIGHT + HORIZONTAL_NAV_HEIGHT;
pub(crate) const HORIZONTAL_TAB_HEIGHT: f32 = ROW_HEIGHT - 8.0;
pub(crate) const HORIZONTAL_TAB_RADIUS: f32 = radius::MD;
pub(crate) const HORIZONTAL_FIELD_RADIUS: f32 = radius::MD;
pub(crate) const HORIZONTAL_COMPACT_TAB: f32 = HORIZONTAL_TAB_HEIGHT;
pub(crate) const HORIZONTAL_TAB_MIN: f32 = 104.0;
pub(crate) const HORIZONTAL_TAB_MAX: f32 = 176.0;
pub(crate) const HORIZONTAL_SPLIT_MARKER_HEIGHT: f32 = 2.0;
pub(crate) const HORIZONTAL_EMPTY_SECTION: f32 = space::S2;
pub(crate) const HEADER_HEIGHT: f32 = 38.0;
/// The top-edge target that reveals the Linux toolbar while it is hidden.
#[cfg(elaris_ui = "linux")]
pub(crate) const TITLEBAR_HOT_ZONE: f32 = 6.0;
/// The Linux toolbar and the content beneath it settle with a short, quiet spring.
#[cfg(elaris_ui = "linux")]
pub(crate) const TITLEBAR_REVEAL: Spring = Spring::new(0.28, 0.0);
/// Where the window controls sit: centred in the header, as far in from the side as from the top.
pub(crate) const TRAFFIC_LIGHTS: (f32, f32) = (
    (HEADER_HEIGHT - rune::Chrome::TRAFFIC_LIGHT) / 2.0,
    (HEADER_HEIGHT - rune::Chrome::TRAFFIC_LIGHT) / 2.0,
);

#[cfg(elaris_ui = "macos")]
pub(crate) const fn native_titlebar(height: f32) -> rune::Titlebar {
    rune::Titlebar::new(height).traffic_lights(
        TRAFFIC_LIGHTS.0,
        (height - rune::Chrome::TRAFFIC_LIGHT) / 2.0,
    )
}

/// Square icon buttons: back, forward, reload, and each tab's close button.
pub(crate) const ICON_BUTTON: f32 = 28.0;
pub(crate) const ICON_BUTTON_RADIUS: f32 = radius::MD;
pub(crate) const LOADING_PERIOD: f64 = 1.0;
pub(crate) const ICON: f32 = 16.0;
/// How visible a sleeping tab is in the tab list.
pub(crate) const ASLEEP_OPACITY: f32 = 0.5;
/// The bed beside a tab preview's Memory Saver status.
pub(crate) const PREVIEW_STATUS_ICON: f32 = 12.0;
pub(crate) const CLOSE_BUTTON: f32 = 24.0;
/// The space between the navigation buttons, and between them and the sidebar toggle while the
/// sidebar is hidden.
pub(crate) const NAV_GAP: f32 = space::S1;
pub(crate) const HEADER_GROUP_GAP: f32 = space::S3;

/// How far buttons and rows shrink while pressed, springing back on release.
pub(crate) const PRESS_SCALE: f32 = 0.97;
/// How long the copy-link button shows its tick, in seconds.
pub(crate) const COPY_LINK_CONFIRM: f64 = 1.5;
pub(crate) const TICK_STROKE: f32 = 1.5;

/// The round back/forward indicator a history swipe slides in from the page's edge.
pub(crate) const SWIPE_INDICATOR: f32 = 56.0;
pub(crate) const SWIPE_INDICATOR_ICON: f32 = 24.0;
/// How far in from the edge it rests once the swipe is armed.
pub(crate) const SWIPE_INDICATOR_INSET: f32 = space::S6;
/// How visible it is as the swipe starts, before it grows to full opacity.
pub(crate) const SWIPE_INDICATOR_MIN_OPACITY: f32 = 0.35;
/// Its scale before arming; it pops to full size when releasing will navigate.
pub(crate) const SWIPE_INDICATOR_REST_SCALE: f32 = 0.86;
pub(crate) const SWIPE_INDICATOR_SETTLE: Anim = Anim::new(Curve::tween(0.14, Easing::EASE_OUT));

pub(super) const HIGHLIGHT_MOTION: Spring = Spring::new(0.10, 0.0);
pub(super) const HOVER_ENTER: Anim = Anim::new(Curve::tween(0.08, Easing::EASE_OUT));
pub(super) const HOVER_EXIT: Anim = Anim::new(Curve::tween(0.15, Easing::EASE_OUT));
/// The scale a swapped-out glyph shrinks to (and a swapped-in one grows from).
pub(crate) const GLYPH_MORPH_SCALE: f32 = 0.6;
/// The blur, in points, a swapped-out glyph dissolves into.
pub(crate) const GLYPH_MORPH_BLUR: f32 = 2.0;
/// The incoming glyph pops in with a slight overshoot; the outgoing one leaves quickly.
pub(crate) const GLYPH_MORPH_MOTION: SpringPair =
    SpringPair::new(Spring::new(0.3, 0.35), Spring::new(0.15, 0.0));

/// One glyph replacing another in place: it shrinks, blurs and fades out as its successor
/// pops up and sharpens in.
pub(crate) fn glyph_morph(reduced_motion: bool) -> Transition {
    if reduced_motion {
        return Transition::none();
    }
    Transition::scale(GLYPH_MORPH_SCALE)
        .combined(Transition::blur(GLYPH_MORPH_BLUR))
        .springs(GLYPH_MORPH_MOTION)
        .allow_small_scale()
}

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
/// Headings above each grouped surface on a native page.
pub(crate) const PAGE_SECTION_TITLE: rune::TextStyle = rune::TextStyle::new(14.0, 600.0).lh(19.0);
/// How far a native page's content sits from the top, and the gap under each section heading.
pub(crate) const PAGE_TOP_PADDING: f32 = space::S6;
pub(crate) const PAGE_SECTION_TITLE_GAP: f32 = 9.0;
/// Headings that open a category of sections, such as a group of Settings.
pub(crate) const PAGE_CATEGORY_TITLE: rune::TextStyle = rune::TextStyle::new(20.0, 600.0).lh(26.0);
/// The title of a single row inside a grouped surface.
pub(crate) const PAGE_ROW_TITLE: rune::TextStyle = rune::TextStyle::new(14.0, 500.0).lh(19.0);
/// The rounded surface that groups a native page's rows.
pub(crate) const PAGE_GROUP_RADIUS: f32 = radius::LG;
pub(crate) const PAGE_GROUP_PADDING: f32 = space::S3 + space::S1;
/// Native page column width, and the pane width below which rows stack.
pub(crate) const PAGE_CONTENT_WIDTH: f32 = 760.0;
pub(crate) const PAGE_COMPACT_BREAKPOINT: f32 = 560.0;
pub(crate) const PAGE_HEADER_ICON: f32 = 32.0;
/// The message a tab shows when its page crashed.
pub(crate) const CRASH_PAGE_WIDTH: f32 = 420.0;
/// Space between sections, and the larger space between categories (twice as wide, so each
/// category reads as one group).
pub(crate) const PAGE_SECTION_GAP: f32 = space::S6;
/// Width of confirmation and notice dialogs on internal pages.
pub(crate) const PAGE_DIALOG_WIDTH: f32 = 400.0;
pub(crate) const PAGE_CATEGORY_GAP: f32 = PAGE_SECTION_GAP * 2.0;
/// Vertical padding of a grouped row, the gap between its copy and control, and the copy's
/// line gap.
pub(crate) const PAGE_ROW_PADDING: f32 = space::S3;
pub(crate) const PAGE_ROW_MIN_HEIGHT: f32 = 52.0;
pub(crate) const PAGE_ROW_GAP: f32 = space::S3 + space::S1;
pub(crate) const PAGE_ROW_COPY_GAP: f32 = 3.0;
/// The icon tile that can lead a grouped row, and its corner radius.
pub(crate) const PAGE_ROW_ICON: f32 = 32.0;
pub(crate) const PAGE_ROW_ICON_RADIUS: f32 = radius::MD;
/// Side navigation on pages with categories: its width, row height and inner padding, and the
/// pane width below which it folds into a horizontal row under the title.
pub(crate) const PAGE_NAV_WIDTH: f32 = 196.0;
pub(crate) const PAGE_NAV_ROW_HEIGHT: f32 = 32.0;
pub(crate) const PAGE_NAV_ROW_PADDING: f32 = space::S2;
pub(crate) const PAGE_NAV_BREAKPOINT: f32 = 760.0;
/// Inset between the navigation container and the highlights that slide inside it.
pub(crate) const PAGE_NAV_INSET: f32 = space::S1;
/// Space between navigation items.
pub(crate) const PAGE_NAV_GAP: f32 = space::S1 / 2.0;
/// Width shared by field-style controls (selects, segmented controls, sliders) in page rows.
pub(crate) const PAGE_CONTROL_WIDTH: f32 = 220.0;
/// Task Manager rows, and the width of each figure column after the name.
pub(crate) const TASK_MANAGER_ROW_HEIGHT: f32 = 36.0;
pub(crate) const TASK_MANAGER_METRIC_WIDTH: f32 = 84.0;
pub(crate) const TASK_MANAGER_PID_WIDTH: f32 = 64.0;
/// How far a site's processes sit inside its row.
pub(crate) const TASK_MANAGER_INDENT: f32 = space::S6;

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
/// How long a website permission prompt ignores clicks, so a page cannot time one under the pointer.
pub(crate) const PERMISSION_PROMPT_ARM_SECONDS: f64 = 0.6;
/// How long the page-zoom chip stays up after a zoom change, in seconds.
pub(crate) const ZOOM_CHIP_SECONDS: f64 = 2.0;
/// The page-zoom chip's fixed width, so a change in its number does not resize it.
pub(crate) const ZOOM_CHIP_WIDTH: f32 = 48.0;
/// How long the exit hint stays up after a page enters fullscreen.
pub(crate) const FULLSCREEN_NOTICE_SECONDS: f64 = 4.0;
pub(crate) const FIND_RADIUS: f32 = radius::LG;
pub(crate) const FIND_MOTION: Spring = Spring::new(0.22, 0.0);
pub(crate) const FIND_BUTTON: f32 = 26.0;
pub(crate) const FIND_PADDING: f32 = 6.0;
pub(crate) const FIND_GAP: f32 = 4.0;
pub(crate) const FIND_SHADOW_Y: f32 = 3.0;
pub(crate) const FIND_SHADOW_BLUR: f32 = 12.0;
pub(crate) const FIND_INPUT_TEXT: rune::TextStyle = rune::TextStyle::new(14.0, 400.0);

/// Toasts: short confirmations that float over the window's bottom edge.
pub(crate) const TOAST_WIDTH: f32 = 320.0;
pub(crate) const TOAST_INSET: f32 = 16.0;
pub(crate) const TOAST_GAP: f32 = 10.0;
pub(crate) const TOAST_LINE_GAP: f32 = 2.0;
pub(crate) const TOAST_RADIUS: f32 = radius::LG;
pub(crate) const TOAST_ICON: f32 = 16.0;
pub(crate) const TOAST_SHADOW_Y: f32 = 6.0;
pub(crate) const TOAST_SHADOW_BLUR: f32 = 18.0;
/// How far a toast starts below its place and how much it shrinks, as it rises in and falls out.
pub(crate) const TOAST_RISE: f32 = 12.0;
pub(crate) const TOAST_SCALE: f32 = 0.96;
/// The spring a toast enters and leaves on: a little bounce, so it seems to land.
pub(crate) const TOAST_PRESENCE: SpringPair =
    SpringPair::new(Spring::new(0.32, 0.14), Spring::new(0.18, 0.0));
/// The spring that slides the rest of the stack into the gap a leaving toast leaves.
pub(crate) const TOAST_REFLOW: Spring = Spring::new(0.3, 0.0);

/// Menus, context menus and popovers: a rounded panel whose rows sit in an inset container, as the
/// Settings category list does. Each highlight is concentric with the panel's corners.
pub(crate) const MENU_RADIUS: f32 = radius::LG;
pub(crate) const MENU_INSET: f32 = PAGE_NAV_INSET;
pub(crate) const MENU_ITEM_RADIUS: f32 = MENU_RADIUS - MENU_INSET;
pub(crate) const MENU_MIN_WIDTH: f32 = 160.0;
/// The gap between a menu and its trigger, and the rows' height and inner padding.
pub(crate) const MENU_GAP: f32 = 6.0;
pub(crate) const MENU_ROW_HEIGHT: f32 = 28.0;
pub(crate) const MENU_ROW_PADDING: f32 = space::S2 + space::S1 / 2.0;
pub(crate) const MENU_ICON: f32 = 14.0;
pub(crate) const MENU_ICON_GAP: f32 = space::S2;
pub(crate) const MENU_SEPARATOR_GAP: f32 = space::S1;
pub(crate) const MENU_SHADOW_Y: f32 = 8.0;
pub(crate) const MENU_SHADOW_BLUR: f32 = 22.0;
/// How far each row rises as it fades in, and the gap between the first and last row's entrance.
pub(crate) const MENU_ROW_OFFSET: f32 = 4.0;
/// The scale a menu panel grows from as it opens.
pub(crate) const MENU_SCALE_FROM: f32 = 0.97;
pub(crate) const MENU_ROW_STAGGER: f32 = 0.03;
/// The page menu's header: the page's mark in a rounded tile beside its title and host, above a
/// row of navigation buttons.
pub(crate) const MENU_HEADER_TILE: f32 = 32.0;
pub(crate) const MENU_HEADER_ICON: f32 = 18.0;
pub(crate) const MENU_HEADER_TITLE: rune::TextStyle = rune::TextStyle::new(14.0, 600.0).lh(19.0);
pub(crate) const MENU_HEADER_PADDING: f32 = space::S2;
pub(crate) const MENU_NAV_WIDTH: f32 = 92.0;
pub(crate) const MENU_NAV_HEIGHT: f32 = 30.0;
pub(crate) const MENU_NAV_GAP: f32 = space::S1;
/// Concentric with the body card, which is concentric with the panel.
pub(crate) const MENU_NAV_RADIUS: f32 = MENU_ITEM_RADIUS - MENU_INSET;

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
/// How a tab row opens and its gap closes: brisk and without overshoot, so it lands cleanly.
pub(crate) const TAB_PRESENCE_MOTION: Spring = Spring::new(0.22, 0.0);
/// The feathered edge a tab row reveals through as it opens: shallow and quick to clear.
pub(crate) const TAB_PRESENCE_EDGE: rune::StreamEdge = rune::edge::FAST_FEATHER;
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
pub(crate) const HORIZONTAL_VIEWPORT_RADIUS: f32 = radius::MD;
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
