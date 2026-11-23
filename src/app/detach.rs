// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::time::{Duration, Instant};

use rune::{AppHandle, Tx, WindowHandle};

use super::{AppState, unregister_window};
use crate::tab::TabId;

const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(32);

thread_local! {
    static PENDING: RefCell<Option<Detach>> = const { RefCell::new(None) };
}

struct Detach {
    source: u64,
    destination: u64,
    tab: TabId,
    window: WindowHandle,
    app: AppHandle<AppState>,
    deadline: Instant,
    submitted: bool,
}

pub(crate) fn release(source: &AppState, tab: TabId) -> bool {
    if rune::is_headless()
        || source.browser.tab(tab).is_none()
        || super::transfer::in_progress()
        || PENDING.with_borrow(Option::is_some)
    {
        return false;
    }
    let Some(placement) = placement(source.window_id) else {
        return false;
    };
    let mut destination = AppState::new_window();
    if source.browser.private {
        destination.make_private();
    }
    destination.omnibox.close();
    destination.pending_detach = true;
    let id = destination.window_id;
    let (window, app) = crate::queue_browser_window(destination, Some(placement));
    PENDING.replace(Some(Detach {
        source: source.window_id,
        destination: id,
        tab,
        window,
        app,
        deadline: Instant::now() + OPEN_TIMEOUT,
        submitted: false,
    }));
    true
}

pub(super) fn poll(state: &mut AppState, tx: &mut Tx) {
    PENDING.with_borrow_mut(|pending| {
        let Some(detach) = pending
            .as_mut()
            .filter(|detach| detach.source == state.window_id)
        else {
            return;
        };
        if detach.submitted {
            if !super::transfer::in_progress()
                && let Some(detach) = pending.take()
            {
                discard(detach);
            }
            return;
        }
        let error = detach.app.window_error().or_else(|| {
            (Instant::now() >= detach.deadline)
                .then(|| "the new window did not become ready".to_owned())
        });
        if let Some(error) = error {
            eprintln!("Elaris: could not detach tab: {error}");
            if let Some(detach) = pending.take() {
                discard(detach);
            }
            return;
        }
        let Some(tab) = state.browser.tab(detach.tab) else {
            if let Some(detach) = pending.take() {
                discard(detach);
            }
            return;
        };
        if detach.window.is_ready() {
            let section = tab.section();
            if state.request_tab_transfer(detach.tab, detach.destination, section, 0) {
                detach.submitted = true;
                return;
            }
        }
        tx.wake_after(READY_POLL_INTERVAL);
    });
}

pub(super) fn committed(state: &mut AppState, tab: TabId) {
    if PENDING.with_borrow(|pending| {
        pending
            .as_ref()
            .is_some_and(|detach| detach.destination == state.window_id && detach.tab == tab)
    }) {
        PENDING.replace(None);
        state.pending_detach = false;
    }
}

pub(crate) fn cancel(state: &mut AppState) -> bool {
    cancel_pending(state, false)
}

pub(super) fn closing(state: &mut AppState) {
    cancel_pending(state, true);
}

fn cancel_pending(state: &mut AppState, closing: bool) -> bool {
    let detach = PENDING.with_borrow_mut(|pending| {
        if pending.as_ref().is_some_and(|detach| {
            detach.source == state.window_id || detach.destination == state.window_id
        }) {
            pending.take()
        } else {
            None
        }
    });
    if let Some(detach) = detach {
        if !closing || detach.destination != state.window_id {
            discard(detach);
        }
        state.cancel_tab_transfer();
        return true;
    }
    false
}

fn discard(detach: Detach) {
    // A cancelled destination no longer counts as a browser window during source teardown.
    unregister_window(detach.destination);
    detach.window.close();
}

#[derive(Clone, Copy)]
pub(crate) struct Placement {
    #[cfg(target_os = "macos")]
    pointer: objc2_foundation_02::NSPoint,
    #[cfg(target_os = "macos")]
    usable: objc2_foundation_02::NSRect,
    #[cfg(target_os = "linux")]
    x11: x11::Placement,
}

#[cfg(target_os = "macos")]
fn placement(source: u64) -> Option<Placement> {
    use objc2_app_kit_02::{NSEvent, NSScreen};
    use objc2_foundation_02::MainThreadMarker;

    let windows = super::WINDOW_REGISTRY.get()?.lock().ok()?;
    let frame = crate::window_macos::with_window(windows.get(&source)?, |native| native.frame())?;
    let main = MainThreadMarker::new()?;
    // SAFETY: the drag handler is on AppKit's main thread.
    let pointer = unsafe { NSEvent::mouseLocation() };
    let hit = crate::drag_overlay_macos::window_below_preview(pointer, main);
    // Finder and the wallpaper can have window numbers. Only an Elaris hit consumes this drop.
    let browser_hit = u32::try_from(hit).ok().is_some_and(|hit| {
        windows
            .values()
            .any(|window| window.window_number() == Some(hit))
    });
    if !can_detach_at(frame, pointer, browser_hit) {
        return None;
    }
    let usable = NSScreen::screens(main)
        .iter()
        .map(|screen| screen.visibleFrame())
        .find(|frame| contains(*frame, pointer, 0.0))?;
    Some(Placement { pointer, usable })
}

#[cfg(target_os = "macos")]
fn can_detach_at(
    source: objc2_foundation_02::NSRect,
    pointer: objc2_foundation_02::NSPoint,
    browser_hit: bool,
) -> bool {
    const EDGE_DISTANCE: f64 = 28.0;
    !browser_hit && !contains(source, pointer, EDGE_DISTANCE)
}

#[cfg(target_os = "macos")]
fn contains(
    rect: objc2_foundation_02::NSRect,
    point: objc2_foundation_02::NSPoint,
    margin: f64,
) -> bool {
    point.x >= rect.origin.x - margin
        && point.y >= rect.origin.y - margin
        && point.x < rect.origin.x + rect.size.width + margin
        && point.y < rect.origin.y + rect.size.height + margin
}

#[cfg(target_os = "linux")]
mod x11;

#[cfg(target_os = "linux")]
fn placement(source: u64) -> Option<Placement> {
    let window = super::native_window(source)?;
    Some(Placement {
        x11: x11::placement(&window)?,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn placement(_: u64) -> Option<Placement> {
    None
}

impl Placement {
    pub(crate) fn apply(self, window: &WindowHandle) {
        #[cfg(target_os = "macos")]
        crate::window_macos::with_window(window, |native| {
            native.setFrame_display(self.frame_near_pointer(native.frame().size), true);
        });
        #[cfg(target_os = "linux")]
        self.x11.apply(window);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = window;
    }

    #[cfg(target_os = "macos")]
    fn frame_near_pointer(self, size: objc2_foundation_02::NSSize) -> objc2_foundation_02::NSRect {
        use objc2_foundation_02::{NSPoint, NSRect, NSSize};

        let width = size.width.min(self.usable.size.width);
        let height = size.height.min(self.usable.size.height);
        let x = (self.pointer.x - width / 4.0).clamp(
            self.usable.origin.x,
            self.usable.origin.x + self.usable.size.width - width,
        );
        let y = (self.pointer.y - height + 48.0).clamp(
            self.usable.origin.y,
            self.usable.origin.y + self.usable.size.height - height,
        );
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(source: &AppState, destination: AppState, deadline: Instant) -> u64 {
        let id = destination.window_id;
        let app =
            rune::App::new(crate::app_spec(AppState::new_window, false)).init(move || destination);
        let handle = app.handle();
        let window =
            rune::open_window(rune::WindowSpec::from_app(format!("detach-test-{id}"), app));
        super::super::register_window(id, window.clone());
        PENDING.replace(Some(Detach {
            source: source.window_id,
            destination: id,
            tab: source.browser.active_tab().unwrap().id(),
            window,
            app: handle,
            deadline,
            submitted: false,
        }));
        id
    }

    #[test]
    fn native_creation_timeout_preserves_source_and_removes_destination() {
        let mut source = AppState::new(Some("one.example"));
        let before = source.session_snapshot();
        let id = pending(&source, AppState::new_window(), Instant::now());
        poll(&mut source, &mut Tx::default());
        assert!(PENDING.with_borrow(Option::is_none));
        assert_eq!(source.session_snapshot(), before);
        assert!(
            !super::super::WINDOW_REGISTRY
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .contains_key(&id)
        );
    }

    #[test]
    fn cancellation_before_readiness_keeps_the_original_tab() {
        let mut source = AppState::new(Some("one.example"));
        let id = source.browser.active_tab().unwrap().id();
        pending(
            &source,
            AppState::new_window(),
            Instant::now() + OPEN_TIMEOUT,
        );
        poll(&mut source, &mut Tx::default());
        assert!(PENDING.with_borrow(Option::is_some));
        assert!(cancel(&mut source));
        assert!(!cancel(&mut source));
        assert_eq!(source.browser.active_tab().unwrap().id(), id);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn placement_clamps_to_the_pointer_display_in_negative_coordinates() {
        use objc2_foundation_02::{NSPoint, NSRect, NSSize};

        let usable = NSRect::new(NSPoint::new(-1200.0, 40.0), NSSize::new(1200.0, 800.0));
        let placement = Placement {
            pointer: NSPoint::new(-20.0, 55.0),
            usable,
        };
        let frame = placement.frame_near_pointer(NSSize::new(960.0, 640.0));
        assert_eq!(frame.origin, NSPoint::new(-960.0, 40.0));
        assert_eq!(frame.size, NSSize::new(960.0, 640.0));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn detachment_requires_clearance_and_yields_to_browser_windows() {
        use objc2_foundation_02::{NSPoint, NSRect, NSSize};

        let source = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
        assert!(!can_detach_at(source, NSPoint::new(810.0, 300.0), false));
        assert!(!can_detach_at(source, NSPoint::new(900.0, 300.0), true));
        assert!(can_detach_at(source, NSPoint::new(900.0, 300.0), false));
    }
}
