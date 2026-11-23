// SPDX-License-Identifier: GPL-3.0-only

//! The shared tab drag state, its gesture model, and the setters the sidebar writes each frame.

use std::collections::HashMap;

use rune::Live;

use super::geometry::{favourite_columns, favourite_slot_rect, section_index, valid_rect};
use super::types::{DragVisual, TabDropTarget};
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::theme;

pub(crate) struct TabDragState {
    pub(super) horizontal: bool,
    pub(super) gesture: Option<Gesture>,
    pub(super) incoming: Option<(TabId, TabDropTarget)>,
    pub(super) outside: bool,
    pub(super) incoming_visual: Option<DragVisual>,
    pub(super) settling: Option<Settling>,
    pub(super) tab_rects: HashMap<TabId, [f32; 4]>,
    pub(super) section_rects: [Option<[f32; 4]>; TabSection::ORDER.len()],
    pub(super) favourite_grid: Option<[f32; 4]>,
    pub(super) empty_favourite_area: Option<[f32; 4]>,
    pub(super) placeholder: Option<(TabSection, [f32; 4])>,
    pub(super) viewport: Option<[f32; 4]>,
    /// The scroll viewport publishes its current offset and range without rebuilding the view.
    pub(super) scroll: Live<[f32; 2]>,
    pub(super) last_scroll_tick: Option<f64>,
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

pub(super) enum Gesture {
    Pressed(Pressed),
    Dragging(Dragging),
}

pub(super) struct Pressed {
    pub(super) tab: TabId,
    pub(super) members: Vec<TabId>,
    pub(super) source: TabDropTarget,
    pub(super) order: Vec<(TabId, TabSection)>,
    pub(super) start: [f32; 2],
    pub(super) pointer: [f32; 2],
    pub(super) grab: [f32; 2],
    pub(super) origin: [f32; 2],
    pub(super) size: [f32; 2],
}

pub(super) struct Dragging {
    pub(super) pressed: Pressed,
    pub(super) target: Option<TabDropTarget>,
    pub(super) valid: bool,
}

pub(super) struct Settling {
    pub(super) tab: TabId,
    pub(super) source_section: TabSection,
    pub(super) target_section: TabSection,
    pub(super) target_index: usize,
    pub(super) from: [f32; 2],
    pub(super) to: [f32; 2],
    pub(super) from_size: [f32; 2],
    pub(super) to_size: [f32; 2],
    pub(super) start_pending: bool,
    pub(super) target_pending: bool,
    pub(super) target_wait_started: Option<f64>,
    pub(super) incoming: bool,
    pub(super) deadline: Option<f64>,
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
}
