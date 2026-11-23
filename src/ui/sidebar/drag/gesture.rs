// SPDX-License-Identifier: GPL-3.0-only

//! Pointer gestures: pressing a tab, crossing the drag threshold, and settling on release or cancel.

use crate::tab::{Tab, TabId, TabSection};

use super::geometry::past_drag_threshold;
use super::state::{Dragging, Gesture, Pressed, Settling, TabDragState};
use super::types::{Drop, MoveResult, TabDropTarget};

impl TabDragState {
    #[cfg(test)]
    pub(super) fn press(
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
        // Tick as the drop slot moves, but not when the drag first picks one.
        if previous
            .0
            .is_some_and(|target| Some(target) != dragging.target)
        {
            crate::platform::trackpad::step_haptic();
        }
        previous != (dragging.target, dragging.valid)
    }

    fn matches_order(&self, expected: &[(TabId, TabSection)], tabs: &[Tab]) -> bool {
        expected.len() == tabs.len()
            && expected
                .iter()
                .copied()
                .eq(tabs.iter().map(|tab| (tab.id(), tab.section())))
    }
}
