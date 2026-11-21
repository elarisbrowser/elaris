// SPDX-License-Identifier: GPL-3.0-only

//! Fade the native macOS traffic lights in only while the pointer is over them. At rest they are
//! transparent, over the plain circles the sidebar draws in their place.

use objc2_05::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2_05::{ClassType, msg_send, sel};
use objc2_app_kit_02::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSButton, NSEvent, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindowButton,
};
use objc2_foundation_02::{NSPoint, NSRect, NSSize};
use rune::WindowHandle as RuneWindowHandle;
use rune::raw_window_handle::RawWindowHandle;

const FADE_DURATION: f64 = 0.18;
/// Fully transparent: the sidebar's circles show through.
const RESTING_ALPHA: f64 = 0.0;

/// Installs hover tracking on the native window controls when Rune creates the window.
pub(crate) fn install(window: &RuneWindowHandle) {
    let _ = window.with_raw_handle(|handle| {
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: the handle belongs to the live window and this callback runs on AppKit's thread.
        let Some(content) =
            (unsafe { objc2_05::rc::Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) })
        else {
            return;
        };
        let Some(native_window) = content.window() else {
            return;
        };
        let buttons: Vec<_> = [
            NSWindowButton::NSWindowCloseButton,
            NSWindowButton::NSWindowMiniaturizeButton,
            NSWindowButton::NSWindowZoomButton,
        ]
        .into_iter()
        .filter_map(|kind| native_window.standardWindowButton(kind))
        .collect();
        let Some(first) = buttons.first() else {
            return;
        };
        let Some(parent) = (unsafe { first.superview() }) else {
            return;
        };
        let mut group = first.frame();
        for button in buttons.iter().skip(1) {
            let frame = button.frame();
            let right = group.origin.x + group.size.width;
            let bottom = group.origin.y + group.size.height;
            let frame_right = frame.origin.x + frame.size.width;
            let frame_bottom = frame.origin.y + frame.size.height;
            let left = group.origin.x.min(frame.origin.x);
            let top = group.origin.y.min(frame.origin.y);
            group = NSRect::new(
                NSPoint::new(left, top),
                NSSize::new(
                    right.max(frame_right) - left,
                    bottom.max(frame_bottom) - top,
                ),
            );
        }
        // Rune moves the buttons within this parent when the tab layout changes. The
        // tracking area covers both positions, whichever layout the window opened in.
        let travel = f64::from(
            (crate::ui::theme::HEADER_HEIGHT - crate::ui::theme::HORIZONTAL_STRIP_HEIGHT).abs()
                / 2.0,
        );
        group.origin.y -= travel;
        group.size.height += 2.0 * travel;
        for button in buttons {
            let Some(button_class) = hover_button_class(button.class()) else {
                continue;
            };
            // SAFETY: the button is a live AppKit view, and the subclass adds no ivars.
            unsafe { AnyObject::set_class(&button, button_class) };
            // Disabled at rest so a click on the hidden buttons does nothing; the tracking
            // area below enables them, and fades them in, on hover.
            button.setEnabled(false);
            // SAFETY: alphaValue is animatable; the buttons rest hidden over the sidebar's circles.
            unsafe { button.setAlphaValue(RESTING_ALPHA) };
            let owner = unsafe { &*(std::ptr::from_ref(&*button).cast::<AnyObject>()) };
            // SAFETY: AppKit creates and owns the tracking area on its main thread.
            let options = NSTrackingAreaOptions::NSTrackingMouseEnteredAndExited
                | NSTrackingAreaOptions::NSTrackingActiveAlways;
            let tracking = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    group,
                    options,
                    Some(owner),
                    None,
                )
            };
            // SAFETY: the tracking area is retained by the view after installation.
            unsafe { parent.addTrackingArea(&tracking) };
        }
    });
}

/// Reads the hardware display and refresh rate for Rune's native window.
pub(crate) fn display_metadata(
    window: &RuneWindowHandle,
) -> Option<elaris::ladybird::DisplayMetadata> {
    window
        .with_raw_handle(|handle| {
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return None;
            };
            // SAFETY: Rune keeps the native view alive for this callback on AppKit's thread.
            let content = unsafe {
                objc2_05::rc::Retained::retain(handle.ns_view.as_ptr().cast::<NSView>())
            }?;
            let screen = content.window()?.screen()?;
            // SAFETY: `maximumFramesPerSecond` is queried on AppKit's main thread.
            let hz = unsafe { screen.maximumFramesPerSecond() };
            let key = objc2_foundation_02::NSString::from_str("NSScreenNumber");
            // SAFETY: AppKit provides a dictionary containing the screen number as an NSNumber.
            // A missing entry produces zero when messaged through Objective-C.
            let id: u32 = unsafe {
                let description: *mut AnyObject = msg_send![&*screen, deviceDescription];
                let number: *mut AnyObject = msg_send![description, objectForKey: &*key];
                msg_send![number, unsignedIntValue]
            };
            Some(elaris::ladybird::DisplayMetadata {
                id: (id != 0).then_some(id),
                refresh_rate: if hz > 0 { hz as f64 } else { 60.0 },
            })
        })
        .ok()
        .flatten()
}

fn hover_button_class(base: &'static AnyClass) -> Option<&'static AnyClass> {
    const PREFIX: &str = "ElarisHoverTrafficLight_";
    if base.name().starts_with(PREFIX) {
        return Some(base);
    }
    let name = format!("{PREFIX}{}", base.name());
    if let Some(class) = AnyClass::get(&name) {
        return Some(class);
    }

    let mut builder = ClassBuilder::new(&name, base)?;
    // SAFETY: these selectors match NSResponder's mouse tracking callbacks.
    unsafe {
        builder.add_method(
            sel!(mouseEntered:),
            mouse_entered as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(mouseExited:),
            mouse_exited as unsafe extern "C" fn(_, _, _),
        );
    }
    Some(builder.register())
}

unsafe extern "C" fn mouse_entered(button: &AnyObject, _: Sel, event: &NSEvent) {
    let Some(base) = button.class().superclass() else {
        return;
    };
    // SAFETY: each tracking area is owned by one of the window's native NSButtons.
    unsafe { (&*(std::ptr::from_ref(button).cast::<NSButton>())).setEnabled(true) };
    // SAFETY: forward native hover handling after enabling the button.
    unsafe {
        let _: () = msg_send![super(button, base), mouseEntered: event];
    }
    animate_opacity(button, 1.0);
}

unsafe extern "C" fn mouse_exited(button: &AnyObject, _: Sel, event: &NSEvent) {
    let Some(base) = button.class().superclass() else {
        return;
    };
    // SAFETY: forward native hover handling before fading the button back out.
    unsafe {
        let _: () = msg_send![super(button, base), mouseExited: event];
    }
    // SAFETY: disable the native button again as it fades out when the pointer leaves.
    unsafe { (&*(std::ptr::from_ref(button).cast::<NSButton>())).setEnabled(false) };
    animate_opacity(button, RESTING_ALPHA);
}

fn animate_opacity(button: &AnyObject, alpha: f64) {
    let view = unsafe { &*(std::ptr::from_ref(button).cast::<NSView>()) };
    // SAFETY: the animation context and native button are used on AppKit's main thread.
    unsafe {
        NSAnimationContext::beginGrouping();
        NSAnimationContext::currentContext().setDuration(FADE_DURATION);
        NSAnimatablePropertyContainer::animator(view).setAlphaValue(alpha);
        NSAnimationContext::endGrouping();
    }
}
