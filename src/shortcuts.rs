// SPDX-License-Identifier: GPL-3.0-only

//! The browser's keyboard shortcuts: which key does what, and doing it. Rune reports the
//! platform's shortcut modifier as `cmd`, Command on macOS and Control elsewhere, so the same
//! table serves both. The page never sees these: it takes no Command or Control chords, so they
//! reach the window.

use rune::{Key, KeyEvent, Tx};

use crate::app::{self, AppState};
use crate::browser::Browser;
use crate::tab::{Tab, TabId};
use crate::ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortcut {
    NewWindow,
    NewTab,
    ReopenClosedTab,
    Close,
    /// Open the omnibox with the active page address selected.
    FocusOmnibox,
    Find,
    FindNext,
    FindPrevious,
    Reload,
    PreviousTab,
    NextTab,
    /// The tab at this position, from zero.
    SelectTab(usize),
    SelectLastTab,
    ToggleSidebar,
    OpenSettings,
    OpenHistory,
}

/// Rune's hook for keys nothing focused took: runs the shortcut the key is, if any.
pub(crate) fn on_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
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
        Shortcut::NewTab => app::new_tab(state, tx),
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
        Shortcut::ToggleSidebar if !state.horizontal_tabs() => ui::toggle_sidebar(state, tx),
        Shortcut::ToggleSidebar => {}
        Shortcut::OpenSettings => app::open_settings(state, tx),
        Shortcut::OpenHistory => app::open_history(state, tx),
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

/// The shortcut `event` is, if any.
pub(crate) fn shortcut(event: &KeyEvent) -> Option<Shortcut> {
    let Key::Command(key) = event.key else {
        return None;
    };
    let shift = event.mods.shift;
    Some(match (key.to_ascii_lowercase(), shift) {
        ('n', false) => Shortcut::NewWindow,
        ('f', false) => Shortcut::Find,
        ('g', false) => Shortcut::FindNext,
        ('g', true) => Shortcut::FindPrevious,
        ('t', false) => Shortcut::NewTab,
        ('t' | 'T', true) => Shortcut::ReopenClosedTab,
        ('w', false) => Shortcut::Close,
        ('l', false) => Shortcut::FocusOmnibox,
        ('r', false) => Shortcut::Reload,
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
        assert_eq!(press('t', false), Some(Shortcut::NewTab));
        assert_eq!(press('t', true), Some(Shortcut::ReopenClosedTab));
        assert_eq!(press('T', true), Some(Shortcut::ReopenClosedTab));
        assert_eq!(press('w', false), Some(Shortcut::Close));
        assert_eq!(press('l', false), Some(Shortcut::FocusOmnibox));
        assert_eq!(press('r', false), Some(Shortcut::Reload));
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
        assert_eq!(press('0', false), None);
        assert_eq!(press('q', false), None);
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
}
