// SPDX-License-Identifier: GPL-3.0-only

//! Native macOS application menus, dispatched into Elaris's shared browser actions.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyClass, AnyObject, AnyProtocol, ClassBuilder, ProtocolObject, Sel};
use objc2_05::{ClassType, msg_send, msg_send_id, sel};
use objc2_app_kit_02::{
    NSApplication, NSEventModifierFlags, NSMenu, NSMenuDelegate, NSMenuItem, NSWindowStyleMask,
};
use objc2_foundation_02::{MainThreadMarker, NSString};
use rune::Tx;

use crate::app::AppState;
use crate::menu_state::{MenuCommand, MenuState};
use crate::shortcuts::{self, Shortcut};
use crate::tab::{Tab, TabSection};

thread_local! {
    static MENU_ITEMS: RefCell<Vec<Retained<NSMenuItem>>> = const { RefCell::new(Vec::new()) };
    static MENU_TARGET: RefCell<Option<Retained<ProtocolObject<dyn NSMenuDelegate>>>> = const { RefCell::new(None) };
    static FULL_SCREEN_ITEM: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
    static MENU_STATE: RefCell<Option<MenuState>> = const { RefCell::new(None) };
    static APP_HANDLES: RefCell<HashMap<u64, rune::AppHandle<AppState>>> = RefCell::new(HashMap::new());
    static ACTIVE_WINDOW: Cell<Option<u64>> = const { Cell::new(None) };
    /// Windows whose pages have not yet agreed to unload for a pending quit.
    static QUIT_PENDING: RefCell<Option<HashSet<u64>>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Shortcut(Shortcut),
    Back,
    Forward,
    Favourite,
    SplitLeft,
    SplitRight,
    ExitSplit,
    Edit(EditAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[derive(Clone, Copy)]
struct ItemSpec {
    title: &'static str,
    action: Option<Action>,
    key: &'static str,
    shift: bool,
    option: bool,
    control: bool,
}

const fn item(title: &'static str, action: Action, key: &'static str, shift: bool) -> ItemSpec {
    ItemSpec {
        title,
        action: Some(action),
        key,
        shift,
        option: false,
        control: false,
    }
}

const fn edit(title: &'static str, action: EditAction, key: &'static str, shift: bool) -> ItemSpec {
    item(title, Action::Edit(action), key, shift)
}

const fn native(title: &'static str, key: &'static str, shift: bool) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift,
        option: false,
        control: false,
    }
}

const fn native_option(title: &'static str, key: &'static str) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift: false,
        option: true,
        control: false,
    }
}

const fn native_control(title: &'static str, key: &'static str) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift: false,
        option: false,
        control: true,
    }
}

const fn separator() -> ItemSpec {
    ItemSpec {
        title: "",
        action: None,
        key: "",
        shift: false,
        option: false,
        control: false,
    }
}

fn menu(
    title: &str,
    specs: &[ItemSpec],
    mtm: MainThreadMarker,
    target: &ProtocolObject<dyn NSMenuDelegate>,
    items: &mut Vec<Retained<NSMenuItem>>,
) -> Retained<NSMenu> {
    let menu = unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str(title)) };
    unsafe { menu.setAutoenablesItems(false) };
    // SAFETY: the target class declares NSMenuDelegate conformance and lives as long as menu items.
    unsafe { menu.setDelegate(Some(target)) };
    for spec in specs {
        if spec.title.is_empty() {
            let separator = NSMenuItem::separatorItem(mtm);
            menu.addItem(&separator);
            continue;
        }
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(spec.title),
                Some(if spec.action.is_some() {
                    sel!(menuAction:)
                } else {
                    native_selector(spec.title)
                }),
                &NSString::from_str(spec.key),
            )
        };
        if let Some(action) = spec.action {
            unsafe {
                item.setTarget(Some(target_object(target)));
                item.setTag(action_id(action));
            }
            items.push(item.clone());
        } else if spec.title == "Enter Full Screen" {
            FULL_SCREEN_ITEM.with(|slot| *slot.borrow_mut() = Some(item.clone()));
        }
        if !spec.key.is_empty() {
            item.setKeyEquivalentModifierMask(
                NSEventModifierFlags::NSEventModifierFlagCommand
                    | if spec.shift {
                        NSEventModifierFlags::NSEventModifierFlagShift
                    } else {
                        NSEventModifierFlags(0)
                    }
                    | if spec.option {
                        NSEventModifierFlags::NSEventModifierFlagOption
                    } else {
                        NSEventModifierFlags(0)
                    }
                    | if spec.control {
                        NSEventModifierFlags::NSEventModifierFlagControl
                    } else {
                        NSEventModifierFlags(0)
                    },
            );
        }
        menu.addItem(&item);
    }
    menu
}

fn target_object(target: &ProtocolObject<dyn NSMenuDelegate>) -> &AnyObject {
    // SAFETY: ProtocolObject is repr(C) over AnyObject, and this object is the registered target.
    unsafe { &*(std::ptr::from_ref(target).cast::<AnyObject>()) }
}

fn native_selector(title: &str) -> Sel {
    match title {
        "About Elaris" => sel!(orderFrontStandardAboutPanel:),
        "Hide Elaris" => sel!(hide:),
        "Hide Others" => sel!(hideOtherApplications:),
        "Show All" => sel!(unhideAllApplications:),
        "Quit Elaris" => sel!(terminate:),
        "Undo" => sel!(undo:),
        "Redo" => sel!(redo:),
        "Cut" => sel!(cut:),
        "Copy" => sel!(copy:),
        "Paste" => sel!(paste:),
        "Select All" => sel!(selectAll:),
        "Minimize" => sel!(performMiniaturize:),
        "Zoom" => sel!(performZoom:),
        "Enter Full Screen" | "Exit Full Screen" => sel!(toggleFullScreen:),
        "Bring All to Front" => sel!(arrangeInFront:),
        _ => sel!(doNothing:),
    }
}

/// Installs Elaris's native menu bar on the AppKit main thread.
pub(crate) fn install(app: rune::AppHandle<AppState>) {
    let _ = app;
    rune::set_quit_guard(quit_guard);
    let mtm = MainThreadMarker::new().expect("AppKit menus install on the main thread");
    let application = NSApplication::sharedApplication(mtm);
    let target: Retained<ProtocolObject<dyn NSMenuDelegate>> =
        unsafe { msg_send_id![menu_target_class(), new] };
    let mut items = Vec::new();
    let app_menu = menu(
        "Elaris",
        &[
            native("About Elaris", "", false),
            separator(),
            item(
                "Settings…",
                Action::Shortcut(Shortcut::OpenSettings),
                ",",
                false,
            ),
            separator(),
            native("Hide Elaris", "h", false),
            native_option("Hide Others", "h"),
            native("Show All", "", false),
            separator(),
            native("Quit Elaris", "q", false),
        ],
        mtm,
        &target,
        &mut items,
    );
    let file = menu(
        "File",
        &[
            item(
                "New Window",
                Action::Shortcut(Shortcut::NewWindow),
                "n",
                false,
            ),
            item(
                "New Private Window",
                Action::Shortcut(Shortcut::NewPrivateWindow),
                "n",
                true,
            ),
            item("New Tab", Action::Shortcut(Shortcut::NewTab), "t", false),
            item(
                "Open File…",
                Action::Shortcut(Shortcut::OpenFile),
                "o",
                false,
            ),
            item("Close Tab", Action::Shortcut(Shortcut::Close), "w", false),
            item(
                "Reopen Closed Tab",
                Action::Shortcut(Shortcut::ReopenClosedTab),
                "t",
                true,
            ),
        ],
        mtm,
        &target,
        &mut items,
    );
    let edit = menu(
        "Edit",
        &[
            edit("Undo", EditAction::Undo, "z", false),
            edit("Redo", EditAction::Redo, "z", true),
            separator(),
            edit("Cut", EditAction::Cut, "x", false),
            edit("Copy", EditAction::Copy, "c", false),
            edit("Paste", EditAction::Paste, "v", false),
            edit("Select All", EditAction::SelectAll, "a", false),
            separator(),
            item("Find in Page", Action::Shortcut(Shortcut::Find), "f", false),
        ],
        mtm,
        &target,
        &mut items,
    );
    let view = menu(
        "View",
        &[
            item("Reload", Action::Shortcut(Shortcut::Reload), "r", false),
            item(
                "Reload Without Cache",
                Action::Shortcut(Shortcut::ReloadBypassingCache),
                "r",
                true,
            ),
            separator(),
            item("Zoom In", Action::Shortcut(Shortcut::ZoomIn), "=", false),
            item("Zoom Out", Action::Shortcut(Shortcut::ZoomOut), "-", false),
            item(
                "Reset Page Zoom",
                Action::Shortcut(Shortcut::ResetZoom),
                "0",
                false,
            ),
            item(
                "Toggle Sidebar",
                Action::Shortcut(Shortcut::ToggleSidebar),
                "s",
                false,
            ),
            separator(),
            item("Split Left", Action::SplitLeft, "", false),
            item("Split Right", Action::SplitRight, "", false),
            item("Exit Split View", Action::ExitSplit, "", false),
            separator(),
            native_control("Enter Full Screen", "f"),
        ],
        mtm,
        &target,
        &mut items,
    );
    let history = menu(
        "History",
        &[
            item(
                "Show History",
                Action::Shortcut(Shortcut::OpenHistory),
                "y",
                false,
            ),
            item("Back", Action::Back, "[", false),
            item("Forward", Action::Forward, "]", false),
            item(
                "Reopen Closed Tab",
                Action::Shortcut(Shortcut::ReopenClosedTab),
                "",
                false,
            ),
        ],
        mtm,
        &target,
        &mut items,
    );
    let tabs = menu(
        "Tabs",
        &[
            item("Next Tab", Action::Shortcut(Shortcut::NextTab), "]", true),
            item(
                "Previous Tab",
                Action::Shortcut(Shortcut::PreviousTab),
                "[",
                true,
            ),
            item("Move Tab to Favourites", Action::Favourite, "", false),
            item("Split Left", Action::SplitLeft, "", false),
            item("Split Right", Action::SplitRight, "", false),
            item("Exit Split View", Action::ExitSplit, "", false),
        ],
        mtm,
        &target,
        &mut items,
    );
    let window = menu(
        "Window",
        &[
            native("Minimize", "m", false),
            native("Zoom", "", false),
            separator(),
            native("Bring All to Front", "", false),
        ],
        mtm,
        &target,
        &mut items,
    );
    let root_menu =
        unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("")) };
    for (name, submenu) in [
        ("Elaris", app_menu),
        ("File", file),
        ("Edit", edit),
        ("View", view),
        ("History", history),
        ("Tabs", tabs),
        ("Window", window),
    ] {
        let root = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(name),
                None,
                &NSString::from_str(""),
            )
        };
        root.setSubmenu(Some(&submenu));
        root_menu.addItem(&root);
    }
    application.setMainMenu(Some(&root_menu));
    MENU_ITEMS.with(|slot| *slot.borrow_mut() = items);
    MENU_TARGET.with(|slot| *slot.borrow_mut() = Some(target));
}

pub(crate) fn register_window(id: u64, app: rune::AppHandle<AppState>) {
    APP_HANDLES.with(|handles| {
        handles.borrow_mut().insert(id, app);
    });
}

pub(crate) fn set_active_window(id: u64, active: bool) {
    if active {
        ACTIVE_WINDOW.with(|slot| slot.set(Some(id)));
    } else {
        ACTIVE_WINDOW.with(|slot| {
            if slot.get() == Some(id) {
                slot.set(None);
            }
        });
    }
}

pub(crate) fn unregister_window(id: u64) {
    APP_HANDLES.with(|handles| {
        handles.borrow_mut().remove(&id);
    });
    QUIT_PENDING.with(|pending| {
        if let Some(pending) = pending.borrow_mut().as_mut() {
            pending.remove(&id);
        }
    });
    set_active_window(id, false);
}

/// Refreshes native item availability and labels after Rune builds the current browser state.
#[cfg_attr(elaris_ui = "linux", allow(dead_code))]
pub(crate) fn refresh(state: &AppState) {
    if ACTIVE_WINDOW.with(Cell::get) != Some(state.window_id) {
        return;
    }
    let snapshot = MenuState::from(state);
    // Unchanged state needs no AppKit calls: every frame would otherwise rewrite each item.
    // The menu reapplies its state when it opens, which also covers the full-screen title.
    let previous = MENU_STATE.with(|slot| slot.borrow_mut().replace(snapshot));
    if previous == Some(snapshot) {
        return;
    }
    if let Some(mtm) = MainThreadMarker::new() {
        apply_menu_state(snapshot, mtm);
    }
}

unsafe extern "C" fn refresh_from_menu(_: &AnyObject, _: Sel, _: &AnyObject) {
    MENU_STATE.with(|slot| {
        if let Some(snapshot) = *slot.borrow()
            && let Some(mtm) = MainThreadMarker::new()
        {
            apply_menu_state(snapshot, mtm);
        }
    });
}

fn apply_menu_state(snapshot: MenuState, mtm: MainThreadMarker) {
    MENU_ITEMS.with(|slot| {
        for item in slot.borrow().iter() {
            let tag = unsafe { msg_send![&**item, tag] };
            let enabled = action_from_id(tag)
                .and_then(menu_command)
                .is_some_and(|command| snapshot.enabled(command));
            unsafe { item.setEnabled(enabled) };
            match action_from_id(tag) {
                Some(Action::Favourite) => unsafe {
                    item.setTitle(&NSString::from_str(if snapshot.favourite {
                        "Remove from Favourites"
                    } else {
                        "Move Tab to Favourites"
                    }))
                },
                Some(Action::Shortcut(Shortcut::ToggleSidebar)) => unsafe {
                    item.setState(if snapshot.sidebar_visible { 1 } else { 0 })
                },
                _ => {}
            }
        }
    });
    let full_screen = NSApplication::sharedApplication(mtm)
        .keyWindow()
        .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen));
    FULL_SCREEN_ITEM.with(|slot| {
        if let Some(item) = slot.borrow().as_ref() {
            unsafe {
                item.setTitle(&NSString::from_str(if full_screen {
                    "Exit Full Screen"
                } else {
                    "Enter Full Screen"
                }))
            };
        }
    });
}

fn menu_command(action: Action) -> Option<MenuCommand> {
    Some(match action {
        Action::Back => MenuCommand::Back,
        Action::Forward => MenuCommand::Forward,
        Action::Favourite => MenuCommand::Favourite,
        Action::SplitLeft | Action::SplitRight => MenuCommand::Split,
        Action::ExitSplit => MenuCommand::ExitSplit,
        Action::Edit(_) => MenuCommand::Edit,
        Action::Shortcut(Shortcut::NewTab) => MenuCommand::NewTab,
        Action::Shortcut(Shortcut::NewWindow | Shortcut::NewPrivateWindow) => {
            MenuCommand::NewWindow
        }
        Action::Shortcut(Shortcut::Close) => MenuCommand::CloseTab,
        Action::Shortcut(Shortcut::ReopenClosedTab) => MenuCommand::ReopenClosedTab,
        Action::Shortcut(Shortcut::Find) => MenuCommand::Find,
        Action::Shortcut(Shortcut::Reload) => MenuCommand::Reload,
        Action::Shortcut(
            Shortcut::ReloadBypassingCache
            | Shortcut::ZoomIn
            | Shortcut::ZoomOut
            | Shortcut::ResetZoom,
        ) => MenuCommand::WebPage,
        Action::Shortcut(Shortcut::ToggleSidebar) => MenuCommand::ToggleSidebar,
        Action::Shortcut(Shortcut::OpenHistory) => MenuCommand::History,
        Action::Shortcut(Shortcut::NextTab) => MenuCommand::NextTab,
        Action::Shortcut(Shortcut::PreviousTab) => MenuCommand::PreviousTab,
        Action::Shortcut(Shortcut::OpenSettings) => MenuCommand::Settings,
        _ => return None,
    })
}

fn perform_action(state: &mut AppState, action: Action, tx: &mut Tx) {
    let tab = state.browser.active_tab().map(Tab::id);
    match action {
        Action::Shortcut(shortcut) => shortcuts::dispatch(state, shortcut, tx),
        Action::Back => {
            if let Some(tab) = tab {
                state.browser.go_back(tab);
            }
        }
        Action::Forward => {
            if let Some(tab) = tab {
                state.browser.go_forward(tab);
            }
        }
        Action::Favourite => {
            if let Some(tab) = tab {
                let section = state.browser.tab(tab).map(Tab::section);
                let destination = if section == Some(TabSection::Favourite) {
                    TabSection::Regular
                } else {
                    TabSection::Favourite
                };
                state.move_tab_to_section_animated(tab, destination, tx);
            }
        }
        Action::SplitLeft | Action::SplitRight => {
            if let Some(tab) = tab {
                state.begin_split_picker(tab, usize::from(matches!(action, Action::SplitRight)));
            }
        }
        Action::ExitSplit => {
            state.browser.exit_split(tab);
            tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
            tx.animated();
        }
        Action::Edit(edit) => {
            let page_edit = match edit {
                EditAction::Cut => Some(crate::ui::viewport::PageEdit::Cut),
                EditAction::Copy => Some(crate::ui::viewport::PageEdit::Copy),
                EditAction::Paste => Some(crate::ui::viewport::PageEdit::Paste),
                EditAction::SelectAll => Some(crate::ui::viewport::PageEdit::SelectAll),
                EditAction::Undo | EditAction::Redo => None,
            };
            let handled =
                page_edit.is_some_and(|edit| crate::ui::viewport::menu_edit(state, edit, tx));
            if !handled {
                let selector = match edit {
                    EditAction::Undo => sel!(undo:),
                    EditAction::Redo => sel!(redo:),
                    EditAction::Cut => sel!(cut:),
                    EditAction::Copy => sel!(copy:),
                    EditAction::Paste => sel!(paste:),
                    EditAction::SelectAll => sel!(selectAll:),
                };
                let mtm =
                    MainThreadMarker::new().expect("menu actions run on AppKit's main thread");
                let application = NSApplication::sharedApplication(mtm);
                // SAFETY: AppKit resolves a nil target through the active native responder chain.
                unsafe { application.sendAction_to_from(selector, None, None) };
            }
        }
    }
}

unsafe extern "C" fn menu_action(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(action) = action_from_id(tag) else {
        return;
    };
    let handle = ACTIVE_WINDOW
        .with(Cell::get)
        .and_then(|id| APP_HANDLES.with(|handles| handles.borrow().get(&id).cloned()))
        .or_else(|| APP_HANDLES.with(|handles| handles.borrow().values().next().cloned()));
    if let Some(handle) = handle {
        handle.update(move |state, tx| perform_action(state, action, tx));
    } else if action == Action::Shortcut(Shortcut::NewWindow) {
        crate::open_browser_window();
    } else if action == Action::Shortcut(Shortcut::NewPrivateWindow) {
        crate::open_private_browser_window();
    }
}

fn menu_target_class() -> &'static AnyClass {
    const NAME: &str = "ElarisApplicationMenuTarget";
    if let Some(class) = AnyClass::get(NAME) {
        return class;
    }
    let mut builder = ClassBuilder::new(NAME, objc2_05::runtime::NSObject::class())
        .expect("register Elaris menu target");
    if let Some(protocol) = AnyProtocol::get("NSMenuDelegate") {
        builder.add_protocol(protocol);
    }
    unsafe {
        builder.add_method(
            sel!(menuAction:),
            menu_action as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(menuNeedsUpdate:),
            refresh_from_menu as unsafe extern "C" fn(_, _, _),
        );
    }
    builder.register()
}

fn action_id(action: Action) -> isize {
    match action {
        Action::Shortcut(shortcut) => 100 + shortcut_id(shortcut),
        Action::Back => 1,
        Action::Forward => 2,
        Action::Favourite => 4,
        Action::SplitLeft => 5,
        Action::SplitRight => 6,
        Action::ExitSplit => 7,
        Action::Edit(edit) => {
            8 + match edit {
                EditAction::Undo => 0,
                EditAction::Redo => 1,
                EditAction::Cut => 2,
                EditAction::Copy => 3,
                EditAction::Paste => 4,
                EditAction::SelectAll => 5,
            }
        }
    }
}

fn action_from_id(id: isize) -> Option<Action> {
    Some(match id {
        1 => Action::Back,
        2 => Action::Forward,
        4 => Action::Favourite,
        5 => Action::SplitLeft,
        6 => Action::SplitRight,
        7 => Action::ExitSplit,
        8 => Action::Edit(EditAction::Undo),
        9 => Action::Edit(EditAction::Redo),
        10 => Action::Edit(EditAction::Cut),
        11 => Action::Edit(EditAction::Copy),
        12 => Action::Edit(EditAction::Paste),
        13 => Action::Edit(EditAction::SelectAll),
        100 => Action::Shortcut(Shortcut::NewTab),
        101 => Action::Shortcut(Shortcut::Close),
        102 => Action::Shortcut(Shortcut::ReopenClosedTab),
        103 => Action::Shortcut(Shortcut::Find),
        104 => Action::Shortcut(Shortcut::Reload),
        105 => Action::Shortcut(Shortcut::ToggleSidebar),
        106 => Action::Shortcut(Shortcut::OpenHistory),
        107 => Action::Shortcut(Shortcut::NextTab),
        108 => Action::Shortcut(Shortcut::PreviousTab),
        109 => Action::Shortcut(Shortcut::OpenSettings),
        110 => Action::Shortcut(Shortcut::OpenFile),
        111 => Action::Shortcut(Shortcut::ReloadBypassingCache),
        112 => Action::Shortcut(Shortcut::ZoomIn),
        113 => Action::Shortcut(Shortcut::ZoomOut),
        114 => Action::Shortcut(Shortcut::ResetZoom),
        115 => Action::Shortcut(Shortcut::NewPrivateWindow),
        _ => return None,
    })
}

fn shortcut_id(shortcut: Shortcut) -> isize {
    match shortcut {
        Shortcut::NewTab => 0,
        Shortcut::Close => 1,
        Shortcut::ReopenClosedTab => 2,
        Shortcut::Find => 3,
        Shortcut::Reload => 4,
        Shortcut::ToggleSidebar => 5,
        Shortcut::OpenHistory => 6,
        Shortcut::NextTab => 7,
        Shortcut::PreviousTab => 8,
        Shortcut::OpenSettings => 9,
        Shortcut::OpenFile => 10,
        Shortcut::ReloadBypassingCache => 11,
        Shortcut::ZoomIn => 12,
        Shortcut::ZoomOut => 13,
        Shortcut::ResetZoom => 14,
        Shortcut::NewPrivateWindow => 15,
        _ => -1,
    }
}

/// Rune's quit guard: a quit from the menu, the Dock or the system waits for every window's
/// pages to agree to unload, then quits from the last approval.
fn quit_guard() -> bool {
    let handles: Vec<_> = APP_HANDLES.with(|handles| handles.borrow().values().cloned().collect());
    if handles.is_empty() {
        return true;
    }
    let started = QUIT_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.is_some() {
            return false;
        }
        *pending = Some(APP_HANDLES.with(|handles| handles.borrow().keys().copied().collect()));
        true
    });
    if started {
        for handle in handles {
            handle.update(|state, tx| state.request_quit_close(tx));
        }
    }
    false
}

/// Records one window's approval and quits once every window has approved.
pub(crate) fn quit_window_approved(id: u64, tx: &mut Tx) {
    let finished = QUIT_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let Some(windows) = pending.as_mut() else {
            return false;
        };
        windows.remove(&id);
        let finished = windows.is_empty();
        if finished {
            *pending = None;
        }
        finished
    });
    if finished {
        tx.quit();
    }
}

/// A page kept its window open, so the whole quit is abandoned.
pub(crate) fn quit_cancelled() {
    QUIT_PENDING.with(|pending| *pending.borrow_mut() = None);
}
