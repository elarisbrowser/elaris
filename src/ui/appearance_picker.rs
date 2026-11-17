// SPDX-License-Identifier: GPL-3.0-only

//! A segmented control choosing System, Light or Dark.

use rune::tok::type_;
use rune::{El, el, row, text};

use super::theme::{self, Theme};
use crate::app::AppState;
use crate::appearance::Appearance;

pub(crate) fn view(current: Appearance, theme: &Theme) -> El {
    let mut picker = row()
        .key("appearance")
        .h(theme::CONTROL_HEIGHT)
        .px(theme::CONTROL_PADDING)
        .gap(theme::CONTROL_PADDING)
        .center_y()
        .radius(theme::CONTROL_RADIUS)
        .fill(theme.control_off)
        .label("Appearance");
    for appearance in Appearance::ALL {
        picker = picker.child(segment(appearance, appearance == current, theme));
    }
    picker
}

fn segment(appearance: Appearance, selected: bool, theme: &Theme) -> El {
    let label = text(appearance.label())
        .font(type_::LABEL)
        .color(if selected {
            theme.text
        } else {
            theme.text_secondary
        })
        .single_line();
    let segment = el()
        .key(appearance.label())
        .h(theme::SEGMENT_HEIGHT)
        .px(theme::SEGMENT_PADDING)
        .center()
        .radius(theme::SEGMENT_RADIUS)
        .label(appearance.label())
        .on_press(move |state: &mut AppState| state.appearance = appearance)
        .child(label);
    // Only the chosen segment has a thumb; the others show the control's own track.
    if selected {
        segment.fill(theme.indicator)
    } else {
        segment
    }
}
