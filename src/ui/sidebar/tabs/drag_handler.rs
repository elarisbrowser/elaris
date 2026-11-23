// SPDX-License-Identifier: GPL-3.0-only

//! Pointer handling for dragging a tab: pressing, moving, and releasing it into a new place.

use rune::El;

use crate::app::AppState;
use crate::tab::TabId;
use crate::ui::TabDragMoveResult;
use crate::ui::sidebar::hover;
use crate::ui::theme;

fn dragging_single_page(state: &AppState, id: TabId) -> bool {
    state.browser.displayed_split().is_none() && state.browser.visible_tabs()[0] == Some(id)
}

pub(in crate::ui) fn tab_drag_handler(row: El, id: TabId, members: Vec<TabId>) -> El {
    row.on_drag(move |state: &mut AppState, drag, tx| match drag.phase {
        rune::Phase::Down => {
            hover::dismiss(state);
            let group_offset =
                if members.len() > 1 && members.get(1).is_some_and(|member| *member == id) {
                    drag.w + 2.0
                } else {
                    0.0
                };
            let grouped = members.len() > 1;
            state.tab_drag.press_members(
                id,
                members.clone(),
                state.browser.tabs(),
                drag.window_point(),
                [
                    drag.start_x + group_offset,
                    drag.start_y + if grouped { 3.0 } else { 0.0 },
                ],
                [
                    if grouped { drag.w * 2.0 + 8.0 } else { drag.w },
                    if grouped { drag.h + 6.0 } else { drag.h },
                ],
            );
        }
        rune::Phase::Move => {
            let result = state
                .tab_drag
                .move_to(state.browser.tabs(), drag.window_point());
            let active = state.tab_drag.is_dragging();
            let external = active
                && members.len() == 1
                && crate::app::cross_window_drag::track(state, id, drag, false);
            let outside_changed = state.tab_drag.set_outside(external);
            if !active {
                crate::app::cross_window_drag::clear(state.window_id);
            }
            let preview = (active && !external)
                .then(|| crate::ui::viewport::drop_side(state, id, drag.window_point()))
                .flatten();
            if state.split_preview != preview {
                state.split_preview = preview;
                tx.motion(theme::TAB_REORDER_MOTION);
            }
            if state.tab_drag.is_dragging() || result == TabDragMoveResult::Cancelled {
                tx.animated();
            }
            if outside_changed
                || matches!(
                    result,
                    TabDragMoveResult::PreviewChanged | TabDragMoveResult::Cancelled
                )
            {
                tx.motion(theme::TAB_REORDER_MOTION);
            }
        }
        rune::Phase::Up => {
            #[cfg(elaris_ui = "macos")]
            crate::platform::macos::drag_overlay::hide(state.window_id);

            if !state.tab_drag.is_dragging()
                && crate::app::cross_window_drag::release_pending(state.window_id)
            {
                return;
            }

            let active = !drag.cancelled() && state.tab_drag.is_dragging();
            if active && members.len() == 1 {
                state
                    .tab_drag
                    .move_to(state.browser.tabs(), drag.window_point());
            }
            let external = active
                && members.len() == 1
                && crate::app::cross_window_drag::track(state, id, drag, true);
            if !external {
                crate::app::cross_window_drag::clear(state.window_id);
            }
            let detached =
                !external && active && members.len() == 1 && crate::app::detach::release(state, id);
            state.tab_drag.set_outside(external);
            let side = (active && !external && !detached)
                .then(|| crate::ui::viewport::drop_side(state, id, drag.window_point()))
                .flatten();
            state.split_preview = None;
            let (was_dragging, drop) = state.tab_drag.end(
                state.browser.tabs(),
                drag.cancelled() || external || detached || side.is_some(),
            );
            if let Some(side) = side {
                if state.horizontal_tabs() {
                    // The page owns the drop. Reveal the strip tab immediately instead of
                    // animating the floating card back over the newly opened pane.
                    state.tab_drag.cancel();
                }
                if dragging_single_page(state, id) {
                    state.begin_split_picker(id, side);
                    tx.focus(crate::ui::viewport::PICKER, false);
                } else {
                    state.place_split_tab(id, side);
                    tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
                }
            }
            if let Some(drop) = drop
                && let Err(error) =
                    state.commit_tab_move_group(&drop.tabs, drop.target.section, drop.target.index)
            {
                eprintln!("Elaris: {error}");
                state.tab_drag.cancel();
            }
            if was_dragging {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            }
        }
    })
}
