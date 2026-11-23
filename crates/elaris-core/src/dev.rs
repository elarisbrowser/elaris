// SPDX-License-Identifier: GPL-3.0-only

//! Development and test switches. They are read from the environment only in debug builds, so a release build ignores
//! them and a variable set in a user's environment cannot change how a shipped browser behaves.

use std::ffi::OsString;

/// The value of a development or test switch, or `None` in a release build.
pub fn var(name: &str) -> Option<OsString> {
    if cfg!(debug_assertions) {
        std::env::var_os(name)
    } else {
        None
    }
}
