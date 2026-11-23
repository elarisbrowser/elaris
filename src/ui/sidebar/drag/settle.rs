// SPDX-License-Identifier: GPL-3.0-only

//! Shared-element settling: the drop animation that carries a tab from the pointer into its slot.

use std::time::Duration;

use crate::tab::{TabId, TabSection};
use crate::ui::theme;

use super::geometry::valid_rect;
use super::state::{Settling, TabDragState};
use super::types::{DragVisual, TabDropTarget};

impl TabDragState {
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
}
