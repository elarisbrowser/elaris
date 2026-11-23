// SPDX-License-Identifier: GPL-3.0-only

//! Per-frame view helpers: the displayed order of each section and the floating drag visual.

use crate::tab::{Tab, TabSection};

use super::state::{Gesture, TabDragState};
use super::types::{DragVisual, PreviewItem};

impl TabDragState {
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
}
