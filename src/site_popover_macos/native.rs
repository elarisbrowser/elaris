// SPDX-License-Identifier: GPL-3.0-only

//! Native AppKit host for the shared Site Control Centre presentation.

use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2_05::{ClassType, msg_send, msg_send_id, sel};
use objc2_app_kit_02::{
    NSBox, NSBoxType, NSButton, NSColor, NSFont, NSFontWeightMedium, NSFontWeightRegular,
    NSFontWeightSemibold, NSImage, NSImageSymbolConfiguration, NSTextField, NSTrackingArea,
    NSTrackingAreaOptions, NSView,
};
use objc2_foundation_02::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

use super::HOST;

pub(super) fn add_subview(parent: &NSView, child: &NSView) {
    // SAFETY: both views are retained and AppKit mutates this hierarchy on its main thread.
    unsafe { parent.addSubview(child) };
}

pub(super) fn rounded_surface(
    mtm: MainThreadMarker,
    frame: NSRect,
    fill: &NSColor,
    radius: f64,
) -> Retained<NSBox> {
    let surface = unsafe { NSBox::initWithFrame(mtm.alloc::<NSBox>(), frame) };
    // SAFETY: the custom box is retained and styled on AppKit's main thread.
    unsafe {
        surface.setBoxType(NSBoxType::NSBoxCustom);
        surface.setTransparent(false);
        surface.setContentViewMargins(NSSize::new(0.0, 0.0));
        surface.setFillColor(fill);
        surface.setBorderWidth(0.0);
        surface.setCornerRadius(radius);
    }
    surface
}

pub(super) fn hover_button() -> Retained<NSButton> {
    // SAFETY: the registered NSButton subclass only overrides AppKit mouse tracking callbacks.
    unsafe { msg_send_id![hover_button_class(), new] }
}

pub(super) fn add_hover_tracking(button: &NSButton, mtm: MainThreadMarker) {
    let options = NSTrackingAreaOptions::NSTrackingMouseEnteredAndExited
        | NSTrackingAreaOptions::NSTrackingActiveInKeyWindow
        | NSTrackingAreaOptions::NSTrackingInVisibleRect;
    // SAFETY: AppKit retains the tracking area when attached; InVisibleRect follows frame changes.
    let tracking_area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            mtm.alloc::<NSTrackingArea>(),
            rect(0.0, 0.0, 0.0, 0.0),
            options,
            Some(button),
            None,
        )
    };
    // SAFETY: both objects belong to AppKit's main thread and the button owns its tracking area.
    unsafe { button.addTrackingArea(&tracking_area) };
}

pub(super) fn label(
    value: &str,
    size: f64,
    weight: FontWeight,
    foreground: rune::Color,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let text = NSString::from_str(value);
    // SAFETY: label creation and style changes run on AppKit's main thread.
    let field = unsafe { NSTextField::labelWithString(&text, mtm) };
    let font = font(size, weight);
    let color = ns_color(foreground);
    unsafe {
        field.setFont(Some(&font));
        field.setTextColor(Some(&color));
    }
    field
}

#[derive(Clone, Copy)]
pub(super) enum FontWeight {
    Regular,
    Medium,
    Semibold,
}

fn font(size: f64, weight: FontWeight) -> Retained<NSFont> {
    // SAFETY: AppKit font construction runs on its main thread.
    unsafe {
        let weight = match weight {
            FontWeight::Regular => NSFontWeightRegular,
            FontWeight::Medium => NSFontWeightMedium,
            FontWeight::Semibold => NSFontWeightSemibold,
        };
        NSFont::systemFontOfSize_weight(size, weight)
    }
}

pub(super) fn symbol_image(
    name: &NSString,
    description: &NSString,
    size: f64,
    tint: &NSColor,
) -> Option<Retained<NSImage>> {
    // Bake the semantic foreground into the SF Symbol. NSButton's content tint does not reliably
    // reach symbol images in our AppKit host, even when they are marked as templates.
    let source = unsafe {
        NSImage::imageWithSystemSymbolName_accessibilityDescription(name, Some(description))
    }?;
    let configuration =
        unsafe { NSImageSymbolConfiguration::configurationWithHierarchicalColor(tint) };
    let image = unsafe { source.imageWithSymbolConfiguration(&configuration) }?;
    unsafe {
        image.setTemplate(false);
        image.setSize(NSSize::new(size, size));
    }
    Some(image)
}
unsafe extern "C" fn hover_entered(button: &AnyObject, _: Sel, _: &AnyObject) {
    let tag: isize = unsafe { msg_send![button, tag] };
    update_hover_surface(tag, true);
}

unsafe extern "C" fn hover_exited(button: &AnyObject, _: Sel, _: &AnyObject) {
    let tag: isize = unsafe { msg_send![button, tag] };
    update_hover_surface(tag, false);
}

fn update_hover_surface(tag: isize, hovered: bool) {
    let Ok(index) = usize::try_from(tag) else {
        return;
    };
    HOST.with(|slot| {
        let host = slot.borrow();
        let Some(surface) = host
            .as_ref()
            .and_then(|host| host.hover_surfaces.get(index))
        else {
            return;
        };
        // SAFETY: tracking callbacks and their views are on AppKit's main thread.
        unsafe {
            if surface.hover_only {
                surface.view.setHidden(!hovered);
            } else {
                surface.view.setFillColor(if hovered {
                    &surface.hover_color
                } else {
                    &surface.resting_color
                });
            }
        }
    });
}

fn hover_button_class() -> &'static AnyClass {
    const NAME: &str = "ElarisSiteControlHoverButton";
    if let Some(class) = AnyClass::get(NAME) {
        return class;
    }
    let mut builder =
        ClassBuilder::new(NAME, NSButton::class()).expect("register site popover hover button");
    // SAFETY: AppKit calls these NSResponder mouse-tracking callbacks with the event argument.
    unsafe {
        builder.add_method(
            sel!(mouseEntered:),
            hover_entered as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(mouseExited:),
            hover_exited as unsafe extern "C" fn(_, _, _),
        );
    }
    builder.register()
}
pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

pub(super) fn anchor_rect(view: &NSView, rune_rect: [f32; 4]) -> NSRect {
    // Rune reports window-point rectangles from the top-left. Winit's AppKit content view is
    // flipped, so its local coordinates use that same origin; keep the conversion correct if
    // the host view ever changes. Both APIs use logical points, not backing pixels.
    let bounds = view.bounds();
    let height = f64::from(rune_rect[3]);
    let y = if view.isFlipped() {
        bounds.origin.y + f64::from(rune_rect[1])
    } else {
        bounds.origin.y + bounds.size.height - f64::from(rune_rect[1]) - height
    };
    rect(
        bounds.origin.x + f64::from(rune_rect[0]),
        y,
        f64::from(rune_rect[2]),
        height,
    )
}

pub(super) fn top_frame(x: f64, top: f64, width: f64, height: f64, content_height: f64) -> NSRect {
    rect(x, content_height - top - height, width, height)
}

pub(super) fn set_top_frame(view: &NSView, top: f64, width: f64, height: f64, content_height: f64) {
    set_top_frame_at(view, 12.0, top, width, height, content_height);
}

pub(super) fn set_top_frame_at(
    view: &NSView,
    x: f64,
    top: f64,
    width: f64,
    height: f64,
    content_height: f64,
) {
    // SAFETY: `view` is owned by the popover controller and this runs on AppKit's main thread.
    unsafe { view.setFrame(top_frame(x, top, width, height, content_height)) };
}

pub(super) fn ns_color(color: rune::Color) -> Retained<NSColor> {
    // Rune stores linear sRGB; AppKit's sRGB initializer expects encoded components.
    unsafe {
        NSColor::colorWithSRGBRed_green_blue_alpha(
            rune::color::srgb_encode(color.r) as f64,
            rune::color::srgb_encode(color.g) as f64,
            rune::color::srgb_encode(color.b) as f64,
            color.a as f64,
        )
    }
}
