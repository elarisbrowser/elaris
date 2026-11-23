// SPDX-License-Identifier: GPL-3.0-only

//! The browser's keyboard shortcuts: which key does what, and doing it. Rune reports the
//! platform's shortcut modifier as `cmd`, Command on macOS and Control elsewhere, so the same
//! table serves both. The page never sees these: it takes no Command or Control chords, so they
//! reach the window.

use rune::{FileDialogOptions, Key, KeyEvent, Tx};

use crate::app::{self, AppState};
use crate::browser::Browser;
use crate::tab::{Tab, TabId};
use crate::toast::Toast;
use crate::ui::{self, icons};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortcut {
    NewWindow,
    NewPrivateWindow,
    NewTab,
    OpenFile,
    ReopenClosedTab,
    Close,
    /// Open the omnibox with the active page address selected.
    FocusOmnibox,
    /// Copy the active page's address, and confirm with a toast.
    CopyAddress,
    Find,
    FindNext,
    FindPrevious,
    Reload,
    ReloadBypassingCache,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    PreviousTab,
    NextTab,
    /// The tab at this position, from zero.
    SelectTab(usize),
    SelectLastTab,
    ToggleSidebar,
    OpenSettings,
    OpenHistory,
    /// Has no key of its own; the menu bar and the internal pages' menu open it.
    OpenTaskManager,
}

/// Rune's hook for keys nothing focused took: runs the shortcut the key is, if any.
pub(crate) fn on_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if event.key == Key::Escape && state.browser.exit_web_fullscreen() {
        return true;
    }
    if event.key == Key::Escape
        && (crate::app::detach::cancel(state) || state.cancel_tab_transfer())
    {
        state.cancel_tab_drag();
        tx.animated();
        tx.motion(ui::TAB_REORDER_MOTION);
        return true;
    }
    if event.key == Key::Escape && ui::peek::dismiss(state, tx) {
        return true;
    }
    if crate::ui::tab_switcher::cycle_key(state, event, tx) {
        return true;
    }
    if event.key == Key::Escape
        && (state.tab_drag.is_dragging() || crate::app::cross_window_drag::clear(state.window_id))
    {
        state.cancel_tab_drag();
        tx.animated();
        tx.motion(ui::TAB_REORDER_MOTION);
        return true;
    }
    if event.key == Key::Escape
        && let Some(side) = state.split_picker_side()
    {
        state.browser.cancel_split_picker(side);
        tx.focus(ui::viewport::focus_key(&state.browser), false);
        tx.animated();
        return true;
    }
    if event.key == Key::Escape && state.find_open {
        state.close_find(tx);
        return true;
    }
    if event.key == Key::Escape && crate::ui::components::menu::escape(state) {
        return true;
    }
    if crate::ui::site_permissions::escape(state, event) {
        return true;
    }
    let Some(shortcut) = shortcut(event) else {
        return false;
    };
    dispatch(state, shortcut, tx);
    true
}

/// Runs a browser shortcut from either Rune's key hook or a native menu action.
pub(crate) fn dispatch(state: &mut AppState, shortcut: Shortcut, tx: &mut Tx) {
    if ui::peek::dismiss(state, tx) && shortcut == Shortcut::Close {
        return;
    }
    crate::ui::tab_switcher::finish(state, None, tx);
    match shortcut {
        Shortcut::NewWindow => crate::open_browser_window(),
        Shortcut::NewPrivateWindow => crate::open_private_browser_window(),
        Shortcut::NewTab => app::new_tab(state, tx),
        Shortcut::OpenFile => open_file(tx),
        Shortcut::ReopenClosedTab => {
            if state.reopen_recently_closed(0).is_some() {
                tx.focus(ui::viewport::focus_key(&state.browser), false);
            }
        }
        Shortcut::Close => {
            if state.omnibox.open {
                state.omnibox.close();
                tx.focus(ui::viewport::focus_key(&state.browser), false);
            } else if state.browser.active_tab().is_some() {
                state.close_active();
                if state.browser.active_tab().is_some() {
                    tx.focus(ui::viewport::focus_key(&state.browser), false);
                }
            } else {
                tx.close_window();
            }
        }
        Shortcut::FocusOmnibox => focus_omnibox(state, tx),
        Shortcut::CopyAddress => copy_address(state, tx),
        Shortcut::Find => state.open_find(tx),
        Shortcut::FindNext | Shortcut::FindPrevious => {
            if !state.find_open {
                state.open_find(tx);
            }
            if let Some(tab) = state.find_tab {
                if shortcut == Shortcut::FindPrevious {
                    let _ = state.browser.find_previous(tab);
                } else {
                    let _ = state.browser.find_next(tab);
                }
                tx.focus(ui::FIND_INPUT, false);
            }
        }
        Shortcut::Reload => state.with_active_tab(Browser::reload),
        Shortcut::ReloadBypassingCache => state.with_active_tab(Browser::reload_bypassing_cache),
        Shortcut::ZoomIn => state.with_active_tab(|browser, id| browser.change_zoom(id, 10)),
        Shortcut::ZoomOut => state.with_active_tab(|browser, id| browser.change_zoom(id, -10)),
        Shortcut::ResetZoom => state.with_active_tab(|browser, id| browser.change_zoom(id, 0)),
        Shortcut::ToggleSidebar if !state.horizontal_tabs() => ui::toggle_sidebar(state, tx),
        Shortcut::ToggleSidebar => {}
        Shortcut::OpenSettings => app::open_settings(state, tx),
        Shortcut::OpenHistory => app::open_history(state, tx),
        Shortcut::OpenTaskManager => app::open_task_manager(state, tx),
        Shortcut::PreviousTab
        | Shortcut::NextTab
        | Shortcut::SelectTab(_)
        | Shortcut::SelectLastTab => {
            if let Some(tab) = target(&state.browser, shortcut) {
                state.activate(tab);
            }
        }
    }
}

pub(crate) fn open_file(tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Open File".into(),
        ..Default::default()
    };
    tx.open_file(options, |state: &mut AppState, result, tx| match result {
        Ok(Some(path)) => {
            let url = elaris_core::address::file_url(&path);
            match state.browser.open_tab(&url) {
                Ok(tab) => {
                    state.omnibox.close();
                    state.activate(tab);
                    tx.focus(ui::viewport::focus_key(&state.browser), false);
                }
                Err(error) => eprintln!("Elaris: {error}"),
            }
        }
        Ok(None) => {}
        Err(error) => eprintln!("Elaris: {error}"),
    });
}

/// The tab a tab-selecting shortcut picks.
fn target(browser: &Browser, shortcut: Shortcut) -> Option<TabId> {
    let tabs = browser.tabs();
    let active = || browser.active_tab().map(Tab::id);
    match shortcut {
        Shortcut::PreviousTab => browser.tab_beside(active()?, -1),
        Shortcut::NextTab => browser.tab_beside(active()?, 1),
        Shortcut::SelectTab(index) => tabs.get(index).map(Tab::id),
        Shortcut::SelectLastTab => tabs.last().map(Tab::id),
        _ => None,
    }
}

/// Opens the omnibox over the active page, whether or not the sidebar is shown.
fn focus_omnibox(state: &mut AppState, tx: &mut Tx) {
    state.open_omnibox(crate::omnibox::Mode::Current);
    tx.focus("omnibox-input", false);
}

/// Copies the address the omnibox shows, as the sidebar's copy button does, and confirms it.
fn copy_address(state: &mut AppState, tx: &mut Tx) {
    let address = state
        .browser
        .active_tab()
        .map(|tab| tab.url.as_str())
        .filter(|url| !url.is_empty() && *url != "about:blank")
        .map(str::to_owned);
    match address {
        Some(address) => {
            tx.copy(address.clone());
            state.toasts.show(
                tx,
                Toast::new("Copied address")
                    .icon(&icons::COPY)
                    .detail(address),
            );
        }
        None => state.toasts.show(tx, "No address to copy"),
    }
}

/// The shortcut `event` is, if any.
pub(crate) fn shortcut(event: &KeyEvent) -> Option<Shortcut> {
    let Key::Command(key) = event.key else {
        return None;
    };
    let shift = event.mods.shift;
    Some(match (key.to_ascii_lowercase(), shift) {
        ('n', false) => Shortcut::NewWindow,
        ('n' | 'N', true) => Shortcut::NewPrivateWindow,
        ('o', false) => Shortcut::OpenFile,
        ('f', false) => Shortcut::Find,
        ('g', false) => Shortcut::FindNext,
        ('g', true) => Shortcut::FindPrevious,
        ('t', false) => Shortcut::NewTab,
        ('t' | 'T', true) => Shortcut::ReopenClosedTab,
        ('w', false) => Shortcut::Close,
        ('l', false) => Shortcut::FocusOmnibox,
        ('c', true) => Shortcut::CopyAddress,
        ('r', false) => Shortcut::Reload,
        ('r', true) => Shortcut::ReloadBypassingCache,
        ('=' | '+', _) => Shortcut::ZoomIn,
        ('-', false) => Shortcut::ZoomOut,
        ('0', false) => Shortcut::ResetZoom,
        ('s', false) => Shortcut::ToggleSidebar,
        #[cfg(target_os = "macos")]
        ('y', false) => Shortcut::OpenHistory,
        #[cfg(target_os = "linux")]
        ('h', false) => Shortcut::OpenHistory,
        (',', false) => Shortcut::OpenSettings,
        // Shift turns [ and ] into { and } on most layouts.
        ('[' | '{', true) => Shortcut::PreviousTab,
        (']' | '}', true) => Shortcut::NextTab,
        ('9', false) => Shortcut::SelectLastTab,
        ('1'..='8', false) => Shortcut::SelectTab(key as usize - '1' as usize),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use rune::Mods;

    use super::*;

    fn press(key: char, shift: bool) -> Option<Shortcut> {
        shortcut(&KeyEvent {
            key: Key::Command(key),
            mods: Mods {
                cmd: true,
                shift,
                ..Mods::default()
            },
        })
    }

    #[test]
    fn knows_the_browser_shortcuts() {
        assert_eq!(press('n', false), Some(Shortcut::NewWindow));
        assert_eq!(press('n', true), Some(Shortcut::NewPrivateWindow));
        assert_eq!(press('t', false), Some(Shortcut::NewTab));
        assert_eq!(press('t', true), Some(Shortcut::ReopenClosedTab));
        assert_eq!(press('T', true), Some(Shortcut::ReopenClosedTab));
        assert_eq!(press('w', false), Some(Shortcut::Close));
        assert_eq!(press('l', false), Some(Shortcut::FocusOmnibox));
        assert_eq!(press('c', true), Some(Shortcut::CopyAddress));
        assert_eq!(press('r', false), Some(Shortcut::Reload));
        assert_eq!(press('R', true), Some(Shortcut::ReloadBypassingCache));
        assert_eq!(press('=', false), Some(Shortcut::ZoomIn));
        assert_eq!(press('-', false), Some(Shortcut::ZoomOut));
        assert_eq!(press('0', false), Some(Shortcut::ResetZoom));
        assert_eq!(press('f', false), Some(Shortcut::Find));
        assert_eq!(press('g', false), Some(Shortcut::FindNext));
        assert_eq!(press('G', true), Some(Shortcut::FindPrevious));
        assert_eq!(press('s', false), Some(Shortcut::ToggleSidebar));
        assert_eq!(press(',', false), Some(Shortcut::OpenSettings));
        assert_eq!(press('{', true), Some(Shortcut::PreviousTab));
        assert_eq!(press(']', true), Some(Shortcut::NextTab));
        assert_eq!(press('1', false), Some(Shortcut::SelectTab(0)));
        assert_eq!(press('8', false), Some(Shortcut::SelectTab(7)));
        assert_eq!(press('9', false), Some(Shortcut::SelectLastTab));
    }

    #[test]
    fn leaves_other_keys_alone() {
        assert_eq!(press('q', true), None);
        assert_eq!(press('[', false), None);
        assert_eq!(press('0', true), None);
        assert_eq!(press('q', false), None);
        assert_eq!(press('c', false), None);
        let typed = KeyEvent {
            key: Key::Text("t".into()),
            mods: Mods::default(),
        };
        assert_eq!(shortcut(&typed), None);
    }

    #[test]
    fn dispatched_menu_shortcuts_reuse_browser_actions() {
        let mut state = AppState::new(Some("example.com"));
        let original = state.browser.active_tab().unwrap().id();
        dispatch(&mut state, Shortcut::NewTab, &mut Tx::default());
        assert_eq!(state.browser.tabs().len(), 1);
        assert!(state.omnibox.open);

        dispatch(&mut state, Shortcut::Close, &mut Tx::default());
        assert_eq!(state.browser.tabs().len(), 1);
        assert_eq!(state.browser.active_tab().unwrap().id(), original);
        assert!(!state.omnibox.open);
    }

    #[test]
    fn copy_address_confirms_with_a_toast() {
        let mut state = AppState::new(Some("example.com"));
        dispatch(&mut state, Shortcut::CopyAddress, &mut Tx::default());
        let toast = &state.toasts.shown.last().unwrap().toast;
        assert_eq!(toast.message, "Copied address");
        assert!(toast.detail.is_some());

        let mut empty = AppState::new(Some("about:blank"));
        dispatch(&mut empty, Shortcut::CopyAddress, &mut Tx::default());
        assert_eq!(
            empty.toasts.shown.last().unwrap().toast.message,
            "No address to copy"
        );
    }
}
