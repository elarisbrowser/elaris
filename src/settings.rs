// SPDX-License-Identifier: GPL-3.0-only

//! The browser's single typed set of user preferences.

use std::path::PathBuf;

use crate::address::SearchEngine;
use crate::appearance::Appearance;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) appearance: Appearance,
    pub(crate) search_engine: SearchEngine,
    pub(crate) download_directory: DownloadDirectory,
    pub(crate) startup: StartupMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            search_engine: SearchEngine::DuckDuckGo,
            download_directory: DownloadDirectory::Default,
            startup: StartupMode::RestoreSession,
        }
    }
}

impl Settings {
    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsChange {
    Appearance(Appearance),
    SearchEngine(SearchEngine),
    DownloadDirectory(DownloadDirectory),
    Startup(StartupMode),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible_and_download_paths_are_validated() {
        let settings = Settings::default();
        assert_eq!(settings.appearance, Appearance::System);
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
