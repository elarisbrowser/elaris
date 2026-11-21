// SPDX-License-Identifier: GPL-3.0-only

use super::{Browser, TabId};

pub(crate) const DIVIDER: f32 = 8.0;
const MIN_PANE: f32 = 240.0;
pub(crate) const MAX_PANES: usize = 64;

#[derive(Clone, Copy)]
pub(crate) struct Split {
    pub(crate) panes: [Option<TabId>; MAX_PANES],
    pub(crate) pane_count: usize,
    pub(crate) ratio: f32,
}

impl Split {
    pub(crate) fn panes(&self, limit: usize) -> &[Option<TabId>] {
        &self.panes[..self.pane_count.min(limit).min(MAX_PANES)]
    }
}

pub(crate) fn widths(width: f32, ratio: f32) -> [f32; 2] {
    let available = (width - DIVIDER).max(0.0);
    let min = MIN_PANE.min(available / 2.0);
    let left = (available * ratio).clamp(min, available - min);
    [left, available - left]
}

pub(crate) fn grid_cell_rect(rect: [f32; 4], count: usize, index: usize, ratio: f32) -> [f32; 4] {
    let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(columns).max(1);
    let [x, y, width, height] = rect;
    let row = index / columns;
    let row_columns = (count - row * columns).min(columns);
    let column = index % columns;
    let available = (width - DIVIDER * (row_columns - 1) as f32).max(0.0);
    let first = if row_columns == 1 {
        available
    } else {
        available * grid_first_share(row_columns, ratio)
    };
    let rest = (available - first) / row_columns.saturating_sub(1).max(1) as f32;
    let (offset, cell_width) = if column == 0 {
        (0.0, first)
    } else {
        (
            first + DIVIDER + (column - 1) as f32 * (rest + DIVIDER),
            rest,
        )
    };
    let cell_height = (height - DIVIDER * (rows.saturating_sub(1) as f32)) / rows as f32;
    [
        x + offset,
        y + row as f32 * (cell_height + DIVIDER),
        cell_width,
        cell_height,
    ]
}

// Keep the default ratio at equal widths for any number of columns.
pub(crate) fn grid_first_share(columns: usize, ratio: f32) -> f32 {
    let equal = 1.0 / columns as f32;
    (equal + ratio - 0.5).clamp(equal * 0.5, 1.0 - equal * 0.5)
}

pub(crate) fn target(rect: [f32; 4], point: [f32; 2], count: usize, ratio: f32) -> Option<usize> {
    (0..count).find(|index| {
        let [x, y, width, height] = grid_cell_rect(rect, count, *index, ratio);
        point[0] >= x && point[0] < x + width && point[1] >= y && point[1] < y + height
    })
}

impl Browser {
    pub(crate) fn focused_side(&self) -> usize {
        self.displayed_split()
            .and_then(|split| {
                split
                    .panes(self.split_view_limit)
                    .iter()
                    .position(|pane| *pane == self.active)
            })
            .unwrap_or(0)
    }

    pub(crate) fn visible_tabs(&self) -> Vec<Option<TabId>> {
        self.displayed_split().map_or_else(
            || vec![self.active, None],
            |split| {
                let visible = split.panes(self.split_view_limit);
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
            if self.fullscreen_tab().is_some() {
                return false;
            }
            let panes = split.panes(self.split_view_limit);
            panes.contains(&None) || panes.contains(&self.active)
        })
    }

    pub(crate) fn begin_split(&mut self, id: TabId, side: usize) {
        if self.tab(id).is_none() {
            return;
        }
        let mut panes = [None; MAX_PANES];
        let pane_count = 2;
        panes[side.min(pane_count - 1)] = Some(id);
        self.split = Some(Split {
            panes,
            pane_count,
            ratio: 0.5,
        });
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
            if side >= split.pane_count && side < self.split_view_limit.min(MAX_PANES) {
                split.pane_count = side + 1;
            }
            let side = side.min(split.pane_count.saturating_sub(1));
            if let Some(existing) = split.panes.iter().position(|pane| *pane == Some(id)) {
                split.panes.swap(existing, side);
            } else {
                split.panes[side] = Some(id);
            }
        }
        self.activate_tab(id);
    }

    pub(crate) fn add_split_pane(&mut self) -> bool {
        let Some(split) = &mut self.split else {
            return false;
        };
        if split.pane_count >= self.split_view_limit.min(MAX_PANES) {
            return false;
        }
        split.pane_count += 1;
        true
    }

    pub(crate) fn cancel_split_picker(&mut self, side: usize) {
        let Some(split) = &mut self.split else { return };
        let count = split.panes(self.split_view_limit).len();
        if split.panes(self.split_view_limit).get(side) != Some(&None) {
            return;
        }
        if count == 2 {
            self.exit_split(None);
        } else {
            split.panes.copy_within(side + 1..split.pane_count, side);
            split.pane_count -= 1;
            split.panes[split.pane_count] = None;
        }
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
        assert_eq!(target(rect, [99.0, 10.0], 2, 0.5), None);
        assert_eq!(target(rect, [200.0, 200.0], 2, 0.5), Some(0));
        assert_eq!(target(rect, [700.0, 200.0], 2, 0.5), Some(1));
        assert_eq!(target(rect, [899.0, 410.0], 2, 0.5), None);
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
        browser.place_split(tabs[0], 0);
        browser.place_split(tabs[1], 1);
        assert_eq!(browser.visible_tabs().len(), 2);
        assert!(browser.add_split_pane());
        browser.place_split(tabs[2], 2);
        assert_eq!(browser.visible_tabs().len(), 3);
        assert!(browser.add_split_pane());
        browser.place_split(tabs[3], 3);
        assert!(!browser.add_split_pane());
        assert_eq!(
            browser.visible_tabs(),
            tabs.into_iter().map(Some).collect::<Vec<_>>()
        );
    }

    #[test]
    fn grid_hit_testing_finds_each_cell_and_ignores_gaps() {
        let rect = [0.0, 0.0, 408.0, 408.0];
        assert_eq!(target(rect, [100.0, 100.0], 4, 0.5), Some(0));
        assert_eq!(target(rect, [300.0, 100.0], 4, 0.5), Some(1));
        assert_eq!(target(rect, [100.0, 300.0], 4, 0.5), Some(2));
        assert_eq!(target(rect, [300.0, 300.0], 4, 0.5), Some(3));
        assert_eq!(target(rect, [204.0, 100.0], 4, 0.5), None);
    }
}
