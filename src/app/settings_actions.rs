// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use crate::address::SearchEngine;
use crate::appearance::{AccentColor, Appearance, GlassStyle};
use crate::config::ConfigLoad;
use crate::internal::InternalPage;
use crate::permissions::{
    AutoplayPolicy, PermissionDecision, PermissionKind, PermissionState, PermissionStore,
};
#[cfg(elaris_ui = "linux")]
use crate::settings::WindowDecorations;
use crate::settings::{
    ContentBlockList, DownloadDirectory, NewTabPosition, PageStyle, SettingsChange, SidebarSide,
    StartupMode, TabLayout,
};
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

    pub(crate) fn sidebar_on_right(&self) -> bool {
        !self.horizontal_tabs() && self.settings.sidebar_side == SidebarSide::Right
    }

    pub(crate) fn set_sidebar_side(&mut self, side: SidebarSide) {
        if self.settings.sidebar_side != side {
            self.reset_tab_chrome();
            self.settings.sidebar_side = side;
            self.save_setting(SettingsChange::SidebarSide(side));
        }
    }

    /// Saves the Linux window frame. The open windows keep theirs, so a change that would give a
    /// different frame asks for a restart.
    #[cfg(elaris_ui = "linux")]
    pub(crate) fn set_window_decorations(&mut self, mode: WindowDecorations) {
        if self.settings.window_decorations == mode {
            return;
        }
        self.settings.window_decorations = mode;
        self.save_setting(SettingsChange::WindowDecorations(mode));
        let running = super::LinuxDecorations::current();
        let next = super::LinuxDecorations::for_setting(
            super::LinuxDecorations::override_setting().unwrap_or(mode),
        );
        self.restart_notice = next != running;
    }

    pub(crate) fn set_new_tab_position(&mut self, position: NewTabPosition) {
        if self.settings.new_tab_position != position {
            self.settings.new_tab_position = position;
            self.save_setting(SettingsChange::NewTabPosition(position));
        }
    }

    pub(crate) fn set_page_style(&mut self, style: PageStyle) {
        if self.settings.page_style != style {
            self.settings.page_style = style;
            self.save_setting(SettingsChange::PageStyle(style));
        }
    }

    /// How far the page sits in from the window and sidebar edges; nothing when edge to edge.
    pub(crate) fn page_inset(&self) -> f32 {
        match self.settings.page_style {
            PageStyle::Card => crate::ui::theme::VIEWPORT_INSET,
            PageStyle::EdgeToEdge => 0.0,
        }
    }

    /// The page's corner radius; square when edge to edge.
    pub(crate) fn page_radius(&self) -> f32 {
        match self.settings.page_style {
            PageStyle::EdgeToEdge => 0.0,
            PageStyle::Card if self.horizontal_tabs() => {
                crate::ui::theme::HORIZONTAL_VIEWPORT_RADIUS
            }
            PageStyle::Card => crate::ui::theme::VIEWPORT_RADIUS,
        }
    }

    pub(super) fn prepare_tab_layout(&mut self, layout: TabLayout) {
        if self.settings.tab_layout != layout {
            self.reset_tab_chrome();
        }
    }

    /// Drops drags, menus and anchors that belong to where the tabs were drawn before a move.
    fn reset_tab_chrome(&mut self) {
        self.cancel_tab_drag();
        self.hover_highlight = None;
        self.tab_drag = Default::default();
        self.dismiss_context_menu();
        self.page_menu = Default::default();
        crate::ui::dismiss_tab_preview(self);
        self.site_controls_anchor = None;
        self.site_permissions_open = false;
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
        self.refresh_default_browser();
        if let Some(tab) = self.browser.internal_tab(InternalPage::Settings) {
            return tab;
        }
        self.browser
            .open_tab(InternalPage::Settings.address())
            .expect("the built-in settings route is valid")
    }

    pub(crate) fn refresh_default_browser(&mut self) {
        self.default_browser = crate::default_browser::is_default();
    }

    pub(crate) fn make_default_browser(&mut self) {
        self.default_browser_error = crate::default_browser::make_default().err();
        self.refresh_default_browser();
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

    pub(crate) fn set_glass_style(&mut self, style: GlassStyle) {
        if self.settings.glass_style == style {
            return;
        }
        self.settings.set_glass_style(style);
        self.save_setting(SettingsChange::GlassStyle(style));
    }

    pub(crate) fn set_tint_opacity(&mut self, opacity: u8) {
        let opacity = opacity.min(100);
        if self.settings.tint_opacity == opacity {
            return;
        }
        self.settings.set_tint_opacity(opacity);
        self.save_setting(SettingsChange::TintOpacity(opacity));
    }

    pub(crate) fn set_grain(&mut self, grain: u8) {
        let grain = grain.min(100);
        if self.settings.grain == grain {
            return;
        }
        self.settings.set_grain(grain);
        self.save_setting(SettingsChange::Grain(grain));
    }

    pub(crate) fn set_reduce_motion(&mut self, enabled: bool) {
        if self.settings.reduce_motion == enabled {
            return;
        }
        self.settings.set_reduce_motion(enabled);
        self.save_setting(SettingsChange::ReduceMotion(enabled));
    }

    pub(crate) fn set_page_zoom(&mut self, zoom: u16) {
        if self.settings.page_zoom != zoom {
            self.settings.page_zoom = zoom;
            self.browser.set_default_zoom(zoom);
            self.save_setting(SettingsChange::PageZoom(zoom));
        }
    }

    pub(crate) fn set_autoplay_default(&mut self, policy: AutoplayPolicy) {
        if self.settings.autoplay != policy {
            self.settings.autoplay = policy;
            self.sync_engine_autoplay();
            self.save_setting(SettingsChange::Autoplay(policy));
        }
    }

    /// Sets the current site's autoplay exception; `None` follows the browser default.
    pub(crate) fn set_site_autoplay(&mut self, policy: Option<AutoplayPolicy>) {
        let Some(origin) = self.current_permission_origin() else {
            return;
        };
        self.permission_store.set_autoplay(&origin, policy);
        super::publish_permissions(&self.permission_store);
        self.sync_engine_autoplay();
        if let Some(storage) = &self.storage
            && let Err(error) = storage.save_autoplay(origin, policy)
        {
            self.configuration_issue = Some(error);
        }
    }

    /// Turns content blocking on or off, and sends the engine its state.
    pub(crate) fn set_content_blocking(&mut self, enabled: bool) {
        if self.settings.content_blocking.enabled == enabled {
            return;
        }
        self.settings.content_blocking.enabled = enabled;
        self.sync_engine_content_blocking();
        self.save_setting(SettingsChange::ContentBlocking(enabled));
    }

    /// Enables or disables one built-in filter list, and sends the engine the change.
    pub(crate) fn set_content_blocking_list(&mut self, list: ContentBlockList, enabled: bool) {
        let lists = &mut self.settings.content_blocking.lists;
        if lists.contains(&list) == enabled {
            return;
        }
        if enabled {
            lists.insert(list);
        } else {
            lists.remove(&list);
        }
        self.sync_engine_content_blocking();
        self.save_setting(SettingsChange::ContentBlockingList(list, enabled));
    }

    /// Sends the engine the content blocker's state and its enabled filter lists.
    pub(crate) fn sync_engine_content_blocking(&self) {
        let blocking = &self.settings.content_blocking;
        let lists = blocking
            .lists
            .iter()
            .map(|list| list.identifier())
            .collect();
        self.browser
            .send_engine(crate::engine::Command::SetContentBlocking(
                blocking.enabled,
                lists,
            ));
    }

    /// Sends the engine the autoplay default and every site exception.
    pub(crate) fn sync_engine_autoplay(&self) {
        let sites = self
            .permission_store
            .autoplay_entries()
            .map(|(origin, policy)| (origin.to_owned(), policy))
            .collect();
        self.browser
            .send_engine(crate::engine::Command::SetAutoplay(
                self.settings.autoplay,
                sites,
            ));
    }

    pub(crate) fn set_split_view_limit(&mut self, limit: usize) {
        let limit = limit.clamp(2, 4);
        if self.settings.split_view_limit == limit {
            return;
        }
        self.settings.split_view_limit = limit;
        self.browser.split_view_limit = limit;
        self.save_setting(SettingsChange::SplitViewLimit(limit));
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
        self.browser.set_default_zoom(self.settings.page_zoom);
        self.browser.split_view_limit = self.settings.split_view_limit;
        super::publish_settings(&self.settings);
        self.sync_engine_permissions(&load.permissions);
        self.permission_store = load.permissions;
        super::publish_permissions(&self.permission_store);
        self.sync_engine_autoplay();
        self.sync_engine_content_blocking();
        let download_directory = self.engine_download_directory();
        if previous_download_directory != download_directory {
            self.browser.set_download_directory(download_directory);
        }
        self.configuration_issue = (!load.issues.is_empty()).then(|| load.issues.join("\n"));
    }

    /// Sends the engine every site decision and default that differs in `next`.
    fn sync_engine_permissions(&self, next: &PermissionStore) {
        let current = &self.permission_store;
        let changed_sites = current
            .entries()
            .chain(next.entries())
            .filter(|&(origin, kind, _)| current.state(origin, kind) != next.state(origin, kind))
            .map(|(origin, kind, _)| (origin, kind))
            .collect::<std::collections::BTreeSet<_>>();
        for (origin, kind) in changed_sites {
            self.browser
                .send_engine(crate::engine::Command::SetPermission(
                    origin.to_owned(),
                    kind,
                    next.state(origin, kind),
                ));
        }
        for kind in PermissionKind::ALL {
            let blocked = next.default_blocked(kind);
            if blocked != current.default_blocked(kind) {
                self.browser
                    .send_engine(crate::engine::Command::SetPermissionDefault(kind, blocked));
            }
        }
    }

    pub(crate) fn current_permission_origin(&self) -> Option<String> {
        self.browser
            .active_tab()
            .and_then(|tab| PermissionStore::origin(&tab.committed_url))
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
        self.browser
            .send_engine(crate::engine::Command::SetPermission(
                origin.clone(),
                kind,
                permission,
            ));
        self.save_permission_choice(origin, kind, permission);
    }

    pub(crate) fn save_permission_choice(
        &mut self,
        origin: String,
        kind: PermissionKind,
        permission: PermissionState,
    ) {
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
        let had_autoplay = self.permission_store.autoplay(&origin).is_some();
        self.permission_store.reset(&origin);
        super::publish_permissions(&self.permission_store);
        if had_autoplay {
            self.sync_engine_autoplay();
            if let Some(storage) = &self.storage
                && let Err(error) = storage.save_autoplay(origin.clone(), None)
            {
                self.configuration_issue = Some(error);
            }
        }
        for kind in PermissionKind::ALL {
            self.browser
                .send_engine(crate::engine::Command::SetPermission(
                    origin.clone(),
                    kind,
                    PermissionState::Ask,
                ));
            if let Some(storage) = &self.storage
                && let Err(error) =
                    storage.save_permission(origin.clone(), kind, PermissionState::Ask)
            {
                self.configuration_issue = Some(error);
            }
        }
    }
}
