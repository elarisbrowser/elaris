// SPDX-License-Identifier: GPL-3.0-only

//! The browser's keyboard shortcuts: which key does what. Rune reports the platform's shortcut
//! modifier as `cmd`, Command on macOS and Control elsewhere, so the same table serves both.
//! The page never sees these: it takes no Command or Control chords, so they reach the window.

use rune::{Key, KeyEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortcut {
    NewTab,
    CloseTab,
    /// Focus the address field with its text selected.
    FocusAddress,
    Reload,
    PreviousTab,
    NextTab,
    /// The tab at this position, from zero.
    SelectTab(usize),
    SelectLastTab,
    ToggleSidebar,
}

/// The shortcut `event` is, if any.
pub(crate) fn shortcut(event: &KeyEvent) -> Option<Shortcut> {
    let Key::Command(key) = event.key else {
        return None;
    };
    let shift = event.mods.shift;
    Some(match (key, shift) {
        ('t', false) => Shortcut::NewTab,
        ('w', false) => Shortcut::CloseTab,
        ('l', false) => Shortcut::FocusAddress,
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
        assert_eq!(press('w', false), Some(Shortcut::CloseTab));
        assert_eq!(press('l', false), Some(Shortcut::FocusAddress));
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
