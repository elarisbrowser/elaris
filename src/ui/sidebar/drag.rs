// SPDX-License-Identifier: GPL-3.0-only

//! The short-lived pointer gesture and preview used to reorder tabs across sidebar sections.

use std::collections::HashMap;
use std::time::Duration;

use rune::Live;

use crate::tab::{Tab, TabId, TabSection};
use crate::ui::theme;

// Rune suppresses a click once travel exceeds four points on either axis.
const DRAG_THRESHOLD: f32 = 4.0;
const TARGET_HYSTERESIS: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabDropTarget {
    pub(crate) section: TabSection,
    pub(crate) index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewItem {
    Tab(TabId),
    SourceSlot(TabId),
    DropSlot(TabId, bool),
}

pub(crate) fn favourite_columns(content_width: f32) -> usize {
    let step = theme::FAVOURITE_TILE_MIN_WIDTH + theme::FAVOURITE_GAP;
    (((content_width + theme::FAVOURITE_GAP) / step).floor() as usize)
        .clamp(1, theme::FAVOURITE_COLUMNS)
}

pub(crate) fn favourite_row_ranges(count: usize, columns: usize) -> Vec<std::ops::Range<usize>> {
    let columns = columns.clamp(1, theme::FAVOURITE_COLUMNS);
    (0..count)
        .step_by(columns)
        .map(|start| start..(start + columns).min(count))
        .collect()
}

pub(crate) fn favourite_tile_width(content_width: f32, items_in_row: usize) -> f32 {
    if items_in_row == 0 {
        return 0.0;
    }
    let content_width = content_width.max(0.0);
    (content_width - theme::FAVOURITE_GAP * (items_in_row - 1) as f32).max(0.0)
        / items_in_row as f32
}

pub(crate) struct TabDragState {
    gesture: Option<Gesture>,
    settling: Option<Settling>,
    tab_rects: HashMap<TabId, [f32; 4]>,
    section_rects: [Option<[f32; 4]>; 3],
    favourite_grid: Option<[f32; 4]>,
    empty_favourite_area: Option<[f32; 4]>,
    placeholder: Option<(TabSection, [f32; 4])>,
    viewport: Option<[f32; 4]>,
    /// The scroll viewport publishes its current offset and range without rebuilding the view.
    scroll: Live<[f32; 2]>,
    last_scroll_tick: Option<f64>,
}

impl Default for TabDragState {
    fn default() -> Self {
        Self {
            gesture: None,
            settling: None,
            tab_rects: HashMap::new(),
            section_rects: [None; 3],
            favourite_grid: None,
            empty_favourite_area: None,
            placeholder: None,
            viewport: None,
            scroll: Live::new([0.0; 2]),
            last_scroll_tick: None,
        }
    }
}

enum Gesture {
    Pressed(Pressed),
    Dragging(Dragging),
}

struct Pressed {
    tab: TabId,
    source: TabDropTarget,
    order: Vec<(TabId, TabSection)>,
    start: [f32; 2],
    pointer: [f32; 2],
    grab: [f32; 2],
    origin: [f32; 2],
    size: [f32; 2],
}

struct Dragging {
    pressed: Pressed,
    target: Option<TabDropTarget>,
    valid: bool,
}

struct Settling {
    tab: TabId,
    source_section: TabSection,
    target_section: TabSection,
    target_index: usize,
    from: [f32; 2],
    to: [f32; 2],
    from_size: [f32; 2],
    to_size: [f32; 2],
    start_pending: bool,
    target_pending: bool,
    target_wait_started: Option<f64>,
    deadline: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct DragVisual {
    pub(crate) tab: TabId,
    pub(crate) section: TabSection,
    pub(crate) to: [f32; 2],
    pub(crate) to_size: [f32; 2],
    pub(crate) settling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveResult {
    NoChange,
    PreviewChanged,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Drop {
    pub(crate) tab: TabId,
    pub(crate) target: TabDropTarget,
}

impl TabDragState {
    pub(crate) fn set_viewport(&mut self, rect: [f32; 4]) {
        self.viewport = Some(rect);
    }

    pub(crate) fn set_section_rect(
        &mut self,
        section: TabSection,
        rect: [f32; 4],
        tabs: &[Tab],
    ) -> bool {
        if rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] >= 0.0 {
            self.section_rects[section_index(section)] = Some(rect);
            if section == TabSection::Favourite && rect[3] > 0.0 && !self.is_dragging() {
                self.empty_favourite_area = None;
            }
            if let Some(settling) = self.settling.as_mut()
                && settling.target_section == section
                && settling.target_pending
            {
                let item_count = tabs.iter().filter(|tab| tab.section() == section).count();
                let destination = if section == TabSection::Favourite {
                    favourite_slot_rect(
                        rect,
                        settling.target_index,
                        item_count,
                        favourite_columns(rect[2]),
                    )
                } else {
                    [
                        rect[0],
                        rect[1]
                            + settling.target_index as f32 * (theme::ROW_HEIGHT + theme::TAB_GAP),
                        rect[2],
                        theme::ROW_HEIGHT,
                    ]
                };
                settling.to = [destination[0], destination[1]];
                settling.to_size = [destination[2], destination[3]];
                settling.target_pending = false;
                settling.start_pending = false;
                settling.target_wait_started = None;
                return true;
            }
        }
        false
    }

    pub(crate) fn set_favourite_grid(&mut self, rect: [f32; 4]) {
        if rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] >= 0.0 {
            self.favourite_grid = Some(rect);
        }
    }

    pub(crate) fn set_empty_favourites_anchor(&mut self, scroll_viewport: [f32; 4]) {
        if !valid_rect(scroll_viewport) {
            return;
        }
        let grid_top = scroll_viewport[1];
        let drop_top = grid_top - theme::SIDEBAR_GAP - theme::FAVOURITE_TILE_HEIGHT;
        let area = [
            scroll_viewport[0],
            drop_top,
            scroll_viewport[2],
            grid_top - drop_top,
        ];
        self.empty_favourite_area = Some(area);
        self.favourite_grid = Some([area[0], grid_top, area[2], 0.0]);
        self.section_rects[section_index(TabSection::Favourite)] =
            Some([area[0], area[1], area[2], 0.0]);
    }

    pub(crate) fn record_tab_rect(
        &mut self,
        section: TabSection,
        tab: TabId,
        rect: [f32; 4],
    ) -> bool {
        if valid_rect(rect) {
            self.tab_rects.insert(tab, rect);
            if let Some(settling) = self.settling.as_mut()
                && settling.tab == tab
                && settling.target_section == section
                && settling.target_pending
            {
                settling.to = [rect[0], rect[1]];
                settling.to_size = [rect[2], rect[3]];
                settling.target_pending = false;
                settling.start_pending = false;
                settling.target_wait_started = None;
                return true;
            }
        }
        false
    }

    pub(crate) fn tab_rect(&self, tab: TabId) -> Option<[f32; 4]> {
        self.tab_rects.get(&tab).copied()
    }

    pub(crate) fn record_placeholder(&mut self, section: TabSection, rect: [f32; 4]) {
        if valid_rect(rect) {
            self.placeholder = Some((section, rect));
        }
    }

    pub(crate) fn scroll_live(&self) -> Live<[f32; 2]> {
        self.scroll.clone()
    }

    pub(crate) fn press(
        &mut self,
        tab: TabId,
        tabs: &[Tab],
        pointer: [f32; 2],
        grab: [f32; 2],
        size: [f32; 2],
    ) {
        self.gesture = None;
        self.last_scroll_tick = None;
        let Some(tab_index) = tabs.iter().position(|candidate| candidate.id() == tab) else {
            return;
        };
        let section = tabs[tab_index].section();
        let index = tabs[..tab_index]
            .iter()
            .filter(|candidate| candidate.section() == section)
            .count();
        let origin = [pointer[0] - grab[0], pointer[1] - grab[1]];
        self.gesture = Some(Gesture::Pressed(Pressed {
            tab,
            source: TabDropTarget { section, index },
            order: tabs
                .iter()
                .map(|candidate| (candidate.id(), candidate.section()))
                .collect(),
            start: pointer,
            pointer,
            grab,
            origin,
            size,
        }));
    }

    /// Updates the preview while preserving browser membership and order until release.
    pub(crate) fn move_to(&mut self, tabs: &[Tab], pointer: [f32; 2]) -> MoveResult {
        let Some(gesture) = self.gesture.take() else {
            return MoveResult::NoChange;
        };
        match gesture {
            Gesture::Pressed(mut pressed) => {
                if !self.matches_order(&pressed.order, tabs) {
                    return MoveResult::Cancelled;
                }
                pressed.pointer = pointer;
                if !past_drag_threshold(pressed.start, pointer) {
                    self.gesture = Some(Gesture::Pressed(pressed));
                    return MoveResult::NoChange;
                }
                let mut dragging = Dragging {
                    pressed,
                    target: None,
                    valid: false,
                };
                let changed = self.update_target(&mut dragging, tabs);
                self.gesture = Some(Gesture::Dragging(dragging));
                if changed {
                    MoveResult::PreviewChanged
                } else {
                    MoveResult::NoChange
                }
            }
            Gesture::Dragging(mut dragging) => {
                if !self.matches_order(&dragging.pressed.order, tabs) {
                    return MoveResult::Cancelled;
                }
                dragging.pressed.pointer = pointer;
                let changed = self.update_target(&mut dragging, tabs);
                self.gesture = Some(Gesture::Dragging(dragging));
                if changed {
                    MoveResult::PreviewChanged
                } else {
                    MoveResult::NoChange
                }
            }
        }
    }

    pub(crate) fn refresh_target(&mut self, tabs: &[Tab]) -> MoveResult {
        let Some(Gesture::Dragging(mut dragging)) = self.gesture.take() else {
            return MoveResult::NoChange;
        };
        if !self.matches_order(&dragging.pressed.order, tabs) {
            return MoveResult::Cancelled;
        }
        let changed = self.update_target(&mut dragging, tabs);
        self.gesture = Some(Gesture::Dragging(dragging));
        if changed {
            MoveResult::PreviewChanged
        } else {
            MoveResult::NoChange
        }
    }

    pub(crate) fn end(&mut self, tabs: &[Tab], cancelled: bool) -> (bool, Option<Drop>) {
        self.last_scroll_tick = None;
        let Some(gesture) = self.gesture.take() else {
            return (false, None);
        };
        let Gesture::Dragging(dragging) = gesture else {
            return (false, None);
        };
        let current = [
            dragging.pressed.pointer[0] - dragging.pressed.grab[0],
            dragging.pressed.pointer[1] - dragging.pressed.grab[1],
        ];
        let is_current = self.matches_order(&dragging.pressed.order, tabs)
            && tabs.iter().any(|tab| tab.id() == dragging.pressed.tab);
        let target = dragging.target.filter(|target| {
            target.index
                <= tabs
                    .iter()
                    .filter(|tab| tab.section() == target.section)
                    .count()
        });
        let placeholder = target.and_then(|target| {
            self.placeholder
                .filter(|(section, _)| *section == target.section)
        });
        if let Some(target) = target.filter(|_| !cancelled && dragging.valid && is_current) {
            let cross_section = dragging.pressed.source.section != target.section;
            let rect = placeholder.map_or_else(
                || self.target_rect(tabs, target, dragging.pressed.source.section),
                |(_, rect)| rect,
            );
            let to = [rect[0], rect[1]];
            self.settling = Some(Settling {
                tab: dragging.pressed.tab,
                source_section: dragging.pressed.source.section,
                target_section: target.section,
                target_index: target.index,
                from: current,
                to,
                from_size: dragging.pressed.size,
                to_size: [rect[2], rect[3]],
                start_pending: cross_section,
                target_pending: cross_section,
                target_wait_started: None,
                deadline: None,
            });
            return (
                true,
                Some(Drop {
                    tab: dragging.pressed.tab,
                    target,
                }),
            );
        }

        self.settling = Some(Settling {
            tab: dragging.pressed.tab,
            source_section: dragging.pressed.source.section,
            target_section: dragging.pressed.source.section,
            target_index: dragging.pressed.source.index,
            from: current,
            to: dragging.pressed.origin,
            from_size: dragging.pressed.size,
            to_size: dragging.pressed.size,
            start_pending: false,
            target_pending: false,
            target_wait_started: None,
            deadline: None,
        });
        (true, None)
    }

    pub(crate) fn cancel(&mut self) -> bool {
        self.last_scroll_tick = None;
        let Some(gesture) = self.gesture.take() else {
            let changed = self.settling.is_some();
            self.settling = None;
            return changed;
        };
        match gesture {
            Gesture::Pressed(_) => false,
            Gesture::Dragging(dragging) => {
                self.settling = Some(Settling {
                    tab: dragging.pressed.tab,
                    source_section: dragging.pressed.source.section,
                    target_section: dragging.pressed.source.section,
                    target_index: dragging.pressed.source.index,
                    from: [
                        dragging.pressed.pointer[0] - dragging.pressed.grab[0],
                        dragging.pressed.pointer[1] - dragging.pressed.grab[1],
                    ],
                    to: dragging.pressed.origin,
                    from_size: dragging.pressed.size,
                    to_size: dragging.pressed.size,
                    start_pending: false,
                    target_pending: false,
                    target_wait_started: None,
                    deadline: None,
                });
                true
            }
        }
    }

    pub(crate) fn is_dragging(&self) -> bool {
        matches!(self.gesture, Some(Gesture::Dragging(_)))
    }

    pub(crate) fn is_hidden(&self, tab: TabId) -> bool {
        matches!(&self.gesture, Some(Gesture::Dragging(dragging)) if dragging.pressed.tab == tab)
            || self
                .settling
                .as_ref()
                .is_some_and(|settling| settling.tab == tab)
    }

    /// The displayed order for a sidebar section, with distinct source and destination slots.
    pub(crate) fn preview_items(&self, tabs: &[Tab], section: TabSection) -> Vec<PreviewItem> {
        let mut items: Vec<_> = tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .map(|tab| PreviewItem::Tab(tab.id()))
            .collect();
        let Some(Gesture::Dragging(dragging)) = &self.gesture else {
            return items;
        };
        if dragging.pressed.source.section == section {
            items.retain(|item| *item != PreviewItem::Tab(dragging.pressed.tab));
        }
        let target = dragging.target.filter(|_| dragging.valid);
        if dragging.pressed.source.section == section {
            let moving_within_section = target.is_some_and(|target| target.section == section);
            let slot = if moving_within_section {
                target.expect("checked target").index
            } else {
                dragging.pressed.source.index
            };
            let item = if moving_within_section {
                PreviewItem::DropSlot(dragging.pressed.tab, true)
            } else {
                PreviewItem::SourceSlot(dragging.pressed.tab)
            };
            items.insert(slot.min(items.len()), item);
        } else if let Some(target) = target.filter(|target| target.section == section) {
            items.insert(
                target.index.min(items.len()),
                PreviewItem::DropSlot(dragging.pressed.tab, false),
            );
        }
        items
    }

    pub(crate) fn visual(&self) -> Option<DragVisual> {
        match &self.gesture {
            Some(Gesture::Dragging(dragging)) => Some(DragVisual {
                tab: dragging.pressed.tab,
                section: dragging.pressed.source.section,
                to: [
                    dragging.pressed.pointer[0] - dragging.pressed.grab[0],
                    dragging.pressed.pointer[1] - dragging.pressed.grab[1],
                ],
                to_size: dragging.pressed.size,
                settling: false,
            }),
            _ => self.settling.as_ref().map(|settling| DragVisual {
                tab: settling.tab,
                section: if settling.start_pending {
                    settling.source_section
                } else {
                    settling.target_section
                },
                to: if settling.start_pending {
                    settling.from
                } else {
                    settling.to
                },
                to_size: if settling.start_pending {
                    settling.from_size
                } else {
                    settling.to_size
                },
                settling: !settling.start_pending,
            }),
        }
    }

    /// Starts the same shared-element settle used by a pointer drop, for a menu action.
    pub(crate) fn transition(
        &mut self,
        tab: TabId,
        source_section: TabSection,
        target_section: TabSection,
        target_index: usize,
        from: [f32; 4],
        to: [f32; 4],
    ) {
        if !valid_rect(from) || !valid_rect(to) {
            return;
        }
        self.settling = Some(Settling {
            tab,
            source_section,
            target_section,
            target_index,
            from: [from[0], from[1]],
            to: [to[0], to[1]],
            from_size: [from[2], from[3]],
            to_size: [to[2], to[3]],
            start_pending: true,
            target_pending: true,
            target_wait_started: None,
            deadline: None,
        });
    }

    /// Scrolls at the list edge by a speed that grows with the pointer's proximity to it.
    pub(crate) fn scroll_target(&mut self, now: f64) -> Option<f32> {
        let Some(Gesture::Dragging(dragging)) = &self.gesture else {
            self.last_scroll_tick = None;
            return None;
        };
        let [_, top, _, height] = self.viewport?;
        let pointer_y = dragging.pressed.pointer[1];
        let edge = theme::TAB_DRAG_SCROLL_EDGE;
        let top_pressure = ((top + edge - pointer_y) / edge).clamp(0.0, 1.0);
        let bottom = top + height;
        let bottom_pressure = ((pointer_y - (bottom - edge)) / edge).clamp(0.0, 1.0);
        let pressure = if top_pressure > 0.0 {
            -top_pressure * top_pressure
        } else {
            bottom_pressure * bottom_pressure
        };
        if pressure == 0.0 {
            self.last_scroll_tick = None;
            return None;
        }
        let [offset, max] = self.scroll.get();
        if (pressure < 0.0 && offset <= 0.0) || (pressure > 0.0 && offset >= max) {
            self.last_scroll_tick = None;
            return None;
        }
        let elapsed = self
            .last_scroll_tick
            .replace(now)
            .map_or(1.0 / 60.0, |last| (now - last).clamp(0.0, 0.05));
        let y = (offset + pressure * theme::TAB_DRAG_SCROLL_MAX * elapsed as f32).clamp(0.0, max);
        ((y - offset).abs() > 0.01).then_some(y)
    }

    /// Keeps the shared transition above the sidebar until Rune's drop spring has landed.
    pub(crate) fn settle_tick(&mut self, now: f64) -> bool {
        let Some(settling) = &mut self.settling else {
            return false;
        };
        if settling.start_pending {
            if settling.target_pending {
                let since = settling.target_wait_started.get_or_insert(now);
                if now - *since < 0.032 {
                    return false;
                }
                settling.target_pending = false;
            }
            settling.start_pending = false;
            settling.target_wait_started = None;
            return true;
        }
        let Some(deadline) = settling.deadline else {
            settling.deadline = Some(now + f64::from(theme::TAB_DROP_SETTLE_SECONDS));
            return false;
        };
        if now < deadline {
            return false;
        }
        self.settling = None;
        true
    }

    pub(crate) fn settle_wake(&self) -> Option<Duration> {
        self.settling.as_ref().and_then(|settling| {
            (settling.target_pending || settling.deadline.is_some())
                .then_some(Duration::from_millis(16))
        })
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
            return [
                rect[0],
                rect[1] + rect[3] + theme::TAB_GAP,
                rect[2],
                rect[3],
            ];
        }
        let section = self.section_rects[section_index(target.section)].unwrap_or([0.0; 4]);
        [
            section[0],
            section[1] + section[3] - theme::ROW_HEIGHT,
            section[2],
            theme::ROW_HEIGHT,
        ]
    }

    fn update_target(&self, dragging: &mut Dragging, tabs: &[Tab]) -> bool {
        let previous = (dragging.target, dragging.valid);
        let pointer = dragging.pressed.pointer;
        let Some(section) = self.section_at(pointer) else {
            dragging.target = None;
            dragging.valid = false;
            return previous != (dragging.target, dragging.valid);
        };
        let index = self.insertion_index(
            tabs,
            dragging.pressed.tab,
            section,
            pointer,
            dragging.target,
        );
        dragging.target = Some(TabDropTarget { section, index });
        dragging.valid = true;
        previous != (dragging.target, dragging.valid)
    }

    fn section_at(&self, point: [f32; 2]) -> Option<TabSection> {
        if self
            .empty_favourite_area
            .is_some_and(|area| contains(area, point) && point[1] < area[1] + area[3])
        {
            return Some(TabSection::Favourite);
        }
        TabSection::ORDER.into_iter().find(|section| {
            self.section_rects[section_index(*section)].is_some_and(|rect| contains(rect, point))
        })
    }

    fn insertion_index(
        &self,
        tabs: &[Tab],
        dragged: TabId,
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
            .filter(|tab| tab.section() == section && tab.id() != dragged)
        {
            let Some(rect) = self.tab_rects.get(&tab.id()) else {
                continue;
            };
            let moving_down =
                previous.is_some_and(|target| target.section == section && index >= target.index);
            let moving_up =
                previous.is_some_and(|target| target.section == section && index < target.index);
            if rect_before_pointer(*rect, pointer, moving_down, moving_up) {
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
        dragged: TabId,
        pointer: [f32; 2],
        previous: Option<TabDropTarget>,
        grid: [f32; 4],
    ) -> usize {
        let existing_count = tabs
            .iter()
            .filter(|tab| tab.section() == TabSection::Favourite && tab.id() != dragged)
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

    fn matches_order(&self, expected: &[(TabId, TabSection)], tabs: &[Tab]) -> bool {
        expected.len() == tabs.len()
            && expected
                .iter()
                .copied()
                .eq(tabs.iter().map(|tab| (tab.id(), tab.section())))
    }
}

fn section_index(section: TabSection) -> usize {
    match section {
        TabSection::Favourite => 0,
        TabSection::Pinned => 1,
        TabSection::Regular => 2,
    }
}

fn favourite_slot_rect(
    grid: [f32; 4],
    index: usize,
    item_count: usize,
    columns: usize,
) -> [f32; 4] {
    let columns = columns.clamp(1, theme::FAVOURITE_COLUMNS);
    let row_index = index / columns;
    let column_index = index % columns;
    let row_start = row_index * columns;
    let row_items = item_count.saturating_sub(row_start).min(columns).max(1);
    let width = favourite_tile_width(grid[2], row_items);
    let x = grid[0] + column_index as f32 * (width + theme::FAVOURITE_GAP);
    let y = grid[1] + row_index as f32 * (theme::FAVOURITE_TILE_HEIGHT + theme::FAVOURITE_GAP);
    [x, y, width, theme::FAVOURITE_TILE_HEIGHT]
}

fn valid_rect(rect: [f32; 4]) -> bool {
    rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] > 0.0
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[1] >= rect[1]
        && point[0] <= rect[0] + rect[2]
        && point[1] <= rect[1] + rect[3]
}

fn rect_before_pointer(
    rect: [f32; 4],
    pointer: [f32; 2],
    moving_down: bool,
    moving_up: bool,
) -> bool {
    let center = [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
    let hysteresis = if moving_down {
        TARGET_HYSTERESIS
    } else if moving_up {
        -TARGET_HYSTERESIS
    } else {
        0.0
    };
    pointer[1] > center[1] + hysteresis
        || ((pointer[1] - center[1]).abs() <= hysteresis.abs()
            && pointer[0] > center[0] + hysteresis)
}

fn past_drag_threshold(start: [f32; 2], current: [f32; 2]) -> bool {
    let (dx, dy) = ((current[0] - start[0]).abs(), (current[1] - start[1]).abs());
    dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(count: usize) -> Vec<Tab> {
        (0..count)
            .map(|index| Tab::new(TabId::from_raw(index as u64 + 1), "about:blank".into()))
            .collect()
    }

    fn begin(state: &mut TabDragState, tabs: &[Tab], index: usize) {
        state.set_section_rect(TabSection::Regular, [0.0, 100.0, 220.0, 300.0], tabs);
        for (row, tab) in tabs.iter().enumerate() {
            state.record_tab_rect(
                TabSection::Regular,
                tab.id(),
                [0.0, 100.0 + row as f32 * 38.0, 220.0, 34.0],
            );
        }
        state.set_viewport([0.0, 100.0, 220.0, 300.0]);
        let y = 100.0 + index as f32 * 38.0 + 17.0;
        state.press(
            tabs[index].id(),
            tabs,
            [20.0, y],
            [10.0, 17.0],
            [200.0, 34.0],
        );
    }

    #[test]
    fn a_click_does_not_start_a_preview() {
        let tabs = tabs(3);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 1);
        assert_eq!(state.move_to(&tabs, [24.0, 159.0]), MoveResult::NoChange);
        assert!(!state.is_dragging());
        assert_eq!(state.end(&tabs, false), (false, None));
    }

    #[test]
    fn crossing_the_threshold_starts_the_drag_without_changing_browser_order() {
        let tabs = tabs(3);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 1);
        assert_eq!(state.move_to(&tabs, [20.0, 159.0]), MoveResult::NoChange);
        assert!(!state.is_dragging());
        assert_ne!(state.move_to(&tabs, [20.0, 159.1]), MoveResult::Cancelled);
        assert!(state.is_dragging());
        assert_eq!(
            state.preview_items(&tabs, TabSection::Regular),
            vec![
                PreviewItem::Tab(tabs[0].id()),
                PreviewItem::DropSlot(tabs[1].id(), true),
                PreviewItem::Tab(tabs[2].id()),
            ]
        );
    }

    #[test]
    fn a_single_tab_can_move_to_another_section() {
        let tabs = tabs(1);
        let mut state = TabDragState::default();
        state.set_section_rect(TabSection::Regular, [0.0, 100.0, 220.0, 100.0], &tabs);
        state.set_section_rect(TabSection::Favourite, [0.0, 20.0, 220.0, 70.0], &tabs);
        state.set_favourite_grid([0.0, 35.0, 220.0, 40.0]);
        state.record_tab_rect(TabSection::Regular, tabs[0].id(), [0.0, 100.0, 220.0, 34.0]);
        state.press(
            tabs[0].id(),
            &tabs,
            [20.0, 117.0],
            [10.0, 17.0],
            [200.0, 34.0],
        );
        assert_eq!(
            state.move_to(&tabs, [20.0, 60.0]),
            MoveResult::PreviewChanged
        );
        assert_eq!(
            state.preview_items(&tabs, TabSection::Regular),
            vec![PreviewItem::SourceSlot(tabs[0].id())]
        );
        assert_eq!(
            state.preview_items(&tabs, TabSection::Favourite),
            vec![PreviewItem::DropSlot(tabs[0].id(), false)]
        );
    }

    #[test]
    fn crossing_sections_previews_a_new_slot_without_changing_browser_order() {
        let tabs = tabs(3);
        let mut state = TabDragState::default();
        state.set_section_rect(TabSection::Regular, [0.0, 130.0, 220.0, 220.0], &tabs);
        state.set_section_rect(TabSection::Favourite, [0.0, 20.0, 220.0, 80.0], &tabs);
        state.set_favourite_grid([0.0, 35.0, 220.0, 40.0]);
        for (index, tab) in tabs.iter().enumerate() {
            state.record_tab_rect(
                TabSection::Regular,
                tab.id(),
                [0.0, 130.0 + index as f32 * 38.0, 220.0, 34.0],
            );
        }
        state.press(
            tabs[1].id(),
            &tabs,
            [20.0, 147.0],
            [10.0, 17.0],
            [200.0, 34.0],
        );
        state.move_to(&tabs, [20.0, 60.0]);
        assert_eq!(
            state.preview_items(&tabs, TabSection::Regular),
            vec![
                PreviewItem::Tab(tabs[0].id()),
                PreviewItem::SourceSlot(tabs[1].id()),
                PreviewItem::Tab(tabs[2].id())
            ]
        );
        assert_eq!(
            state.preview_items(&tabs, TabSection::Favourite),
            vec![PreviewItem::DropSlot(tabs[1].id(), false)]
        );
        assert_eq!(
            tabs.iter().map(Tab::id).collect::<Vec<_>>(),
            [TabId::from_raw(1), TabId::from_raw(2), TabId::from_raw(3),]
        );
    }

    #[test]
    fn scroll_offset_participates_in_edge_autoscroll() {
        let tabs = tabs(4);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 0);
        state.scroll.set([100.0, 400.0]);
        state.move_to(&tabs, [20.0, 123.0]);
        assert!(state.is_dragging());
        assert!(state.scroll_target(1.0).is_some());
    }

    #[test]
    fn cancellation_or_a_changed_tab_order_cannot_commit_a_drop() {
        let mut tabs = tabs(3);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 1);
        state.move_to(&tabs, [20.0, 195.0]);
        assert_eq!(state.end(&tabs, true), (true, None));
        assert!(!state.is_dragging());
        assert!(
            state.visual().is_some(),
            "cancel settles back to the source"
        );

        state.cancel();
        begin(&mut state, &tabs, 1);
        state.move_to(&tabs, [20.0, 195.0]);
        tabs.remove(1);
        assert_eq!(state.move_to(&tabs, [20.0, 160.0]), MoveResult::Cancelled);
        assert_eq!(state.end(&tabs, false), (false, None));
    }

    #[test]
    fn favourite_rows_wrap_at_four_and_fill_the_last_row() {
        let expected = [
            vec![],
            vec![0..1],
            vec![0..2],
            vec![0..3],
            vec![0..4],
            vec![0..4, 4..5],
            vec![0..4, 4..6],
            vec![0..4, 4..7],
            vec![0..4, 4..8],
            vec![0..4, 4..8, 8..9],
        ];
        for (count, ranges) in expected.iter().enumerate() {
            assert_eq!(favourite_row_ranges(count, 4), *ranges, "count {count}");
        }
    }

    #[test]
    fn favourite_tile_widths_distribute_available_space_without_overflow() {
        for content_width in [40.0, 96.0, 196.0, 360.0] {
            for count in 1..=4 {
                let tile_width = favourite_tile_width(content_width, count);
                let row_width =
                    tile_width * count as f32 + theme::FAVOURITE_GAP * (count - 1) as f32;
                assert!(row_width <= content_width + f32::EPSILON);
                if content_width
                    >= theme::FAVOURITE_TILE_MIN_WIDTH * count as f32
                        + theme::FAVOURITE_GAP * (count - 1) as f32
                {
                    assert!((row_width - content_width).abs() < f32::EPSILON);
                }
            }
        }
        assert_eq!(favourite_columns(200.0), 4);
        assert_eq!(favourite_columns(180.0), 3);
        assert_eq!(favourite_tile_width(24.0, 1), 24.0);
    }

    #[test]
    fn favourite_destination_rect_uses_partial_row_width_and_index() {
        let tabs = tabs(5);
        let mut state = TabDragState::default();
        state.set_favourite_grid([10.0, 20.0, 200.0, 96.0]);
        let existing: Vec<_> = tabs
            .into_iter()
            .enumerate()
            .map(|(index, mut tab)| {
                if index < 4 {
                    tab.section = TabSection::Favourite;
                }
                tab
            })
            .collect();
        let target = state.target_rect(
            &existing,
            TabDropTarget {
                section: TabSection::Favourite,
                index: 4,
            },
            TabSection::Regular,
        );
        assert_eq!(target, [10.0, 76.0, 200.0, 48.0]);
    }

    #[test]
    fn favourite_insertion_uses_responsive_grid_slots_across_rows() {
        let mut tabs = tabs(6);
        for tab in &mut tabs[..5] {
            tab.section = TabSection::Favourite;
        }
        let mut state = TabDragState::default();
        state.set_section_rect(TabSection::Favourite, [10.0, 20.0, 200.0, 104.0], &tabs);
        state.set_favourite_grid([10.0, 20.0, 200.0, 104.0]);
        for (index, tab) in tabs[..5].iter().enumerate() {
            let row = index / 4;
            let col = index % 4;
            let width = if row == 0 { 44.0 } else { 200.0 };
            let x = if row == 0 {
                10.0 + col as f32 * 52.0
            } else {
                10.0
            };
            let y = 20.0 + row as f32 * 56.0;
            state.record_tab_rect(TabSection::Favourite, tab.id(), [x, y, width, 48.0]);
        }
        let source = tabs[5].id();
        state.press(source, &tabs, [100.0, 160.0], [10.0, 17.0], [200.0, 34.0]);
        state.move_to(&tabs, [60.0, 100.0]);
        assert_eq!(
            state.gesture.as_ref().and_then(|gesture| match gesture {
                Gesture::Dragging(dragging) => dragging.target,
                Gesture::Pressed(_) => None,
            }),
            Some(TabDropTarget {
                section: TabSection::Favourite,
                index: 4,
            })
        );
    }

    #[test]
    fn menu_transition_uses_the_committed_destination_layout() {
        let tab = tabs(1).remove(0);
        let mut state = TabDragState::default();
        state.transition(
            tab.id(),
            TabSection::Regular,
            TabSection::Favourite,
            0,
            [12.0, 180.0, 200.0, 34.0],
            [12.0, 124.0, 200.0, 48.0],
        );

        let initial = state.visual().unwrap();
        assert_eq!(initial.to, [12.0, 180.0]);
        assert_eq!(initial.to_size, [200.0, 34.0]);
        assert!(!state.settle_tick(0.0));

        assert!(
            state.record_tab_rect(TabSection::Favourite, tab.id(), [16.0, 196.0, 184.0, 48.0],)
        );
        let moving = state.visual().unwrap();
        assert_eq!(moving.section, TabSection::Favourite);
        assert_eq!(moving.to, [16.0, 196.0]);
        assert_eq!(moving.to_size, [184.0, 48.0]);
    }

    #[test]
    fn cross_section_drop_waits_for_the_destination_after_layout_reflow() {
        let tabs = tabs(1);
        let mut state = TabDragState::default();
        state.set_section_rect(TabSection::Regular, [12.0, 120.0, 196.0, 100.0], &tabs);
        state.set_section_rect(TabSection::Favourite, [12.0, 64.0, 196.0, 48.0], &tabs);
        state.set_favourite_grid([12.0, 64.0, 196.0, 48.0]);
        state.record_tab_rect(
            TabSection::Regular,
            tabs[0].id(),
            [12.0, 120.0, 196.0, 34.0],
        );
        state.press(
            tabs[0].id(),
            &tabs,
            [32.0, 137.0],
            [12.0, 17.0],
            [196.0, 34.0],
        );
        state.move_to(&tabs, [32.0, 92.0]);
        state.record_placeholder(TabSection::Favourite, [12.0, 64.0, 196.0, 48.0]);

        let (dragged, drop) = state.end(&tabs, false);
        assert!(dragged);
        assert_eq!(drop.unwrap().target.section, TabSection::Favourite);
        let waiting = state.visual().unwrap();
        assert_eq!(waiting.section, TabSection::Regular);
        assert_eq!(waiting.to, [20.0, 75.0]);
        assert!(!state.settle_tick(0.0));

        assert!(state.record_tab_rect(
            TabSection::Favourite,
            tabs[0].id(),
            [12.0, 120.0, 196.0, 48.0],
        ));
        let settled = state.visual().unwrap();
        assert_eq!(settled.section, TabSection::Favourite);
        assert_eq!(settled.to, [12.0, 120.0]);
    }

    #[test]
    fn empty_favourite_anchor_matches_the_grid_position_after_it_appears() {
        let tabs = tabs(1);
        let mut state = TabDragState::default();
        state.set_empty_favourites_anchor([12.0, 150.0, 196.0, 300.0]);
        assert_eq!(state.favourite_grid, Some([12.0, 150.0, 196.0, 0.0]));
        let target = state.target_rect(
            &tabs,
            TabDropTarget {
                section: TabSection::Favourite,
                index: 0,
            },
            TabSection::Regular,
        );
        assert_eq!(target, [12.0, 150.0, 196.0, 48.0]);
    }

    #[test]
    fn empty_favourite_preview_remains_valid_when_the_grid_becomes_visible() {
        let tabs = tabs(1);
        let mut state = TabDragState::default();
        state.set_empty_favourites_anchor([12.0, 150.0, 196.0, 300.0]);
        state.set_section_rect(TabSection::Regular, [12.0, 150.0, 196.0, 100.0], &tabs);
        state.record_tab_rect(
            TabSection::Regular,
            tabs[0].id(),
            [12.0, 150.0, 196.0, 34.0],
        );
        state.press(
            tabs[0].id(),
            &tabs,
            [30.0, 167.0],
            [10.0, 17.0],
            [196.0, 34.0],
        );
        assert_eq!(
            state.move_to(&tabs, [30.0, 130.0]),
            MoveResult::PreviewChanged
        );
        assert!(state.empty_favourite_area.is_some());

        state.set_section_rect(TabSection::Favourite, [12.0, 150.0, 196.0, 48.0], &tabs);
        assert_eq!(state.refresh_target(&tabs), MoveResult::NoChange);
        assert_eq!(
            state.gesture.as_ref().and_then(|gesture| match gesture {
                Gesture::Dragging(dragging) => dragging.target,
                Gesture::Pressed(_) => None,
            }),
            Some(TabDropTarget {
                section: TabSection::Favourite,
                index: 0,
            })
        );
    }
}
