// SPDX-License-Identifier: GPL-3.0-only

//! The `AppState` struct: the state each Rune window holds, and the record of recently closed tabs.

#[cfg(elaris_ui = "linux")]
use super::LinuxTitlebar;
use super::MiniStart;
use super::clear_data;
use super::history::HistoryPage;
use super::tabs;
use super::task_manager::TaskManagerPage;
use crate::browser::Browser;
use crate::omnibox;
use crate::storage::{Session, Storage};
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::SidebarState;
use elaris_core::permissions::{PermissionKind, PermissionStore};
use elaris_core::settings::Settings;
use rune::{PointerPress, TextEdit};
use std::path::PathBuf;

/// The window's title when the page has none.
pub(crate) const APP_NAME: &str = "Elaris";
pub(super) const RECENTLY_CLOSED_LIMIT: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecentlyClosedTab {
    pub(super) url: String,
    pub(super) title: String,
    pub(super) section: TabSection,
}

impl RecentlyClosedTab {
    pub(super) fn from_tab(tab: &Tab) -> Self {
        Self {
            url: tab.url.clone(),
            title: tab.title.clone(),
            section: tab.section(),
        }
    }
}

/// State for one Rune window. Its Browser owns its tabs; process services are shared separately.
pub(crate) struct AppState {
    pub(crate) window_id: u64,
    /// Maximum page frame rate reported by the window's current display.
    #[cfg(any(target_os = "macos", feature = "ladybird-bridge"))]
    #[cfg_attr(elaris_ui = "linux", allow(dead_code))]
    pub(super) display: elaris::ladybird::DisplayMetadata,
    pub(crate) omnibox: omnibox::State,
    /// The open Clear Browsing Data dialog.
    pub(crate) clear_data: Option<clear_data::ClearDataDialog>,
    /// The result of the last Clear Browsing Data request.
    pub(crate) clear_data_status: Option<String>,
    /// Clock guards for website permission prompts and fullscreen.
    pub(crate) page_guards: crate::ui::permission_prompt::Guards,
    /// Whether the floating Find in Page bar is mounted.
    pub(crate) find_open: bool,
    pub(crate) tab_switcher: Option<crate::ui::tab_switcher::Switcher>,
    /// The active tab's find input. Each tab separately remembers its last query.
    pub(crate) find_input: TextEdit,
    /// The tab whose query currently backs the floating bar.
    pub(crate) find_tab: Option<TabId>,
    /// Identifies asynchronous selected-text reads so previous tabs cannot seed this query.
    pub(super) find_selection_request: u64,
    pub(crate) settings: Settings,
    /// Why the last submitted query could not be opened, until the next is.
    pub(crate) address_error: Option<String>,
    pub(crate) browser: Browser,
    /// Whether the downloads popover is open.
    pub(crate) downloads_open: bool,
    /// Whether the site controls popover is mounted.
    pub(crate) site_permissions_open: bool,
    /// Rune-space bounds of the URL-bar control, used as AppKit's popover anchor.
    pub(crate) site_controls_anchor: Option<[f32; 4]>,
    /// The page currently shown inside the site controls popover.
    pub(crate) site_controls_page: crate::site_controls::SiteControlsPage,
    /// Layout bounds for each Rune permission row, in `PermissionKind::index()` order.
    pub(crate) permission_row_anchors: [Option<[f32; 4]>; 4],
    /// The open permission choice menu, if any.
    pub(crate) permission_menu: Option<PermissionKind>,
    /// Whether the site controls autoplay menu is open, and where its row sits.
    pub(crate) autoplay_menu_open: bool,
    pub(crate) autoplay_row_anchor: Option<[f32; 4]>,
    /// Whether the compact site overflow menu is open.
    pub(crate) site_overflow_open: bool,
    /// The site action showing its confirmation, and when it returns to its glyph on the window clock.
    pub(crate) site_action_done: Option<(crate::site_controls::SiteControlAction, f64)>,
    /// The site footer's overflow-button position for the Rune menu host.
    pub(crate) site_overflow_anchor: Option<PointerPress>,
    /// Per-origin permission choices loaded from config.toml.
    pub(crate) permission_store: PermissionStore,
    pub(crate) page_menu: crate::ui::PageMenuState,
    pub(crate) form_picker: Option<crate::ui::form_picker::ActivePicker>,
    pub(crate) sidebar: SidebarState,
    pub(crate) hover_highlight: Option<crate::ui::components::hover_highlight::Target>,
    /// The animated icon under the pointer, which plays its motion glyph.
    pub(crate) animated_icon_hover: Option<String>,
    pub(crate) context_menu_highlight: Option<String>,
    /// The menu row keyboard focus was last seen on, so a poll only moves the highlight when
    /// focus itself moves.
    pub(crate) context_menu_focus: Option<String>,
    /// The row the pointer entered, applied by the next poll so the highlight glides there.
    pub(crate) context_menu_hover: Option<String>,
    pub(crate) split_rect: [f32; 4],
    pub(crate) peek_presentation: crate::ui::peek::Presentation,
    pub(crate) split_preview: Option<usize>,
    pub(crate) split_selection: usize,
    pub(crate) tab_drag: crate::ui::TabDragState,
    /// The category a native page with side navigation shows; its first when unset.
    pub(crate) page_category: Option<&'static str>,
    /// The page navigation item under the pointer, which the hover highlight glides to.
    pub(crate) page_nav_hover: Option<usize>,
    pub(crate) settings_picker_open: bool,
    pub(crate) startup_picker_open: bool,
    #[cfg(elaris_ui = "linux")]
    pub(crate) decorations_picker_open: bool,
    /// The window frame setting changed and applies the next time Elaris opens.
    #[cfg(elaris_ui = "linux")]
    pub(crate) restart_notice: bool,
    #[cfg(elaris_ui = "linux")]
    pub(crate) titlebar: LinuxTitlebar,
    pub(super) storage: Option<std::sync::Arc<Storage>>,
    pub(super) workers_started: bool,
    pub(super) pending_detach: bool,
    pub(super) merged_away: bool,
    /// Whether this is a compact single-page window rather than a full browser window.
    pub(super) mini: bool,
    /// What a mini window opens as soon as it can.
    pub(super) mini_start: Option<MiniStart>,
    #[cfg(feature = "ladybird-bridge")]
    pub(super) profile_dir: Option<PathBuf>,
    pub(super) config_file: Option<PathBuf>,
    pub(super) default_download_directory: PathBuf,
    pub(super) download_directory_override: Option<PathBuf>,
    pub(crate) configuration_issue: Option<String>,
    /// Whether Elaris is the default web browser, as last read from the system.
    pub(crate) default_browser: Option<bool>,
    /// Why the last request to become the default browser failed, until the next request.
    pub(crate) default_browser_error: Option<String>,
    #[cfg(feature = "ladybird-bridge")]
    pub(super) storage_error: Option<String>,
    pub(super) last_saved_session: Option<Session>,
    pub(super) restored_windows: Vec<Session>,
    pub(super) history_revision: u64,
    pub(crate) history_page: HistoryPage,
    pub(crate) task_manager: TaskManagerPage,
    pub(crate) memory_saver: super::memory_saver::MemorySaverState,
    /// The page title the window shows.
    pub(super) window_title: Option<String>,
    pub(crate) zoom_menu: Option<TabId>,
    /// The tab whose translate menu is open in the address field.
    pub(crate) translate_menu: Option<TabId>,
    /// The tab whose zoom chip shows, and the clock time it hides.
    pub(crate) zoom_chip: Option<(TabId, f64)>,
    /// The toasts on screen, shown over the window.
    pub(crate) toasts: crate::toast::Toasts,
    pub(crate) navigation_menu: Option<(TabId, PointerPress)>,
    pub(super) context_menu: Option<(Option<TabId>, PointerPress)>,
    pub(super) recently_closed: Vec<RecentlyClosedTab>,
    pub(super) closing_tabs: Option<tabs::ClosingTabs>,
}

impl Default for AppState {
    fn default() -> Self {
        // The first argument Rune leaves alone is a page to open.
        Self::new(rune::extra_args().first().map(String::as_str))
    }
}

impl AppState {
    #[cfg(target_os = "macos")]
    #[cfg_attr(elaris_ui = "linux", allow(dead_code))]
    pub(crate) fn set_display(&mut self, display: elaris::ladybird::DisplayMetadata) {
        self.display = display;
    }
}
