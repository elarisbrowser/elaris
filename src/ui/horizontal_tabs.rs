// SPDX-License-Identifier: GPL-3.0-only

//! Two-row browser chrome using the shared browser, tab actions and drag gesture.

use std::collections::HashMap;

use rune::{Cx, El, Spring, Transition, Tx, col, el, row};
use rune_kit::{Menu, MenuItem, Placement};

use super::controls::{icon_button, icon_button_style, row_button};
use super::sidebar::drag::PreviewItem;
use super::sidebar::tabs::{drag_overlay, tab_drag_handler, tab_icon};
use super::sidebar::{header, hover};
use super::theme::{self, Theme};
use super::{downloads, icons};
use crate::app::{self, AppState};
use crate::tab::{Tab, TabId, TabSection};

pub(crate) const SCROLL: &str = "horizontal-tabs-scroll";

pub(crate) fn key(id: TabId) -> String {
    format!("horizontal-tab-{}", id.raw())
}

pub(crate) fn reveal_active(state: &mut AppState, tx: &mut Tx) {
    if !state.horizontal_tabs() {
        state.sidebar.revealed_tab = None;
        state.sidebar.horizontal_strip_width = None;
        state.sidebar.overflow_menu_open = false;
    } else if !state.tab_drag.has_preview() {
        let active = state.browser.active_tab().and_then(|tab| {
            state
                .browser
                .tabs()
                .iter()
                .position(|other| other.id() == tab.id())
                .map(|index| (tab.id(), tab.section(), index))
        });
        if active != state.sidebar.revealed_tab {
            state.sidebar.revealed_tab = active;
            if let Some((id, _, _)) = active {
                tx.color_motion(theme::SELECTION_MOTION);
                tx.scroll_into_view_in(SCROLL, key(id));
            }
        }
    }
}

pub(super) fn view(
    state: &AppState,
    cx: &mut Cx<AppState>,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced: bool,
) -> El {
    let motion = if reduced {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
    let mut strip = row()
        .key("horizontal-tab-strip")
        .w_full()
        .h(theme::HORIZONTAL_STRIP_HEIGHT)
        .pr(theme::SIDEBAR_PADDING)
        .gap(theme::NAV_GAP)
        .center_y()
        .window_drag()
        .on_layout_tx(|state: &mut AppState, rect, tx| {
            if state.sidebar.horizontal_strip_width == Some(rect[2]) {
                tx.live_only();
            } else {
                state.sidebar.horizontal_strip_width = Some(rect[2]);
                state.sidebar.revealed_tab = None;
            }
        });
    #[cfg(target_os = "macos")]
    let inset = cx.titlebar_inset();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let inset = 0.0;
    #[cfg(not(target_os = "linux"))]
    {
        strip = if inset > 0.0 {
            strip.child(header::window_controls(inset, theme))
        } else {
            strip.pl(theme::SIDEBAR_PADDING)
        };
    }
    #[cfg(target_os = "linux")]
    let window_controls = {
        strip = strip.pl(theme::SIDEBAR_PADDING);
        header::linux_window_controls(state, theme)
    };
    #[cfg(target_os = "linux")]
    let inset = (theme::ICON_BUTTON + theme::NAV_GAP) * window_controls.len() as f32;

    let strip_width = state.sidebar.horizontal_strip_width.unwrap_or_else(|| {
        cx.layout_size("window")
            .map_or(theme::WINDOW_MIN_SIZE.0, |size| size[0])
    });
    #[cfg(not(target_os = "linux"))]
    let leading = inset.max(theme::SIDEBAR_PADDING);
    #[cfg(target_os = "linux")]
    let leading = inset + theme::SIDEBAR_PADDING;
    let gaps = if cfg!(target_os = "linux") || inset == 0.0 {
        2.0
    } else {
        3.0
    };
    let viewport_width = strip_width
        - leading
        - theme::SIDEBAR_PADDING
        - theme::ICON_BUTTON
        - theme::ROW_PADDING
        - gaps * theme::NAV_GAP;
    let sections = TabSection::ORDER
        .map(|section| state.tab_drag.preview_items(state.browser.tabs(), section));
    let regular = state
        .browser
        .tabs()
        .iter()
        .filter(|tab| tab.section() == TabSection::Regular)
        .count();
    let favourites = state.browser.tabs().len() - regular;
    let compact_width = section_width(favourites, theme::HORIZONTAL_COMPACT_TAB);
    let min_regular = regular_section_width(regular, theme::HORIZONTAL_TAB_MIN);
    let overflow = compact_width + min_regular > viewport_width + 0.5;
    let reserve = if overflow {
        theme::ICON_BUTTON + theme::NAV_GAP
    } else {
        0.0
    };
    let widths = regular_widths(
        state,
        viewport_width - reserve - compact_width - min_regular,
    );
    let width_of = |section, items: &[PreviewItem]| {
        let tabs = items
            .iter()
            .map(|item| item_width(section, *item, &widths))
            .sum::<f32>();
        let gaps = match section {
            TabSection::Favourite => items.len(),
            TabSection::Regular => items.len().saturating_sub(1),
        } as f32
            * theme::TAB_GAP;
        (tabs + gaps).max(theme::HORIZONTAL_EMPTY_SECTION)
    };
    let regular_width = state
        .browser
        .tabs()
        .iter()
        .filter(|tab| tab.section() == TabSection::Regular)
        .map(|tab| widths[&tab.id()])
        .sum::<f32>()
        + regular.saturating_sub(1) as f32 * theme::TAB_GAP;
    let committed_width = compact_width + regular_width.max(theme::HORIZONTAL_EMPTY_SECTION);
    let preview_width =
        width_of(TabSection::Favourite, &sections[0]) + width_of(TabSection::Regular, &sections[1]);
    // Keep the viewport from contracting beneath the pointer when a drag removes a tab.
    let content_width = committed_width.max(preview_width);
    let mut content = row()
        .key("horizontal-tab-sections")
        .role("tablist")
        .label("Tabs")
        .h(theme::HORIZONTAL_TAB_HEIGHT)
        .center_y();
    for (section, items) in TabSection::ORDER.into_iter().zip(sections) {
        content = content.child(section_view(state, section, &items, &widths, motion, theme));
    }
    let live = state.tab_drag.scroll_live();
    strip = strip
        .child(
            row()
                .key(SCROLL)
                .w(content_width)
                .min_w(0.0)
                .shrink()
                .h_full()
                .center_y()
                .no_window_drag()
                .scroll_x()
                .scroll_edge_fade(theme::SIDEBAR_PADDING)
                .on_layout_tx(move |state: &mut AppState, rect, tx| {
                    state.tab_drag.set_horizontal_viewport(rect);
                    reveal_active(state, tx);
                    if !overflow && state.sidebar.overflow_menu_open {
                        state.sidebar.overflow_menu_open = false;
                    } else {
                        tx.live_only();
                    }
                })
                .on_scroll_offset(move |position| {
                    live.set([position.offset[0], position.max[0]]);
                })
                .on_secondary_press(|state: &mut AppState, press, _| {
                    state.open_context_menu(None, press)
                })
                .child(content),
        )
        .child(
            icon_button(
                "new-tab",
                "New Tab",
                &icons::PLUS,
                theme::ICON_BUTTON,
                theme,
            )
            .shortcut_hint("cmd+t")
            .on_press_tx(app::new_tab),
        );
    if overflow {
        strip = strip.child(overflow_menu(state, cx, theme, overlays));
    }
    strip = strip.child(
        el().key("horizontal-window-drag")
            .grow(1.0)
            .min_w(theme::ROW_PADDING)
            .h_full()
            .window_drag()
            .on_secondary_press(|state: &mut AppState, press, _| {
                state.open_context_menu(None, press);
            }),
    );
    #[cfg(target_os = "linux")]
    {
        strip = strip.children(window_controls);
    }

    let navigation = row()
        .key("horizontal-navigation")
        .w_full()
        .h(theme::HORIZONTAL_NAV_HEIGHT)
        .px(theme::SIDEBAR_PADDING)
        .gap(theme::NAV_GAP)
        .center_y()
        .children(header::navigation(state, theme))
        .child(super::omnibox::address_bar(
            state, cx, theme, reduced, overlays,
        ))
        .child(downloads::trigger(state, theme, overlays));
    drag_overlay(state, theme, overlays, reduced);

    col()
        .key("horizontal-chrome")
        .w_full()
        .transition(if reduced {
            Transition::none()
        } else {
            Transition::opacity()
        })
        .child(strip)
        .child(navigation)
}

fn overflow_menu(state: &AppState, cx: &Cx<AppState>, theme: &Theme, overlays: &mut Vec<El>) -> El {
    let trigger = icon_button(
        "tab-overflow",
        "Hidden tabs",
        &icons::CHEVRON_DOWN,
        theme::ICON_BUTTON,
        theme,
    )
    .on_press(|state: &mut AppState| {
        state.sidebar.overflow_menu_open = !state.sidebar.overflow_menu_open;
        hover::dismiss(state);
    });
    let open = state.sidebar.overflow_menu_open;
    let mut menu = Menu::new("tab-overflow")
        .trigger(trigger)
        .placement(Placement::BottomEnd)
        .min_width(theme::HORIZONTAL_TAB_MAX)
        .open(open)
        .on_close(|state: &mut AppState| state.sidebar.overflow_menu_open = false);
    if open && let Some([left, _, width, _]) = cx.window_layout(SCROLL) {
        let right = left + width;
        let active = state.browser.active_tab().map(Tab::id);
        for tab in state.browser.tabs() {
            let id = tab.id();
            if cx
                .window_layout(&key(id))
                .is_some_and(|rect| rect[0] >= left - 0.5 && rect[0] + rect[2] <= right + 0.5)
            {
                continue;
            }
            menu = menu.item(
                MenuItem::new(tab.label())
                    .lead(tab_icon(state, tab, active == Some(id), theme::ICON, theme))
                    .checked(active == Some(id))
                    .on_select_tx(move |state: &mut AppState, tx| {
                        state.sidebar.overflow_menu_open = false;
                        state.activate(id);
                        tx.scroll_into_view_in(SCROLL, key(id));
                        tx.focus(super::viewport::focus_key(&state.browser), false);
                    }),
            );
        }
    }
    menu.build(&theme.kit(theme.overlay_panel), overlays)
}

fn section_width(count: usize, tab_width: f32) -> f32 {
    (count as f32 * (tab_width + theme::TAB_GAP)).max(theme::HORIZONTAL_EMPTY_SECTION)
}

fn regular_section_width(count: usize, tab_width: f32) -> f32 {
    (count as f32 * tab_width + count.saturating_sub(1) as f32 * theme::TAB_GAP)
        .max(theme::HORIZONTAL_EMPTY_SECTION)
}

fn regular_widths(state: &AppState, spare: f32) -> HashMap<TabId, f32> {
    let tabs = state.browser.tabs();
    let mut preferred = state.sidebar.preferred_tab_widths.borrow_mut();
    let freeze = state.tab_drag.has_preview();
    let mut widths = HashMap::with_capacity(tabs.len());
    let mut extra = 0.0;
    for tab in tabs {
        let id = tab.id();
        let width = if freeze || tab.loading {
            preferred
                .get(&id)
                .copied()
                .unwrap_or_else(|| preferred_width(tab.label()))
        } else {
            preferred_width(tab.label())
        };
        preferred.insert(id, width);
        widths.insert(id, width);
        if tab.section() == TabSection::Regular {
            extra += width - theme::HORIZONTAL_TAB_MIN;
        }
    }
    preferred.retain(|id, _| widths.contains_key(id));
    let fraction = if extra > 0.0 {
        (spare / extra).clamp(0.0, 1.0)
    } else {
        0.0
    };
    for width in widths.values_mut() {
        *width = theme::HORIZONTAL_TAB_MIN + (*width - theme::HORIZONTAL_TAB_MIN) * fraction;
    }
    widths
}

fn preferred_width(title: &str) -> f32 {
    use rune::tok::space;
    // Approximate the single-line label in points. Rune truncates the actual shaped text.
    let label = title
        .chars()
        .take(32)
        .map(|ch| match ch {
            'i' | 'l' | 'I' | ' ' | '.' | ':' | '!' => space::S1,
            'm' | 'w' | 'M' | 'W' => space::S3,
            ch if ch.is_ascii() => space::S2,
            _ => theme::ICON,
        })
        .sum::<f32>();
    let chrome =
        theme::ICON + theme::CLOSE_BUTTON + 1.5 * theme::ROW_PADDING + 2.0 * theme::ROW_GAP;
    (label + chrome).clamp(theme::HORIZONTAL_TAB_MIN, theme::HORIZONTAL_TAB_MAX)
}

fn item_width(section: TabSection, item: PreviewItem, widths: &HashMap<TabId, f32>) -> f32 {
    if section == TabSection::Favourite {
        return theme::HORIZONTAL_COMPACT_TAB;
    }
    let id = match item {
        PreviewItem::Tab(id) | PreviewItem::SourceSlot(id) | PreviewItem::DropSlot(id, _) => id,
    };
    widths
        .get(&id)
        .copied()
        .unwrap_or(theme::HORIZONTAL_TAB_MIN)
}

fn section_view(
    state: &AppState,
    section: TabSection,
    items: &[PreviewItem],
    widths: &HashMap<TabId, f32>,
    motion: Spring,
    theme: &Theme,
) -> El {
    let mut group = row()
        .key(match section {
            TabSection::Favourite => "horizontal-favourites",
            TabSection::Regular => "horizontal-regular",
        })
        .h(theme::HORIZONTAL_TAB_HEIGHT)
        .min_w(theme::HORIZONTAL_EMPTY_SECTION)
        .pr(if section == TabSection::Favourite {
            theme::TAB_GAP
        } else {
            0.0
        })
        .gap(theme::TAB_GAP)
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            state
                .tab_drag
                .set_section_rect(section, rect, state.browser.tabs());
            tx.live_only();
        });

    if items.is_empty() && state.tab_drag.has_preview() {
        group = group.child(
            el().size(theme::HORIZONTAL_EMPTY_SECTION / 2.0, theme::CLOSE_BUTTON)
                .radius(theme::ROW_RADIUS)
                .fill(theme.hover)
                .a11y_hidden(),
        );
    }
    for (index, item) in items.iter().copied().enumerate() {
        let element = match item {
            PreviewItem::Tab(id) => {
                let Some(tab) = state.browser.tab(id) else {
                    continue;
                };
                tab_view(state, tab, theme)
            }
            PreviewItem::SourceSlot(id) => tab_drag_handler(el().key(key(id)), id, vec![id]),
            PreviewItem::DropSlot(id, same_section) => {
                let slot = el()
                    .key(if same_section {
                        key(id)
                    } else {
                        format!("{}-drop", key(id))
                    })
                    .radius(theme::ROW_RADIUS)
                    .fill(theme.tab_drop_indicator)
                    .opacity(0.38)
                    .on_layout_tx(move |state: &mut AppState, rect, tx| {
                        state.tab_drag.record_placeholder(section, rect);
                        tx.live_only();
                    });
                if same_section {
                    tab_drag_handler(slot, id, vec![id])
                } else {
                    slot
                }
            }
        };
        group = group.child(
            element
                .size(
                    item_width(section, item, widths),
                    theme::HORIZONTAL_TAB_HEIGHT,
                )
                .position_in_set(index + 1, items.len())
                .motion(motion)
                .animate_layout(),
        );
    }
    group
}

fn tab_view(state: &AppState, tab: &Tab, theme: &Theme) -> El {
    let id = tab.id();
    let active = state.browser.active_tab().is_some_and(|tab| tab.id() == id);
    let section = tab.section();
    let label = tab.label();
    let icon = tab_icon(state, tab, active, theme::ICON, theme);
    let mut tile = if section == TabSection::Regular {
        let close = icon_button_style(
            &format!("close-{}", id.raw()),
            "Close tab",
            &icons::CLOSE,
            theme::CLOSE_BUTTON,
            theme,
        )
        .hover(theme.hover)
        .on_press(move |state: &mut AppState| state.close(id));
        row_button(
            &key(id),
            icon,
            label,
            if active { theme.text } else { theme.text_muted },
            theme,
        )
        .hover_group()
        .press_scale(1.0)
        .child(if active {
            close
        } else {
            close.group_opacity(0.0, 1.0)
        })
    } else {
        el().key(key(id))
            .center()
            .radius(theme::ROW_RADIUS)
            .continuous_corners()
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .child(icon)
    };
    tile = tile
        .radius(theme::HORIZONTAL_TAB_RADIUS)
        .continuous_corners();
    if active {
        tile = tile.fill(theme.tab_active).shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    } else if section == TabSection::Favourite {
        tile = tile.fill(theme.address_field);
    }
    if state.browser.displayed_split().is_some_and(|split| {
        split
            .panes(state.browser.split_view_limit)
            .iter()
            .all(Option::is_some)
            && split.panes.contains(&Some(id))
    }) {
        tile = tile.child(
            row()
                .pin(
                    Some(0.0),
                    None,
                    Some(0.0),
                    Some(theme::HORIZONTAL_SPLIT_MARKER_HEIGHT),
                )
                .justify_center()
                .inert()
                .a11y_hidden()
                .child(
                    el().key(format!("split-tab-marker-{}", id.raw()))
                        .size(theme::ICON, theme::HORIZONTAL_SPLIT_MARKER_HEIGHT)
                        .radius(theme::HORIZONTAL_SPLIT_MARKER_HEIGHT / 2.0)
                        .fill(if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                ),
        );
    }
    if state.tab_drag.is_hidden(id) {
        tile = tile.opacity(0.0).opacity_motion(rune::Anim::INSTANT);
    }
    let tile = tile
        .label(label)
        .role("tab")
        .checked(active)
        .focusable(true)
        .on_press_tx(move |state: &mut AppState, tx| {
            state.activate(id);
            tx.color_motion(theme::SELECTION_MOTION);
            if state.browser.split.is_some() {
                tx.focus(super::viewport::focus_key(&state.browser), false);
            }
        })
        .on_middle_press(move |state: &mut AppState, _| state.close(id))
        .on_secondary_press(move |state: &mut AppState, press, _| {
            state.open_context_menu(Some(id), press)
        })
        .on_layout_tx(move |state: &mut AppState, rect, tx| {
            let settled = state.tab_drag.record_tab_rect(section, id, rect);
            if settled {
                tx.animated();
                tx.motion(theme::TAB_REORDER_MOTION);
                tx.wake_after(std::time::Duration::ZERO);
            } else {
                tx.live_only();
            }
        });
    hover::attach(tab_drag_handler(tile, id, vec![id]), id)
}
