//! The browser's name and marks, in one place so it can be renamed.
//!
//! Browser names and internal page addresses come from here. Keep profile
//! storage and native app identifiers stable when changing the visible name.

/// The browser's name, as shown to people.
pub const NAME: &str = "elaris.";

/// The name of the web engine, as shown in notices about it.
pub const ENGINE_NAME: &str = "elaris. Engine";

/// The scheme of the browser's own pages, as `elaris` in `elaris://settings`.
pub const PAGE_SCHEME: &str = "elaris";

/// The former internal page scheme, kept working for saved links and tabs.
pub const LEGACY_PAGE_SCHEME: &str = "photon";

/// The macOS bundle identifier of the app `elaris run` builds for
/// development, so macOS can open web links with it.
pub const DEVELOPMENT_BUNDLE_IDENTIFIER: &str = "org.photonbrowser.photon.development";

/// The freedesktop desktop-entry ID that Linux installs register Photon under.
pub const LINUX_DESKTOP_ID: &str = "org.photonbrowser.photon.desktop";

/// The current application data folder.
pub const DATA_FOLDER: &str = "Elaris";

/// The former application data folder, used to migrate existing profiles.
pub const LEGACY_DATA_FOLDER: &str = "Photon";

/// The macOS app icon's Icon Composer document, which holds its artwork
/// and how it looks in each appearance: default, dark, clear and tinted.
pub const APP_ICON_DOCUMENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/AppIcon.icon");

/// The macOS app icon as a 1024-pixel PNG on Apple's icon grid, the shape
/// 824 pixels across on a transparent canvas, for building without Xcode,
/// which compiles the document.
pub const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

/// The planet logo, as SVG in its own colors.
pub const LOGO_SVG: &[u8] = include_bytes!("../assets/logo.svg");
