// SPDX-License-Identifier: GPL-3.0-only

//! The short-lived pointer gesture and preview used to reorder sidebar tabs.

use std::collections::HashMap;
use std::time::Duration;

use rune::Live;

use crate::tab::{Tab, TabId};
use crate::ui::theme;

// Rune suppresses a click once travel exceeds four points on either axis.
const DRAG_THRESHOLD: f32 = 4.0;

pub(crate) struct TabDragState {
    gesture: Option<Gesture>,
    settling: Option<Settling>,
    row_heights: HashMap<TabId, f32>,
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
            row_heights: HashMap::new(),
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
    original_index: usize,
    order: Vec<TabId>,
    start: [f32; 2],
    pointer: [f32; 2],
    grab: [f32; 2],
    origin: [f32; 2],
    size: [f32; 2],
}

struct Dragging {
    pressed: Pressed,
    target_index: usize,
}

struct Settling {
    tab: TabId,
    from: [f32; 2],
    to: [f32; 2],
    size: [f32; 2],
    deadline: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct DragVisual {
    pub(crate) tab: TabId,
    pub(crate) from: [f32; 2],
    pub(crate) to: [f32; 2],
    pub(crate) size: [f32; 2],
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
    pub(crate) target_index: usize,
}

impl TabDragState {
    pub(crate) fn set_viewport(&mut self, rect: [f32; 4]) {
        self.viewport = Some(rect);
    }

    pub(crate) fn scroll_live(&self) -> Live<[f32; 2]> {
        self.scroll.clone()
    }

    pub(crate) fn record_row_height(&mut self, tab: TabId, height: f32) {
        if height.is_finite() && height > 0.0 {
            self.row_heights.insert(tab, height);
        }
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
        if tabs.len() < 2 {
            return;
        }
        let Some(original_index) = tabs.iter().position(|candidate| candidate.id() == tab) else {
            return;
        };
        let origin = [pointer[0] - grab[0], pointer[1] - grab[1]];
        let pressed = Pressed {
            tab,
            original_index,
            order: tabs.iter().map(Tab::id).collect(),
            start: pointer,
            pointer,
            grab,
            origin,
            size,
        };
        self.record_row_height(tab, size[1]);
        self.gesture = Some(Gesture::Pressed(pressed));
    }

    /// Updates the preview while preserving the original browser order until release.
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
                    target_index: pressed.original_index,
                    pressed,
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
        let valid = self.matches_order(&dragging.pressed.order, tabs)
            && dragging.target_index < tabs.len()
            && tabs.iter().any(|tab| tab.id() == dragging.pressed.tab);
        if cancelled || !valid {
            return (true, None);
        }

        let drop = Drop {
            tab: dragging.pressed.tab,
            target_index: dragging.target_index,
        };
        if let Some(viewport) = self.viewport {
            let target_top = self.preview_top(tabs, &dragging, dragging.target_index);
            let scroll_y = self.scroll.get()[0];
            let from = [
                dragging.pressed.pointer[0] - dragging.pressed.grab[0],
                dragging.pressed.pointer[1] - dragging.pressed.grab[1],
            ];
            let to = [
                dragging.pressed.origin[0],
                viewport[1] + target_top - scroll_y,
            ];
            self.settling = Some(Settling {
                tab: dragging.pressed.tab,
                from,
                to,
                size: dragging.pressed.size,
                deadline: None,
            });
        }
        (true, Some(drop))
    }

    pub(crate) fn cancel(&mut self) -> bool {
        let changed = matches!(self.gesture, Some(Gesture::Dragging(_))) || self.settling.is_some();
        self.gesture = None;
        self.settling = None;
        self.last_scroll_tick = None;
        changed
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

    pub(crate) fn visual(&self) -> Option<DragVisual> {
        match &self.gesture {
            Some(Gesture::Dragging(dragging)) => Some(DragVisual {
                tab: dragging.pressed.tab,
                from: [
                    dragging.pressed.pointer[0] - dragging.pressed.grab[0],
                    dragging.pressed.pointer[1] - dragging.pressed.grab[1],
                ],
                to: [
                    dragging.pressed.pointer[0] - dragging.pressed.grab[0],
                    dragging.pressed.pointer[1] - dragging.pressed.grab[1],
                ],
                size: dragging.pressed.size,
                settling: false,
            }),
            _ => self.settling.as_ref().map(|settling| DragVisual {
                tab: settling.tab,
                from: settling.from,
                to: settling.to,
                size: settling.size,
                settling: true,
            }),
        }
    }

    /// Maps a displayed preview slot back to the unchanged browser vector.
    pub(crate) fn original_index_at(&self, preview_index: usize) -> usize {
        let Some(Gesture::Dragging(dragging)) = &self.gesture else {
            return preview_index;
        };
        let original = dragging.pressed.original_index;
        let target = dragging.target_index;
        if preview_index == target {
            original
        } else if original < target && (original..target).contains(&preview_index) {
            preview_index + 1
        } else if target < original && (target + 1..=original).contains(&preview_index) {
            preview_index - 1
        } else {
            preview_index
        }
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

    /// Keeps a settled overlay above the list until Rune's drop spring has had time to land.
    pub(crate) fn settle_tick(&mut self, now: f64) -> bool {
        let Some(settling) = &mut self.settling else {
            return false;
        };
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
        self.settling
            .as_ref()
            .and_then(|settling| settling.deadline)
            .map(|_| Duration::from_millis(16))
    }

    fn update_target(&self, dragging: &mut Dragging, tabs: &[Tab]) -> bool {
        let Some([_, viewport_y, _, _]) = self.viewport else {
            return false;
        };
        let scroll_y = self.scroll.get()[0];
        let dragged_center = dragging.pressed.pointer[1] - viewport_y + scroll_y
            - dragging.pressed.grab[1]
            + dragging.pressed.size[1] / 2.0;
        let old_target = dragging.target_index;
        let last = tabs.len().saturating_sub(1);

        while dragging.target_index < last {
            let next = dragging.target_index + 1;
            let center = self.preview_center(tabs, dragging, next);
            if dragged_center <= center {
                break;
            }
            dragging.target_index += 1;
        }
        while dragging.target_index > 0 {
            let previous = dragging.target_index - 1;
            let center = self.preview_center(tabs, dragging, previous);
            if dragged_center >= center {
                break;
            }
            dragging.target_index -= 1;
        }
        dragging.target_index != old_target
    }

    fn preview_center(&self, tabs: &[Tab], dragging: &Dragging, preview_index: usize) -> f32 {
        let mut top = 0.0;
        for index in 0..preview_index {
            let original_index = original_index_at(index, dragging);
            top += self.row_height(tabs[original_index].id()) + theme::TAB_GAP;
        }
        let original_index = original_index_at(preview_index, dragging);
        top + self.row_height(tabs[original_index].id()) / 2.0
    }

    fn preview_top(&self, tabs: &[Tab], dragging: &Dragging, preview_index: usize) -> f32 {
        let mut top = 0.0;
        for index in 0..preview_index {
            let original_index = original_index_at(index, dragging);
            top += self.row_height(tabs[original_index].id()) + theme::TAB_GAP;
        }
        top
    }

    fn row_height(&self, tab: TabId) -> f32 {
        self.row_heights
            .get(&tab)
            .copied()
            .unwrap_or(theme::ROW_HEIGHT)
    }

    fn matches_order(&self, expected: &[TabId], tabs: &[Tab]) -> bool {
        expected.len() == tabs.len() && expected.iter().copied().eq(tabs.iter().map(Tab::id))
    }
}

fn original_index_at(preview_index: usize, dragging: &Dragging) -> usize {
    let original = dragging.pressed.original_index;
    let target = dragging.target_index;
    if preview_index == target {
        original
    } else if original < target && (original..target).contains(&preview_index) {
        preview_index + 1
    } else if target < original && (target + 1..=original).contains(&preview_index) {
        preview_index - 1
    } else {
        preview_index
    }
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
        state.set_viewport([0.0, 100.0, 220.0, 300.0]);
        let y = 100.0 + index as f32 * (theme::ROW_HEIGHT + theme::TAB_GAP) + 17.0;
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
        assert_eq!(state.move_to(&tabs, [20.0, 159.1]), MoveResult::NoChange);
        assert!(state.is_dragging());
        assert_eq!(state.original_index_at(1), 1);
    }

    #[test]
    fn it_crosses_several_midpoints_and_reverses_without_losing_the_preview() {
        let tabs = tabs(6);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 1);
        state.move_to(&tabs, [20.0, 195.0]);
        assert_eq!(state.original_index_at(1), 2);
        assert_eq!(state.original_index_at(2), 1);
        assert_eq!(
            state.move_to(&tabs, [20.0, 350.0]),
            MoveResult::PreviewChanged
        );
        assert_eq!(state.original_index_at(5), 1);
        assert_eq!(
            state.move_to(&tabs, [20.0, 150.0]),
            MoveResult::PreviewChanged
        );
        assert_eq!(state.original_index_at(0), 0);
        assert_eq!(state.original_index_at(1), 1);
    }

    #[test]
    fn scroll_offset_participates_in_midpoint_calculation_and_edges_clamp() {
        let tabs = tabs(4);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 0);
        state.scroll.set([100.0, 400.0]);
        assert_eq!(state.move_to(&tabs, [20.0, 117.0]), MoveResult::NoChange);
        assert!(!state.is_dragging());
        assert_eq!(
            state.move_to(&tabs, [20.0, 123.0]),
            MoveResult::PreviewChanged
        );
        assert_eq!(state.original_index_at(2), 0);
        assert_eq!(
            state.move_to(&tabs, [20.0, -100.0]),
            MoveResult::PreviewChanged
        );
        assert_eq!(state.original_index_at(0), 0);
    }

    #[test]
    fn cancellation_or_a_changed_tab_order_cannot_commit_a_drop() {
        let mut tabs = tabs(3);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 1);
        state.move_to(&tabs, [20.0, 195.0]);
        assert_eq!(state.end(&tabs, true), (true, None));
        assert!(!state.is_dragging());

        begin(&mut state, &tabs, 1);
        state.move_to(&tabs, [20.0, 195.0]);
        tabs.remove(1);
        assert_eq!(state.move_to(&tabs, [20.0, 160.0]), MoveResult::Cancelled);
        assert_eq!(state.end(&tabs, false), (false, None));
    }

    #[test]
    fn a_single_tab_does_not_start_a_drag() {
        let tabs = tabs(1);
        let mut state = TabDragState::default();
        begin(&mut state, &tabs, 0);
        assert_eq!(state.move_to(&tabs, [20.0, 200.0]), MoveResult::NoChange);
        assert!(!state.is_dragging());
    }
}
