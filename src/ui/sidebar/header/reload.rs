// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark stay mounted and morph into each other: on
//! press the arrow spins as it shrinks and blurs away, and the stop mark grows in its place.

use std::f32::consts::TAU;

use rune::{Anim, El, icon};

use crate::app::AppState;
use crate::browser::Browser;
use crate::ui::controls;
use crate::ui::hover_highlight;
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

use super::Navigate;

pub(super) fn button(state: &AppState, theme: &Theme) -> El {
    let tab = state.browser.active_tab();
    let loading = tab.is_some_and(|tab| tab.loading);
    let enabled = tab.is_some_and(|tab| !tab.is_internal());
    let reduced = state.sidebar.hover.reduced.get();
    let (label, navigate): (&str, Navigate) = if loading {
        ("Stop loading", Browser::stop_loading)
    } else {
        ("Reload", Browser::reload)
    };
    let frame = controls::icon_button_frame("reload", label, theme::ICON_BUTTON, theme);
    let button = if enabled {
        hover_highlight::attach(frame, "reload", hover_highlight::Group::Controls)
            .on_press(move |state: &mut AppState| state.with_active_tab(navigate))
    } else {
        frame.disabled(true).opacity(theme.disabled_opacity)
    };
    // A full turn looks the same as none, so the arrow can snap back while it is invisible and
    // spin forwards again on the next press.
    let arrow = icon(&icons::RELOAD, theme::ICON)
        .key("reload-arrow")
        .color(theme.icon)
        .rotate(if loading { TAU } else { 0.0 })
        .rotate_motion(if loading && !reduced {
            theme::RELOAD_SPIN
        } else {
            Anim::INSTANT
        });
    let cross = icon(&icons::CLOSE, theme::ICON)
        .key("reload-stop-cross")
        .color(theme.icon);
    button
        .child(status_glyph(arrow, !loading, reduced))
        .child(status_glyph(cross, loading, reduced))
}

/// A glyph centred in the reload button. The hidden one shrinks, blurs and fades away.
fn status_glyph(mark: El, shown: bool, reduced: bool) -> El {
    let inset = (theme::ICON_BUTTON - theme::ICON) / 2.0;
    let motion = if reduced {
        Anim::INSTANT
    } else {
        theme::STATUS_GLYPH
    };
    let mut mark = mark
        .absolute(inset, inset)
        .opacity(if shown { 1.0 } else { 0.0 })
        .scale(if shown { 1.0 } else { theme::GLYPH_MORPH_SCALE })
        .blur(if shown { 0.0 } else { theme::GLYPH_MORPH_BLUR })
        .opacity_motion(motion)
        .scale_motion(motion)
        .blur_motion(motion);
    if !shown {
        mark = mark.a11y_hidden();
    }
    mark
}
