// SPDX-License-Identifier: GPL-3.0-only

//! Floating drag visuals for tabs: the overlay layer that follows the pointer and settles into place.

use rune::tok::type_;
use rune::{El, Spring, el, icon, row, text};

use super::icon::tab_icon;
use crate::app::AppState;
use crate::tab::{Tab, TabId, TabSection};
use crate::ui::icons;
use crate::ui::sidebar::drag::DragVisual;
use crate::ui::theme::{self, Theme};

pub(in crate::ui) fn drag_overlay(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
    reduced_motion: bool,
) {
    let active = state.browser.active_tab().map(Tab::id);
    let show_inline = {
        #[cfg(elaris_ui = "macos")]
        let show = !crate::platform::macos::drag_overlay::shown();
        #[cfg(not(elaris_ui = "macos"))]
        let show = true;
        show
    };
    let incoming = show_inline
        .then(|| {
            crate::app::cross_window_drag::incoming_visual(state.window_id, |appearance, visual| {
                floating_drag_preview(appearance, visual, theme, reduced_motion)
            })
        })
        .flatten();
    if let Some(incoming) = incoming {
        overlays.push(incoming);
    } else if show_inline
        && !crate::app::cross_window_drag::shows_elsewhere(state.window_id)
        && let Some(visual) = state.tab_drag.visual()
        && let Some(tab) = state.browser.tab(visual.tab)
    {
        let peer = state.browser.split.and_then(|split| {
            if !state.horizontal_tabs() && split.panes.contains(&Some(visual.tab)) {
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
        if state.tab_drag.is_incoming_settle(visual.tab) {
            let icon_size = if visual.section == TabSection::Favourite {
                theme::FAVOURITE_ICON
            } else {
                theme::ICON
            };
            // Start with the floating card's colors, then settle to the active row's appearance.
            let active = visual.settling;
            overlays.push(single_drag_visual(
                tab.label(),
                tab_icon(state, tab, active, icon_size, theme),
                active,
                visual,
                theme,
                reduced_motion,
                true,
            ));
        } else {
            overlays.push(tab_drag_visual(
                state,
                tab,
                peer,
                active,
                visual,
                theme,
                reduced_motion,
            ));
        }
    }
}

fn tab_drag_visual(
    state: &AppState,
    tab: &Tab,
    peer: Option<&Tab>,
    active_tab: Option<TabId>,
    visual: DragVisual,
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let favourite = visual.section == TabSection::Favourite;
    let icon_size = if favourite {
        theme::FAVOURITE_ICON
    } else {
        theme::ICON
    };
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
                    .child(tab_icon(state, member, member_active, icon_size, theme))
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
    single_drag_visual(
        tab.label(),
        tab_icon(state, tab, active, icon_size, theme),
        active,
        visual,
        theme,
        reduced_motion,
        false,
    )
}

pub(crate) fn floating_drag_preview(
    appearance: &crate::app::cross_window_drag::DragAppearance,
    visual: DragVisual,
    theme: &Theme,
    reduced_motion: bool,
) -> El {
    let size = if visual.section == TabSection::Favourite {
        theme::FAVOURITE_ICON
    } else {
        theme::ICON
    };
    let icon = appearance.favicon.as_ref().map_or_else(
        || icon(&icons::PAGE, size).color(theme.text),
        |favicon| el().size(size, size).center().pixels(favicon),
    );
    single_drag_visual(
        &appearance.label,
        icon,
        false,
        visual,
        theme,
        reduced_motion,
        false,
    )
}

fn single_drag_visual(
    label: &str,
    icon: El,
    active: bool,
    visual: DragVisual,
    theme: &Theme,
    reduced_motion: bool,
    incoming: bool,
) -> El {
    let favourite = visual.section == TabSection::Favourite;
    let title_visible = !favourite;
    let title_width = if favourite { 0.0 } else { 1.0 };
    let motion = if reduced_motion {
        Spring::NONE
    } else {
        theme::TAB_REORDER_MOTION
    };
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
        .child(icon.key("drag-tab-icon").motion(motion))
        .child(
            text(label)
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
    if !incoming || !visual.settling {
        row = row.shadow(
            theme::TAB_DRAG_SHADOW_Y,
            theme::TAB_DRAG_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
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
