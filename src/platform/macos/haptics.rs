// SPDX-License-Identifier: GPL-3.0-only

//! AppKit's `NSHapticFeedbackManager`.

use objc2_05::msg_send;
use objc2_05::runtime::{AnyClass, AnyObject};

/// `NSHapticFeedbackPerformanceTimeNow`.
const NOW: usize = 1;

/// Plays an `NSHapticFeedbackPattern`.
pub(crate) fn perform(pattern: isize) {
    let Some(class) = AnyClass::get("NSHapticFeedbackManager") else {
        return;
    };
    // SAFETY: `defaultPerformer` returns a shared object conforming to
    // `NSHapticFeedbackPerformer`, which takes these two integer enums.
    unsafe {
        let performer: *mut AnyObject = msg_send![class, defaultPerformer];
        if let Some(performer) = performer.as_ref() {
            let _: () = msg_send![performer, performFeedbackPattern: pattern, performanceTime: NOW];
        }
    }
}
