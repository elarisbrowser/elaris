// SPDX-License-Identifier: GPL-3.0-only

//! Two-row browser chrome using the shared browser, tab actions and drag gesture.

use rune::{Cx, El, Spring, Transition, Tx, col, el, row};

use super::controls::{icon_button, row_button};
use super::sidebar::drag::PreviewItem;
use super::sidebar::tabs::{drag_overlay, tab_drag_handler, tab_icon};
use super::sidebar::{self, header, hover};
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
        state.sidebar.horizontal_tab_width = None;
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
        .window_drag();
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

    let viewport_width = state.sidebar.horizontal_tab_width.unwrap_or_else(|| {
        cx.layout_size("window")
            .map_or(theme::WINDOW_MIN_SIZE.0, |size| size[0])
            - inset
            - 2.0 * (theme::SIDEBAR_PADDING + theme::NAV_GAP)
            - theme::ICON_BUTTON
    });
    let sections = TabSection::ORDER
        .map(|section| state.tab_drag.preview_items(state.browser.tabs(), section));
    let regular = state
        .browser
        .tabs()
        .iter()
        .filter(|tab| tab.section() == TabSection::Regular)
        .count();
    let favourites = state.browser.tabs().len() - regular;
    let regular_width = regular_width(viewport_width, [favourites, regular]);
    let mut content = row()
        .key("horizontal-tab-sections")
        .role("tablist")
        .label("Tabs")
        .h(theme::HORIZONTAL_TAB_HEIGHT)
        .center_y();
    for (section, items) in TabSection::ORDER.into_iter().zip(sections) {
        content = content.child(section_view(
            state,
            section,
            &items,
            regular_width,
            motion,
            theme,
        ));
    }
    let live = state.tab_drag.scroll_live();
    strip = strip
        .child(
            row()
                .key(SCROLL)
                .w(0.0)
                .min_w(0.0)
                .grow(1.0)
                .h_full()
                .center_y()
                .scroll_x()
                .scroll_edge_fade(theme::SIDEBAR_PADDING)
                .on_layout_tx(|state: &mut AppState, rect, tx| {
                    state.tab_drag.set_horizontal_viewport(rect);
                    if state.sidebar.horizontal_tab_width == Some(rect[2]) {
                        tx.live_only();
                    } else {
                        state.sidebar.horizontal_tab_width = Some(rect[2]);
                        state.sidebar.revealed_tab = None;
                        reveal_active(state, tx);
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
        .child(downloads::trigger(state, theme, overlays))
        .child(sidebar::appearance(state.settings.appearance, theme));
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

fn regular_width(available: f32, [favourites, regular]: [usize; 2]) -> f32 {
    let compact_width = if favourites == 0 {
        theme::HORIZONTAL_EMPTY_SECTION
    } else {
        favourites as f32 * (theme::HORIZONTAL_COMPACT_TAB + theme::TAB_GAP)
    };
    ((available - compact_width) / regular.max(1) as f32 - theme::TAB_GAP)
        .clamp(theme::HORIZONTAL_TAB_MIN, theme::HORIZONTAL_TAB_MAX)
}

fn section_view(
    state: &AppState,
    section: TabSection,
    items: &[PreviewItem],
    regular_width: f32,
    motion: Spring,
    theme: &Theme,
) -> El {
    let width = if section == TabSection::Regular {
        regular_width
    } else {
        theme::HORIZONTAL_COMPACT_TAB
    };
    let mut group = row()
        .key(match section {
            TabSection::Favourite => "horizontal-favourites",
            TabSection::Regular => "horizontal-regular",
        })
        .h(theme::HORIZONTAL_TAB_HEIGHT)
        .min_w(theme::HORIZONTAL_EMPTY_SECTION)
        .pr(theme::TAB_GAP)
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
                .size(width, theme::HORIZONTAL_TAB_HEIGHT)
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
    let icon = tab_icon(tab, active, theme::ICON, theme);
    let mut tile = if section == TabSection::Regular {
        let close = icon_button(
            &format!("close-{}", id.raw()),
            "Close tab",
            &icons::CLOSE,
            theme::CLOSE_BUTTON,
            theme,
        )
        .on_press(move |state: &mut AppState| state.close(id));
        row_button(
            &key(id),
            icon,
            label,
            if active { theme.text } else { theme.text_muted },
            theme,
        )
        .hover_group()
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
            .hover(theme.hover)
            .press(theme.press)
            .press_scale(theme::PRESS_SCALE)
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .child(icon)
    };
    if active {
        tile = tile.fill(theme.tab_active).shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    } else if section == TabSection::Favourite {
        tile = tile.fill(theme.address_field);
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
