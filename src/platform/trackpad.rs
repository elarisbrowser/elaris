// SPDX-License-Identifier: GPL-3.0-only

//! Trackpad scroll direction. Only macOS has a public API for it; elsewhere scrolling counts as
//! natural.

#[cfg(target_os = "macos")]
use super::macos::trackpad as platform;

/// Whether scrolled content follows the fingers (macOS's "Natural scrolling", on by default).
pub(crate) fn natural_scrolling() -> bool {
    #[cfg(target_os = "macos")]
    return platform::natural_scrolling();
    #[cfg(not(target_os = "macos"))]
    true
}
