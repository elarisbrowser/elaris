// SPDX-License-Identifier: GPL-3.0-only

//! Drop geometry: where a pointer lands in the tab lists and which insertion slot that means.

use crate::tab::{Tab, TabId, TabSection};
use crate::ui::theme;

use super::TARGET_HYSTERESIS;
use super::geometry::{
    contains, favourite_columns, favourite_slot_rect, rect_before_pointer, section_index,
};
use super::state::TabDragState;
use super::types::TabDropTarget;

impl TabDragState {
    pub(crate) fn incoming_target(&self, tabs: &[Tab], point: [f32; 2]) -> Option<TabDropTarget> {
        let in_list = self.viewport.is_some_and(|rect| contains(rect, point));
        let section = if self.horizontal {
            self.section_at(point)?
        } else {
            match self.section_at(point) {
                Some(TabSection::Favourite) => TabSection::Favourite,
                _ if in_list
                    && self.section_rects[section_index(TabSection::Regular)]
                        .is_some_and(|rect| point[1] >= rect[1]) =>
                {
                    TabSection::Regular
                }
                _ => return None,
            }
        };
        let index = self.insertion_index(
            tabs,
            &[],
            section,
            point,
            self.incoming.map(|(_, target)| target),
        );
        Some(TabDropTarget { section, index })
    }

    pub(crate) fn target_rect(
        &self,
        tabs: &[Tab],
        target: TabDropTarget,
        source_section: TabSection,
    ) -> [f32; 4] {
        if target.section == TabSection::Favourite
            && let Some(grid) = self.favourite_grid
        {
            let columns = favourite_columns(grid[2]);
            let final_count = tabs
                .iter()
                .filter(|tab| tab.section() == TabSection::Favourite)
                .count()
                + usize::from(source_section != TabSection::Favourite);
            return favourite_slot_rect(grid, target.index, final_count, columns);
        }
        let items = tabs.iter().filter(|tab| tab.section() == target.section);
        let count = items.clone().count();
        if target.index < count {
            if let Some(tab) = items.clone().nth(target.index)
                && let Some(rect) = self.tab_rects.get(&tab.id())
            {
                return *rect;
            }
        } else if let Some(tab) = items.clone().next_back()
            && let Some(rect) = self.tab_rects.get(&tab.id())
        {
            if self.horizontal {
                return [
                    rect[0] + rect[2] + theme::TAB_GAP,
                    rect[1],
                    rect[2],
                    rect[3],
                ];
            }
            return [
                rect[0],
                rect[1] + rect[3] + theme::TAB_GAP,
                rect[2],
                rect[3],
            ];
        }
        let section = self.section_rects[section_index(target.section)].unwrap_or([0.0; 4]);
        if self.horizontal {
            let width = if target.section == TabSection::Regular {
                theme::HORIZONTAL_TAB_MIN
            } else {
                theme::HORIZONTAL_COMPACT_TAB
            };
            return [section[0], section[1], width, theme::HORIZONTAL_TAB_HEIGHT];
        }
        [
            section[0],
            section[1] + section[3] - theme::ROW_HEIGHT,
            section[2],
            theme::ROW_HEIGHT,
        ]
    }

    pub(super) fn section_at(&self, point: [f32; 2]) -> Option<TabSection> {
        if self.horizontal && !self.viewport.is_some_and(|rect| contains(rect, point)) {
            return None;
        }
        if self
            .empty_favourite_area
            .is_some_and(|area| contains(area, point) && point[1] < area[1] + area[3])
        {
            return Some(TabSection::Favourite);
        }
        TabSection::ORDER
            .into_iter()
            .find(|section| {
                self.section_rects[section_index(*section)]
                    .is_some_and(|rect| contains(rect, point))
            })
            .or_else(|| {
                (self.horizontal
                    && self.section_rects[section_index(TabSection::Regular)]
                        .is_some_and(|rect| point[0] >= rect[0]))
                .then_some(TabSection::Regular)
            })
    }

    pub(super) fn insertion_index(
        &self,
        tabs: &[Tab],
        dragged: &[TabId],
        section: TabSection,
        pointer: [f32; 2],
        previous: Option<TabDropTarget>,
    ) -> usize {
        if section == TabSection::Favourite
            && let Some(grid) = self.favourite_grid
        {
            return self.favourite_insertion_index(tabs, dragged, pointer, previous, grid);
        }
        let mut index = 0;
        for tab in tabs
            .iter()
            .filter(|tab| tab.section() == section && !dragged.contains(&tab.id()))
        {
            let Some(rect) = self.tab_rects.get(&tab.id()) else {
                continue;
            };
            let moving_down =
                previous.is_some_and(|target| target.section == section && index >= target.index);
            let moving_up =
                previous.is_some_and(|target| target.section == section && index < target.index);
            let before = if self.horizontal {
                let hysteresis = if moving_down {
                    TARGET_HYSTERESIS
                } else if moving_up {
                    -TARGET_HYSTERESIS
                } else {
                    0.0
                };
                pointer[0] > rect[0] + rect[2] / 2.0 + hysteresis
            } else {
                rect_before_pointer(*rect, pointer, moving_down, moving_up)
            };
            if before {
                index += 1;
            } else {
                break;
            }
        }
        index
    }

    fn favourite_insertion_index(
        &self,
        tabs: &[Tab],
        dragged: &[TabId],
        pointer: [f32; 2],
        previous: Option<TabDropTarget>,
        grid: [f32; 4],
    ) -> usize {
        let existing_count = tabs
            .iter()
            .filter(|tab| tab.section() == TabSection::Favourite && !dragged.contains(&tab.id()))
            .count();
        let final_count = existing_count + 1;
        let columns = favourite_columns(grid[2]);
        let closest = (0..=existing_count)
            .map(|index| {
                let rect = favourite_slot_rect(grid, index, final_count, columns);
                let center = [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
                let distance = (pointer[0] - center[0]).hypot(pointer[1] - center[1]);
                (index, distance)
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map_or(0, |(index, _)| index);

        if let Some(previous) = previous.filter(|target| {
            target.section == TabSection::Favourite && target.index <= existing_count
        }) {
            let rect = favourite_slot_rect(grid, previous.index, final_count, columns);
            let center = [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
            let previous_distance = (pointer[0] - center[0]).hypot(pointer[1] - center[1]);
            let closest_rect = favourite_slot_rect(grid, closest, final_count, columns);
            let closest_center = [
                closest_rect[0] + closest_rect[2] / 2.0,
                closest_rect[1] + closest_rect[3] / 2.0,
            ];
            let closest_distance =
                (pointer[0] - closest_center[0]).hypot(pointer[1] - closest_center[1]);
            if previous_distance <= closest_distance + 2.0 * TARGET_HYSTERESIS {
                return previous.index;
            }
        }
        closest
    }
}
