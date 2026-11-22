// SPDX-License-Identifier: GPL-3.0-only

//! The reload control. The arrow and the stop mark stay mounted and crossfade.

use rune::{El, icon};

use crate::app::AppState;
use crate::browser::Browser;
use crate::ui::animated_icon;
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
        hover_highlight::attach_with(
            frame,
            "reload",
            hover_highlight::Group::Controls,
            animated_icon::hover_change("reload"),
        )
        .on_press(move |state: &mut AppState| state.with_active_tab(navigate))
    } else {
        frame.disabled(true).opacity(theme.disabled_opacity)
    };
    let arrow = animated_icon::glyph(
        state,
        &animated_icon::RELOAD,
        "reload",
        "reload-arrow",
        theme::ICON,
        theme.icon,
        reduced,
    );
    let cross = icon(&icons::CLOSE, theme::ICON)
        .key("reload-stop-cross")
        .color(theme.icon);
    button
        .child(status_glyph(arrow, !loading, reduced))
        .child(status_glyph(cross, loading, reduced))
}

/// A glyph centred in the reload button. The hidden one fades out and leaves the tree.
fn status_glyph(mark: El, shown: bool, reduced: bool) -> El {
    let inset = (theme::ICON_BUTTON - theme::ICON) / 2.0;
    let mut mark = mark
        .absolute(inset, inset)
        .opacity(if shown { 1.0 } else { 0.0 })
        .opacity_motion(if reduced {
            rune::Anim::INSTANT
        } else {
            theme::STATUS_GLYPH
        });
    if !shown {
        mark = mark.a11y_hidden();
    }
    mark
}
