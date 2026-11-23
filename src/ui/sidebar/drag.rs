// SPDX-License-Identifier: GPL-3.0-only

//! The short-lived pointer gesture and preview used to reorder tabs across sidebar sections.

mod geometry;
mod gesture;
mod scroll;
mod settle;
mod state;
mod target;
#[cfg(test)]
mod tests;
mod types;
mod view;

pub(crate) use geometry::{favourite_columns, favourite_row_ranges, favourite_tile_width};
pub(crate) use state::TabDragState;
pub(crate) use types::{DragVisual, MoveResult, PreviewItem, TabDropTarget};

// Rune suppresses a click once travel exceeds four points on either axis.
const DRAG_THRESHOLD: f32 = 4.0;
const TARGET_HYSTERESIS: f32 = 4.0;
