// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use crate::address::SearchEngine;
use crate::appearance::{AccentColor, Appearance};
use crate::config::ConfigLoad;
use crate::internal::InternalPage;
use crate::permissions::{PermissionDecision, PermissionKind, PermissionState, PermissionStore};
use crate::settings::{DownloadDirectory, SettingsChange, StartupMode, TabLayout};
#[cfg(target_os = "macos")]
use crate::site_controls::SiteControlPresentation;
use crate::tab::TabId;

impl super::AppState {
    pub(crate) fn horizontal_tabs(&self) -> bool {
        self.settings.tab_layout == TabLayout::Horizontal
    }

    pub(crate) fn set_tab_layout(&mut self, layout: TabLayout) {
        if self.settings.tab_layout != layout {
            self.prepare_tab_layout(layout);
            self.settings.tab_layout = layout;
            self.save_setting(SettingsChange::TabLayout(layout));
        }
    }

    pub(super) fn prepare_tab_layout(&mut self, layout: TabLayout) {
        if self.settings.tab_layout != layout {
            self.cancel_tab_drag();
            self.tab_drag = Default::default();
            self.dismiss_context_menu();
            self.page_menu = Default::default();
            crate::ui::dismiss_tab_preview(self);
            self.site_controls_anchor = None;
            #[cfg(target_os = "macos")]
            if self.site_permissions_open {
                crate::site_popover_macos::close();
            }
            self.site_permissions_open = false;
        }
    }

    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        if self.settings.appearance == appearance {
            return;
        }
        self.settings.set_appearance(appearance);
        self.save_setting(SettingsChange::Appearance(appearance));
    }

    pub(crate) fn set_accent_color(&mut self, accent_color: AccentColor) {
        if self.settings.accent_color == accent_color {
            return;
        }
        self.settings.set_accent_color(accent_color);
        self.save_setting(SettingsChange::AccentColor(accent_color));
    }

    pub(crate) fn settings_tab(&mut self) -> TabId {
        if let Some(tab) = self.browser.internal_tab(InternalPage::Settings) {
            return tab;
        }
        self.browser
            .open_tab(InternalPage::Settings.address())
            .expect("the built-in settings route is valid")
    }

    pub(crate) fn set_search_engine(&mut self, search_engine: SearchEngine) {
        if self.settings.search_engine == search_engine {
            return;
        }
        self.settings.set_search_engine(search_engine);
        self.save_setting(SettingsChange::SearchEngine(search_engine));
    }

    pub(crate) fn set_download_directory(&mut self, directory: DownloadDirectory) {
        if let Err(error) = self.settings.set_download_directory(directory.clone()) {
            self.configuration_issue = Some(error);
            return;
        }
        self.browser
            .set_download_directory(self.engine_download_directory());
        self.save_setting(SettingsChange::DownloadDirectory(directory));
    }

    pub(crate) fn download_directory(&self) -> PathBuf {
        self.engine_download_directory()
            .unwrap_or_else(|| self.default_download_directory.clone())
    }

    pub(super) fn engine_download_directory(&self) -> Option<PathBuf> {
        self.download_directory_override.clone().or_else(|| {
            match &self.settings.download_directory {
                DownloadDirectory::Default => None,
                DownloadDirectory::Custom(path) => Some(path.clone()),
            }
        })
    }

    pub(crate) fn set_startup(&mut self, startup: StartupMode) {
        if self.settings.startup == startup {
            return;
        }
        self.settings.set_startup(startup);
        self.save_setting(SettingsChange::Startup(startup));
    }

    pub(crate) fn set_glass_enabled(&mut self, enabled: bool) {
        if self.settings.glass_enabled == enabled {
            return;
        }
        self.settings.set_glass_enabled(enabled);
        self.save_setting(SettingsChange::GlassEnabled(enabled));
    }

    pub(crate) fn set_tint_opacity(&mut self, opacity: u8) {
        let opacity = opacity.min(100);
        if self.settings.tint_opacity == opacity {
            return;
        }
        self.settings.set_tint_opacity(opacity);
        self.save_setting(SettingsChange::TintOpacity(opacity));
    }

    fn save_setting(&mut self, change: SettingsChange) {
        super::publish_settings(&self.settings);
        if let Some(storage) = &self.storage
            && let Err(error) = storage.save_setting(change)
        {
            self.configuration_issue = Some(error);
        }
    }

    pub(crate) fn reload_configuration(&mut self) {
        let result = self
            .storage
            .as_ref()
            .ok_or_else(|| "Configuration storage is unavailable".to_owned())
            .and_then(|storage| storage.reload_configuration());
        if let Err(error) = result {
            self.configuration_issue = Some(error);
        }
    }

    pub(crate) fn open_config_file(&mut self) {
        let Some(path) = &self.config_file else {
            self.configuration_issue = Some("Configuration file path is unavailable".into());
            return;
        };
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(path).spawn();
        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open").arg(path).spawn();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let result: std::io::Result<std::process::Child> = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "opening config.toml is not supported on this platform",
        ));
        if let Err(error) = result {
            self.configuration_issue = Some(format!("Cannot open config.toml: {error}"));
        }
    }

    pub(super) fn apply_config_load(&mut self, load: ConfigLoad) {
        let previous_download_directory = self.engine_download_directory();
        self.prepare_tab_layout(load.settings.tab_layout);
        self.settings = load.settings;
        super::publish_settings(&self.settings);
        self.permission_store = load.permissions;
        super::publish_permissions(&self.permission_store);
        let download_directory = self.engine_download_directory();
        if previous_download_directory != download_directory {
            self.browser.set_download_directory(download_directory);
        }
        self.configuration_issue = (!load.issues.is_empty()).then(|| load.issues.join("\n"));
    }

    pub(crate) fn current_permission_origin(&self) -> Option<String> {
        self.browser
            .active_tab()
            .and_then(|tab| PermissionStore::origin(&tab.url))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn site_control_presentation(&self) -> SiteControlPresentation {
        SiteControlPresentation::from_state(self)
    }

    /// Returns the saved decision for a request; `Prompt` means the engine must ask the user.
    pub(crate) fn permission_decision(
        &self,
        origin: &str,
        kind: PermissionKind,
    ) -> PermissionDecision {
        self.permission_store.decision(origin, kind)
    }

    pub(crate) fn set_site_permission(
        &mut self,
        kind: PermissionKind,
        permission: PermissionState,
    ) {
        let Some(origin) = self.current_permission_origin() else {
            return;
        };
        self.permission_store.set(&origin, kind, permission);
        super::publish_permissions(&self.permission_store);
        if let Some(storage) = &self.storage
            && let Err(error) = storage.save_permission(origin, kind, permission)
        {
            self.configuration_issue = Some(error);
        }
    }

    pub(crate) fn reset_site_permissions(&mut self) {
        let Some(origin) = self.current_permission_origin() else {
            return;
        };
        self.permission_store.reset(&origin);
        super::publish_permissions(&self.permission_store);
        for kind in PermissionKind::ALL {
            if let Some(storage) = &self.storage
                && let Err(error) =
                    storage.save_permission(origin.clone(), kind, PermissionState::Ask)
            {
                self.configuration_issue = Some(error);
            }
        }
    }
}
