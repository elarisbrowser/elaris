// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark morph into each other: the outgoing glyph
//! shrinks and blurs away as the incoming one pops back up, and on press the arrow spins out as
//! the stop mark spins in.

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
    if !loading {
        // Leaving, the arrow hands over to its spinning copy below rather than morphing itself.
        let leave = if reduced { morph } else { Transition::none() };
        return button.child(glyph(
            &icons::RELOAD,
            "reload-arrow",
            Transition::asymmetric(morph, leave),
        ));
    }
    if reduced {
        return button.child(glyph(&icons::CLOSE, "reload-stop", morph));
    }
    button
        .child(glyph(
            &ARROW_SPIN_OUT,
            "reload-arrow-out",
            Transition::none(),
        ))
        .child(glyph(&STOP_SPIN_IN, "reload-stop", morph))
}

// The press morph is SMIL, so each half plays once on mount without a rotated layer: the arrow
// turns, shrinks and fades out, and the stop mark turns in on the same beat as it grows and
// pops up and sharpens through its transition.

static ARROW_SPIN_OUT: Icon = Icon {
    name: "reload-arrow-spin-out",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="0 8 8;180 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.4 0 1 1" dur="0.25s" fill="freeze"/><animate attributeName="opacity" values="1;0" keyTimes="0;1" dur="0.25s" fill="freeze"/><g transform="translate(8 8)"><g><animateTransform attributeName="transform" type="scale" values="1;0.6" keyTimes="0;1" dur="0.25s" fill="freeze"/><g transform="translate(-8 -8)"><path d="M12.5 8A4.5 4.5 0 1 1 11.2 4.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><path d="M12.5 2.5V5.5H9.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></g></g></g></g>"#,
};

static STOP_SPIN_IN: Icon = Icon {
    name: "reload-stop-spin-in",
    view: 16.0,
    body: r#"<g><animateTransform attributeName="transform" type="rotate" values="-180 8 8;0 8 8" keyTimes="0;1" calcMode="spline" keySplines="0.2 0 0 1" dur="0.4s"/><path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></g>"#,
};
