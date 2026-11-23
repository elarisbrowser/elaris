// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark morph into each other: the outgoing glyph
//! shrinks and blurs away as the incoming one pops back up to full size.

use rune::{El, Icon, Transition, icon};

use crate::app::AppState;
use crate::browser::Browser;
use crate::ui::components::controls;
use crate::ui::components::hover_highlight;
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
    let glyph = |icon_: &'static Icon, key: &str, transition: Transition| {
        icon(icon_, theme::ICON)
            .key(key)
            .color(theme.icon)
            .absolute(inset, inset)
            .transition(transition)
    };
    let (glyph_icon, key) = if loading {
        (&icons::CLOSE, "reload-stop")
    } else {
        (&icons::RELOAD, "reload-arrow")
    };
    button.child(glyph(glyph_icon, key, morph))
}
