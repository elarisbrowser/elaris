// SPDX-License-Identifier: GPL-3.0-only

//! Fade the native macOS traffic lights in only while the pointer is over them.

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
const RESTING_ALPHA: f64 = 0.65;

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
        for button in buttons {
            let Some(button_class) = hover_button_class(button.class()) else {
                continue;
            };
            // SAFETY: the button is a live AppKit view, and the subclass adds no ivars.
            unsafe { AnyObject::set_class(&button, button_class) };
            // Disabled native titlebar buttons render as grey markers while retaining their
            // original size and position. The tracking area below enables them on hover.
            button.setEnabled(false);
            // SAFETY: alphaValue is animatable; keep the resting markers visible.
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
    // SAFETY: keep the native button grey and visible when the pointer leaves the cluster.
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
