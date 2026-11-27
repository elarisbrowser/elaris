// SPDX-License-Identifier: GPL-3.0-only

//! The window's display state: viewport, scale, appearance, focus, and page thumbnails.

use super::Browser;
use std::sync::Arc;

use super::split;
use crate::engine::{Command, ViewCommand};
use crate::tab::{Tab, TabId};

impl Browser {
    pub(crate) fn fullscreen_tab(&self) -> Option<&Tab> {
        self.active_tab().filter(|tab| tab.web_fullscreen)
    }

    pub(crate) fn exit_web_fullscreen(&self) -> bool {
        if let Some(tab) = self.fullscreen_tab() {
            self.send(tab.id(), ViewCommand::ExitWebFullscreen);
            return true;
        }
        false
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.window.scale.set(scale);
    }

    /// Records whether the interface is dark, so pages follow it. The view calls it as it
    /// builds, because only the view knows the system's appearance; a change goes to the engine
    /// at once, since no poll may follow this build.
    pub(crate) fn note_dark(&self, dark: bool) {
        self.window.dark.set(Some(dark));
        self.send_color_scheme();
    }

    pub(crate) fn set_viewport(&mut self, size: [f32; 2]) {
        self.window.viewport = size;
    }

    pub(crate) fn content_width(&self) -> f32 {
        self.window.viewport[0]
    }

    pub(crate) fn viewport_width(&self) -> f32 {
        self.pane_width(self.focused_side())
    }

    pub(crate) fn pane_width(&self, side: usize) -> f32 {
        self.displayed_split()
            .map_or(self.content_width(), |split| {
                let count = split.pane_count.min(self.split_view_limit);
                if count == 2 {
                    return split::widths(self.content_width(), split.ratio)[side];
                }
                split::grid_cell_rect(
                    [0.0, 0.0, self.content_width(), self.window.viewport[1]],
                    count,
                    side,
                    split.ratio,
                )[2]
            })
    }

    pub(crate) fn window_focused(&self) -> bool {
        self.window.focused
    }

    pub(crate) fn thumbnail(&self, id: TabId) -> Option<Arc<rune::Pixels>> {
        let tab = self.tab(id).filter(|tab| !tab.is_internal())?;
        tab.thumbnail.as_ref()?.get(&tab.surface)
    }

    pub(crate) fn set_window_focus(&mut self, focused: bool) {
        if !focused {
            self.reset_input();
        }
        self.window.focused = focused;
    }

    /// Suspends every live page while the Rune window is closed but the app remains running.
    pub(crate) fn suspend_window(&mut self) {
        self.set_window_focus(false);
        self.window.suspended = true;
        self.dismiss_peek();
        let Some(engine) = &self.engine else {
            return;
        };
        for tab in &mut self.tabs {
            if tab.is_internal() {
                continue;
            }
            if tab.engine.focused {
                tab.engine.focused = false;
                engine.send(Command::View(tab.id(), ViewCommand::Focus(false)));
            }
            if !tab.engine.hidden {
                tab.engine.hidden = true;
                engine.send(Command::View(tab.id(), ViewCommand::Visible(false)));
            }
        }
    }

    /// Lets the next engine sync show the pages that are visible in this window again.
    pub(crate) fn resume_window(&mut self) {
        self.window.suspended = false;
    }

    pub(crate) fn set_viewport_focus(&mut self, focused: bool) -> bool {
        let changed = self.window.viewport_focused != focused;
        if self.window.viewport_focused && !focused {
            self.reset_input();
        }
        self.window.viewport_focused = focused;
        changed
    }
    pub(crate) fn page_focused(&self) -> bool {
        self.window.focused && self.window.viewport_focused
    }
    pub(crate) fn scale(&self) -> f32 {
        self.window.scale.get()
    }
}
