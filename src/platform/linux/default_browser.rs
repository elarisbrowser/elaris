// SPDX-License-Identifier: GPL-3.0-only

//! Default web browser through `xdg-settings`, which every desktop-file-based Linux session reads.

use std::process::Command;

/// The desktop entry that Elaris installs. It must match `bridge/src/external_url_handler_linux.cpp`.
const DESKTOP_ID: &str = "org.elarisbrowser.Elaris.desktop";

pub(crate) fn is_default() -> Option<bool> {
    let output = Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim() == DESKTOP_ID)
}

pub(crate) fn make_default() -> Result<(), String> {
    let output = Command::new("xdg-settings")
        .args(["set", "default-web-browser", DESKTOP_ID])
        .output()
        .map_err(|error| format!("xdg-settings could not run: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
