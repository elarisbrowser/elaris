// SPDX-License-Identifier: GPL-3.0-only

//! A DEV or CANARY tag on the Dock icon. The packager writes the build's channel into Info.plist under
//! `ElarisChannel`. App icons are clipped to their rounded square, so the Dock tile draws the icon itself with
//! the tag on top, where it can hang over the icon's corner. Stable builds and unbundled runs show no tag.

use objc2_05::msg_send;
use objc2_05::runtime::{AnyClass, AnyObject};
use objc2_foundation_02::{NSBundle, NSPoint, NSRect, NSSize, NSString};

const CHANNEL_KEY: &str = "ElarisChannel";

/// Brand colours as (r, g, b): the logo's yellow for the pill and the icon background for the text.
const TAG_FILL: (f64, f64, f64) = (1.0, 0.788, 0.122);
/// How strongly the pill's inner edge catches light.
const TAG_EDGE_ALPHA: f64 = 0.35;
const TAG_INK: (f64, f64, f64) = (0.161, 0.169, 0.2);

/// Tag geometry as fractions of the Dock tile. The icon's rounded square ends about 10% in from the tile's
/// edge, so a tag ending at 98% sits over the corner.
const TAG_RIGHT: f64 = 0.98;
const TAG_BOTTOM: f64 = 0.04;
const TAG_HEIGHT: f64 = 0.26;
const TAG_TEXT: f64 = 0.15;
const TAG_PADDING: f64 = 0.07;
/// `NSFontWeightHeavy`.
const TAG_WEIGHT: f64 = 0.56;

/// The tag for this build's channel, or `None` for stable builds and runs outside a bundle.
fn channel_tag() -> Option<&'static str> {
    let key = NSString::from_str(CHANNEL_KEY);
    let value = unsafe { NSBundle::mainBundle().objectForInfoDictionaryKey(&key) }?;
    [("dev", "DEV"), ("canary", "CANARY")]
        .into_iter()
        .find(|(channel, _)| {
            let channel = NSString::from_str(channel);
            // SAFETY: `isEqual:` takes any object and returns a BOOL.
            unsafe { msg_send![&*value, isEqual: &*channel] }
        })
        .map(|(_, tag)| tag)
}

/// Draws the app icon with the channel's tag into the Dock tile. Call once the app has finished launching.
pub(crate) fn install() {
    let Some(label) = channel_tag() else {
        return;
    };
    let (Some(app_class), Some(image_view_class)) =
        (AnyClass::get("NSApplication"), AnyClass::get("NSImageView"))
    else {
        return;
    };
    // SAFETY: every message below is a documented AppKit method called on the main thread with the
    // argument types it declares; nil results are checked before use.
    unsafe {
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        let Some(app) = app.as_ref() else { return };
        let tile: *mut AnyObject = msg_send![app, dockTile];
        let icon: *mut AnyObject = msg_send![app, applicationIconImage];
        let (Some(tile), Some(icon)) = (tile.as_ref(), icon.as_ref()) else {
            return;
        };
        let size: NSSize = msg_send![tile, size];
        let Some(image) = tagged_icon(icon, size, label) else {
            return;
        };
        let view: *mut AnyObject = msg_send![image_view_class, imageViewWithImage: image];
        let Some(view) = view.as_ref() else { return };
        let _: () = msg_send![view, setFrame: NSRect::new(NSPoint::new(0.0, 0.0), size)];
        let _: () = msg_send![tile, setContentView: view];
        let _: () = msg_send![tile, display];
    }
}

/// The icon at the tile's size with a pill carrying `label` in its bottom-right corner.
///
/// # Safety
/// Call on the main thread with `icon` an `NSImage`.
unsafe fn tagged_icon(icon: &AnyObject, size: NSSize, label: &str) -> Option<*mut AnyObject> {
    // SAFETY: documented AppKit drawing methods, called with their declared argument types.
    unsafe {
        let image_class = AnyClass::get("NSImage")?;
        let path_class = AnyClass::get("NSBezierPath")?;
        let shadow_class = AnyClass::get("NSShadow")?;
        let font_class = AnyClass::get("NSFont")?;
        let dictionary_class = AnyClass::get("NSMutableDictionary")?;
        let context_class = AnyClass::get("NSGraphicsContext")?;

        let (w, h) = (size.width, size.height);
        let image: *mut AnyObject = msg_send![image_class, alloc];
        let image: *mut AnyObject = msg_send![image, initWithSize: size];
        let image = image.as_ref()?;
        let label = NSString::from_str(label);

        // Text attributes, measured first so the pill fits its label.
        let font: *mut AnyObject =
            msg_send![font_class, systemFontOfSize: h * TAG_TEXT, weight: TAG_WEIGHT];
        let attributes: *mut AnyObject = msg_send![dictionary_class, dictionary];
        let attributes = attributes.as_ref()?;
        let _: () = msg_send![attributes, setObject: font, forKey: &*NSString::from_str("NSFont")];
        let _: () = msg_send![attributes, setObject: color(TAG_INK), forKey: &*NSString::from_str("NSColor")];
        let text: NSSize = msg_send![&*label, sizeWithAttributes: attributes];

        let pill_h = h * TAG_HEIGHT;
        let pill_w = (text.width + 2.0 * w * TAG_PADDING).max(pill_h);
        let pill = NSRect::new(
            NSPoint::new(w * TAG_RIGHT - pill_w, h * TAG_BOTTOM),
            NSSize::new(pill_w, pill_h),
        );
        let radius = pill_h / 2.0;

        let _: () = msg_send![image, lockFocus];
        let _: () = msg_send![
            icon,
            drawInRect: NSRect::new(NSPoint::new(0.0, 0.0), size),
            fromRect: NSRect::ZERO,
            operation: 2usize, // NSCompositingOperationSourceOver
            fraction: 1.0f64
        ];

        let shape: *mut AnyObject = msg_send![path_class, bezierPathWithRoundedRect: pill, xRadius: radius, yRadius: radius];
        let shape = shape.as_ref()?;

        // A soft shadow lifts the pill off the icon.
        let _: () = msg_send![context_class, saveGraphicsState];
        let shadow: *mut AnyObject = msg_send![shadow_class, new];
        let _: () = msg_send![shadow, setShadowColor: color_alpha((0.0, 0.0, 0.0), 0.45)];
        let _: () = msg_send![shadow, setShadowBlurRadius: h * 0.03];
        let _: () = msg_send![shadow, setShadowOffset: NSSize::new(0.0, -h * 0.01)];
        let _: () = msg_send![shadow, set];
        let _: () = msg_send![color(TAG_FILL), setFill];
        let _: () = msg_send![shape, fill];
        let _: () = msg_send![context_class, restoreGraphicsState];

        // A faint inner edge, the way macOS's glass badges catch light, without a glossy sheen.
        let inset = (h * 0.006).max(0.5);
        let edge_rect = NSRect::new(
            NSPoint::new(pill.origin.x + inset, pill.origin.y + inset),
            NSSize::new(pill_w - 2.0 * inset, pill_h - 2.0 * inset),
        );
        let edge_radius = radius - inset;
        let edge: *mut AnyObject = msg_send![path_class, bezierPathWithRoundedRect: edge_rect, xRadius: edge_radius, yRadius: edge_radius];
        let _: () = msg_send![color_alpha((1.0, 1.0, 1.0), TAG_EDGE_ALPHA), setStroke];
        let _: () = msg_send![edge, setLineWidth: 2.0 * inset];
        let _: () = msg_send![edge, stroke];

        let origin = NSPoint::new(
            pill.origin.x + (pill_w - text.width) / 2.0,
            pill.origin.y + (pill_h - text.height) / 2.0,
        );
        let _: () = msg_send![&*label, drawAtPoint: origin, withAttributes: attributes];
        let _: () = msg_send![image, unlockFocus];
        Some(image as *const AnyObject as *mut AnyObject)
    }
}

fn color((r, g, b): (f64, f64, f64)) -> *mut AnyObject {
    color_alpha((r, g, b), 1.0)
}

fn color_alpha((r, g, b): (f64, f64, f64), alpha: f64) -> *mut AnyObject {
    let Some(class) = AnyClass::get("NSColor") else {
        return std::ptr::null_mut();
    };
    // SAFETY: `colorWithSRGBRed:green:blue:alpha:` takes four CGFloats and returns an autoreleased colour.
    unsafe { msg_send![class, colorWithSRGBRed: r, green: g, blue: b, alpha: alpha] }
}
