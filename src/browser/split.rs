// SPDX-License-Identifier: GPL-3.0-only

use super::{Browser, TabId};

pub(crate) const DIVIDER: f32 = 8.0;
const MIN_PANE: f32 = 240.0;
pub(crate) const MAX_PANES: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct Split {
    pub(crate) panes: [Option<TabId>; MAX_PANES],
    pub(crate) ratio: f32,
}

pub(crate) fn widths(width: f32, ratio: f32) -> [f32; 2] {
    let available = (width - DIVIDER).max(0.0);
    let min = MIN_PANE.min(available / 2.0);
    let left = (available * ratio).clamp(min, available - min);
    [left, available - left]
}

pub(crate) fn grid_cell_rect(rect: [f32; 4], count: usize, index: usize) -> [f32; 4] {
    let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(columns).max(1);
    let [x, y, width, height] = rect;
    let cell_width = (width - DIVIDER * (columns.saturating_sub(1) as f32)) / columns as f32;
    let row = index / columns;
    let column = index % columns;
    let cell_height = (height - DIVIDER * (rows.saturating_sub(1) as f32)) / rows as f32;
    [
        x + column as f32 * (cell_width + DIVIDER),
        y + row as f32 * (cell_height + DIVIDER),
        cell_width,
        cell_height,
    ]
}

pub(crate) fn target(rect: [f32; 4], point: [f32; 2], count: usize) -> Option<usize> {
    (0..count).find(|index| {
        let [x, y, width, height] = grid_cell_rect(rect, count, *index);
        point[0] >= x && point[0] < x + width && point[1] >= y && point[1] < y + height
    })
}

impl Browser {
    pub(crate) fn focused_side(&self) -> usize {
        self.split
            .and_then(|split| {
                split.panes[..self.split_view_limit.min(MAX_PANES)]
                    .iter()
                    .position(|pane| *pane == self.active)
            })
            .unwrap_or(0)
    }

    pub(crate) fn visible_tabs(&self) -> Vec<Option<TabId>> {
        self.split.map_or_else(
            || vec![self.active, None],
            |split| {
                let visible = &split.panes[..self.split_view_limit.min(MAX_PANES)];
                if visible.contains(&None) || visible.contains(&self.active) {
                    visible.to_vec()
                } else {
                    vec![self.active, None]
                }
            },
        )
    }

    pub(crate) fn displayed_split(&self) -> Option<Split> {
        self.split.filter(|split| {
            let panes = &split.panes[..self.split_view_limit.min(MAX_PANES)];
            panes.contains(&None) || panes.contains(&self.active)
        })
    }

    pub(crate) fn begin_split(&mut self, id: TabId, side: usize) {
        if self.tab(id).is_none() {
            return;
        }
        let count = self.split_view_limit.max(2);
        let mut panes = [None; MAX_PANES];
        panes[side.min(count - 1)] = Some(id);
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
            let side = side.min(self.split_view_limit.min(MAX_PANES).saturating_sub(1));
            if let Some(existing) = split.panes.iter().position(|pane| *pane == Some(id)) {
                split.panes.swap(existing, side);
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
        assert_eq!(target(rect, [99.0, 10.0], 2), None);
        assert_eq!(target(rect, [200.0, 200.0], 2), Some(0));
        assert_eq!(target(rect, [700.0, 200.0], 2), Some(1));
        assert_eq!(target(rect, [899.0, 410.0], 2), None);
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
    fn activating_another_tab_preserves_the_split_group() {
        let mut browser = Browser::default();
        let left = browser.open_tab("left.example").unwrap();
        let right = browser.open_tab("right.example").unwrap();
        let other = browser.open_tab("other.example").unwrap();
        browser.place_split(left, 0);
        browser.place_split(right, 1);

        browser.activate_tab(other);
        assert_eq!(browser.visible_tabs(), [Some(other), None]);
        assert_eq!(
            browser.split.map(|split| split.panes[..2].to_vec()),
            Some(vec![Some(left), Some(right)])
        );

        browser.activate_tab(right);
        assert_eq!(browser.visible_tabs(), [Some(left), Some(right)]);
        assert_eq!(browser.active_tab().map(|tab| tab.id()), Some(right));
    }

    #[test]
    fn swapping_preserves_the_group_when_another_tab_is_selected() {
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
        assert_eq!(browser.visible_tabs(), [Some(third), None]);
        assert!(browser.tab(first).is_some());
        browser.close_tab(third);
        assert_eq!(
            browser.split.map(|split| split.panes[..2].to_vec()),
            Some(vec![Some(first), Some(second)])
        );
        assert_eq!(browser.visible_tabs(), [Some(first), Some(second)]);
    }

    #[test]
    fn configured_split_limit_exposes_additional_grid_cells() {
        let mut browser = Browser::default();
        browser.split_view_limit = 4;
        let tabs: Vec<_> = [
            "one.example",
            "two.example",
            "three.example",
            "four.example",
        ]
        .into_iter()
        .map(|url| browser.open_tab(url).unwrap())
        .collect();
        for (side, tab) in tabs.iter().copied().enumerate() {
            browser.place_split(tab, side);
        }
        assert_eq!(
            browser.visible_tabs(),
            tabs.into_iter().map(Some).collect::<Vec<_>>()
        );
    }

    #[test]
    fn grid_hit_testing_finds_each_cell_and_ignores_gaps() {
        let rect = [0.0, 0.0, 408.0, 408.0];
        assert_eq!(target(rect, [100.0, 100.0], 4), Some(0));
        assert_eq!(target(rect, [300.0, 100.0], 4), Some(1));
        assert_eq!(target(rect, [100.0, 300.0], 4), Some(2));
        assert_eq!(target(rect, [300.0, 300.0], 4), Some(3));
        assert_eq!(target(rect, [204.0, 100.0], 4), None);
    }
}
