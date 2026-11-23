// SPDX-License-Identifier: GPL-3.0-only

//! The split view's draggable dividers, which resize the panes on either side.
use crate::platform::haptics::{self, Haptic};

use crate::app::AppState;
use crate::browser::split;
use crate::ui::theme::Theme;
use rune::{El, Phase, el};

pub(super) fn divider(live: bool, theme: &Theme) -> El {
    let handle = el()
        .key("split-divider")
        .w(split::DIVIDER)
        .h_full()
        .center()
        .child(
            el().w(1.0)
                .h(24.0)
                .radius(0.5)
                .fill(theme.overlay_border)
                .inert(),
        );

    if !live {
        return handle;
    }
    handle
        .cursor(rune::Cursor::ResizeColumn)
        .label("Resize split panes")
        .on_drag(|state: &mut AppState, drag, tx| {
            if drag.phase == Phase::Down {
                if let Some(split) = state.browser.displayed_split() {
                    drag.data = split::widths(state.browser.content_width(), split.ratio)[0];
                }
            } else if !drag.cancelled() {
                if state.browser.resize_split(drag.data + drag.dx) {
                    haptics::play(Haptic::Snap);
                }
                tx.instant();
            }
        })
}

pub(super) fn grid_divider(row: usize, columns: usize, theme: &Theme) -> El {
    divider(false, theme)
        .key(format!("split-grid-divider-{row}"))
        .cursor(rune::Cursor::ResizeColumn)
        .label("Resize split panes")
        .on_drag(move |state: &mut AppState, drag, tx| {
            let available =
                (state.browser.content_width() - split::DIVIDER * (columns - 1) as f32).max(1.0);
            if let Some(split) = &mut state.browser.split {
                if drag.phase == Phase::Down {
                    drag.data = split::grid_first_share(columns, split.ratio);
                } else if !drag.cancelled() {
                    let share = drag.data + drag.dx / available;
                    let equal = 1.0 / columns as f32;
                    let ratio = (share - equal + 0.5).clamp(0.0, 1.0);
                    if matches!(ratio, 0.0 | 1.0) && ratio != split.ratio {
                        haptics::play(Haptic::Snap);
                    }
                    split.ratio = ratio;
                    tx.instant();
                }
            }
        })
}
