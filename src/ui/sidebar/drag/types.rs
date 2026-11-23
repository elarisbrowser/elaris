// SPDX-License-Identifier: GPL-3.0-only

//! Drop targets, preview slots, and move results shared by the vertical and horizontal tab drags.

use crate::tab::{TabId, TabSection};

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
