// SPDX-License-Identifier: GPL-3.0-only

//! The interface's shared control shapes: square icon buttons, and full-width rows that tabs and
//! sidebar buttons share.

use rune::tok::type_;
use rune::{Color, El, Icon, el, icon, row, text};

use super::theme::{self, Theme};

/// A square button showing `glyph`, labelled for assistive technology and its tooltip.
pub(crate) fn icon_button(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    size: f32,
    theme: &Theme,
) -> El {
    el().key(key)
        .size(size, size)
        .center()
        .radius(theme::ICON_BUTTON_RADIUS)
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .label(label)
        .help(label)
        .child(icon(glyph, theme::ICON).color(theme.icon))
}

/// A full-width row with an icon and a label, for a tab or a button shaped like one. Callers
/// add what follows the label.
pub(crate) fn row_button(key: &str, leading: El, label: &str, color: Color, theme: &Theme) -> El {
    row()
        .key(key)
        .w_full()
        .h(theme::ROW_HEIGHT)
        .pl(theme::ROW_PADDING)
        .pr(theme::ROW_PADDING / 2.0)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .hover(theme.hover)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .label(label)
        .child(leading)
        .child(
            text(label)
                .font(type_::BODY)
                .color(color)
                .single_line()
                .truncate()
                .grow(1.0),
        )
}
