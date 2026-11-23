// SPDX-License-Identifier: GPL-3.0-only

//! Presentation and membership for the two tabs in a split group.

use rune::{El, row};

use crate::app::AppState;
use crate::tab::{TabId, TabSection};
use crate::ui::theme::{self, Theme};

use super::row::tab_row;

#[derive(Clone, Copy)]
pub(super) enum TabRowStyle {
    Standalone,
    Grouped { selected: bool },
}

pub(super) fn members(state: &AppState, section: TabSection) -> Option<[TabId; 2]> {
    let [Some(left), Some(right), ..] = state.browser.split?.panes else {
        return None;
    };
    let members = [left, right];
    members
        .iter()
        .all(|id| {
            state
                .browser
                .tab(*id)
                .is_some_and(|tab| tab.section() == section)
        })
        .then_some(members)
}

pub(super) fn view(
    state: &AppState,
    section: TabSection,
    members: [TabId; 2],
    index: usize,
    count: usize,
    theme: &Theme,
) -> El {
    let active = state.browser.active_tab().map(|tab| tab.id());
    let selected = active.is_some_and(|active| members.contains(&active));
    let mut group = row()
        .key("split-tab-group")
        .w_full()
        .h(theme::ROW_HEIGHT)
        .gap(2.0)
        .px(3.0)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .fill(if selected {
            theme.tab_active
        } else {
            theme.address_field
        });
    if selected {
        group = group.shadow(
            theme::ACTIVE_SHADOW_Y,
            theme::ACTIVE_SHADOW_BLUR,
            theme.tab_active_shadow,
        );
    }
    for (offset, id) in members.into_iter().enumerate() {
        if let Some(tab) = state.browser.tab(id) {
            group = group.child(
                tab_row(
                    state,
                    tab,
                    section,
                    Some(id) == active,
                    index + offset,
                    count,
                    theme,
                    TabRowStyle::Grouped { selected },
                )
                .w(0.0)
                .grow(1.0)
                .h(theme::ROW_HEIGHT - 6.0)
                .radius(theme::ROW_RADIUS - 3.0),
            );
        }
    }
    group
}
