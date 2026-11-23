// SPDX-License-Identifier: GPL-3.0-only

//! The browser's single typed set of user preferences.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::address::SearchEngine;
use crate::appearance::{AccentColor, Appearance, GlassStyle};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StartupMode {
    #[default]
    RestoreSession,
    BlankWindow,
}

impl StartupMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::RestoreSession => "Restore previous session",
            Self::BlankWindow => "Open a blank window",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TabLayout {
    #[default]
    Vertical,
    Horizontal,
}

impl TabLayout {
    pub(crate) const ALL: [Self; 2] = [Self::Vertical, Self::Horizontal];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// Which edge of the window the vertical sidebar sits against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SidebarSide {
    #[default]
    Left,
    Right,
}

impl SidebarSide {
    pub(crate) const ALL: [Self; 2] = [Self::Left, Self::Right];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub(crate) fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Where the New Tab button sits in the vertical sidebar's tab list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NewTabPosition {
    #[default]
    Top,
    Bottom,
}

impl NewTabPosition {
    pub(crate) const ALL: [Self; 2] = [Self::Top, Self::Bottom];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// How the web page sits in the window: inset as a rounded card, or filling its area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PageStyle {
    #[default]
    Card,
    EdgeToEdge,
}

impl PageStyle {
    pub(crate) const ALL: [Self; 2] = [Self::Card, Self::EdgeToEdge];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Card => "As a card",
            Self::EdgeToEdge => "Edge to edge",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::EdgeToEdge => "edge_to_edge",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DownloadDirectory {
    #[default]
    Default,
    Custom(PathBuf),
}

impl DownloadDirectory {
    pub(crate) fn custom(path: PathBuf) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Download directory must be an absolute path".to_owned());
        }
        if path.to_str().is_none() {
            return Err(
                "Download directory must be representable as UTF-8 in config.toml".to_owned(),
            );
        }
        Ok(Self::Custom(path))
    }
}

/// A built-in filter list that the content blocker can use. The identifiers are the engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ContentBlockList {
    EasyList,
    EasyPrivacy,
    CookieNotices,
    Annoyances,
}

impl ContentBlockList {
    pub(crate) const ALL: [Self; 4] = [
        Self::EasyList,
        Self::EasyPrivacy,
        Self::CookieNotices,
        Self::Annoyances,
    ];

    pub(crate) fn identifier(self) -> &'static str {
        match self {
            Self::EasyList => "easyList",
            Self::EasyPrivacy => "easyPrivacy",
            Self::CookieNotices => "easyListCookie",
            Self::Annoyances => "fanboyAnnoyances",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::EasyList => "easylist",
            Self::EasyPrivacy => "easyprivacy",
            Self::CookieNotices => "cookie_notices",
            Self::Annoyances => "annoyances",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::EasyList => "EasyList",
            Self::EasyPrivacy => "EasyPrivacy",
            Self::CookieNotices => "Cookie notices",
            Self::Annoyances => "Annoyances",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::EasyList => "Blocks advertising.",
            Self::EasyPrivacy => "Blocks tracking scripts, pixels and other tracking requests.",
            Self::CookieNotices => "Hides cookie banners and other cookie notices.",
            Self::Annoyances => "Hides pop-ups, social widgets, newsletters and other annoyances.",
        }
    }
}

/// Whether the content blocker is on, and which built-in filter lists it uses while it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContentBlocking {
    pub(crate) enabled: bool,
    pub(crate) lists: BTreeSet<ContentBlockList>,
}

impl Default for ContentBlocking {
    fn default() -> Self {
        Self {
            enabled: false,
            lists: BTreeSet::from([ContentBlockList::EasyList, ContentBlockList::EasyPrivacy]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) page_zoom: u16,
    pub(crate) autoplay: crate::permissions::AutoplayPolicy,
    pub(crate) tab_layout: TabLayout,
    pub(crate) sidebar_side: SidebarSide,
    pub(crate) new_tab_position: NewTabPosition,
    pub(crate) page_style: PageStyle,
    pub(crate) split_view_limit: usize,
    pub(crate) appearance: Appearance,
    pub(crate) accent_color: AccentColor,
    pub(crate) search_engine: SearchEngine,
    pub(crate) download_directory: DownloadDirectory,
    pub(crate) startup: StartupMode,
    pub(crate) glass_enabled: bool,
    pub(crate) glass_style: GlassStyle,
    /// How much of the window's colour tints the glass, 0 through 100. The settings page shows
    /// the inverse as transparency.
    pub(crate) tint_opacity: u8,
    /// How strong the still noise over the glass is, 0 through 100.
    pub(crate) grain: u8,
    pub(crate) reduce_motion: bool,
    pub(crate) content_blocking: ContentBlocking,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            page_zoom: 100,
            autoplay: crate::permissions::AutoplayPolicy::default(),
            tab_layout: TabLayout::default(),
            sidebar_side: SidebarSide::default(),
            new_tab_position: NewTabPosition::default(),
            page_style: PageStyle::default(),
            split_view_limit: 2,
            appearance: Appearance::System,
            accent_color: AccentColor::Blue,
            search_engine: SearchEngine::DuckDuckGo,
            download_directory: DownloadDirectory::Default,
            startup: StartupMode::RestoreSession,
            glass_enabled: true,
            glass_style: GlassStyle::default(),
            tint_opacity: 100,
            grain: 0,
            reduce_motion: false,
            content_blocking: ContentBlocking::default(),
        }
    }
}

impl Settings {
    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
    }

    pub(crate) fn set_accent_color(&mut self, accent_color: AccentColor) {
        self.accent_color = accent_color;
    }

    pub(crate) fn set_search_engine(&mut self, search_engine: SearchEngine) {
        self.search_engine = search_engine;
    }

    pub(crate) fn set_download_directory(
        &mut self,
        download_directory: DownloadDirectory,
    ) -> Result<(), String> {
        if let DownloadDirectory::Custom(path) = &download_directory {
            DownloadDirectory::custom(path.clone())?;
        }
        self.download_directory = download_directory;
        Ok(())
    }

    pub(crate) fn set_startup(&mut self, startup: StartupMode) {
        self.startup = startup;
    }

    pub(crate) fn set_glass_enabled(&mut self, enabled: bool) {
        self.glass_enabled = enabled;
    }

    pub(crate) fn set_glass_style(&mut self, style: GlassStyle) {
        self.glass_style = style;
    }

    pub(crate) fn set_tint_opacity(&mut self, opacity: u8) {
        self.tint_opacity = opacity.min(100);
    }

    pub(crate) fn set_grain(&mut self, grain: u8) {
        self.grain = grain.min(100);
    }

    pub(crate) fn set_reduce_motion(&mut self, enabled: bool) {
        self.reduce_motion = enabled;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsChange {
    PageZoom(u16),
    Autoplay(crate::permissions::AutoplayPolicy),
    TabLayout(TabLayout),
    SidebarSide(SidebarSide),
    NewTabPosition(NewTabPosition),
    PageStyle(PageStyle),
    Appearance(Appearance),
    AccentColor(AccentColor),
    SearchEngine(SearchEngine),
    DownloadDirectory(DownloadDirectory),
    Startup(StartupMode),
    GlassEnabled(bool),
    GlassStyle(GlassStyle),
    TintOpacity(u8),
    Grain(u8),
    ReduceMotion(bool),
    SplitViewLimit(usize),
    ContentBlocking(bool),
    ContentBlockingList(ContentBlockList, bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible_and_download_paths_are_validated() {
        let settings = Settings::default();
        assert_eq!(settings.appearance, Appearance::System);
        assert_eq!(settings.accent_color, AccentColor::Blue);
        assert_eq!(settings.search_engine, SearchEngine::DuckDuckGo);
        assert_eq!(settings.download_directory, DownloadDirectory::Default);
        assert_eq!(settings.startup, StartupMode::RestoreSession);

        assert!(DownloadDirectory::custom(PathBuf::from("relative")).is_err());
        assert_eq!(
            DownloadDirectory::custom(PathBuf::from("/tmp/downloads")),
            Ok(DownloadDirectory::Custom(PathBuf::from("/tmp/downloads")))
        );
    }
}
