// SPDX-License-Identifier: GPL-3.0-only

//! Whether Elaris opens web links by default, and the request that makes it so.

#[cfg(target_os = "linux")]
use crate::default_browser_linux as platform;
#[cfg(target_os = "macos")]
use crate::default_browser_macos as platform;

/// Whether Elaris is the default web browser, or `None` when the system cannot say.
pub(crate) fn is_default() -> Option<bool> {
    platform::is_default()
}

/// Asks the system to make Elaris the default web browser.
pub(crate) fn make_default() -> Result<(), String> {
    platform::make_default()
}
