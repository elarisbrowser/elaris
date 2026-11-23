// SPDX-License-Identifier: GPL-3.0-only

//! The browser's keyboard shortcuts: which key does what, and doing it. Rune reports the
//! platform's shortcut modifier as `cmd`, Command on macOS and Control elsewhere, so the same
//! table serves both. The page never sees these: it takes no Command or Control chords, so they
//! reach the window.

use std::sync::RwLock;

use elaris_core::keybindings::{Chord, KeyBindings};
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
    /// Open the omnibox searching only bookmarks.
    SearchBookmarks,
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

/// One chord that runs a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) shortcut: Shortcut,
    pub(crate) chord: Chord,
}

/// Every chord the browser answers to by default. `[keybindings]` in `config.toml` can replace
/// any action's chords; [`resolve`] merges the two. Menus read their key equivalents from the
/// result, and the first entry for a shortcut is the one they show.
///
/// Chords kept out of this table on purpose:
/// - Native macOS keys (Cmd+Z/X/C/V/A, Cmd+Q, Cmd+H, Cmd+M, Ctrl+Cmd+F) are wired by AppKit in
///   `platform/macos/menu/construct.rs`, as every macOS app does. [`RESERVED`] keeps them free.
/// - Escape and Ctrl+Tab depend on what is open (find bar, split picker, tab switcher, drag,
///   fullscreen), so `on_key` and `tab_switcher::cycle_key` handle them in order.
const DEFAULTS: &[Binding] = &[
    bind(Shortcut::NewWindow, 'n', false),
    bind(Shortcut::NewPrivateWindow, 'n', true),
    bind(Shortcut::NewTab, 't', false),
    bind(Shortcut::ReopenClosedTab, 't', true),
    bind(Shortcut::OpenFile, 'o', false),
    bind(Shortcut::Close, 'w', false),
    bind(Shortcut::FocusOmnibox, 'l', false),
    bind(Shortcut::CopyAddress, 'c', true),
    bind(Shortcut::Find, 'f', false),
    bind(Shortcut::FindNext, 'g', false),
    bind(Shortcut::FindPrevious, 'g', true),
    bind(Shortcut::Reload, 'r', false),
    bind(Shortcut::ReloadBypassingCache, 'r', true),
    bind(Shortcut::Back, '[', false),
    bind(Shortcut::Forward, ']', false),
    bind(Shortcut::ZoomIn, '=', false),
    bind(Shortcut::ZoomIn, '=', true),
    bind(Shortcut::ZoomOut, '-', false),
    bind(Shortcut::ResetZoom, '0', false),
    bind(Shortcut::PreviousTab, '[', true),
    bind(Shortcut::NextTab, ']', true),
    bind(Shortcut::SelectLastTab, '9', false),
    bind(Shortcut::ToggleSidebar, 's', false),
    bind(Shortcut::OpenHistory, HISTORY_KEY, false),
    bind(Shortcut::OpenSettings, ',', false),
    bind(Shortcut::BookmarkPage, 'd', false),
    bind(Shortcut::BookmarkAllTabs, 'd', true),
    Binding {
        shortcut: Shortcut::OpenBookmarks,
        chord: Chord::new('b', false, true),
    },
    bind(Shortcut::ToggleBookmarksBar, 'b', true),
    bind(Shortcut::SearchBookmarks, 'o', true),
];

/// Command-Y on macOS, and Control-H elsewhere.
const HISTORY_KEY: char = if cfg!(target_os = "macos") { 'y' } else { 'h' };

const fn bind(shortcut: Shortcut, key: char, shift: bool) -> Binding {
    Binding {
        shortcut,
        chord: Chord::new(key, shift, false),
    }
}

/// Chords the system's edit and application keys own, which a page and the menu bar rely on.
const RESERVED: &[(Chord, &str)] = &[
    (Chord::new('z', false, false), "Undo"),
    (Chord::new('z', true, false), "Redo"),
    (Chord::new('x', false, false), "Cut"),
    (Chord::new('c', false, false), "Copy"),
    (Chord::new('v', false, false), "Paste"),
    (Chord::new('a', false, false), "Select All"),
    (Chord::new('q', false, false), "Quit"),
    #[cfg(target_os = "macos")]
    (Chord::new('h', false, false), "Hide Elaris"),
    #[cfg(target_os = "macos")]
    (Chord::new('h', false, true), "Hide Others"),
    #[cfg(target_os = "macos")]
    (Chord::new('m', false, false), "Minimize"),
];

/// How Settings lists the actions: each group's title and its actions with their titles. An
/// action missing here still shows, under "Other", titled from its name.
const GROUPS: &[(&str, &[(Shortcut, &str)])] = &[
    (
        "Windows and tabs",
        &[
            (Shortcut::NewWindow, "New window"),
            (Shortcut::NewPrivateWindow, "New private window"),
            (Shortcut::NewTab, "New tab"),
            (Shortcut::ReopenClosedTab, "Reopen closed tab"),
            (Shortcut::Close, "Close tab"),
            (Shortcut::PreviousTab, "Previous tab"),
            (Shortcut::NextTab, "Next tab"),
            (Shortcut::SelectLastTab, "Last tab"),
            (Shortcut::ToggleSidebar, "Show or hide the sidebar"),
        ],
    ),
    (
        "Navigation",
        &[
            (Shortcut::FocusOmnibox, "Address bar"),
            (Shortcut::CopyAddress, "Copy address"),
            (Shortcut::Back, "Back"),
            (Shortcut::Forward, "Forward"),
            (Shortcut::Reload, "Reload"),
            (Shortcut::ReloadBypassingCache, "Reload without cache"),
            (Shortcut::OpenFile, "Open file"),
        ],
    ),
    (
        "Page",
        &[
            (Shortcut::Find, "Find in page"),
            (Shortcut::FindNext, "Find next"),
            (Shortcut::FindPrevious, "Find previous"),
            (Shortcut::ZoomIn, "Zoom in"),
            (Shortcut::ZoomOut, "Zoom out"),
            (Shortcut::ResetZoom, "Reset zoom"),
        ],
    ),
    (
        "Bookmarks and history",
        &[
            (Shortcut::BookmarkPage, "Bookmark this page"),
            (Shortcut::BookmarkAllTabs, "Bookmark all tabs"),
            (Shortcut::OpenBookmarks, "Manage bookmarks"),
            (Shortcut::SearchBookmarks, "Search bookmarks"),
            (
                Shortcut::ToggleBookmarksBar,
                "Show or hide the bookmarks bar",
            ),
            (Shortcut::OpenHistory, "Show history"),
        ],
    ),
    (
        "Browser",
        &[
            (Shortcut::OpenSettings, "Settings"),
            (Shortcut::OpenTaskManager, "Task manager"),
        ],
    ),
];

/// Each action `[keybindings]` can rebind, by its name there. Picking a tab by its number is
/// built in and is not listed.
const NAMES: &[(Shortcut, &str)] = &[
    (Shortcut::NewWindow, "new_window"),
    (Shortcut::NewPrivateWindow, "new_private_window"),
    (Shortcut::NewTab, "new_tab"),
    (Shortcut::OpenFile, "open_file"),
    (Shortcut::ReopenClosedTab, "reopen_closed_tab"),
    (Shortcut::Close, "close_tab"),
    (Shortcut::FocusOmnibox, "focus_address_bar"),
    (Shortcut::CopyAddress, "copy_address"),
    (Shortcut::Find, "find"),
    (Shortcut::FindNext, "find_next"),
    (Shortcut::FindPrevious, "find_previous"),
    (Shortcut::Reload, "reload"),
    (Shortcut::ReloadBypassingCache, "reload_bypassing_cache"),
    (Shortcut::ZoomIn, "zoom_in"),
    (Shortcut::ZoomOut, "zoom_out"),
    (Shortcut::ResetZoom, "reset_zoom"),
    (Shortcut::PreviousTab, "previous_tab"),
    (Shortcut::NextTab, "next_tab"),
    (Shortcut::SelectLastTab, "select_last_tab"),
    (Shortcut::ToggleSidebar, "toggle_sidebar"),
    (Shortcut::OpenSettings, "open_settings"),
    (Shortcut::OpenHistory, "open_history"),
    (Shortcut::BookmarkPage, "bookmark_page"),
    (Shortcut::BookmarkAllTabs, "bookmark_all_tabs"),
    (Shortcut::OpenBookmarks, "open_bookmarks"),
    (Shortcut::SearchBookmarks, "search_bookmarks"),
    (Shortcut::ToggleBookmarksBar, "toggle_bookmarks_bar"),
    (Shortcut::Back, "back"),
    (Shortcut::Forward, "forward"),
    (Shortcut::OpenTaskManager, "open_task_manager"),
];

/// The keymap in use, shared by every window like the menu bar. `None` until the configuration
/// is first applied, which means [`DEFAULTS`].
static KEYMAP: RwLock<Option<Vec<Binding>>> = RwLock::new(None);

fn with_keymap<R>(read: impl FnOnce(&[Binding]) -> R) -> R {
    let keymap = KEYMAP.read().expect("keymap lock poisoned");
    read(keymap.as_deref().unwrap_or(DEFAULTS))
}

/// Merges `[keybindings]` into the defaults. An overridden action keeps only the chords it is
/// given, and a chord it claims is taken from whichever action had it by default. Returns the
/// keymap and a message for each override that could not be used.
fn resolve(overrides: &KeyBindings) -> (Vec<Binding>, Vec<String>) {
    let mut keymap: Vec<Binding> = Vec::new();
    let mut overridden = Vec::new();
    let mut issues = Vec::new();
    for (name, chords) in overrides {
        let Some(&(shortcut, _)) = NAMES.iter().find(|(_, known)| known == name) else {
            issues.push(format!("Unknown action [keybindings].{name}"));
            continue;
        };
        overridden.push(shortcut);
        for &chord in chords {
            if let Some((_, owner)) = RESERVED.iter().find(|(reserved, _)| *reserved == chord) {
                issues.push(format!(
                    "[keybindings].{name}: {chord} is reserved for {owner}"
                ));
            } else if let Some(taken) = keymap.iter().find(|binding| binding.chord == chord) {
                issues.push(format!(
                    "[keybindings].{name}: {chord} is already bound to {}",
                    taken.shortcut.name().unwrap_or_default()
                ));
            } else {
                keymap.push(Binding { shortcut, chord });
            }
        }
    }
    let claimed: Vec<Chord> = keymap.iter().map(|binding| binding.chord).collect();
    keymap.extend(DEFAULTS.iter().copied().filter(|binding| {
        !overridden.contains(&binding.shortcut) && !claimed.contains(&binding.chord)
    }));
    (keymap, issues)
}

/// Makes `overrides` the process's keymap and updates the menu bar's key equivalents. Returns
/// a message for each override that could not be used.
pub(crate) fn apply_keybindings(overrides: &KeyBindings) -> Vec<String> {
    let (keymap, issues) = resolve(overrides);
    // Tests run in parallel in one process, so they check `resolve` and keep the defaults here.
    if cfg!(not(test)) {
        *KEYMAP.write().expect("keymap lock poisoned") = Some(keymap);
        #[cfg(target_os = "macos")]
        crate::platform::macos::menu::apply_key_equivalents();
    }
    issues
}

impl Shortcut {
    /// The chord that runs this shortcut, if it has one.
    pub(crate) fn binding(self) -> Option<Binding> {
        with_keymap(|keymap| {
            keymap
                .iter()
                .copied()
                .find(|binding| binding.shortcut == self)
        })
    }

    /// The action's name in `[keybindings]`.
    pub(crate) fn name(self) -> Option<&'static str> {
        NAMES
            .iter()
            .find(|(shortcut, _)| *shortcut == self)
            .map(|(_, name)| *name)
    }

    /// The chord as a menu shows it: "⇧⌘D" on macOS, "Ctrl+Shift+D" elsewhere.
    pub(crate) fn hint(self) -> Option<String> {
        self.binding().map(|binding| chord_label(binding.chord))
    }
}

/// A chord as menus show it: "⌥⇧⌘D" on macOS, "Ctrl+Shift+Alt+D" elsewhere.
pub(crate) fn chord_label(chord: Chord) -> String {
    let key = chord.key.to_ascii_uppercase();
    if cfg!(target_os = "macos") {
        let shift = if chord.shift { "⇧" } else { "" };
        let option = if chord.option { "⌥" } else { "" };
        format!("{option}{shift}⌘{key}")
    } else {
        let shift = if chord.shift { "+Shift" } else { "" };
        let option = if chord.option { "+Alt" } else { "" };
        format!("Ctrl{shift}{option}+{key}")
    }
}

/// What a reserved chord does, if `chord` is one.
pub(crate) fn reserved_by(chord: Chord) -> Option<&'static str> {
    RESERVED
        .iter()
        .find(|(reserved, _)| *reserved == chord)
        .map(|(_, owner)| *owner)
}

/// The action `chord` runs now, if any.
pub(crate) fn owner(chord: Chord) -> Option<Shortcut> {
    with_keymap(|keymap| {
        keymap
            .iter()
            .find(|binding| binding.chord == chord)
            .map(|binding| binding.shortcut)
    })
}

/// The chord a key press is, if it uses the shortcut modifier and no Control on macOS, which
/// chords cannot express.
pub(crate) fn chord_of(event: &KeyEvent) -> Option<Chord> {
    let Key::Command(typed) = event.key else {
        return None;
    };
    if cfg!(target_os = "macos") && event.mods.ctrl {
        return None;
    }
    Some(Chord::new(
        base_key(typed),
        event.mods.shift,
        event.mods.alt,
    ))
}

/// The action's title in Settings.
pub(crate) fn title(shortcut: Shortcut) -> String {
    settings_list()
        .into_iter()
        .find(|(_, listed, _)| *listed == shortcut)
        .map_or_else(String::new, |(_, _, title)| title)
}

/// Every rebindable action in Settings order, as (group title, action, action title).
pub(crate) fn settings_list() -> Vec<(&'static str, Shortcut, String)> {
    let mut listed: Vec<_> = GROUPS
        .iter()
        .flat_map(|(group, actions)| {
            actions
                .iter()
                .map(move |(shortcut, title)| (*group, *shortcut, (*title).to_owned()))
        })
        .collect();
    for &(shortcut, name) in NAMES {
        if !listed.iter().any(|(_, listed, _)| *listed == shortcut) {
            let mut title = name.replace('_', " ");
            title[..1].make_ascii_uppercase();
            listed.push(("Other", shortcut, title));
        }
    }
    listed
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
        Shortcut::SearchBookmarks => app::search_bookmarks(state, tx),
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
    with_keymap(|keymap| shortcut_in(keymap, event))
}

fn shortcut_in(keymap: &[Binding], event: &KeyEvent) -> Option<Shortcut> {
    let Key::Command(typed) = event.key else {
        return None;
    };
    let pressed = Chord::new(base_key(typed), event.mods.shift, event.mods.alt);
    if let Some(binding) = keymap.iter().find(|binding| binding.chord == pressed) {
        return Some(binding.shortcut);
    }
    // Command-1 to Command-8 pick a tab by its position, so they are not listed one by one.
    match pressed.key {
        key @ '1'..='8' if !pressed.shift && !pressed.option => {
            Some(Shortcut::SelectTab(key as usize - '1' as usize))
        }
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
        assert_eq!(press('O', true), Some(Shortcut::SearchBookmarks));
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
        for binding in DEFAULTS {
            assert_eq!(
                shortcut(&chord_event(binding.chord)),
                Some(binding.shortcut),
                "{binding:?}"
            );
            assert!(
                binding.shortcut.name().is_some(),
                "{binding:?} has no [keybindings] name"
            );
        }
    }

    fn chord_event(chord: Chord) -> KeyEvent {
        KeyEvent {
            key: Key::Command(chord.key),
            mods: Mods {
                cmd: true,
                shift: chord.shift,
                alt: chord.option,
                ..Mods::default()
            },
        }
    }

    fn overrides(entries: &[(&str, &[&str])]) -> KeyBindings {
        entries
            .iter()
            .map(|(name, chords)| {
                let chords = chords
                    .iter()
                    .map(|text| Chord::parse(text).unwrap())
                    .collect();
                ((*name).to_owned(), chords)
            })
            .collect()
    }

    #[test]
    fn overrides_replace_an_actions_keys_and_take_their_chords() {
        let (keymap, issues) = resolve(&overrides(&[
            ("new_tab", &["mod+y"]),
            ("toggle_sidebar", &["mod+shift+t", "mod+alt+s"]),
            ("copy_address", &[]),
        ]));
        assert!(issues.is_empty(), "{issues:?}");
        let run = |text| shortcut_in(&keymap, &chord_event(Chord::parse(text).unwrap()));
        assert_eq!(run("mod+y"), Some(Shortcut::NewTab));
        assert_eq!(run("mod+t"), None);
        assert_eq!(run("mod+shift+t"), Some(Shortcut::ToggleSidebar));
        assert_eq!(run("mod+alt+s"), Some(Shortcut::ToggleSidebar));
        assert_eq!(run("mod+s"), None);
        assert_eq!(run("mod+shift+c"), None);
        assert_eq!(run("mod+w"), Some(Shortcut::Close));
        // The first chord an override lists is the one menus show.
        let sidebar = keymap
            .iter()
            .find(|b| b.shortcut == Shortcut::ToggleSidebar);
        assert_eq!(sidebar.unwrap().chord, Chord::parse("mod+shift+t").unwrap());
    }

    #[test]
    fn unusable_overrides_are_reported_and_skipped() {
        let (keymap, issues) = resolve(&overrides(&[
            ("open_task_manager", &["mod+k"]),
            ("reload", &["mod+k"]),
            ("find", &["mod+c"]),
            ("teleport", &["mod+j"]),
        ]));
        assert_eq!(issues.len(), 3, "{issues:?}");
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("reserved for Copy"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("[keybindings].teleport"))
        );
        // `find` sorts before `open_task_manager` and `reload`, and `open_task_manager` claims
        // mod+k before `reload` does.
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("already bound to open_task_manager"))
        );
        let run = |text| shortcut_in(&keymap, &chord_event(Chord::parse(text).unwrap()));
        assert_eq!(run("mod+k"), Some(Shortcut::OpenTaskManager));
        assert_eq!(run("mod+r"), None);
        assert_eq!(run("mod+f"), None);
    }

    #[test]
    fn every_named_action_is_distinct() {
        for (index, (shortcut, name)) in NAMES.iter().enumerate() {
            assert!(
                NAMES[index + 1..]
                    .iter()
                    .all(|(other, other_name)| other != shortcut && other_name != name),
                "{name}"
            );
        }
    }

    #[test]
    fn menus_show_the_registry_chord() {
        let back = Shortcut::Back.binding().unwrap();
        assert_eq!(back.chord, Chord::new('[', false, false));
        assert!(Shortcut::OpenBookmarks.binding().unwrap().chord.option);
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
