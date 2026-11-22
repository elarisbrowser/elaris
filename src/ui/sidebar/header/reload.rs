// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark morph into each other: on press the arrow
//! spins as it shrinks and blurs away, and the stop mark grows in its place.

use std::f32::consts::TAU;

use rune::{Anim, El, el, icon};

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
    let inset = (theme::ICON_BUTTON - theme::ICON) / 2.0;
    let morph = theme::glyph_morph(reduced);
    // The arrow sits in a turntable that spins as the arrow leaves. A full turn looks the same as
    // none, so the empty turntable snaps back while loading and spins forwards on the next press.
    let arrow = (!loading).then(|| {
        icon(&icons::RELOAD, theme::ICON)
            .key("reload-arrow")
            .color(theme.icon)
            .transition(morph)
    });
    let turntable = el()
        .key("reload-arrow-spin")
        .size(theme::ICON, theme::ICON)
        .absolute(inset, inset)
        .rotate(if loading { TAU } else { 0.0 })
        .rotate_motion(if loading && !reduced {
            theme::RELOAD_SPIN
        } else {
            Anim::INSTANT
        })
        .children(arrow);
    let cross = loading.then(|| {
        icon(&icons::CLOSE, theme::ICON)
            .key("reload-stop-cross")
            .color(theme.icon)
            .absolute(inset, inset)
            .transition(morph)
    });
    button.child(turntable).children(cross)
}
