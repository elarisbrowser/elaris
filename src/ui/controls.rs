// SPDX-License-Identifier: GPL-3.0-only

//! The interface's shared control shapes: square icon buttons, and full-width rows that tabs and
//! sidebar buttons share.

use rune::tok::type_;
use rune::{Color, El, Icon, Transition, el, icon, row, text};

use super::hover_highlight::{self, Group};
use super::icons;
use super::theme::{self, Theme};

/// A square button showing `glyph`, labelled for assistive technology and its tooltip.
pub(crate) fn icon_button(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    size: f32,
    theme: &Theme,
) -> El {
    hover_highlight::attach(
        icon_button_style(key, label, glyph, size, theme),
        key,
        Group::Controls,
    )
}

pub(super) fn icon_button_style(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    size: f32,
    theme: &Theme,
) -> El {
    icon_button_frame(key, label, size, theme).child(icon(glyph, theme::ICON).color(theme.icon))
}

/// An icon button's surface without its glyph, for a caller that stacks its own.
pub(super) fn icon_button_frame(key: &str, label: &str, size: f32, theme: &Theme) -> El {
    el().key(key)
        .size(size, size)
        .center()
        .radius(theme::ICON_BUTTON_RADIUS)
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .label(label)
        .help(label)
}

/// `glyph`, or the confirmation tick once `confirmed`: the tick draws itself on as the glyph
/// shrinks and blurs away. `key` names both so each keeps its own transition.
pub(crate) fn confirm_glyph(
    key: &str,
    glyph: &'static Icon,
    size: f32,
    confirmed: bool,
    theme: &Theme,
    reduced: bool,
) -> El {
    if confirmed {
        rune::canvas(
            rune::Shape::new()
                .stroke(icons::TICK.to_vec(), theme::TICK_STROKE, theme.icon)
                .view_box(theme::ICON, theme::ICON),
        )
        .key(format!("{key}-tick"))
        .size(size, size)
        .transition(if reduced {
            Transition::none()
        } else {
            Transition::asymmetric(Transition::draw(), theme::glyph_morph(false))
        })
    } else {
        icon(glyph, size)
            .key(format!("{key}-glyph"))
            .color(theme.icon)
            .transition(theme::glyph_morph(reduced))
    }
}

/// A standalone icon button that stays legible over webpage content.
pub(crate) fn floating_icon_button(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    size: f32,
    theme: &Theme,
) -> El {
    icon_button(key, label, glyph, size, theme)
        .fill(theme.overlay_panel)
        .border(1.0, theme.control_border)
}

/// A full-width row with an icon and a label, for a tab or a button shaped like one. Callers
/// add what follows the label.
pub(crate) fn row_button(
    key: &str,
    leading: impl Into<Option<El>>,
    label: &str,
    color: Color,
    theme: &Theme,
) -> El {
    let row = row()
        .key(key)
        .w_full()
        .h(theme::ROW_HEIGHT)
        .pl(theme::ROW_PADDING)
        .pr(theme::ROW_PADDING / 2.0)
        .gap(theme::ROW_GAP)
        .center_y()
        .radius(theme::ROW_RADIUS)
        .continuous_corners()
        .press(theme.press)
        .press_scale(theme::PRESS_SCALE)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .label(label)
        .children(leading.into())
        .child(
            text(label)
                .key("label")
                .font(type_::BODY)
                .color(color)
                .single_line()
                .truncate()
                .grow(1.0),
        );
    hover_highlight::attach(row, key, Group::Controls)
}
