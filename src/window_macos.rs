// SPDX-License-Identifier: GPL-3.0-only

//! Native macOS window restoration owned by AppKit.

use objc2_05::rc::Retained;
use objc2_app_kit_02::{NSView, NSWindowFrameAutosaveName};
use objc2_foundation_02::NSString;
use rune::WindowHandle as RuneWindowHandle;
use rune::raw_window_handle::RawWindowHandle;

const FRAME_NAME: &str = "ElarisMainWindow";

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
