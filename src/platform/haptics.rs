// SPDX-License-Identifier: GPL-3.0-only

//! Trackpad haptic feedback: `haptics::play(Haptic::Tick)`. Only macOS has a public API for it,
//! and it plays only on a Force Touch trackpad while the user is touching it; elsewhere `play`
//! does nothing. macOS has no intensity control, so each kind names what the feedback means.

/// What a haptic signals. The values are AppKit's `NSHapticFeedbackPattern`s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "a kind may have no caller yet on this platform")]
pub(crate) enum Haptic {
    /// A general tap, such as confirming an action.
    Tap = 0,
    /// A firm snap as something lines up or crosses a threshold or limit.
    Snap = 1,
    /// A light tick as a value steps between discrete positions.
    Tick = 2,
}

/// Plays `haptic` now.
pub(crate) fn play(haptic: Haptic) {
    #[cfg(target_os = "macos")]
    super::macos::haptics::perform(haptic as isize);
    #[cfg(not(target_os = "macos"))]
    let _ = haptic;
}
