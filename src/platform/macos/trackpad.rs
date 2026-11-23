// SPDX-License-Identifier: GPL-3.0-only

//! AppKit's natural-scrolling preference.

use objc2_05::msg_send;
use objc2_05::runtime::{AnyClass, AnyObject};
use objc2_foundation_02::NSString;

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
