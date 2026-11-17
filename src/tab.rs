// SPDX-License-Identifier: GPL-3.0-only

//! A tab: its stable identity, what the interface shows about its page, and what the engine was
//! last told about it. The browser owns every tab and is the only one to change them.

use crate::engine::{Surface, TabUpdate};
use rune::Pixels;

/// A tab's identity, stable for as long as the tab is open and never reused afterwards, so a
/// stale id can only miss, never reach another tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(u64);

impl TabId {
    /// The number Ladybird tags the tab's view and events with.
    pub(crate) fn raw(self) -> u64 {
        self.0
    }

    pub(crate) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

pub(crate) struct Tab {
    id: TabId,
    /// The page's address, once Ladybird has committed it; the address asked for until then.
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) loading: bool,
    pub(crate) favicon: Option<Pixels>,
    pub(crate) error: Option<String>,
    pub(crate) can_go_back: bool,
    pub(crate) can_go_forward: bool,
    /// Where the engine shows the page's frames, which the viewport draws at `frame_size`.
    pub(crate) surface: Surface,
    /// Points the latest frame covers.
    pub(crate) frame_size: [f32; 2],
    pub(crate) engine: Applied,
}

/// What the engine was last told about the tab, so the browser sends only what changed.
#[derive(Default)]
pub(crate) struct Applied {
    /// The first page, held until the view can lay it out at its real size.
    pub(crate) pending_load: Option<String>,
    /// The device-pixel size and scale the view was last given.
    pub(crate) viewport: Option<(i32, i32, f64)>,
    pub(crate) focused: bool,
    /// Views start visible.
    pub(crate) hidden: bool,
    /// The surface's frame count at the last poll.
    pub(crate) frames: u64,
}

impl Tab {
    pub(crate) fn new(id: TabId, url: String) -> Self {
        Self {
            id,
            url: url.clone(),
            title: String::new(),
            loading: false,
            favicon: None,
            error: None,
            can_go_back: false,
            can_go_forward: false,
            surface: Surface::default(),
            frame_size: [0.0; 2],
            engine: Applied {
                pending_load: Some(url),
                ..Applied::default()
            },
        }
    }

    pub(crate) fn id(&self) -> TabId {
        self.id
    }

    /// What the sidebar calls the tab: its title, or its address until it has one.
    pub(crate) fn label(&self) -> &str {
        match (self.title.trim(), self.url.as_str()) {
            ("", "" | "about:blank") => "New Tab",
            ("", url) => url,
            (title, _) => title,
        }
    }

    /// Takes in what the engine reported about the tab.
    pub(crate) fn update(&mut self, update: TabUpdate) {
        match update {
            TabUpdate::Navigation(url) => {
                self.url = url;
                self.error = None;
            }
            TabUpdate::Title(title) => self.title = title,
            TabUpdate::Loading(loading) => self.loading = loading,
            TabUpdate::Favicon(favicon) => {
                self.favicon = favicon.map(|favicon| {
                    let pixels = Pixels::new();
                    pixels.set(favicon.width, favicon.height, favicon.rgba);
                    pixels
                });
            }
            TabUpdate::History { back, forward } => {
                self.can_go_back = back;
                self.can_go_forward = forward;
            }
            TabUpdate::Error(error) => self.error = Some(error),
            TabUpdate::Resized {
                width,
                height,
                size,
                rgba,
            } => {
                self.surface.pixels.set(width, height, rgba);
                self.frame_size = size;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        tab.update(TabUpdate::Resized {
            width: 4,
            height: 2,
            size: [2.0, 1.0],
            rgba: vec![0; 32],
        });
        assert!(tab.loading);
        assert_eq!(tab.url, "https://example.com/a");
        assert_eq!(tab.error, None);
        assert!(tab.can_go_back && !tab.can_go_forward);
        assert_eq!(tab.frame_size, [2.0, 1.0]);
    }
}
