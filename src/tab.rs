// SPDX-License-Identifier: GPL-3.0-only

//! A tab: its stable identity, what the interface shows about its page, and what the engine was
//! last told about it. The browser owns every tab and is the only one to change them.

use crate::engine::{FindResult, Surface, TabUpdate, Thumbnail};
use elaris_core::internal::InternalRoute;
use elaris_core::permissions::PermissionKind;
use rune::Pixels;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// A tab's position in the sidebar. Tabs remain the same live objects when this changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TabSection {
    Favourite,
    #[default]
    Regular,
}

impl TabSection {
    pub(crate) const ORDER: [Self; 2] = [Self::Favourite, Self::Regular];
}

/// A tab's identity, stable for as long as the tab is open and never reused afterwards, so a
/// stale id can only miss, never reach another tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(u64);

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);
static LIVE_TAB_IDS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

impl TabId {
    /// The number Ladybird tags the tab's view and events with.
    pub(crate) fn raw(self) -> u64 {
        self.0
    }

    #[cfg_attr(not(any(test, feature = "ladybird-bridge")), allow(dead_code))]
    pub(crate) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub(crate) fn next() -> Self {
        let id = Self(NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed));
        LIVE_TAB_IDS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.0);
        id
    }

    pub(crate) fn release(self) {
        if let Some(ids) = LIVE_TAB_IDS.get() {
            ids.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.0);
        }
    }

    pub(crate) fn is_live(self) -> bool {
        LIVE_TAB_IDS.get().is_some_and(|ids| {
            ids.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&self.0)
        })
    }
}

pub(crate) struct Tab {
    id: TabId,
    pub(crate) section: TabSection,
    /// The page's address, once Ladybird has committed it; the address asked for until then.
    pub(crate) url: String,
    /// Last top-level URL Ladybird confirmed as committed.
    pub(crate) committed_url: String,
    pub(crate) title: String,
    pub(crate) loading: bool,
    pub(crate) ime_caret: Option<[i32; 4]>,
    pub(crate) hovered_link: String,
    pub(crate) tooltip: String,
    pub(crate) page_cursor: rune::Cursor,
    pub(crate) page_dialog: Option<PageDialog>,
    pub(crate) permission_prompt: Option<elaris::ladybird::PermissionPrompt>,
    pub(crate) external_url: Option<elaris::ladybird::ExternalUrl>,
    /// Accessibility reports not yet applied; the browser applies them with the page zoom.
    pub(crate) accessibility_events: Vec<elaris::ladybird::AccessibilityEvent>,
    pub(crate) accessibility: crate::page_accessibility::PageAccessibility,
    pub(crate) web_fullscreen: bool,
    pub(crate) permissions_in_use: [bool; PermissionKind::ALL.len()],
    pub(crate) awaiting_close: bool,
    pub(crate) favicon: Option<Pixels>,
    pub(crate) favicon_data: Option<std::sync::Arc<crate::storage::SessionFavicon>>,
    pub(crate) error: Option<String>,
    pub(crate) internal: Option<InternalRoute>,
    pub(crate) zoom_override: Option<u16>,
    pub(crate) navigation_items: Vec<elaris::ladybird::NavigationItem>,
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
    /// The query and latest acknowledged Ladybird result for this tab.
    pub(crate) find: FindState,
    /// Where the engine shows the page's frames, which the viewport scales to its bounds.
    pub(crate) surface: Surface,
    /// Weak handle to the engine's last cached preview, scoped to this surface.
    pub(crate) thumbnail: Option<Thumbnail>,
    pub(crate) engine: Applied,
}

/// What the engine was last told about the tab, so the browser sends only what changed.
#[derive(Default)]
pub(crate) struct Applied {
    /// The first page, held until the view can lay it out at its real size.
    pub(crate) pending_load: Option<String>,
    /// Restored background tabs wait until selected before their first layout and load.
    pub(crate) deferred_load: bool,
    /// The device-pixel size and scale the view was last given.
    pub(crate) viewport: Option<(i32, i32, f64)>,
    pub(crate) focused: bool,
    /// Views start visible.
    pub(crate) hidden: bool,
    /// The surface's frame count at the last poll.
    pub(crate) frames: u64,
}

pub(crate) struct PageDialog {
    pub(crate) request: elaris::ladybird::PageDialog,
    pub(crate) response: rune::TextEdit,
}

impl Tab {
    pub(crate) fn new(id: TabId, url: String) -> Self {
        Self {
            id,
            section: TabSection::Regular,
            url: url.clone(),
            committed_url: "about:blank".to_owned(),
            title: String::new(),
            loading: false,
            ime_caret: None,
            hovered_link: String::new(),
            tooltip: String::new(),
            page_cursor: rune::Cursor::Default,
            page_dialog: None,
            permission_prompt: None,
            external_url: None,
            accessibility_events: Vec::new(),
            accessibility: Default::default(),
            web_fullscreen: false,
            permissions_in_use: [false; PermissionKind::ALL.len()],
            awaiting_close: false,
            favicon: None,
            favicon_data: None,
            error: None,
            internal: None,
            zoom_override: None,
            navigation_items: Vec::new(),
            can_go_back: false,
            can_go_forward: false,
            find: FindState::default(),
            surface: Surface::default(),
            thumbnail: None,
            engine: Applied {
                pending_load: Some(url),
                ..Applied::default()
            },
        }
    }

    pub(crate) fn new_internal(id: TabId, route: InternalRoute) -> Self {
        let address = match &route {
            InternalRoute::Page(page) => page.address().to_owned(),
            InternalRoute::NotFound(address) => address.clone(),
        };
        Self {
            id,
            section: TabSection::Regular,
            url: address,
            committed_url: String::new(),
            title: route.title().to_owned(),
            loading: false,
            ime_caret: None,
            hovered_link: String::new(),
            tooltip: String::new(),
            page_cursor: rune::Cursor::Default,
            page_dialog: None,
            permission_prompt: None,
            external_url: None,
            accessibility_events: Vec::new(),
            accessibility: Default::default(),
            web_fullscreen: false,
            permissions_in_use: [false; PermissionKind::ALL.len()],
            awaiting_close: false,
            favicon: None,
            favicon_data: None,
            error: None,
            internal: Some(route),
            zoom_override: None,
            navigation_items: Vec::new(),
            can_go_back: false,
            can_go_forward: false,
            find: FindState::default(),
            surface: Surface::default(),
            thumbnail: None,
            engine: Default::default(),
        }
    }

    pub(crate) fn is_internal(&self) -> bool {
        self.internal.is_some()
    }

    pub(crate) fn id(&self) -> TabId {
        self.id
    }

    pub(crate) fn section(&self) -> TabSection {
        self.section
    }

    /// What the sidebar calls the tab: its title, or its address until it has one.
    pub(crate) fn label(&self) -> &str {
        match (self.title.trim(), self.url.as_str()) {
            ("", "" | "about:blank") => "New Tab",
            ("", url) => url,
            (title, _) => title,
        }
    }

    pub(crate) fn clear_page_hover(&mut self) {
        self.hovered_link.clear();
        self.tooltip.clear();
        self.page_cursor = rune::Cursor::Default;
    }

    /// Takes in an engine update and reports whether the window should rebuild.
    pub(crate) fn update(&mut self, update: TabUpdate) -> bool {
        match update {
            TabUpdate::FindResult { request_id, result } => {
                self.find.apply_result(request_id, result);
            }
            TabUpdate::ImeCaret(rect) => {
                return std::mem::replace(&mut self.ime_caret, rect) != rect;
            }
            TabUpdate::WebFullscreen(active) => self.web_fullscreen = active,
            // The browser's swipe tracking takes these before they reach a tab.
            TabUpdate::WheelResult(_) => {}
            TabUpdate::PermissionPrompt(prompt) => self.permission_prompt = prompt,
            TabUpdate::ExternalUrl(request) => self.external_url = request,
            TabUpdate::Accessibility(event) => self.accessibility_events.push(event),
            TabUpdate::PermissionUse(kind, visible) => {
                self.permissions_in_use[kind.index()] = visible
            }
            TabUpdate::PageDialog(dialog) => {
                self.page_dialog = dialog.map(|request| {
                    let mut response = rune::TextEdit::default();
                    response.replace_text(request.default_text.clone());
                    PageDialog { request, response }
                });
            }
            TabUpdate::HoveredLink(url) => {
                if self.hovered_link == url {
                    return false;
                }
                self.hovered_link = url;
            }
            TabUpdate::Tooltip(text) => {
                self.tooltip = text
                    .chars()
                    .map(|ch| if ch.is_control() { ' ' } else { ch })
                    .collect();
            }
            TabUpdate::Cursor(name) => {
                use rune::Cursor;
                let cursor = match name.as_str() {
                    "Hidden" => Cursor::Hidden,
                    "Crosshair" | "Eyedropper" => Cursor::Crosshair,
                    "IBeam" => Cursor::Text,
                    "ResizeHorizontal" | "ResizeColumn" => Cursor::ResizeColumn,
                    "ResizeVertical" | "ResizeRow" => Cursor::ResizeRow,
                    "Hand" => Cursor::Pointer,
                    "OpenHand" => Cursor::Grab,
                    "Drag" | "DragCopy" => Cursor::Grabbing,
                    "Move" => Cursor::Move,
                    "Disallowed" => Cursor::NotAllowed,
                    "Zoom" => Cursor::ZoomIn,
                    _ => Cursor::Default,
                };
                return std::mem::replace(&mut self.page_cursor, cursor) != cursor;
            }
            TabUpdate::Navigation(url) => {
                self.thumbnail = None;
                self.clear_page_hover();
                self.url = url;
                self.error = None;
            }
            TabUpdate::NavigationCommitted(url) => {
                self.thumbnail = None;
                self.clear_page_hover();
                self.url.clone_from(&url);
                self.committed_url = url;
                self.error = None;
            }
            TabUpdate::Title(title) => self.title = title,
            TabUpdate::Loading(loading) => self.loading = loading,
            TabUpdate::Favicon(favicon) => {
                self.favicon_data = favicon.as_ref().map(|favicon| {
                    std::sync::Arc::new(crate::storage::SessionFavicon {
                        width: favicon.width,
                        height: favicon.height,
                        rgba: favicon.rgba.clone(),
                    })
                });
                self.favicon = favicon.map(|favicon| {
                    let pixels = Pixels::new();
                    pixels.set(favicon.width, favicon.height, favicon.rgba);
                    pixels
                });
            }
            TabUpdate::NavigationItems(items) => self.navigation_items = items,
            TabUpdate::History { back, forward } => {
                self.can_go_back = back;
                self.can_go_forward = forward;
            }
            TabUpdate::LoadError(_) => {}
            TabUpdate::Error(error) => self.error = Some(error),
        }
        true
    }
}

/// Per-tab find state. Request IDs make late engine results harmless after edits or navigation.
#[derive(Debug, Default)]
pub(crate) struct FindState {
    pub(crate) query: String,
    pub(crate) result: Option<FindResult>,
    request_id: u64,
    pending: bool,
}

impl FindState {
    /// Starts a query only when it differs from the current or in-flight query.
    pub(crate) fn begin_query(&mut self, query: &str) -> Option<u64> {
        if self.query == query && (self.pending || self.result.is_some()) {
            return None;
        }
        self.query.clear();
        self.query.push_str(query);
        self.begin_request()
    }

    /// Starts a navigation request for the current non-empty query.
    pub(crate) fn begin_navigation(&mut self) -> Option<u64> {
        if self.query.is_empty() {
            None
        } else {
            self.begin_request()
        }
    }

    /// Invalidates counts and any outstanding result without forgetting the query.
    pub(crate) fn invalidate(&mut self) {
        self.request_id = self.request_id.wrapping_add(1);
        self.pending = false;
        self.result = None;
    }

    /// Clears query and result state while leaving request IDs monotonic.
    pub(crate) fn clear_query(&mut self) {
        self.invalidate();
        self.query.clear();
    }

    pub(crate) fn apply_result(&mut self, request_id: u64, result: FindResult) -> bool {
        if request_id != self.request_id {
            return false;
        }
        self.result = Some(result);
        self.pending = false;
        true
    }

    fn begin_request(&mut self) -> Option<u64> {
        self.request_id = self.request_id.wrapping_add(1);
        self.pending = true;
        self.result = None;
        Some(self.request_id)
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    pub(crate) fn request_id(&self) -> u64 {
        self.request_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::FindResult;

    fn tab(url: &str) -> Tab {
        Tab::new(TabId::from_raw(1), url.to_owned())
    }

    #[test]
    fn a_tab_is_called_by_its_title_or_address() {
        let mut tab = tab("about:blank");
        assert_eq!(tab.label(), "New Tab");
        tab.update(TabUpdate::Navigation("https://example.com/".into()));
        assert_eq!(tab.label(), "https://example.com/");
        tab.update(TabUpdate::Title("Example".into()));
        assert_eq!(tab.label(), "Example");
    }

    #[test]
    fn takes_in_what_the_engine_reports() {
        let mut tab = tab("https://example.com/");
        tab.error = Some("old".into());
        tab.update(TabUpdate::Loading(true));
        tab.update(TabUpdate::Navigation("https://example.com/a".into()));
        tab.update(TabUpdate::History {
            back: true,
            forward: false,
        });
        assert!(tab.loading);
        assert_eq!(tab.url, "https://example.com/a");
        assert_eq!(tab.error, None);
        assert!(tab.can_go_back && !tab.can_go_forward);
    }

    #[test]
    fn repeated_pointer_and_caret_updates_do_not_rebuild_the_window() {
        let mut tab = tab("https://example.com/");
        let updates: [fn() -> TabUpdate; 3] = [
            || TabUpdate::Cursor("Hand".into()),
            || TabUpdate::HoveredLink("https://example.com/link".into()),
            || TabUpdate::ImeCaret(Some([10, 20, 1, 16])),
        ];
        for update in updates {
            assert!(tab.update(update()));
            assert!(!tab.update(update()));
        }
        assert!(tab.update(TabUpdate::Cursor("IBeam".into())));
        assert!(tab.update(TabUpdate::HoveredLink(String::new())));
        assert!(tab.update(TabUpdate::ImeCaret(None)));
    }

    #[test]
    fn find_state_skips_identical_queries_and_ignores_old_results() {
        let mut find = FindState::default();
        let first = find.begin_query("match").expect("first query starts");
        assert!(find.pending());
        assert_eq!(find.begin_query("match"), None);

        let second = find.begin_query("changed").expect("changed query starts");
        assert!(second > first);
        assert!(!find.apply_result(
            first,
            FindResult {
                current_match_index: 0,
                total_match_count: Some(4),
            }
        ));
        assert!(find.pending());
        assert!(find.apply_result(
            second,
            FindResult {
                current_match_index: 0,
                total_match_count: Some(1),
            }
        ));
        assert_eq!(find.result.unwrap().total_match_count, Some(1));
    }

    #[test]
    fn empty_query_and_navigation_invalidate_count_state_without_losing_tab_identity() {
        let mut find = FindState::default();
        assert_eq!(find.begin_navigation(), None);
        let request = find.begin_query("same").expect("query starts");
        find.apply_result(
            request,
            FindResult {
                current_match_index: 1,
                total_match_count: Some(2),
            },
        );
        assert_eq!(find.query, "same");
        assert!(find.begin_navigation().is_some());
        find.invalidate();
        assert_eq!(find.query, "same");
        assert_eq!(find.result, None);
        assert!(!find.pending());

        find.clear_query();
        assert!(find.query.is_empty());
        assert_eq!(find.begin_navigation(), None);
    }
}
