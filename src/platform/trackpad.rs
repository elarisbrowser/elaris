// SPDX-License-Identifier: GPL-3.0-only

//! Trackpad haptics and scroll direction. Only macOS has public APIs for them; elsewhere the
//! haptic does nothing and scrolling counts as natural.

#[cfg(target_os = "macos")]
use super::macos::trackpad as platform;

/// One firm tap, as macOS gives when a gesture crosses the point where it will take effect.
pub(crate) fn threshold_haptic() {
    #[cfg(target_os = "macos")]
    platform::perform(platform::ALIGNMENT);
}

/// Whether scrolled content follows the fingers (macOS's "Natural scrolling", on by default).
pub(crate) fn natural_scrolling() -> bool {
    #[cfg(target_os = "macos")]
    return platform::natural_scrolling();
    #[cfg(not(target_os = "macos"))]
    true
}
