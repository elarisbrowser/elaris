// SPDX-License-Identifier: GPL-3.0-only

//! Persistent favourites, pinned tabs, and the scrollable regular tab list.

use rune::tok::type_;
use rune::{El, GrowthSpec, ScrollSpec, Spring, col, el, icon, row, text};

use super::drag::{PreviewItem, favourite_columns, favourite_row_ranges, favourite_tile_width};
use crate::app::{self, AppState};

mod context_menu;
use crate::internal::{InternalPage, InternalRoute};
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::TabDragMoveResult;
use crate::ui::controls::{icon_button, row_button};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};
pub(crate) use context_menu::context_menu;

pub(super) fn view(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let favourite_items = state
        .tab_drag
        .preview_items(state.browser.tabs(), TabSection::Favourite);
    let pinned_items = state
        .tab_drag
        .preview_items(state.browser.tabs(), TabSection::Pinned);
    let regular_items = state
        .tab_drag
        .preview_items(state.browser.tabs(), TabSection::Regular);
    let has_favourites = !favourite_items.is_empty();
    let reorder_motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let mut scroll_content = col()
        .key("tabs-scroll-content")
        .w_full()
        .gap(theme::TAB_GAP)
        .growth(GrowthSpec {
            spring: reorder_motion,
            ..GrowthSpec::default()
        });
    if !pinned_items.is_empty() {
        scroll_content = scroll_content.child(section_rows(
            state,
            TabSection::Pinned,
            active,
            theme,
            reduced_motion,
        ));
    }
    if !pinned_items.is_empty() && !regular_items.is_empty() {
        scroll_content = scroll_content.child(section_divider(theme));
    }
    scroll_content = scroll_content.child(section_rows(
        state,
        TabSection::Regular,
        active,
        theme,
        reduced_motion,
    ));

    let scroll_live = state.tab_drag.scroll_live();
    let scroll = col()
        .key("tabs-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            state.tab_drag.set_viewport(rect);
            if !has_favourites {
                state.tab_drag.set_empty_favourites_anchor(rect);
            }
            tx.live_only();
        })
        .on_scroll_offset(move |position| {
            scroll_live.set([position.offset[1], position.max[1]]);
        })
        .on_secondary_press(|state: &mut AppState, press, _| {
            state.open_context_menu(None, press);
        })
        .child(scroll_content);

    let mut sidebar_tabs = col().key("tab-sections").w_full().grow(1.0).min_h(0.0);
    if has_favourites {
        sidebar_tabs = sidebar_tabs
            .child(favourites(
                state,
                active,
                &favourite_items,
                theme,
                reduced_motion,
            ))
            .gap(theme::SIDEBAR_GAP);
    }
    sidebar_tabs = sidebar_tabs.child(scroll);

    if let Some(visual) = state.tab_drag.visual()
        && let Some(tab) = state.browser.tab(visual.tab)
    {
        let peer = state.browser.split.and_then(|split| {
            if split.panes.contains(&Some(visual.tab)) {
                split
                    .panes
                    .into_iter()
                    .flatten()
                    .find(|id| *id != visual.tab)
                    .and_then(|id| state.browser.tab(id))
                    .filter(|peer| peer.section() == visual.section)
            } else {
                None
            }
        });
        overlays.push(tab_drag_visual(
            tab,
            peer,
            active,
            visual,
            theme,
            reduced_motion,
        ));
    }
    sidebar_tabs
}

fn favourites(
    state: &AppState,
    active: Option<TabId>,
    items: &[PreviewItem],
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let content_width = (state.sidebar.width - 2.0 * theme::SIDEBAR_PADDING).max(0.0);
    let columns = favourite_columns(content_width);
    let ranges = favourite_row_ranges(items.len(), columns);
    let mut grid = col()
        .key("favourites-grid")
        .w_full()
        .gap(theme::FAVOURITE_GAP)
        .growth(GrowthSpec {
            spring: motion,
            ..GrowthSpec::default()
        })
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            state.tab_drag.set_favourite_grid(rect);
            tx.live_only();
        });
    for (row_index, range) in ranges.iter().enumerate() {
        let row_count = range.len();
        let width = favourite_tile_width(content_width, row_count);
        let mut tile_row = row()
            .key(format!("favourites-row-{row_index}"))
            .w_full()
            .gap(theme::FAVOURITE_GAP)
            .motion(motion)
            .animate_layout();
        for index in range.clone() {
            let item = items[index];
            let tile = match item {
                PreviewItem::Tab(id) => state.browser.tab(id).map_or_else(el, |tab| {
                    favourite_tile(
                        state,
                        tab,
                        Some(id) == active,
                        index,
                        items.len(),
                        width,
                        motion,
                        theme,
                    )
                }),
                PreviewItem::SourceSlot(id) => {
                    source_placeholder(id, TabSection::Favourite, index, items.len(), width)
                }
                PreviewItem::DropSlot(id, same_section) => drop_placeholder(
                    id,
                    same_section,
                    TabSection::Favourite,
                    index,
                    items.len(),
                    width,
                    motion,
                    theme,
                ),
            };
            tile_row = tile_row.child(tile);
        }
        grid = grid.child(tile_row);
    }
    col()
        .key("favourites-section")
        .w_full()
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            let target_ready =
                state
                    .tab_drag
                    .set_section_rect(TabSection::Favourite, rect, state.browser.tabs());
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        })
        .child(grid)
}

fn section_rows(
    state: &AppState,
    section: TabSection,
    active: Option<TabId>,
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let items = state.tab_drag.preview_items(state.browser.tabs(), section);
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let mut rows = col()
        .key(match section {
            TabSection::Pinned => "pinned-rows",
            TabSection::Regular => "regular-rows",
            TabSection::Favourite => unreachable!("favourites use their grid"),
        })
        .w_full()
        .gap(theme::TAB_GAP)
        .motion(motion)
        .growth(GrowthSpec {
            spring: motion,
            ..GrowthSpec::default()
        });
    let split_tabs = state.browser.split.map(|split| split.panes);
    let split_pair = split_tabs
        .and_then(|[left, right]| Some([left?, right?]))
        .filter(|pair| {
            pair.iter().all(|id| {
                state
                    .browser
                    .tab(*id)
                    .is_some_and(|tab| tab.section() == section)
            }) && pair.iter().all(|id| items.contains(&PreviewItem::Tab(*id)))
        });
    let mut index = 0;
    let mut split_group_added = false;
    while index < items.len() {
        let split_tab = match items[index] {
            PreviewItem::Tab(id) => split_pair.filter(|pair| pair.contains(&id)),
            PreviewItem::SourceSlot(_) | PreviewItem::DropSlot(_, _) => None,
        };
        if let Some(pair) = split_tab.filter(|_| !split_group_added) {
            rows = rows.child(split_tab_group(
                state,
                section,
                pair,
                index,
                items.len(),
                theme,
            ));
            split_group_added = true;
            index += 1;
            continue;
        }
        if split_group_added && split_tab.is_some() {
            index += 1;
            continue;
        }
        let item = items[index];
        let element = match item {
            PreviewItem::Tab(id) => state.browser.tab(id).map_or_else(el, |tab| {
                tab_row(
                    state,
                    tab,
                    section,
                    Some(id) == active,
                    index,
                    items.len(),
                    theme,
                    false,
                    false,
                )
            }),
            PreviewItem::SourceSlot(id) => source_placeholder(id, section, index, items.len(), 0.0),
            PreviewItem::DropSlot(id, same_section) => drop_placeholder(
                id,
                same_section,
                section,
                index,
                items.len(),
                0.0,
                motion,
                theme,
            ),
        };
        rows = rows.child(element);
        index += 1;
    }
    if section == TabSection::Regular {
        rows = rows.child(
            row_button(
                "new-tab",
                icon(&icons::PLUS, theme::ICON).color(theme.text_muted),
                "New Tab",
                theme.text_muted,
                theme,
            )
            .on_press_tx(app::new_tab),
        );
    }

    col()
        .key(match section {
            TabSection::Pinned => "pinned-section",
            TabSection::Regular => "regular-section",
            TabSection::Favourite => unreachable!("favourites use their grid"),
        })
        .w_full()
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            let target_ready = state
                .tab_drag
                .set_section_rect(section, rect, state.browser.tabs());
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        })
        .child(rows)
}

fn split_tab_group(
    state: &AppState,
    section: TabSection,
    ids: [TabId; 2],
    index: usize,
    count: usize,
    theme: &Theme,
) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let selected_group = active.is_some_and(|active| ids.contains(&active));
    let mut group = row()
        .key("split-tab-group")
        .w_full()
        .h(theme::ROW_HEIGHT)
        .gap(2.0)
        .px(3.0)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .fill(if selected_group {
            theme.tab_active
        } else {
            theme.address_field
        });
    if selected_group {
        group = group.shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
    for (offset, id) in ids.into_iter().enumerate() {
        if let Some(tab) = state.browser.tab(id) {
            let child = tab_row(
                state,
                tab,
                section,
                Some(id) == active,
                index + offset,
                count,
                theme,
                true,
                selected_group,
            )
            .w(0.0)
            .grow(1.0)
            .h(theme::ROW_HEIGHT - 6.0)
            .radius(theme::ROW_RADIUS - 3.0);
            group = group.child(child);
        }
    }
    group
}

fn section_divider(theme: &Theme) -> El {
    el().key("pinned-regular-divider")
        .w_full()
        .h(1.0)
        .fill(theme.overlay_border)
}

#[allow(clippy::too_many_arguments)]
fn favourite_tile(
    state: &AppState,
    tab: &Tab,
    active: bool,
    index: usize,
    count: usize,
    width: f32,
    motion: Spring,
    theme: &Theme,
) -> El {
    let id = tab.id();
    let label = tab.label();
    let hidden = state.tab_drag.is_hidden(id);
    let mut tile = el()
        .key(format!("favourite-{}", id.raw()))
        .w(width)
        .h(theme::FAVOURITE_TILE_HEIGHT)
        .center()
        .radius(theme::FAVOURITE_TILE_RADIUS)
        .continuous_corners()
        .motion(motion)
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .label(label)
        .child(
            tab_icon(tab, active, theme::FAVOURITE_ICON, theme)
                .key(format!("favourite-icon-{}", id.raw())),
        )
        .position_in_set(index + 1, count)
        .on_press_tx(move |state: &mut AppState, tx| {
            tx.color_motion(theme::SELECTION_MOTION);
            state.activate(id);
            if state.browser.split.is_some() {
                tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            }
        })
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .on_secondary_press(move |state: &mut AppState, press, _| {
            state.open_context_menu(Some(id), press);
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            let target_ready = state
                .tab_drag
                .record_tab_rect(TabSection::Favourite, id, rect);
            if target_ready {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        });
    tile = tab_drag_handler(tile, id, vec![id]);
    if active {
        tile = tile.fill(theme.tab_active);
    } else {
        tile = tile.fill(theme.address_field);
    }
    if hidden {
        tile = tile.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    super::hover::attach(tile, id)
}

#[allow(clippy::too_many_arguments)]
fn tab_row(
    state: &AppState,
    tab: &Tab,
    section: TabSection,
    active: bool,
    index: usize,
    count: usize,
    theme: &Theme,
    grouped: bool,
    group_active: bool,
) -> El {
    let id = tab.id();
    let label = tab.label();
    let hidden = state.tab_drag.is_hidden(id);
    let color = if active { theme.text } else { theme.text_muted };
    let leading = tab_icon(tab, active, theme::ICON, theme);
    let close = icon_button(
        &format!("close-{}", id.raw()),
        "Close tab",
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        theme,
    )
    .on_press(move |state: &mut AppState| state.close(id));
    let close = if active {
        close
    } else {
        close.group_opacity(0.0, 1.0)
    };
    let mut row = row_button(&format!("tab-{}", id.raw()), leading, label, color, theme)
        .hover_group()
        .child(close)
        .position_in_set(index + 1, count);
    row = row
        .on_press_tx(move |state: &mut AppState, tx| {
            tx.color_motion(theme::SELECTION_MOTION);
            state.activate(id);
            if state.browser.split.is_some() {
                tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            }
        })
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .on_secondary_press(move |state: &mut AppState, press, _| {
            state.open_context_menu(Some(id), press);
        });
    if grouped {
        row = row.fill(if group_active {
            theme.address_field
        } else {
            theme.tab_drag
        });
    } else if active {
        row = row.fill(theme.tab_active);
    } else if state.tab_drag.is_hidden(id) {
        row = row.fill(theme.tab_drag);
    }
    if active && !grouped {
        row = row.shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
    if hidden {
        row = row.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    let drag_members = state
        .browser
        .split
        .filter(|split| split.panes.contains(&Some(id)))
        .and_then(|split| Some([split.panes[0]?, split.panes[1]?]))
        .filter(|pair| {
            pair.iter().all(|member| {
                state
                    .browser
                    .tab(*member)
                    .is_some_and(|member| member.section() == section)
            })
        })
        .map_or_else(|| vec![id], |pair| pair.to_vec());
    row = tab_drag_handler(row, id, drag_members);
    let row = row.on_layout_tx(move |state: &mut AppState, rect, tx| {
        let target_ready = state.tab_drag.record_tab_rect(section, id, rect);
        if target_ready {
            tx.animated();
            tx.motion(theme::TAB_REORDER_MOTION);
            tx.wake_after(std::time::Duration::ZERO);
        } else {
            tx.live_only();
        }
    });
    super::hover::attach(row, id)
}

fn tab_icon(tab: &Tab, active: bool, size: f32, theme: &Theme) -> El {
    let color = if active { theme.text } else { theme.text_muted };
    if let Some(favicon) = &tab.favicon {
        el().size(size, size).center().pixels(favicon)
    } else if tab.loading {
        icon(&icons::LOADING, size)
            .color(color)
            .label("Loading")
            .looping(rune::LoopSpec::rotation(theme::LOADING_PERIOD))
    } else if tab.internal == Some(InternalRoute::Page(InternalPage::History)) {
        icon(&icons::HISTORY, size).color(color)
    } else {
        icon(&icons::PAGE, size).color(color)
    }
}

#[allow(clippy::too_many_arguments)]
fn drop_placeholder(
    tab: TabId,
    same_section: bool,
    section: TabSection,
    index: usize,
    count: usize,
    width: f32,
    motion: Spring,
    theme: &Theme,
) -> El {
    if section == TabSection::Favourite {
        let mut slot = el()
            .key(if same_section {
                format!("favourite-{}", tab.raw())
            } else {
                "favourites-drop-placeholder".to_owned()
            })
            .w(width)
            .h(theme::FAVOURITE_TILE_HEIGHT)
            .radius(theme::FAVOURITE_TILE_RADIUS)
            .continuous_corners()
            .fill(theme.tab_drop_indicator)
            .motion(motion)
            .position_in_set(index + 1, count)
            .on_layout_tx(move |state: &mut AppState, rect, tx| {
                state.tab_drag.record_placeholder(section, rect);
                tx.live_only();
            });
        if same_section {
            slot = tab_drag_handler(slot, tab, vec![tab]);
        }
        slot
    } else {
        let mut slot = el()
            .key(match (section, same_section) {
                (TabSection::Pinned | TabSection::Regular, true) => format!("tab-{}", tab.raw()),
                (TabSection::Pinned, false) => "pinned-drop-placeholder".to_owned(),
                (TabSection::Regular, false) => "regular-drop-placeholder".to_owned(),
                (TabSection::Favourite, _) => unreachable!(),
            })
            .w_full()
            .h(theme::ROW_HEIGHT)
            .radius(theme::ROW_RADIUS)
            .continuous_corners()
            .fill(theme.tab_drop_indicator)
            .opacity(0.28)
            .motion(motion)
            .position_in_set(index + 1, count)
            .on_layout_tx(move |state: &mut AppState, rect, tx| {
                state.tab_drag.record_placeholder(section, rect);
                tx.live_only();
            });
        if same_section {
            slot = tab_drag_handler(slot, tab, vec![tab]);
        }
        slot
    }
}

fn source_placeholder(
    tab: TabId,
    section: TabSection,
    index: usize,
    count: usize,
    width: f32,
) -> El {
    let (key, width, height) = if section == TabSection::Favourite {
        (
            format!("favourite-{}", tab.raw()),
            width,
            theme::FAVOURITE_TILE_HEIGHT,
        )
    } else {
        (format!("tab-{}", tab.raw()), 0.0, theme::ROW_HEIGHT)
    };
    let slot = el().key(key).h(height).position_in_set(index + 1, count);
    let slot = if width > 0.0 {
        slot.w(width)
    } else {
        slot.w_full()
    };
    tab_drag_handler(slot, tab, vec![tab])
}

fn dragging_single_page(state: &AppState, id: TabId) -> bool {
    state.browser.displayed_split().is_none() && state.browser.visible_tabs()[0] == Some(id)
}

fn tab_drag_handler(row: El, id: TabId, members: Vec<TabId>) -> El {
    row.on_drag(move |state: &mut AppState, drag, tx| match drag.phase {
        rune::Phase::Down => {
            super::hover::dismiss(state);
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
            let preview = state
                .tab_drag
                .is_dragging()
                .then(|| crate::ui::viewport::drop_side(state, id, drag.window_point()))
                .flatten();
            if state.split_preview != preview {
                state.split_preview = preview;
                tx.motion(theme::TAB_REORDER_MOTION);
            }
            if state.tab_drag.is_dragging() || result == TabDragMoveResult::Cancelled {
                tx.animated();
            }
            if matches!(
                result,
                TabDragMoveResult::PreviewChanged | TabDragMoveResult::Cancelled
            ) {
                tx.motion(theme::TAB_REORDER_MOTION);
            }
        }
        rune::Phase::Up => {
            let side = (!drag.cancelled() && state.tab_drag.is_dragging())
                .then(|| crate::ui::viewport::drop_side(state, id, drag.window_point()))
                .flatten();
            state.split_preview = None;
            let (was_dragging, drop) = state
                .tab_drag
                .end(state.browser.tabs(), drag.cancelled() || side.is_some());
            if let Some(side) = side {
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

fn tab_drag_visual(
    tab: &Tab,
    peer: Option<&Tab>,
    active_tab: Option<TabId>,
    visual: super::drag::DragVisual,
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let favourite = visual.section == TabSection::Favourite;
    let title_visible = !favourite;
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let icon_size = if favourite {
        theme::FAVOURITE_ICON
    } else {
        theme::ICON
    };
    let title_width = if favourite { 0.0 } else { 1.0 };
    if let Some(peer) = peer.filter(|_| !favourite && active_tab.is_some()) {
        let mut group = row()
            .key("tab-drag-visual")
            .size(visual.to_size[0], visual.to_size[1])
            .gap(2.0)
            .px(3.0)
            .center_y()
            .radius(theme::ROW_RADIUS)
            .continuous_corners()
            .clip_soft(0.0)
            .fill(theme.tab_active)
            .shadow(
                theme::TAB_DRAG_SHADOW_Y,
                theme::TAB_DRAG_SHADOW_BLUR,
                theme.tab_active_shadow,
            );
        for member in [tab, peer] {
            let member_active = active_tab == Some(member.id());
            group = group.child(
                row()
                    .w(0.0)
                    .h(theme::ROW_HEIGHT - 6.0)
                    .grow(1.0)
                    .px(theme::ROW_PADDING / 2.0)
                    .gap(theme::ROW_GAP)
                    .center_y()
                    .radius(theme::ROW_RADIUS - 3.0)
                    .fill(theme.address_field)
                    .child(tab_icon(member, member_active, icon_size, theme))
                    .child(
                        text(member.label())
                            .font(type_::BODY)
                            .color(if member_active {
                                theme.text
                            } else {
                                theme.text_muted
                            })
                            .single_line()
                            .truncate()
                            .shrink()
                            .min_w(0.0)
                            .grow(1.0),
                    ),
            );
        }
        return el()
            .key("tab-drag-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .inert()
            .a11y_hidden()
            .child(group.absolute(visual.to[0], visual.to[1]));
    }
    let active = active_tab == Some(tab.id());
    let mut row = row()
        .key("tab-drag-visual")
        .size(visual.to_size[0], visual.to_size[1])
        .pl(if favourite { 0.0 } else { theme::ROW_PADDING })
        .pr(if favourite {
            0.0
        } else {
            theme::ROW_PADDING / 2.0
        })
        .gap(if favourite { 0.0 } else { theme::ROW_GAP })
        .center_y()
        .radius(if favourite {
            theme::FAVOURITE_TILE_RADIUS
        } else {
            theme::ROW_RADIUS
        })
        .continuous_corners()
        .clip_soft(0.0)
        .child(
            tab_icon(tab, active, icon_size, theme)
                .key("drag-tab-icon")
                .motion(motion),
        )
        .child(
            text(tab.label())
                .key("drag-tab-title")
                .font(type_::BODY)
                .color(if active { theme.text } else { theme.text_muted })
                .single_line()
                .truncate()
                .shrink()
                .min_w(0.0)
                .grow(title_width)
                .opacity(if title_visible { 1.0 } else { 0.0 })
                .opacity_motion(motion),
        );
    if favourite {
        row = row.justify_center();
    }
    if active {
        row = row.fill(theme.tab_active);
    } else if favourite {
        row = row.fill(theme.address_field);
    } else if !visual.settling {
        row = row.fill(theme.tab_drag);
    }
    row = row.shadow(
        theme::TAB_DRAG_SHADOW_Y,
        theme::TAB_DRAG_SHADOW_BLUR,
        theme.tab_active_shadow,
    );
    if visual.settling {
        row = row.motion(motion);
    } else {
        row = row.instant_changes();
    }
    row = row.absolute(visual.to[0], visual.to[1]);
    el().key("tab-drag-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .inert()
        .a11y_hidden()
        .child(row)
}
