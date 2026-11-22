// SPDX-License-Identifier: GPL-3.0-only

//! Native macOS window restoration owned by AppKit.

#![cfg_attr(elaris_ui = "linux", allow(dead_code))]

use objc2_05::rc::Retained;
use objc2_app_kit_02::{NSView, NSWindowFrameAutosaveName};
use objc2_foundation_02::NSString;
use rune::WindowHandle as RuneWindowHandle;
use rune::raw_window_handle::RawWindowHandle;

const FRAME_NAME: &str = "ElarisMainWindow";

pub(crate) fn with_window<T>(
    window: &RuneWindowHandle,
    operation: impl FnOnce(&objc2_app_kit_02::NSWindow) -> T,
) -> Option<T> {
    window
        .with_raw_handle(|handle| {
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return None;
            };
            // SAFETY: Rune retains the AppKit view for this main-thread callback.
            let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
            let window = view.window()?;
            Some(operation(&window))
        })
        .ok()
        .flatten()
}

/// Restores and autosaves the native window frame using AppKit's display-aware mechanism.
pub(crate) fn install(window: &RuneWindowHandle) {
    let _ = window.with_raw_handle(|handle| {
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: Rune owns this live AppKit view and calls the window hook on the main thread.
        let Some(content) = (unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) })
        else {
            return;
        };
        let Some(native_window) = content.window() else {
            return;
        };
        let name: &NSWindowFrameAutosaveName = &NSString::from_str(FRAME_NAME);
        // SAFETY: these AppKit frame operations run on the main thread for a live Rune window.
        unsafe {
            native_window.setFrameUsingName(name);
            native_window.setFrameAutosaveName(name);
        }
    });
}

pub(crate) fn application_active() -> bool {
    use objc2_app_kit_02::NSApplication;
    use objc2_foundation_02::MainThreadMarker;

    // SAFETY: the marker keeps this AppKit query on the main thread.
    MainThreadMarker::new()
        .is_none_or(|main| unsafe { NSApplication::sharedApplication(main).isActive() })
}
