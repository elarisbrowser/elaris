// SPDX-License-Identifier: GPL-3.0-only

//! Native AppKit host for the shared Site Control Centre presentation.

mod actions;
mod content;
mod native;

use std::cell::RefCell;
use std::collections::HashMap;

use objc2_05::msg_send_id;
use objc2_05::rc::Retained;
use objc2_05::runtime::AnyObject;
use objc2_app_kit_02::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSBox, NSColor, NSPopover,
    NSPopoverBehavior, NSTextField, NSView, NSViewController,
};
use objc2_foundation_02::{MainThreadMarker, NSRectEdge, NSSize};
use rune::WindowHandle as RuneWindowHandle;
use rune::raw_window_handle::RawWindowHandle;

use crate::app::AppState;
use crate::site_controls::SiteControlPresentation;
use crate::ui::site_control_style as design;
use crate::ui::site_control_style::Palette;

const WIDTH: f64 = design::WIDTH as f64;
const HEIGHT: f64 = design::HEIGHT as f64;
const CORNER_RADIUS: f64 = design::OUTER_RADIUS as f64;

pub(super) struct Host {
    anchor_view: Retained<NSView>,
    popover: Retained<NSPopover>,
    controller: Option<Retained<NSViewController>>,
    presentation: Option<SiteControlPresentation>,
    statuses: Vec<Retained<NSTextField>>,
    hover_surfaces: Vec<HoverSurface>,
    action_target: Option<Retained<NSView>>,
    _escape_monitor: Option<Retained<AnyObject>>,
}

pub(super) struct HoverSurface {
    view: Retained<NSBox>,
    resting_color: Retained<NSColor>,
    hover_color: Retained<NSColor>,
    hover_only: bool,
}

pub(super) struct BuiltContent {
    pub(super) controller: Retained<NSViewController>,
    pub(super) statuses: Vec<Retained<NSTextField>>,
    pub(super) hover_surfaces: Vec<HoverSurface>,
    pub(super) target: Retained<NSView>,
    pub(super) height: f64,
}

thread_local! {
    pub(super) static HOSTS: RefCell<HashMap<String, Host>> = RefCell::new(HashMap::new());
    pub(super) static APP_HANDLES: RefCell<HashMap<String, rune::AppHandle<AppState>>> = RefCell::new(HashMap::new());
    static WINDOW_KEYS: RefCell<HashMap<u64, String>> = RefCell::new(HashMap::new());
    static ACTIVE_KEY: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub(crate) fn install(window: &RuneWindowHandle, app: rune::AppHandle<AppState>) {
    let key = window.key().to_owned();
    let result = window.with_raw_handle(|handle| {
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: Rune owns this AppKit view for the window lifetime; this callback runs on main.
        let Some(anchor_view) =
            (unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) })
        else {
            return;
        };
        // SAFETY: AppKit objects are created on Rune's main/AppKit thread.
        let mtm = MainThreadMarker::new().expect("window hooks run on AppKit's main thread");
        let popover: Retained<NSPopover> = unsafe { msg_send_id![mtm.alloc::<NSPopover>(), init] };
        let escape_monitor = actions::escape_monitor();
        // SAFETY: these setters are called on the popover's AppKit thread.
        unsafe {
            popover.setBehavior(NSPopoverBehavior::Transient);
            popover.setAnimates(true);
        }
        HOSTS.with(|hosts| {
            hosts.borrow_mut().insert(
                key.clone(),
                Host {
                    anchor_view,
                    popover,
                    controller: None,
                    presentation: None,
                    statuses: Vec::new(),
                    hover_surfaces: Vec::new(),
                    action_target: None,
                    _escape_monitor: escape_monitor,
                },
            );
        });
        APP_HANDLES.with(|handles| {
            handles.borrow_mut().insert(key.clone(), app.clone());
        });
        let key = key.clone();
        app.update(move |state, _| {
            WINDOW_KEYS.with(|keys| {
                keys.borrow_mut().insert(state.window_id, key);
            });
        });
    });
    if let Err(error) = result {
        eprintln!("Elaris: cannot attach site popover to Rune window: {error}");
    }
}
/// Opens the native popover at the Rune button's current logical-point bounds.
pub(crate) fn show(data: SiteControlPresentation, anchor: [f32; 4], palette: Palette) -> bool {
    with_active_host(|host| {
        let Some(host) = host else {
            return false;
        };
        // Native transient dismissal handles outside clicks; the local key monitor handles Escape.
        if unsafe { host.popover.isShown() } {
            close_host(host);
            return false;
        }

        // SAFETY: AppKit exports these process-lifetime appearance-name constants.
        let appearance_name = unsafe {
            if palette.dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        let appearance = NSAppearance::appearanceNamed(appearance_name);
        // SAFETY: the native popover and appearance are retained on AppKit's main thread.
        unsafe { host.popover.setAppearance(appearance.as_deref()) };
        let BuiltContent {
            controller,
            statuses,
            hover_surfaces,
            target,
            height,
        } = content::build_content(&data, palette);
        // SAFETY: the content controller and popover belong to this AppKit thread.
        unsafe {
            host.popover.setContentViewController(Some(&controller));
            host.popover.setContentSize(NSSize::new(WIDTH, height));
        }
        host.controller = Some(controller);
        host.statuses = statuses;
        host.hover_surfaces = hover_surfaces;
        host.action_target = Some(target);
        host.presentation = Some(data);

        let rect = native::anchor_rect(&host.anchor_view, anchor);
        // SAFETY: anchor view and popover belong to the current live window on AppKit's thread.
        unsafe {
            host.popover.showRelativeToRect_ofView_preferredEdge(
                rect,
                &host.anchor_view,
                NSRectEdge::NSMaxYEdge,
            );
        }
        true
    })
}

/// Updates the attached button bounds after Rune relayouts the sidebar or window.
pub(crate) fn update_anchor(anchor: [f32; 4]) {
    with_active_host(|host| {
        let Some(host) = host else {
            return;
        };
        if !unsafe { host.popover.isShown() } {
            return;
        }
        let rect = native::anchor_rect(&host.anchor_view, anchor);
        // SAFETY: the popover and anchor view are retained by Host on AppKit's main thread.
        unsafe { host.popover.setPositioningRect(rect) };
    });
}

/// Closes the transient popover if the active page or its tab changes.
pub(crate) fn close() {
    with_active_host(|host| {
        if let Some(host) = host {
            close_host(host);
        }
    });
}

/// Releases the native host before Rune destroys its window on the final-window close path.
pub(crate) fn activate_window(id: u64) {
    WINDOW_KEYS.with(|keys| {
        if let Some(key) = keys.borrow().get(&id).cloned() {
            ACTIVE_KEY.with(|active| *active.borrow_mut() = Some(key));
        }
    });
}

pub(crate) fn detach_window(id: u64) {
    let key = WINDOW_KEYS.with(|keys| keys.borrow_mut().remove(&id));
    let Some(key) = key else { return };
    HOSTS.with(|hosts| {
        if let Some(mut host) = hosts.borrow_mut().remove(&key) {
            close_host(&mut host);
        }
    });
    APP_HANDLES.with(|handles| {
        handles.borrow_mut().remove(&key);
    });
    ACTIVE_KEY.with(|active| {
        if active.borrow().as_ref() == Some(&key) {
            *active.borrow_mut() = None;
        }
    });
}

pub(super) fn active_app_handle() -> Option<rune::AppHandle<AppState>> {
    let key = ACTIVE_KEY.with(|active| active.borrow().clone())?;
    APP_HANDLES.with(|handles| handles.borrow().get(&key).cloned())
}

pub(super) fn with_active_host<R>(f: impl FnOnce(Option<&mut Host>) -> R) -> R {
    let key = ACTIVE_KEY.with(|active| active.borrow().clone());
    HOSTS.with(|hosts| {
        let mut hosts = hosts.borrow_mut();
        f(key.and_then(|key| hosts.get_mut(&key)))
    })
}

pub(super) fn close_host(host: &mut Host) {
    // SAFETY: the popover is retained by Host and this call is on AppKit's main thread.
    unsafe { host.popover.performClose(None) };
    host.presentation = None;
}
