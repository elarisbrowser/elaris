// SPDX-License-Identifier: GPL-3.0-only

//! Operating-system integration. Shared browser code uses the facades declared here; the `macos` and `linux` modules
//! hold each platform's implementation and are compiled only on their own target.

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;

pub(crate) mod default_browser;
pub(crate) mod trackpad;
