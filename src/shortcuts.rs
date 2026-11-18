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
    NewTab,
    Close,
    /// Open the omnibox with the active page address selected.
    FocusOmnibox,
    Reload,
    PreviousTab,
    NextTab,
    /// The tab at this position, from zero.
    SelectTab(usize),
    SelectLastTab,
    ToggleSidebar,
}

/// Rune's hook for keys nothing focused took: runs the shortcut the key is, if any.
pub(crate) fn on_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(shortcut) = shortcut(event) else {
        return false;
    };
    match shortcut {
        Shortcut::NewTab => app::new_tab(state, tx),
        Shortcut::Close => {
            if state.omnibox.open {
                state.omnibox.close();
                tx.focus(ui::VIEWPORT, false);
            } else if state.browser.active_tab().is_some() {
                state.close_active();
            } else {
                tx.close_window();
            }
        }
        Shortcut::FocusOmnibox => focus_omnibox(state, tx),
        Shortcut::Reload => state.with_active_tab(Browser::reload),
        Shortcut::ToggleSidebar => ui::toggle_sidebar(state, tx),
        Shortcut::PreviousTab
        | Shortcut::NextTab
        | Shortcut::SelectTab(_)
        | Shortcut::SelectLastTab => {
            if let Some(tab) = target(&state.browser, shortcut) {
                state.activate(tab);
            }
        }
    }
    true
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
    Some(match (key, shift) {
        ('t', false) => Shortcut::NewTab,
        ('w', false) => Shortcut::Close,
        ('l', false) => Shortcut::FocusOmnibox,
        ('r', false) => Shortcut::Reload,
        ('s', false) => Shortcut::ToggleSidebar,
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
        assert_eq!(press('t', false), Some(Shortcut::NewTab));
        assert_eq!(press('w', false), Some(Shortcut::Close));
        assert_eq!(press('l', false), Some(Shortcut::FocusOmnibox));
        assert_eq!(press('r', false), Some(Shortcut::Reload));
        assert_eq!(press('s', false), Some(Shortcut::ToggleSidebar));
        assert_eq!(press('{', true), Some(Shortcut::PreviousTab));
        assert_eq!(press(']', true), Some(Shortcut::NextTab));
        assert_eq!(press('1', false), Some(Shortcut::SelectTab(0)));
        assert_eq!(press('8', false), Some(Shortcut::SelectTab(7)));
        assert_eq!(press('9', false), Some(Shortcut::SelectLastTab));
    }

    #[test]
    fn leaves_other_keys_alone() {
        assert_eq!(press('t', true), None);
        assert_eq!(press('[', false), None);
        assert_eq!(press('0', false), None);
        assert_eq!(press('q', false), None);
        let typed = KeyEvent {
            key: Key::Text("t".into()),
            mods: Mods::default(),
        };
        assert_eq!(shortcut(&typed), None);
    }
}
