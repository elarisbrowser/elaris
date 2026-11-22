// SPDX-License-Identifier: GPL-3.0-only

//! The mini window: one page under a slim header with navigation, the site, and a way to move
//! the page into a full window.

use rune::tok::type_;
use rune::{Cx, El, row, text};

use super::sidebar::header;
use super::theme::{self, Theme};
use super::{controls, icons, page_dialog, peek, permission_prompt, viewport};
use crate::address::compact_address;
use crate::app::{AppState, open_in_browser};

pub(super) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state.settings.appearance.is_dark(cx.system_dark());
    let theme = theme::palette(
        dark,
        state.settings.glass_enabled,
        state.settings.tint_opacity,
        state.settings.accent_color,
    );
    if state.settings.reduce_motion {
        cx.reduced_motion(true);
    }
    let reduced_motion = cx.is_reduced_motion();
    state.browser.note_scale(cx.env().scale);
    state.browser.note_dark(dark);
    let mut overlays = Vec::new();

    let fullscreen = state.browser.fullscreen_tab().is_some();
    let page = viewport::view(state, &theme, &mut overlays, reduced_motion);
    let mut window = super::window_surface(state, &theme, cfg!(elaris_ui = "linux"));
    if fullscreen {
        window = window.child(page);
    } else if cfg!(elaris_ui = "linux") {
        // The header takes its own row above the page.
        window = window.child(toolbar(state, &theme, cx)).child(
            row()
                .key("window-content")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(page),
        );
    } else {
        // The header floats over the native titlebar band, where the page leaves room for it.
        window = window.child(page).child(toolbar(state, &theme, cx));
    }
    if state.browser.peek.is_some() {
        window = window.child(peek::backdrop());
    }
    window = window.child(peek::layer(state, &theme, reduced_motion));
    viewport::menu::view(state, &theme, &mut overlays);
    header::history_menu(state, &theme, &mut overlays);
    overlays.extend(permission_prompt::view(
        state,
        &theme,
        viewport::page_origin(state),
    ));
    overlays.extend(permission_prompt::fullscreen_notice(state, &theme));
    super::form_picker::view(state, &theme, &mut overlays, reduced_motion);
    page_dialog::view(state, &theme, &mut overlays);
    let (hover_enter, hover_exit) = if reduced_motion {
        (rune::Anim::INSTANT, rune::Anim::INSTANT)
    } else {
        (theme::HOVER_ENTER, theme::HOVER_EXIT)
    };
    rune_kit::root(
        window.hover_motion_within(hover_enter, hover_exit),
        overlays,
    )
}

/// Window controls, navigation, the page's site and the button that opens it in a full window.
fn toolbar(state: &AppState, theme: &Theme, _cx: &mut Cx<AppState>) -> El {
    let site = state
        .browser
        .active_tab()
        .map_or("", |tab| compact_address(&tab.url));
    let mut bar = row()
        .key("mini-header")
        .h(theme::HEADER_HEIGHT)
        .pr(theme::SIDEBAR_PADDING)
        .gap(theme::NAV_GAP)
        .center_y()
        .w_full()
        .window_drag();
    #[cfg(elaris_ui = "linux")]
    {
        bar = bar.raise();
    }
    #[cfg(not(elaris_ui = "linux"))]
    {
        bar = bar
            .absolute(0.0, 0.0)
            .child(header::window_controls(_cx.titlebar_inset(), theme));
    }
    bar = bar.children(header::navigation(state, theme)).child(
        text(site)
            .key("mini-site")
            .font(type_::LABEL)
            .color(theme.text_muted)
            .single_line()
            .truncate()
            .grow(1.0)
            .label("Site"),
    );
    let open = controls::icon_button(
        "mini-open-in-elaris",
        "Open in Elaris",
        &icons::OPEN_TAB,
        theme::ICON_BUTTON,
        theme,
    )
    .on_press_tx(open_in_browser);
    bar = bar.child(open);
    #[cfg(elaris_ui = "linux")]
    {
        bar = bar.children(header::linux_window_controls(state, theme));
    }
    bar
}
