// SPDX-License-Identifier: GPL-3.0-only

//! AppKit context menus for webpage targets.

use std::cell::{Cell, RefCell};

use objc2_05::rc::Retained;
use objc2_05::runtime::{AnyObject, Sel};
use objc2_05::{msg_send, sel};
use objc2_app_kit_02::{NSMenu, NSMenuItem};
use objc2_foundation_02::{MainThreadMarker, NSPoint, NSString};

use crate::app::AppState;
use crate::engine::ViewCommand;
use crate::tab::TabId;
use elaris::ladybird::{Edit, MediaAction, PageMenu};

#[derive(Clone)]
enum Action {
    Open { request: u64, new_tab: bool },
    Copy(String),
    Media(MediaAction),
    Paste,
    SelectAll,
}

thread_local! {
    static ACTIONS: RefCell<Vec<Action>> = const { RefCell::new(Vec::new()) };
    static MENU_TAB: Cell<Option<TabId>> = const { Cell::new(None) };
}

pub(super) unsafe extern "C" fn selected(_: &AnyObject, _: Sel, sender: &AnyObject) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let action = ACTIONS.with(|actions| actions.borrow().get(tag as usize).cloned());
    let handle = super::APP_HANDLES.with(|handles| {
        super::ACTIVE_WINDOW.with(|active| {
            active
                .get()
                .and_then(|id| handles.borrow().get(&id).cloned())
        })
    });
    if let (Some(action), Some(handle)) = (action, handle) {
        let tab = MENU_TAB.with(Cell::get);
        handle.update(move |state: &mut AppState, tx| {
            if tab.is_some_and(|id| {
                state
                    .browser
                    .active_tab()
                    .is_some_and(|active| active.id() == id)
            }) {
                dispatch(state, tx, action);
            }
        });
    }
}

fn dispatch(state: &mut AppState, tx: &mut rune::Tx, action: Action) {
    let Some(tab) = state.browser.active_tab().map(crate::tab::Tab::id) else {
        return;
    };
    match action {
        Action::Open { request, new_tab } => state
            .browser
            .send(tab, ViewCommand::OpenContext { request, new_tab }),
        Action::Copy(text) => tx.copy(text),
        Action::Media(action) => state.browser.send(tab, ViewCommand::MediaAction(action)),
        Action::Paste => crate::ui::viewport::paste(tab, tx),
        Action::SelectAll => state.browser.send(tab, ViewCommand::Edit(Edit::SelectAll)),
    }
}

fn add(menu: &NSMenu, title: &str, action: Action, target: &AnyObject, mtm: MainThreadMarker) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            Some(sel!(pageMenuAction:)),
            &NSString::from_str(""),
        )
    };
    let tag = ACTIONS.with(|actions| {
        let mut actions = actions.borrow_mut();
        let tag = actions.len() as isize;
        actions.push(action);
        tag
    });
    unsafe {
        item.setTarget(Some(target));
        item.setTag(tag);
    }
    menu.addItem(&item);
}

pub(crate) fn show_page_menu(tab: TabId, point: rune::PointerPress, info: PageMenu) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    ACTIONS.with(|actions| actions.borrow_mut().clear());
    MENU_TAB.with(|active| active.set(Some(tab)));
    let menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str("")) };
    let target = super::MENU_TARGET.with(|target| {
        target
            .borrow()
            .as_ref()
            .map(|target| Retained::as_ptr(target) as *const AnyObject)
    });
    let Some(target) = target else { return };
    let target = unsafe { &*target };
    if let Some(media) = info.media {
        add(
            &menu,
            if media.playing { "Pause" } else { "Play" },
            Action::Media(MediaAction::TogglePlay),
            target,
            mtm,
        );
        add(
            &menu,
            if media.muted { "Unmute" } else { "Mute" },
            Action::Media(MediaAction::ToggleMute),
            target,
            mtm,
        );
        if media.can_enter_picture_in_picture || media.in_picture_in_picture {
            add(
                &menu,
                if media.in_picture_in_picture {
                    "Exit Picture in Picture"
                } else {
                    "Picture in Picture"
                },
                Action::Media(MediaAction::TogglePictureInPicture),
                target,
                mtm,
            );
        }
    }
    if !info.url.is_empty() {
        if !info.image {
            add(
                &menu,
                "Open Link",
                Action::Open {
                    request: info.request,
                    new_tab: false,
                },
                target,
                mtm,
            );
        }
        add(
            &menu,
            if info.image {
                "Open Image in New Tab"
            } else {
                "Open Link in New Tab"
            },
            Action::Open {
                request: info.request,
                new_tab: true,
            },
            target,
            mtm,
        );
        add(
            &menu,
            if info.image {
                "Copy Image Address"
            } else {
                "Copy Link Address"
            },
            Action::Copy(info.url.clone()),
            target,
            mtm,
        );
    }
    if !info.selection.is_empty() {
        add(&menu, "Copy", Action::Copy(info.selection), target, mtm);
    }
    if info.editable {
        add(&menu, "Paste", Action::Paste, target, mtm);
    }
    if info.url.is_empty() {
        add(&menu, "Select All", Action::SelectAll, target, mtm);
    }
    let location = NSPoint {
        x: point.x.into(),
        y: point.y.into(),
    };
    unsafe {
        menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);
    }
}
