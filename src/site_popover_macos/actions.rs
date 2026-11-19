// SPDX-License-Identifier: GPL-3.0-only

//! Native AppKit host for the shared Site Control Centre presentation.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2_05::{ClassType, msg_send, sel};
use objc2_app_kit_02::{NSEvent, NSEventMask, NSMenu, NSMenuItem, NSView};
use objc2_foundation_02::{MainThreadMarker, NSPoint, NSString};

use crate::app;
use crate::permissions::{PermissionKind, PermissionState};
use crate::site_controls::SiteControlAction;

use super::{APP_HANDLE, HOST, close, close_host};

fn permission_menu(target: &AnyObject, sender: &AnyObject, kind: PermissionKind) {
    let mtm = MainThreadMarker::new().expect("permission menus run on AppKit's main thread");
    let menu =
        unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("Permission")) };
    let selected = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.as_ref())
            .map(|data| data.permissions[kind.index()].1)
    });
    for (index, (permission, title)) in [
        (PermissionState::Ask, "Ask"),
        (PermissionState::Allow, "Allow"),
        (PermissionState::Block, "Block"),
    ]
    .into_iter()
    .enumerate()
    {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(title),
                Some(sel!(permissionSelected:)),
                &NSString::from_str(""),
            )
        };
        unsafe {
            item.setTarget(Some(target));
            item.setTag((kind.index() * 3 + index) as isize);
            if Some(permission) == selected {
                item.setState(1);
            }
            menu.addItem(&item);
        }
    }
    // SAFETY: this native row button and its menu are live on AppKit's main thread.
    unsafe {
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, 0.0),
            Some(&*(std::ptr::from_ref(sender).cast::<NSView>())),
        );
    }
}

fn overflow_menu(target: &AnyObject, sender: &AnyObject) {
    let mtm = MainThreadMarker::new().expect("native menus run on AppKit's main thread");
    let menu = unsafe { NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("Site")) };
    for (tag, title) in [
        (0, "Reset permissions for this site"),
        (1, "Copy page address"),
    ] {
        if tag == 1
            && HOST.with(|host| {
                host.borrow()
                    .as_ref()
                    .and_then(|host| host.presentation.as_ref())
                    .and_then(|data| data.url.as_ref())
                    .is_none()
            })
        {
            continue;
        }
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc::<NSMenuItem>(),
                &NSString::from_str(title),
                Some(sel!(siteOverflowSelected:)),
                &NSString::from_str(""),
            )
        };
        unsafe {
            item.setTarget(Some(target));
            item.setTag(tag);
            menu.addItem(&item);
        }
    }
    // SAFETY: the menu and its in-popover button are live on AppKit's main thread.
    unsafe {
        menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, 0.0),
            Some(&*(std::ptr::from_ref(sender).cast::<NSView>())),
        );
    }
}

unsafe extern "C" fn site_action(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(action) = SiteControlAction::ALL.get(tag.max(0) as usize).copied() else {
        return;
    };
    let data = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.clone())
    });
    let Some(data) = data else { return };
    APP_HANDLE.with(|handle| {
        if let Some(handle) = handle.borrow().as_ref() {
            let should_close = action == SiteControlAction::Settings;
            handle.update(move |state, tx| match action {
                SiteControlAction::CopyUrl => {
                    if let Some(url) = data.url.as_deref() {
                        tx.copy(url);
                    }
                }
                SiteControlAction::Reload => {
                    if data.tab == state.browser.active_tab().map(|tab| tab.id())
                        && let Some(tab) = data.tab
                    {
                        state.browser.reload(tab);
                    }
                }
                SiteControlAction::DuplicateTab => {
                    if data.tab == state.browser.active_tab().map(|tab| tab.id())
                        && let Some(tab) = data.tab
                    {
                        state.duplicate(tab);
                    }
                }
                SiteControlAction::Settings => app::open_settings(state, tx),
            });
            if should_close {
                close();
            }
        }
    });
}

unsafe extern "C" fn permission_row(target: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(kind_index) = tag.checked_sub(SiteControlAction::ALL.len() as isize) else {
        return;
    };
    let Some(kind) = PermissionKind::ALL.get(kind_index as usize).copied() else {
        return;
    };
    permission_menu(target, sender, kind);
}

unsafe extern "C" fn permission_selected(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    if tag < 0 {
        return;
    }
    let Some(kind) = PermissionKind::ALL.get(tag as usize / 3).copied() else {
        return;
    };
    let permission = match tag as usize % 3 {
        0 => PermissionState::Ask,
        1 => PermissionState::Allow,
        _ => PermissionState::Block,
    };
    let context = HOST.with(|host| {
        let host = host.borrow();
        let host = host.as_ref()?;
        Some((
            host.presentation.as_ref()?.origin.clone()?,
            host.statuses.get(kind.index())?.clone(),
        ))
    });
    let Some((origin, status)) = context else {
        return;
    };
    APP_HANDLE.with(|handle| {
        if let Some(handle) = handle.borrow().as_ref() {
            let expected_origin = origin.clone();
            handle.update(move |state, _| {
                if state.current_permission_origin().as_deref() == Some(&expected_origin) {
                    state.set_site_permission(kind, permission);
                }
            });
        }
    });
    let label = NSString::from_str(permission_label(permission));
    unsafe { status.setStringValue(&label) };
    HOST.with(|host| {
        if let Some(data) = host
            .borrow_mut()
            .as_mut()
            .and_then(|host| host.presentation.as_mut())
        {
            data.permissions[kind.index()].1 = permission;
        }
    });
}

unsafe extern "C" fn site_overflow(target: &AnyObject, _: Sel, sender: &AnyObject) {
    overflow_menu(target, sender);
}

unsafe extern "C" fn site_overflow_selected(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let data = HOST.with(|host| {
        host.borrow()
            .as_ref()
            .and_then(|host| host.presentation.clone())
    });
    let Some(data) = data else { return };
    if tag == 0 {
        let Some(origin) = data.origin.clone() else {
            return;
        };
        APP_HANDLE.with(|handle| {
            if let Some(handle) = handle.borrow().as_ref() {
                handle.update(move |state, _| {
                    if state.current_permission_origin().as_deref() == Some(&origin) {
                        state.reset_site_permissions();
                    }
                });
            }
        });
        for status in HOST.with(|host| {
            host.borrow()
                .as_ref()
                .map(|host| host.statuses.clone())
                .unwrap_or_default()
        }) {
            unsafe { status.setStringValue(&NSString::from_str("Ask")) };
        }
    } else if let Some(url) = data.url {
        APP_HANDLE.with(|handle| {
            if let Some(handle) = handle.borrow().as_ref() {
                handle.update(move |_, tx| tx.copy(url));
            }
        });
    }
    close();
}
pub(super) fn action_target_class() -> &'static AnyClass {
    const NAME: &str = "ElarisSiteControlActionTarget";
    if let Some(class) = AnyClass::get(NAME) {
        return class;
    }
    let mut builder = ClassBuilder::new(NAME, NSView::class())
        .expect("register site popover action target class");
    // SAFETY: selectors use AppKit's target/action ABI: receiver, selector, sender.
    unsafe {
        builder.add_method(
            sel!(siteAction:),
            site_action as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(permissionRow:),
            permission_row as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(permissionSelected:),
            permission_selected as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(siteOverflow:),
            site_overflow as unsafe extern "C" fn(_, _, _),
        );
        builder.add_method(
            sel!(siteOverflowSelected:),
            site_overflow_selected as unsafe extern "C" fn(_, _, _),
        );
    }
    builder.register()
}
pub(super) fn permission_symbol(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Microphone => "mic",
        PermissionKind::Camera => "camera",
        PermissionKind::Location => "location",
        PermissionKind::Notifications => "bell",
    }
}

pub(super) fn permission_label(permission: PermissionState) -> &'static str {
    match permission {
        PermissionState::Ask => "Ask",
        PermissionState::Allow => "Allowed",
        PermissionState::Block => "Blocked for this site",
    }
}

pub(super) fn escape_monitor() -> Option<Retained<AnyObject>> {
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit supplies a live key-down event for the duration of this callback.
        let is_escape = unsafe { event.as_ref().keyCode() == 53 };
        let closed = is_escape
            && HOST.with(|slot| {
                let Ok(mut slot) = slot.try_borrow_mut() else {
                    return false;
                };
                let Some(host) = slot.as_mut() else {
                    return false;
                };
                if !unsafe { host.popover.isShown() } {
                    return false;
                }
                close_host(host);
                true
            });
        if closed {
            std::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    // SAFETY: AppKit retains the monitor block until its returned token is removed.
    unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) }
}
