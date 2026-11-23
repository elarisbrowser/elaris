// SPDX-License-Identifier: GPL-3.0-only

//! Edge auto-scroll: how fast the list scrolls while a dragged tab rests near its edge.

use crate::ui::theme;

use super::state::{Gesture, TabDragState};

impl TabDragState {
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
}
