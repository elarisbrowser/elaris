// SPDX-License-Identifier: GPL-3.0-only

//! Trackpad haptics. Only macOS has a public API for them; elsewhere these do nothing.

/// One firm tap, as macOS gives when a gesture crosses the point where it will take effect.
pub(crate) fn threshold() {
    #[cfg(target_os = "macos")]
    macos::perform(macos::ALIGNMENT);
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_05::msg_send;
    use objc2_05::runtime::{AnyClass, AnyObject};

    /// `NSHapticFeedbackPatternAlignment`.
    pub(super) const ALIGNMENT: isize = 1;
    /// `NSHapticFeedbackPerformanceTimeNow`.
    const NOW: usize = 1;

    pub(super) fn perform(pattern: isize) {
        let Some(class) = AnyClass::get("NSHapticFeedbackManager") else {
            return;
        };
        // SAFETY: `defaultPerformer` returns a shared object conforming to
        // `NSHapticFeedbackPerformer`, which takes these two integer enums.
        unsafe {
            let performer: *mut AnyObject = msg_send![class, defaultPerformer];
            if let Some(performer) = performer.as_ref() {
                let _: () =
                    msg_send![performer, performFeedbackPattern: pattern, performanceTime: NOW];
            }
        }
    }
}
