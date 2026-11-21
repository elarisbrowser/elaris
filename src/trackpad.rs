// SPDX-License-Identifier: GPL-3.0-only

//! Trackpad haptics and scroll direction. Only macOS has public APIs for them; elsewhere the
//! haptic does nothing and scrolling counts as natural.

/// One firm tap, as macOS gives when a gesture crosses the point where it will take effect.
pub(crate) fn threshold_haptic() {
    #[cfg(target_os = "macos")]
    macos::perform(macos::ALIGNMENT);
}

/// Whether scrolled content follows the fingers (macOS's "Natural scrolling", on by default).
pub(crate) fn natural_scrolling() -> bool {
    #[cfg(target_os = "macos")]
    return macos::natural_scrolling();
    #[cfg(not(target_os = "macos"))]
    true
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2_05::msg_send;
    use objc2_05::runtime::{AnyClass, AnyObject};
    use objc2_foundation_02::NSString;

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

    pub(super) fn natural_scrolling() -> bool {
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
}
