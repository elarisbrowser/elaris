// SPDX-License-Identifier: GPL-3.0-only

//! The open tabs, scrolling once they outgrow the sidebar, then the new-tab button.

use rune::{El, GrowthSpec, ScrollSpec, Tx, col, el, icon};
use rune_kit::{Kit, MenuItem, PaletteSeed, context_menu as menu_at};

use crate::app::{self, AppState};
use crate::tab::Tab;
use crate::ui::TabDragMoveResult;
use crate::ui::controls::{icon_button, row_button};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

pub(super) fn view(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let active = state.browser.active_tab().map(Tab::id);
    let tabs = state.browser.tabs();
    let mut list = col()
        .key("tabs")
        .w_full()
        .gap(theme::TAB_GAP)
        .growth(GrowthSpec {
            spring: theme::TAB_REORDER_MOTION,
            ..GrowthSpec::default()
        })
        .label("Tabs");
    for preview_index in 0..tabs.len() {
        let original_index = state.tab_drag.original_index_at(preview_index);
        let tab = &tabs[original_index];
        let id = tab.id();
        let row = tab_row(
            tab,
            Some(id) == active,
            state.tab_drag.is_hidden(id),
            true,
            theme,
        )
        .position_in_set(preview_index + 1, tabs.len())
        .on_drag(move |state: &mut AppState, drag, tx| match drag.phase {
            rune::Phase::Down => state.tab_drag.press(
                id,
                state.browser.tabs(),
                drag.window_point(),
                [drag.start_x, drag.start_y],
                [drag.w, drag.h],
            ),
            rune::Phase::Move => {
                let result = state
                    .tab_drag
                    .move_to(state.browser.tabs(), drag.window_point());
                if matches!(
                    result,
                    TabDragMoveResult::PreviewChanged | TabDragMoveResult::Cancelled
                ) {
                    tx.animated();
                    tx.motion(theme::TAB_REORDER_MOTION);
                }
            }
            rune::Phase::Up => {
                let (was_dragging, drop) =
                    state.tab_drag.end(state.browser.tabs(), drag.cancelled());
                if let Some(drop) = drop
                    && let Err(error) = state.commit_tab_reorder(drop.tab, drop.target_index)
                {
                    eprintln!("Elaris: {error}");
                    state.tab_drag.cancel();
                }
                if was_dragging {
                    tx.animated();
                    tx.motion(theme::TAB_REORDER_MOTION);
                    if drop.is_some() {
                        tx.wake_after(std::time::Duration::ZERO);
                    }
                }
            }
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            state.tab_drag.record_row_height(id, rect[3]);
            tx.live_only();
        });
        list = list.child(row);
    }
    list = list.child(
        row_button(
            "new-tab",
            icon(&icons::PLUS, theme::ICON).color(theme.text_muted),
            "New Tab",
            theme.text_muted,
            theme,
        )
        .on_press_tx(app::new_tab),
    );
    let scroll_live = state.tab_drag.scroll_live();
    let scroll = col()
        .key("tabs-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            state.tab_drag.set_viewport(rect);
            tx.live_only();
        })
        .on_scroll_offset(move |position| {
            scroll_live.set([position.offset[1], position.max[1]]);
        })
        .on_secondary_press(|state: &mut AppState, press, _| {
            state.open_context_menu(None, press);
        })
        .child(list);
    if let Some(visual) = state.tab_drag.visual()
        && let Some(tab) = state.browser.tab(visual.tab)
    {
        overlays.push(tab_drag_visual(
            tab,
            Some(tab.id()) == active,
            visual,
            theme,
        ));
    }
    scroll
}

fn tab_row(tab: &Tab, active: bool, dragged: bool, interactive: bool, theme: &Theme) -> El {
    let id = tab.id();
    let color = if active { theme.text } else { theme.text_muted };
    let leading = if tab.loading {
        icon(&icons::LOADING, theme::ICON)
            .color(color)
            .label("Loading")
            .looping(rune::LoopSpec::rotation(theme::LOADING_PERIOD))
    } else if let Some(favicon) = &tab.favicon {
        el().size(theme::ICON, theme::ICON).center().pixels(favicon)
    } else {
        icon(&icons::PAGE, theme::ICON).color(color)
    };
    let mut close = icon_button(
        &format!("close-{}", id.raw()),
        "Close tab",
        &icons::CLOSE,
        theme::CLOSE_BUTTON,
        theme,
    );
    if interactive {
        close = close.on_press(move |state: &mut AppState| state.close(id));
    }
    // Only the active tab shows its close button at rest; the others show it under the pointer.
    let close = if active || dragged && !interactive {
        close
    } else {
        close.group_opacity(0.0, 1.0)
    };
    let mut row = row_button(
        &format!("tab-{}", id.raw()),
        leading,
        tab.label(),
        color,
        theme,
    )
    .help_when_truncated(tab.label())
    .hover_group();
    if interactive {
        row = row
            .on_press_tx(move |state: &mut AppState, tx| {
                tx.color_motion(theme::SELECTION_MOTION);
                state.activate(id);
            })
            .on_middle_press(move |state: &mut AppState, _| state.close(id))
            .on_secondary_press(move |state: &mut AppState, press, _| {
                state.open_context_menu(Some(id), press);
            });
    }
    row = row.child(close);
    if active {
        row = row.fill(theme.tab_active);
    } else if dragged {
        row = row.fill(theme.tab_drag);
    }
    if dragged {
        row = row.shadow(
            theme::TAB_DRAG_SHADOW_Y,
            theme::TAB_DRAG_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    } else if active {
        row = row.shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
    if interactive && dragged {
        row = row.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    row
}

fn tab_drag_visual(tab: &Tab, active: bool, visual: super::drag::DragVisual, theme: &Theme) -> El {
    let (x, y) = if visual.settling {
        (visual.to[0], visual.to[1])
    } else {
        (visual.from[0], visual.from[1])
    };
    let row = tab_row(tab, active, true, false, theme)
        .key("tab-drag-visual")
        .w(visual.size[0])
        .h(visual.size[1]);
    let row = if visual.settling {
        row.motion(theme::TAB_REORDER_MOTION)
    } else {
        row.instant_changes()
    };
    el().key("tab-drag-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .inert()
        .a11y_hidden()
        .child(row.absolute(x, y))
}

pub(crate) fn context_menu(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some((tab, point)) = state.context_menu() else {
        return;
    };
    let mut menu = menu_at("sidebar-context-menu", el(), Some(point))
        .on_close(|state: &mut AppState| state.dismiss_context_menu());
    if let Some(tab) = tab {
        menu = menu
            .item(
                MenuItem::new("Reload").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.reload(tab);
                }),
            )
            .item(
                MenuItem::new("Duplicate").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.duplicate(tab);
                }),
            )
            .separator()
            .item(
                MenuItem::new("Close Tab").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.close(tab);
                }),
            )
            .item(
                MenuItem::new("Close Other Tabs").on_select(move |state: &mut AppState| {
                    state.dismiss_context_menu();
                    state.close_others(tab);
                }),
            );
    } else {
        menu = menu.item(MenuItem::new("New Tab").on_select_tx(
            |state: &mut AppState, tx: &mut Tx| {
                state.dismiss_context_menu();
                app::new_tab(state, tx);
            },
        ));
    }
    let kit = Kit::from_palette(PaletteSeed {
        bg: theme.window,
        fg: theme.text,
        accent: theme.icon,
    });
    menu.build(&kit, layer);
}
