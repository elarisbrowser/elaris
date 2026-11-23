// SPDX-License-Identifier: GPL-3.0-only

//! The browser's keyboard shortcuts: which key does what, and doing it. Rune reports the
//! platform's shortcut modifier as `cmd`, Command on macOS and Control elsewhere, so the same
//! table serves both. The page never sees these: it takes no Command or Control chords, so they
//! reach the window.

use rune::{FileDialogOptions, Key, KeyEvent, Tx};

use crate::app::{self, AppState};
use crate::browser::Browser;
use crate::tab::{Tab, TabId};
use crate::ui;

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
    /// Bookmark the active page, or edit its bookmark.
    BookmarkPage,
    /// Save every web page in the window to a new bookmark folder.
    BookmarkAllTabs,
    OpenBookmarks,
    /// Show or hide the horizontal layout's bookmarks bar.
    ToggleBookmarksBar,
    /// Go back in the active tab's history.
    Back,
    /// Go forward in the active tab's history.
    Forward,
    /// Has no key of its own; the menu bar and the internal pages' menu open it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenTaskManager,
}

/// One chord that runs a shortcut. `key` is the unshifted character, as menus show it, and
/// `shift` and `option` are the modifiers the chord needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) shortcut: Shortcut,
    pub(crate) key: &'static str,
    pub(crate) shift: bool,
    pub(crate) option: bool,
}

/// Every chord the browser answers to, so this table is the one place to change a key. Menus
/// read their key equivalents from it, and the first entry for a shortcut is the one they show.
///
/// Chords kept out of this table on purpose:
/// - Native macOS keys (Cmd+Z/X/C/V/A, Cmd+Q, Cmd+H, Cmd+M, Ctrl+Cmd+F) are wired by AppKit in
///   `platform/macos/menu/construct.rs`, as every macOS app does.
/// - Escape and Ctrl+Tab depend on what is open (find bar, split picker, tab switcher, drag,
///   fullscreen), so `on_key` and `tab_switcher::cycle_key` handle them in order.
const BINDINGS: &[Binding] = &[
    bind(Shortcut::NewWindow, "n", false),
    bind(Shortcut::NewPrivateWindow, "n", true),
    bind(Shortcut::NewTab, "t", false),
    bind(Shortcut::ReopenClosedTab, "t", true),
    bind(Shortcut::OpenFile, "o", false),
    bind(Shortcut::Close, "w", false),
    bind(Shortcut::FocusOmnibox, "l", false),
    bind(Shortcut::CopyAddress, "c", true),
    bind(Shortcut::Find, "f", false),
    bind(Shortcut::FindNext, "g", false),
    bind(Shortcut::FindPrevious, "g", true),
    bind(Shortcut::Reload, "r", false),
    bind(Shortcut::ReloadBypassingCache, "r", true),
    bind(Shortcut::Back, "[", false),
    bind(Shortcut::Forward, "]", false),
    bind(Shortcut::ZoomIn, "=", false),
    bind(Shortcut::ZoomIn, "=", true),
    bind(Shortcut::ZoomOut, "-", false),
    bind(Shortcut::ResetZoom, "0", false),
    bind(Shortcut::PreviousTab, "[", true),
    bind(Shortcut::NextTab, "]", true),
    bind(Shortcut::SelectLastTab, "9", false),
    bind(Shortcut::ToggleSidebar, "s", false),
    bind(Shortcut::OpenHistory, HISTORY_KEY, false),
    bind(Shortcut::OpenSettings, ",", false),
    bind(Shortcut::BookmarkPage, "d", false),
    bind(Shortcut::BookmarkAllTabs, "d", true),
    bind_option(Shortcut::OpenBookmarks, "b"),
    bind(Shortcut::ToggleBookmarksBar, "b", true),
];

/// Command-Y on macOS, and Control-H elsewhere.
const HISTORY_KEY: &str = if cfg!(target_os = "macos") { "y" } else { "h" };

const fn bind(shortcut: Shortcut, key: &'static str, shift: bool) -> Binding {
    Binding {
        shortcut,
        key,
        shift,
        option: false,
    }
}

const fn bind_option(shortcut: Shortcut, key: &'static str) -> Binding {
    Binding {
        shortcut,
        key,
        shift: false,
        option: true,
    }
}

impl Shortcut {
    /// The chord that runs this shortcut, if it has one.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn binding(self) -> Option<Binding> {
        BINDINGS
            .iter()
            .copied()
            .find(|binding| binding.shortcut == self)
    }
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
        Shortcut::Back => state.with_active_tab(Browser::go_back),
        Shortcut::Forward => state.with_active_tab(Browser::go_forward),
        Shortcut::Reload => state.with_active_tab(Browser::reload),
        Shortcut::ReloadBypassingCache => state.with_active_tab(Browser::reload_bypassing_cache),
        Shortcut::ZoomIn => state.with_active_tab(|browser, id| browser.change_zoom(id, 10)),
        Shortcut::ZoomOut => state.with_active_tab(|browser, id| browser.change_zoom(id, -10)),
        Shortcut::ResetZoom => state.with_active_tab(|browser, id| browser.change_zoom(id, 0)),
        Shortcut::ToggleSidebar if !state.horizontal_tabs() => ui::toggle_sidebar(state, tx),
        Shortcut::ToggleSidebar => {}
        Shortcut::OpenSettings => app::open_settings(state, tx),
        Shortcut::OpenHistory => app::open_history(state, tx),
        Shortcut::BookmarkPage => state.bookmark_active_tab(),
        Shortcut::BookmarkAllTabs => state.bookmark_all_tabs(tx),
        Shortcut::OpenBookmarks => app::open_bookmarks(state, tx),
        Shortcut::ToggleBookmarksBar => {
            state.toggle_bookmarks_bar();
            tx.animated();
            tx.motion(ui::theme::SIDEBAR_TOGGLE);
        }
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
        Some(address) => state.toasts.copy_url(tx, &address),
        None => state.toasts.show(tx, "No address to copy"),
    }
}

/// The shortcut `event` is, if any.
pub(crate) fn shortcut(event: &KeyEvent) -> Option<Shortcut> {
    let Key::Command(typed) = event.key else {
        return None;
    };
    let key = base_key(typed);
    let (shift, option) = (event.mods.shift, event.mods.alt);
    if let Some(binding) = BINDINGS.iter().find(|binding| {
        binding.key.chars().eq([key]) && binding.shift == shift && binding.option == option
    }) {
        return Some(binding.shortcut);
    }
    // Command-1 to Command-8 pick a tab by its position, so they are not listed one by one.
    match key {
        '1'..='8' if !shift && !option => Some(Shortcut::SelectTab(key as usize - '1' as usize)),
        _ => None,
    }
}

/// The unshifted key a typed character came from, so `{` and `+` match `[` and `=`, and Option-B
/// (typed as `∫` on a US Mac layout) matches `b`.
fn base_key(typed: char) -> char {
    match typed.to_ascii_lowercase() {
        '{' => '[',
        '}' => ']',
        '+' => '=',
        '∫' => 'b',
        key => key,
    }
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
        assert_eq!(press('d', false), Some(Shortcut::BookmarkPage));
        assert_eq!(press('D', true), Some(Shortcut::BookmarkAllTabs));
        assert_eq!(press('B', true), Some(Shortcut::ToggleBookmarksBar));
        let manage = KeyEvent {
            key: Key::Command('b'),
            mods: Mods {
                cmd: true,
                alt: true,
                ..Mods::default()
            },
        };
        assert_eq!(shortcut(&manage), Some(Shortcut::OpenBookmarks));
    }

    #[test]
    fn every_binding_answers_to_its_own_chord() {
        // A chord shadowed by an earlier entry, or listed twice, fails here.
        for binding in BINDINGS {
            let key = KeyEvent {
                key: Key::Command(binding.key.chars().next().unwrap()),
                mods: Mods {
                    cmd: true,
                    shift: binding.shift,
                    alt: binding.option,
                    ..Mods::default()
                },
            };
            assert_eq!(shortcut(&key), Some(binding.shortcut), "{binding:?}");
        }
    }

    #[test]
    fn menus_show_the_registry_chord() {
        let back = Shortcut::Back.binding().unwrap();
        assert_eq!((back.key, back.shift, back.option), ("[", false, false));
        assert!(Shortcut::OpenBookmarks.binding().unwrap().option);
        assert_eq!(Shortcut::OpenTaskManager.binding(), None);
    }

    #[test]
    fn leaves_other_keys_alone() {
        assert_eq!(press('q', true), None);
        assert_eq!(press('[', false), Some(Shortcut::Back));
        assert_eq!(press(']', false), Some(Shortcut::Forward));
        assert_eq!(press('0', true), None);
        assert_eq!(press('q', false), None);
        assert_eq!(press('c', false), None);
        assert_eq!(press('x', true), None);
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
