// SPDX-License-Identifier: GPL-3.0-only

//! AppKit's haptic feedback and natural-scrolling preference.

use objc2_05::msg_send;
use objc2_05::runtime::{AnyClass, AnyObject};
use objc2_foundation_02::NSString;

/// `NSHapticFeedbackPatternAlignment`.
pub(crate) const ALIGNMENT: isize = 1;
/// `NSHapticFeedbackPerformanceTimeNow`.
const NOW: usize = 1;

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

pub(crate) fn natural_scrolling() -> bool {
    let Some(class) = AnyClass::get("NSUserDefaults") else {
        return true;
    };
    let key = NSString::from_str("com.apple.swipescrolldirection");
    // SAFETY: `standardUserDefaults` is a shared object that includes the global domain, and
    // `objectForKey:` and `boolForKey:` take an NSString key.
    unsafe {
        let defaults: *mut AnyObject = msg_send![class, standardUserDefaults];
        let Some(defaults) = defaults.as_ref() else {
            return true;
        };
        // Unset means the default, which is natural.
        let value: *mut AnyObject = msg_send![defaults, objectForKey: &*key];
        value.is_null() || msg_send![defaults, boolForKey: &*key]
    }
}
