// SPDX-License-Identifier: GPL-3.0-only

//! The browser state that controls native menu labels and availability.

use crate::app::AppState;
use crate::tab::TabSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuCommand {
    NewWindow,
    NewTab,
    CloseTab,
    ReopenClosedTab,
    Find,
    Reload,
    WebPage,
    ToggleSidebar,
    History,
    NextTab,
    PreviousTab,
    Settings,
    Back,
    Forward,
    Split,
    ExitSplit,
    Favourite,
    Edit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MenuState {
    has_tab: bool,
    has_web_page: bool,
    can_split: bool,
    can_reopen: bool,
    can_go_back: bool,
    can_go_forward: bool,
    has_split: bool,
    has_sidebar: bool,
    pub(crate) sidebar_visible: bool,
    pub(crate) favourite: bool,
    /// Settings is listening for a shortcut's new chord, so the menu's own keys pause.
    pub(crate) recording_shortcut: bool,
}

impl MenuState {
    #[cfg_attr(elaris_ui = "linux", allow(dead_code))]
    pub(crate) fn from(state: &AppState) -> Self {
        let tab = state.browser.active_tab();
        let has_tab = tab.is_some();
        Self {
            has_tab,
            has_web_page: tab.is_some_and(|tab| !tab.is_internal()),
            can_split: has_tab && state.browser.tabs().len() > 1,
            can_reopen: state.can_reopen_closed_tab(),
            can_go_back: tab.is_some_and(|tab| tab.can_go_back),
            can_go_forward: tab.is_some_and(|tab| tab.can_go_forward),
            has_split: state.browser.displayed_split().is_some(),
            has_sidebar: !state.horizontal_tabs(),
            sidebar_visible: !state.horizontal_tabs() && !state.sidebar.collapsed,
            favourite: tab.is_some_and(|tab| tab.section() == TabSection::Favourite),
            recording_shortcut: state.shortcut_recorder.recording.is_some(),
        }
    }

    pub(crate) fn enabled(self, command: MenuCommand) -> bool {
        match command {
            MenuCommand::NewTab
            | MenuCommand::NewWindow
            | MenuCommand::History
            | MenuCommand::Settings
            | MenuCommand::Edit => true,
            MenuCommand::CloseTab
            | MenuCommand::Find
            | MenuCommand::Reload
            | MenuCommand::Favourite => self.has_tab,
            MenuCommand::WebPage => self.has_web_page,
            MenuCommand::ToggleSidebar => self.has_sidebar,
            MenuCommand::ReopenClosedTab => self.can_reopen,
            MenuCommand::NextTab | MenuCommand::PreviousTab => self.has_tab,
            MenuCommand::Back => self.can_go_back,
            MenuCommand::Forward => self.can_go_forward,
            MenuCommand::Split => self.can_split,
            MenuCommand::ExitSplit => self.has_split,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_tracks_tab_history_and_split_state() {
        let mut state = AppState::new(Some("example.com"));
        let first = state.browser.active_tab().unwrap().id();
        let single = MenuState::from(&state);
        assert!(single.enabled(MenuCommand::CloseTab));
        assert!(!single.enabled(MenuCommand::Back));
        assert!(!single.enabled(MenuCommand::Forward));
        assert!(!single.enabled(MenuCommand::Split));
        assert!(!single.enabled(MenuCommand::ExitSplit));

        let second = state.browser.open_tab("second.example").unwrap();
        let two_tabs = MenuState::from(&state);
        assert!(two_tabs.enabled(MenuCommand::Split));
        state.browser.place_split(first, 0);
        state.browser.place_split(second, 1);
        let split = MenuState::from(&state);
        assert!(split.enabled(MenuCommand::ExitSplit));
    }

    #[test]
    fn availability_tracks_recently_closed_tabs_and_empty_windows() {
        let mut state = AppState::new(Some("example.com"));
        let tab = state.browser.active_tab().unwrap().id();
        state.close(tab);
        let empty = MenuState::from(&state);
        assert!(!empty.enabled(MenuCommand::CloseTab));
        assert!(empty.enabled(MenuCommand::ReopenClosedTab));
        assert!(empty.enabled(MenuCommand::NewTab));
    }
}
