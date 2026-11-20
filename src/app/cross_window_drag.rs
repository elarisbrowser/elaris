// SPDX-License-Identifier: GPL-3.0-only

use std::cell::Cell;

use rune::{Drag, Tx, WindowHandle};

use super::{AppState, WINDOW_REGISTRY};
use crate::tab::TabId;
use crate::ui::TabDropTarget;

thread_local! {
    static DRAG: Cell<Option<IncomingDrag>> = const { Cell::new(None) };
}

#[derive(Clone, Copy, PartialEq)]
struct IncomingDrag {
    source: u64,
    destination: u64,
    tab: TabId,
    point: [f32; 2],
    phase: Phase,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Hovering,
    Released,
    Ready(TabDropTarget),
}

/// Returns whether the pointer is over another browser window. The destination computes the
/// insertion target from its own layout, using the release point again before requesting a move.
pub(crate) fn track(state: &AppState, tab: TabId, drag: &Drag, release: bool) -> bool {
    let incoming = locate(state.window_id, drag).map(|(destination, point)| IncomingDrag {
        source: state.window_id,
        destination,
        tab,
        point,
        phase: if release {
            Phase::Released
        } else {
            Phase::Hovering
        },
    });
    if let Some(incoming) = incoming {
        publish(Some(incoming));
        true
    } else {
        clear(state.window_id);
        false
    }
}

pub(crate) fn clear(window: u64) -> bool {
    if DRAG
        .get()
        .is_some_and(|drag| drag.source == window || drag.destination == window)
    {
        publish(None);
        return true;
    }
    false
}

fn publish(incoming: Option<IncomingDrag>) {
    if DRAG.replace(incoming) != incoming {
        crate::engine::wake_all_windows();
    }
}

pub(super) fn poll(state: &mut AppState, tx: &mut Tx) -> bool {
    let mut preview = None;
    if let Some(drag) = DRAG.get() {
        if drag.destination == state.window_id {
            let target = (!state.sidebar.collapsed)
                .then(|| {
                    state
                        .tab_drag
                        .incoming_target(state.browser.tabs(), drag.point)
                })
                .flatten();
            preview = target.map(|target| (drag.tab, target));
            if drag.phase == Phase::Released {
                publish(target.map(|target| IncomingDrag {
                    phase: Phase::Ready(target),
                    ..drag
                }));
            }
        } else if drag.source == state.window_id
            && let Phase::Ready(target) = drag.phase
        {
            publish(None);
            if state.request_tab_transfer(drag.tab, drag.destination, target.section, target.index)
            {
                state.tab_drag.cancel();
            }
        }
    }

    if state.tab_drag.set_incoming(preview) {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        return true;
    }
    false
}

fn locate(source: u64, drag: &Drag) -> Option<(u64, [f32; 2])> {
    let windows = WINDOW_REGISTRY
        .get()?
        .lock()
        .expect("window registry poisoned");
    let pointer = DesktopPointer::from_drag(drag)?;
    let mut hits = windows
        .iter()
        .filter_map(|(&id, window)| pointer.in_window(window).map(|point| (id, point)));
    let hit = hits.next()?;
    // X11 placements alone cannot establish stacking order for overlapping frames.
    (hit.0 != source && hits.next().is_none()).then_some(hit)
}

#[cfg(target_os = "macos")]
struct DesktopPointer {
    point: objc2_foundation_02::NSPoint,
    topmost: u32,
}

#[cfg(target_os = "macos")]
impl DesktopPointer {
    fn from_drag(_drag: &Drag) -> Option<Self> {
        use objc2_app_kit_02::{NSEvent, NSWindow};
        use objc2_foundation_02::MainThreadMarker;

        let main = MainThreadMarker::new()?;
        // SAFETY: drag handlers run on AppKit's main thread.
        let point = unsafe { NSEvent::mouseLocation() };
        let topmost =
            unsafe { NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, main) };
        Some(Self {
            point,
            topmost: u32::try_from(topmost).ok()?,
        })
    }

    fn in_window(&self, window: &WindowHandle) -> Option<[f32; 2]> {
        use objc2_app_kit_02::NSView;
        use rune::raw_window_handle::RawWindowHandle;

        if !window.is_ready() || window.window_number()? != self.topmost {
            return None;
        }
        window
            .with_raw_handle(|handle| {
                let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                    return None;
                };
                // SAFETY: Rune holds the live AppKit view throughout this main-thread callback.
                let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
                let native = view.window()?;
                let point =
                    view.convertPoint_fromView(native.convertPointFromScreen(self.point), None);
                let bounds = view.bounds();
                let x = point.x - bounds.origin.x;
                let y = point.y - bounds.origin.y;
                if x < 0.0 || y < 0.0 || x >= bounds.size.width || y >= bounds.size.height {
                    return None;
                }
                Some([
                    x as f32,
                    if view.isFlipped() {
                        y
                    } else {
                        bounds.size.height - y
                    } as f32,
                ])
            })
            .ok()
            .flatten()
    }
}

#[cfg(not(target_os = "macos"))]
struct DesktopPointer((i32, i32));

#[cfg(not(target_os = "macos"))]
impl DesktopPointer {
    fn from_drag(drag: &Drag) -> Option<Self> {
        drag.screen_point().map(Self)
    }

    fn in_window(&self, window: &WindowHandle) -> Option<[f32; 2]> {
        if !window.is_ready() {
            return None;
        }
        let placement = window.placement()?;
        if placement.minimized {
            return None;
        }
        content_point(
            self.0,
            placement.content_position,
            placement.content_size,
            placement.scale,
        )
    }
}

#[cfg(any(test, not(target_os = "macos")))]
fn content_point(
    point: (i32, i32),
    origin: (i32, i32),
    size: (u32, u32),
    scale: f64,
) -> Option<[f32; 2]> {
    let x = f64::from(point.0) - f64::from(origin.0);
    let y = f64::from(point.1) - f64::from(origin.1);
    (scale.is_finite()
        && scale > 0.0
        && x >= 0.0
        && y >= 0.0
        && x < f64::from(size.0)
        && y < f64::from(size.1))
    .then(|| [(x / scale) as f32, (y / scale) as f32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_negative_desktop_coordinates_using_destination_scale() {
        assert_eq!(
            content_point((-1000, 100), (-1200, 0), (800, 600), 2.0),
            Some([100.0, 50.0])
        );
        assert_eq!(
            content_point((-400, 100), (-1200, 0), (800, 600), 2.0),
            None
        );
    }
}
