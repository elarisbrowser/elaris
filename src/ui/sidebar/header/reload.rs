// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark morph into each other: each shrinks and blurs
//! away as the other grows in, and on press the stop mark spins in as it arrives.

use rune::{El, Icon, icon};

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
    let glyph: &'static Icon = match (loading, reduced) {
        (false, _) => &icons::RELOAD,
        (true, false) => &STOP_SPIN,
        (true, true) => &icons::CLOSE,
    };
    let key = if loading {
        "reload-stop"
    } else {
        "reload-arrow"
    };
    button.child(
        icon(glyph, theme::ICON)
            .key(key)
            .color(theme.icon)
            .absolute(inset, inset)
            .transition(morph),
    )
}

/// The stop mark, spinning in from a half turn as it replaces the reload arrow. The spin is SMIL
/// so it plays once on mount without a rotated layer.
static STOP_SPIN: Icon = Icon {
    name: "reload-stop-spin",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="-180 8 8;0 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.2 0 0 1" dur="0.4s"/><path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></g>"#,
};
