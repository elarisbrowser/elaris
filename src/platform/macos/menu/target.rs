// SPDX-License-Identifier: GPL-3.0-only

//! The Objective-C action target that receives menu clicks and menu-open refreshes.

use std::cell::Cell;

use objc2_05::runtime::{AnyClass, AnyObject, AnyProtocol, ClassBuilder, Sel};
use objc2_05::{ClassType, msg_send, sel};
use objc2_foundation_02::MainThreadMarker;

use super::dispatch::perform_action;
use super::items::{Action, action_from_id};
use super::window::apply_menu_state;
use super::{ACTIVE_WINDOW, APP_HANDLES, MENU_STATE};
use crate::shortcuts::Shortcut;

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

unsafe extern "C" fn refresh_from_menu(_: &AnyObject, _: Sel, _: &AnyObject) {
    MENU_STATE.with(|slot| {
        if let Some(snapshot) = *slot.borrow()
            && let Some(mtm) = MainThreadMarker::new()
        {
            apply_menu_state(snapshot, mtm);
        }
    });
}

pub(super) fn menu_target_class() -> &'static AnyClass {
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
