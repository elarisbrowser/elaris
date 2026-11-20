// SPDX-License-Identifier: GPL-3.0-only

//! The short-lived pointer gesture and preview used to reorder tabs across sidebar sections.

use std::collections::HashMap;
use std::time::Duration;

use rune::Live;

use crate::tab::{Tab, TabId, TabSection};
use crate::ui::theme;

mod geometry;
#[cfg(test)]
mod tests;

use geometry::{
    contains, favourite_slot_rect, past_drag_threshold, rect_before_pointer, section_index,
    valid_rect,
};
pub(crate) use geometry::{favourite_columns, favourite_row_ranges, favourite_tile_width};

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

pub(crate) struct TabDragState {
    horizontal: bool,
    gesture: Option<Gesture>,
    incoming: Option<(TabId, TabDropTarget)>,
    outside: bool,
    incoming_visual: Option<DragVisual>,
    settling: Option<Settling>,
    tab_rects: HashMap<TabId, [f32; 4]>,
    section_rects: [Option<[f32; 4]>; TabSection::ORDER.len()],
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
            horizontal: false,
            gesture: None,
            incoming: None,
            outside: false,
            incoming_visual: None,
            settling: None,
            tab_rects: HashMap::new(),
            section_rects: [None; TabSection::ORDER.len()],
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
    members: Vec<TabId>,
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
    incoming: bool,
    deadline: Option<f64>,
}

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Drop {
    pub(crate) tabs: Vec<TabId>,
    pub(crate) target: TabDropTarget,
}

impl TabDragState {
    pub(crate) fn set_viewport(&mut self, rect: [f32; 4]) {
        self.viewport = Some(rect);
    }

    pub(crate) fn set_horizontal_viewport(&mut self, rect: [f32; 4]) {
        self.horizontal = true;
        self.set_viewport(rect);
    }

    pub(crate) fn set_section_rect(
        &mut self,
        section: TabSection,
        rect: [f32; 4],
        tabs: &[Tab],
    ) -> bool {
        if rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] >= 0.0 {
            self.section_rects[section_index(section)] = Some(rect);
            if section == TabSection::Favourite
                && rect[3] > 0.0
                && !self.is_dragging()
                && self.incoming.is_none()
            {
                self.empty_favourite_area = None;
            }
            if !self.horizontal
                && let Some(settling) = self.settling.as_mut()
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

    #[cfg(test)]
    pub(crate) fn press(
        &mut self,
        tab: TabId,
        tabs: &[Tab],
        pointer: [f32; 2],
        grab: [f32; 2],
        size: [f32; 2],
    ) {
        self.press_members(tab, vec![tab], tabs, pointer, grab, size);
    }

    pub(crate) fn press_members(
        &mut self,
        tab: TabId,
        members: Vec<TabId>,
        tabs: &[Tab],
        pointer: [f32; 2],
        grab: [f32; 2],
        size: [f32; 2],
    ) {
        self.gesture = None;
        self.outside = false;
        self.last_scroll_tick = None;
        let Some(tab_index) = tabs.iter().position(|candidate| candidate.id() == tab) else {
            return;
        };
        let section = tabs[tab_index].section();
        let first_member_index = tabs
            .iter()
            .position(|candidate| members.contains(&candidate.id()))
            .unwrap_or(tab_index);
        let index = tabs[..first_member_index]
            .iter()
            .filter(|candidate| candidate.section() == section)
            .count();
        let origin = [pointer[0] - grab[0], pointer[1] - grab[1]];
        self.gesture = Some(Gesture::Pressed(Pressed {
            tab,
            members,
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
                    .saturating_sub(
                        dragging
                            .pressed
                            .members
                            .iter()
                            .filter(|id| {
                                tabs.iter()
                                    .any(|tab| tab.id() == **id && tab.section() == target.section)
                            })
                            .count(),
                    )
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
                incoming: false,
                deadline: None,
            });
            return (
                true,
                Some(Drop {
                    tabs: dragging.pressed.members.clone(),
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
            incoming: false,
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
                    incoming: false,
                    deadline: None,
                });
                true
            }
        }
    }

    pub(crate) fn has_preview(&self) -> bool {
        self.is_dragging() || self.incoming.is_some()
    }

    pub(crate) fn is_dragging(&self) -> bool {
        matches!(self.gesture, Some(Gesture::Dragging(_)))
    }

    pub(crate) fn is_hidden(&self, tab: TabId) -> bool {
        matches!(&self.gesture, Some(Gesture::Dragging(dragging)) if dragging.pressed.members.contains(&tab))
            || self
                .settling
                .as_ref()
                .is_some_and(|settling| settling.tab == tab)
    }

    pub(crate) fn set_outside(&mut self, outside: bool) -> bool {
        let changed = self.outside != outside;
        self.outside = outside;
        changed
    }

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

    pub(crate) fn set_incoming(&mut self, incoming: Option<(TabId, TabDropTarget)>) -> bool {
        let changed = self.incoming != incoming;
        self.incoming = incoming;
        changed
    }

    pub(crate) fn set_incoming_visual(&mut self, visual: Option<DragVisual>) -> bool {
        let changed = self.incoming_visual != visual;
        self.incoming_visual = visual;
        changed
    }

    /// The displayed order for a sidebar section, with distinct source and destination slots.
    pub(crate) fn preview_items(&self, tabs: &[Tab], section: TabSection) -> Vec<PreviewItem> {
        let mut items: Vec<_> = tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .map(|tab| PreviewItem::Tab(tab.id()))
            .collect();
        let Some(Gesture::Dragging(dragging)) = &self.gesture else {
            if self.outside
                && let Some(settling) = &self.settling
            {
                items.retain(|item| *item != PreviewItem::Tab(settling.tab));
            }
            if let Some((id, target)) = self
                .incoming
                .filter(|(_, target)| target.section == section)
            {
                items.insert(
                    target.index.min(items.len()),
                    PreviewItem::DropSlot(id, false),
                );
            }
            return items;
        };
        if dragging.pressed.source.section == section {
            items.retain(|item| {
                !matches!(item, PreviewItem::Tab(id) if dragging.pressed.members.contains(id))
            });
        }
        if self.outside {
            return items;
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

    #[cfg(target_os = "macos")]
    pub(crate) fn single_drag(&self) -> Option<(DragVisual, [f32; 2])> {
        let Some(Gesture::Dragging(drag)) = &self.gesture else {
            return None;
        };
        (drag.pressed.members.len() == 1).then(|| (self.visual().unwrap(), drag.pressed.grab))
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

    pub(crate) fn is_incoming_settle(&self, tab: TabId) -> bool {
        self.settling
            .as_ref()
            .is_some_and(|settling| settling.tab == tab && settling.incoming)
    }

    pub(crate) fn settle_incoming(&mut self, visual: DragVisual, target: TabDropTarget) {
        let from = [
            visual.to[0],
            visual.to[1],
            visual.to_size[0],
            visual.to_size[1],
        ];
        let to = self
            .placeholder
            .filter(|(section, _)| *section == target.section)
            .map_or(from, |(_, rect)| rect);
        self.transition(
            visual.tab,
            visual.section,
            target.section,
            target.index,
            from,
            to,
        );
        if let Some(settling) = self.settling.as_mut() {
            settling.incoming = true;
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
            incoming: false,
            deadline: None,
        });
    }

    /// Scrolls at the list edge by a speed that grows with the pointer's proximity to it.
    pub(crate) fn scroll_target(&mut self, now: f64) -> Option<f32> {
        let Some(Gesture::Dragging(dragging)) = &self.gesture else {
            self.last_scroll_tick = None;
            return None;
        };
        if self.outside {
            self.last_scroll_tick = None;
            return None;
        }
        self.edge_scroll_target(now, dragging.pressed.pointer)
    }

    pub(crate) fn edge_scroll_target(&mut self, now: f64, pointer: [f32; 2]) -> Option<f32> {
        let viewport = self.viewport?;
        let axis = usize::from(!self.horizontal);
        let start = viewport[axis];
        let end = start + viewport[axis + 2];
        if self.horizontal && !(viewport[1]..=viewport[1] + viewport[3]).contains(&pointer[1]) {
            self.last_scroll_tick = None;
            return None;
        }
        let position = pointer[axis];
        let edge = theme::TAB_DRAG_SCROLL_EDGE;
        let top_pressure = ((start + edge - position) / edge).clamp(0.0, 1.0);
        let bottom_pressure = ((position - (end - edge)) / edge).clamp(0.0, 1.0);
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
        let next =
            (offset + pressure * theme::TAB_DRAG_SCROLL_MAX * elapsed as f32).clamp(0.0, max);
        ((next - offset).abs() > 0.01).then_some(next)
    }

    /// Keeps the shared transition above the sidebar until Rune's drop spring has landed.
    pub(crate) fn settle_tick(&mut self, now: f64) -> bool {
        if self.outside {
            return false;
        }
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
        if self.outside {
            return None;
        }
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
            &dragging.pressed.members,
            section,
            pointer,
            dragging.target,
        );
        dragging.target = Some(TabDropTarget { section, index });
        dragging.valid = true;
        previous != (dragging.target, dragging.valid)
    }

    fn section_at(&self, point: [f32; 2]) -> Option<TabSection> {
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

    fn insertion_index(
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

    fn matches_order(&self, expected: &[(TabId, TabSection)], tabs: &[Tab]) -> bool {
        expected.len() == tabs.len()
            && expected
                .iter()
                .copied()
                .eq(tabs.iter().map(|tab| (tab.id(), tab.section())))
    }
}
