// SPDX-License-Identifier: GPL-3.0-only

//! Builds the native menu bar and its items, and installs it on the AppKit main thread.

use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyObject, ProtocolObject, Sel};
use objc2_05::{msg_send_id, sel};
use objc2_app_kit_02::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuDelegate, NSMenuItem};
use objc2_foundation_02::{MainThreadMarker, NSString};

use super::items::{
    Action, EditAction, ItemSpec, action_id, edit, item, native, native_control, native_option,
    separator,
};
use super::quit::quit_guard;
use super::target::menu_target_class;
use super::{FULL_SCREEN_ITEM, MENU_ITEMS, MENU_TARGET};
use crate::app::AppState;
use crate::shortcuts::Shortcut;

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
            item(
                "Task Manager",
                Action::Shortcut(Shortcut::OpenTaskManager),
                "",
                false,
            ),
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
