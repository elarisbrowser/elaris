// SPDX-License-Identifier: GPL-3.0-only

//! Default web browser through NSWorkspace. LaunchServices accepts the request only from a bundled app
//! whose Info.plist declares the web URL schemes, so `cargo run` builds report that they cannot be set.

use block2::RcBlock;
use objc2_app_kit_02::NSWorkspace;
use objc2_foundation_02::{NSBundle, NSError, NSString, NSURL};

const SAMPLE_WEB_URL: &str = "https://example.com/";
const WEB_SCHEMES: [&str; 2] = ["http", "https"];

/// The app bundle that is running, with symlinks resolved so it compares equal to the handler's path.
fn own_bundle_path() -> Option<String> {
    let bundle_url = unsafe { NSBundle::mainBundle().bundleURL() };
    let resolved = unsafe { bundle_url.URLByResolvingSymlinksInPath() }?;
    unsafe { resolved.path() }.map(|path| path.to_string())
}

pub(crate) fn is_default() -> Option<bool> {
    let sample = NSString::from_str(SAMPLE_WEB_URL);
    let web_url = unsafe { NSURL::URLWithString(&sample) }?;
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    let Some(handler) = (unsafe { workspace.URLForApplicationToOpenURL(&web_url) }) else {
        return Some(false);
    };
    let handler = unsafe { handler.URLByResolvingSymlinksInPath() }?;
    let handler_path = unsafe { handler.path() }?.to_string();
    Some(Some(handler_path) == own_bundle_path())
}

pub(crate) fn make_default() -> Result<(), String> {
    let bundle_url = unsafe { NSBundle::mainBundle().bundleURL() };
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    for scheme in WEB_SCHEMES {
        // macOS asks the person to confirm, and the completion arrives after that answer. The Settings
        // page re-reads the state when it next opens, so a failure is reported on stderr.
        let completion = RcBlock::new(|error: *mut NSError| {
            if let Some(error) = unsafe { error.as_ref() } {
                let description = error.localizedDescription();
                eprintln!("Elaris could not become the default browser: {description}");
            }
        });
        let scheme = NSString::from_str(scheme);
        unsafe {
            workspace.setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
                &bundle_url,
                &scheme,
                Some(&*completion),
            );
        }
    }
    Ok(())
}
