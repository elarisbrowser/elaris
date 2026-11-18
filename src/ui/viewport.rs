// SPDX-License-Identifier: GPL-3.0-only

//! The active tab's page in a rounded container inset from the window's edges, and the pointer,
//! wheel and key input it takes.

pub(crate) mod menu;
use crate::{engine::ViewCommand, tab::TabId};
use elaris::ladybird::{Edit, MouseKind};
use rune::{El, Phase, Transition, el};
use rune::{Ime, Key, KeyEvent, Tx};

use super::VIEWPORT;
use super::theme::{self, Theme};
use crate::app::AppState;
use rune::PointerButton;

/// The container's padding holds the page in from the window's edges, the sidebar being its
/// left; the page's own element clips to rounded corners and takes input in the page's
/// coordinates.
pub(crate) fn view(state: &AppState, theme: &Theme) -> El {
    #[cfg(target_os = "linux")]
    let top_padding = theme::VIEWPORT_INSET;
    #[cfg(not(target_os = "linux"))]
    let top_padding = if state.sidebar.collapsed {
        theme::HEADER_HEIGHT
    } else {
        theme::VIEWPORT_INSET
    };
    let mut viewport = el()
        .key("viewport-container")
        .h_full()
        .grow(1.0)
        // Its size comes from the window, never from the frame inside: otherwise the last
        // frame holds it open and a narrower window never reaches the page.
        .min_w(0.0)
        .min_h(0.0)
        // With the sidebar hidden, the page sits below the header and in from the left edge too.
        .pt(top_padding)
        .pl(if state.sidebar.collapsed {
            theme::VIEWPORT_INSET
        } else {
            0.0
        })
        .pr(theme::VIEWPORT_INSET)
        .pb(theme::VIEWPORT_INSET);
    if state.backdrop_blur_warmup && !state.omnibox.open {
        viewport = viewport.child(
            el().key("backdrop-blur-warmup")
                .absolute(0.0, 0.0)
                .size(2.0, 2.0)
                .backdrop_blur(theme::MODAL_BACKDROP_BLUR)
                .transition(Transition::none()),
        );
    }
    viewport.child(page(state, theme))
}

/// The page's latest frame fills the viewport. With no active page, it stays transparent so the
/// window's native material shows through. During a live resize Rune scales a frame to new bounds.
fn page(state: &AppState, theme: &Theme) -> El {
    let mut page = el().key(VIEWPORT);
    let has_active_tab = state.browser.active_tab().is_some();
    if let Some(tab) = state.browser.active_tab() {
        page = page.pixels(&tab.surface.pixels);
        if !state.omnibox.open
            && !state.page_menu.open()
            && state.browser.page_focused()
            && let Some(rect) = tab.ime_caret
        {
            let [x, y, w, h] = rect.map(|v| v as f32 / state.browser.scale());
            page = page.child(
                el().key("page-ime-caret")
                    .absolute(x, y)
                    .size(w.max(1.0), h.max(1.0))
                    .ime_cursor(),
            );
        }
    }
    let page = page.w_full().h_full();
    let page = if has_active_tab {
        page.fill(theme.page)
    } else {
        page
    };
    page.continuous_corners()
        .clip_rounded(theme::VIEWPORT_RADIUS)
        .focusable(true)
        .no_focus_ring()
        .label("Web page")
        .on_layout(|state: &mut AppState, [_, _, w, h]| state.browser.set_viewport([w, h]))
        .pointer_buttons([PointerButton::Middle])
        .on_secondary_press_local(|state: &mut AppState, p, tx| {
            tx.focus(VIEWPORT, false);
            state.page_menu.anchor = state.browser.active_tab().map(|tab| (tab.id(), p.window()));
            for kind in [MouseKind::Down, MouseKind::Up] {
                state.browser.drag_pointer(
                    kind,
                    [p.x, p.y],
                    PointerButton::Secondary,
                    tx.mods(),
                    1,
                );
            }
        })
        .on_drag(|state: &mut AppState, drag, tx| {
            let kind = match drag.phase {
                Phase::Down => {
                    tx.focus(VIEWPORT, false);
                    MouseKind::Down
                }
                Phase::Move => MouseKind::Move,
                Phase::Up if drag.cancelled() => MouseKind::Cancel,
                Phase::Up => MouseKind::Up,
            };

            state
                .browser
                .drag_pointer(kind, [drag.x, drag.y], tx.button(), tx.mods(), drag.clicks);
        })
        .on_pointer_move_local(|state: &mut AppState, pointer, tx| {
            state
                .browser
                .hover(MouseKind::Move, [pointer.x, pointer.y], tx.mods());
        })
        .on_hover_change(|state: &mut AppState, inside, tx| {
            if !inside {
                state.browser.hover(MouseKind::Leave, [0.0, 0.0], tx.mods());
            }
        })
        .on_wheel(|state: &mut AppState, wheel, _| {
            state.browser.wheel(wheel);
            true
        })
        .on_key(page_key)
        .on_key_up(|state: &mut AppState, key, tx| {
            state.browser.key(key, false, false, tx.physical_key())
        })
}

fn paste(id: TabId, tx: &mut Tx) {
    tx.read_clipboard(move |s: &mut AppState, content, tx| {
        if !s.omnibox.open
            && tx.focused() == Some(VIEWPORT)
            && s.browser.active_tab().is_some_and(|tab| tab.id() == id)
            && !content.text().is_empty()
        {
            s.browser.send(
                id,
                ViewCommand::Edit(Edit::Paste(content.text().to_owned())),
            );
        }
    });
}

fn page_key(state: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(id) = state.browser.active_tab().map(|tab| tab.id()) else {
        return false;
    };
    if state.omnibox.open || state.page_menu.open() {
        return false;
    }
    if key.mods.cmd
        && !key.mods.shift
        && !key.mods.alt
        && let Key::Command(c) = key.key
    {
        let edit = match c {
            'a' => Edit::SelectAll,
            'x' => Edit::Cut,
            'c' => Edit::Copy,
            'v' => {
                paste(id, tx);
                return true;
            }
            _ => return false,
        };
        state.browser.send(id, ViewCommand::Edit(edit));
        return true;
    }
    if let Key::Text(s) = &key.key
        && s.chars().count() > 1
    {
        state
            .browser
            .send(id, ViewCommand::Edit(Edit::Commit(s.clone())));
        return true;
    }
    state
        .browser
        .key(key, true, tx.key_repeat(), tx.physical_key())
}

pub(crate) fn ime(state: &mut AppState, event: &Ime, tx: &mut Tx) -> bool {
    if tx.focused() != Some(VIEWPORT) || state.omnibox.open || state.page_menu.open() {
        return false;
    }
    let Some(tab) = state
        .browser
        .active_tab()
        .filter(|tab| tab.ime_caret.is_some())
    else {
        return false;
    };
    let edit = match event {
        Ime::Preedit(text, ..) => Edit::Mark(text.clone()),
        Ime::Commit(text) => Edit::Commit(text.clone()),
        Ime::Disabled => Edit::Mark(String::new()),
        _ => return false,
    };
    state.browser.send(tab.id(), ViewCommand::Edit(edit));
    true
}
