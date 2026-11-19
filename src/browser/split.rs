// SPDX-License-Identifier: GPL-3.0-only

use super::{Browser, TabId};

pub(crate) const DIVIDER: f32 = 8.0;
const MIN_PANE: f32 = 240.0;

#[derive(Clone, Copy)]
pub(crate) struct Split {
    pub(crate) panes: [Option<TabId>; 2],
    pub(crate) ratio: f32,
}

pub(crate) fn widths(width: f32, ratio: f32) -> [f32; 2] {
    let available = (width - DIVIDER).max(0.0);
    let min = MIN_PANE.min(available / 2.0);
    let left = (available * ratio).clamp(min, available - min);
    [left, available - left]
}

/// Page bounds on each side of the divider, in the same coordinates as the content rect.
pub(crate) fn pane_rects([x, y, width, height]: [f32; 4], ratio: f32) -> [[f32; 4]; 2] {
    let [left, right] = widths(width, ratio);
    [[x, y, left, height], [x + left + DIVIDER, y, right, height]]
}

pub(crate) fn target(rect: [f32; 4], point: [f32; 2], boundary: f32) -> Option<usize> {
    let [x, y, w, h] = rect;
    let [px, py] = point;
    (px >= x && px < x + w && py >= y && py < y + h).then_some(usize::from(px >= x + boundary))
}

impl Browser {
    pub(crate) fn focused_side(&self) -> usize {
        usize::from(
            self.split
                .is_some_and(|split| self.active.is_some() && split.panes[1] == self.active),
        )
    }

    pub(crate) fn visible_tabs(&self) -> [Option<TabId>; 2] {
        self.split.map_or([self.active, None], |split| split.panes)
    }

    pub(crate) fn begin_split(&mut self, id: TabId, side: usize) {
        if self.tab(id).is_none() {
            return;
        }
        let mut panes = [None; 2];
        panes[side] = Some(id);
        self.split = Some(Split { panes, ratio: 0.5 });
        self.activate_tab(id);
    }

    pub(crate) fn place_split(&mut self, id: TabId, side: usize) {
        if self.tab(id).is_none() {
            return;
        }
        if self.split.is_none() {
            if let Some(active) = self.active.filter(|active| *active != id) {
                self.begin_split(active, 1 - side);
            } else {
                self.activate_tab(id);
                return;
            }
        }
        if let Some(split) = &mut self.split {
            if split.panes[1 - side] == Some(id) {
                split.panes.swap(0, 1);
            } else {
                split.panes[side] = Some(id);
            }
        }
        self.activate_tab(id);
    }

    pub(crate) fn swap_split(&mut self) {
        if let Some(split) = &mut self.split {
            split.panes.swap(0, 1);
        }
    }

    pub(crate) fn exit_split(&mut self, keep: Option<TabId>) {
        self.split = None;
        if let Some(id) = keep {
            self.activate_tab(id);
        }
    }

    pub(crate) fn resize_split(&mut self, left: f32) {
        let width = self.window.viewport[0];
        if let Some(split) = &mut self.split {
            let available = (width - DIVIDER).max(1.0);
            split.ratio = widths(width, left / available)[0] / available;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_testing_checks_bounds_and_the_actual_divider() {
        let rect = [100.0, 10.0, 800.0, 400.0];
        assert_eq!(target(rect, [99.0, 10.0], 300.0), None);
        assert_eq!(target(rect, [399.0, 10.0], 300.0), Some(0));
        assert_eq!(target(rect, [400.0, 10.0], 300.0), Some(1));
        assert_eq!(target(rect, [899.0, 410.0], 300.0), None);
    }

    #[test]
    fn clamps_resize_and_shares_space_when_the_window_is_narrow() {
        assert_eq!(widths(1008.0, 0.0), [240.0, 760.0]);
        assert_eq!(widths(1008.0, 1.0), [760.0, 240.0]);
        assert_eq!(widths(408.0, 0.9), [200.0, 200.0]);
    }

    #[test]
    fn picker_cancellation_and_pane_removal_leave_tabs_open() {
        let mut browser = Browser::default();
        let first = browser.open_tab("one.example").unwrap();
        let second = browser.open_tab("two.example").unwrap();
        browser.begin_split(first, 1);
        assert_eq!(browser.visible_tabs(), [None, Some(first)]);
        browser.exit_split(None);
        assert_eq!(browser.visible_tabs(), [Some(first), None]);
        browser.begin_split(first, 1);
        browser.activate_tab(second);
        assert_eq!(browser.visible_tabs(), [Some(second), Some(first)]);
        browser.exit_split(Some(first));
        assert_eq!(browser.visible_tabs(), [Some(first), None]);
        assert_eq!(browser.tabs().len(), 2);
    }

    #[test]
    fn swapping_replacing_and_closing_preserve_the_other_pane() {
        let mut browser = Browser::default();
        let first = browser.open_tab("one.example").unwrap();
        let second = browser.open_tab("two.example").unwrap();
        let third = browser.open_tab("three.example").unwrap();
        browser.place_split(second, 1);
        browser.place_split(first, 1);
        assert_eq!(browser.visible_tabs(), [Some(second), Some(first)]);
        assert_eq!(browser.focused_side(), 1);
        browser.swap_split();
        assert_eq!(browser.focused_side(), 0);
        browser.activate_tab(third);
        assert_eq!(browser.visible_tabs(), [Some(third), Some(second)]);
        assert!(browser.tab(first).is_some());
        browser.close_tab(third);
        assert!(browser.split.is_none());
        assert_eq!(browser.visible_tabs(), [Some(second), None]);
    }
}
