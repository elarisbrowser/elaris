// SPDX-License-Identifier: GPL-3.0-only

//! The page-zoom chip and the menu that changes it.

use crate::ui::menu::{Menu, MenuItem};
use rune::{El, Transition, el, text};

use crate::app::AppState;
use crate::ui::theme::{self, Theme};

/// Hides the zoom chip once its time is up.
pub(crate) fn settle_zoom_chip(state: &mut AppState, clock: f64) -> bool {
    match state.zoom_chip {
        Some((_, until)) if clock >= until => {
            state.zoom_chip = None;
            true
        }
        _ => false,
    }
}

/// The page-zoom chip with its menu. It shows for a moment after the active page's zoom changes,
/// and stays while its menu is open.
pub(in crate::ui) fn zoom_indicator(
    state: &AppState,
    theme: &Theme,
    overlays: &mut Vec<El>,
) -> Option<El> {
    let tab = state
        .browser
        .active_tab()
        .filter(|tab| !tab.is_internal())?;
    let id = tab.id();
    let flashing = state.zoom_chip.is_some_and(|(shown, _)| shown == id);
    if !flashing && state.zoom_menu != Some(id) {
        return None;
    }
    let caption = format!("{}%", state.browser.page_zoom(tab));
    let reduced = state.sidebar.hover.reduced.get();
    let trigger = el()
        .key("zoom-indicator")
        .w(theme::ZOOM_CHIP_WIDTH)
        .h(theme::ICON_BUTTON)
        .transition(if reduced {
            Transition::opacity()
        } else {
            Transition::scale(0.96).combined(Transition::opacity())
        })
        .center()
        .radius(theme::ICON_BUTTON_RADIUS)
        .press(theme.press)
        .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
        .label(format!("Page zoom {caption}"))
        .help("Page zoom")
        .child(
            text(caption)
                .font(rune::tok::type_::BODY)
                .color(theme.text)
                .single_line(),
        )
        .on_press(move |state: &mut AppState| {
            state.dismiss_context_menu();
            state.navigation_menu = None;
            state.page_menu = Default::default();
            state.zoom_menu = (state.zoom_menu != Some(id)).then_some(id);
        });
    let mut menu = Menu::new("page-zoom")
        .trigger(trigger)
        .open(state.zoom_menu == Some(id))
        .on_close(|state: &mut AppState| state.zoom_menu = None);
    for (label, delta) in [("Zoom In", 10), ("Zoom Out", -10), ("Reset Page Zoom", 0)] {
        menu = menu.item(MenuItem::new(label).on_select(move |state: &mut AppState| {
            state.zoom_menu = None;
            if state.browser.active_tab().is_some_and(|tab| tab.id() == id) {
                state.browser.change_zoom(id, delta);
            }
        }));
    }
    Some(menu.build(state, theme, overlays))
}
