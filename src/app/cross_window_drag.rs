// SPDX-License-Identifier: GPL-3.0-only

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use rune::{Drag, Tx, WindowHandle};

use super::{AppState, WINDOW_REGISTRY};
use crate::tab::TabId;
use crate::ui::{DragVisual, TabDropTarget};

#[cfg(target_os = "macos")]
const RELEASE_EVENT_GRACE: Duration = Duration::from_millis(16);

thread_local! {
    static APPEARANCE: RefCell<Option<DragAppearance>> = const { RefCell::new(None) };
    static DRAG: Cell<Option<IncomingDrag>> = const { Cell::new(None) };
}

pub(crate) struct DragAppearance {
    pub(crate) label: String,
    pub(crate) favicon: Option<rune::Pixels>,
}

#[derive(Clone, Copy, PartialEq)]
struct IncomingDrag {
    source: u64,
    destination: u64,
    tab: TabId,
    point: [f32; 2],
    visual: DragVisual,
    phase: Phase,
    deadline: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Hovering,
    Released,
    Ready(TabDropTarget),
    Submitted(TabDropTarget),
}

/// Returns whether the pointer is over another browser window. The destination computes the
/// insertion target from its own layout, using the release point again before requesting a move.
pub(crate) fn track(state: &AppState, tab: TabId, drag: &Drag, release: bool) -> bool {
    let incoming = locate(state.window_id, drag).and_then(|(destination, point)| {
        let mut visual = state.tab_drag.visual()?;
        let local = drag.window_point();
        visual.to = [
            point[0] + visual.to[0] - local[0],
            point[1] + visual.to[1] - local[1],
        ];
        Some(IncomingDrag {
            source: state.window_id,
            destination,
            tab,
            point,
            visual,
            deadline: release.then(|| Instant::now() + Duration::from_secs(2)),
            phase: if release {
                Phase::Released
            } else {
                Phase::Hovering
            },
        })
    });
    if let Some(incoming) = incoming {
        if !DRAG.get().is_some_and(|active| active.tab == tab)
            && let Some(tab) = state.browser.tab(tab)
        {
            APPEARANCE.replace(Some(DragAppearance {
                label: tab.label().to_owned(),
                favicon: tab.favicon.clone(),
            }));
        }
        publish(Some(incoming));
        true
    } else {
        clear(state.window_id);
        false
    }
}

pub(crate) fn release_pending(window: u64) -> bool {
    DRAG.get()
        .is_some_and(|drag| drag.source == window && drag.phase != Phase::Hovering)
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
    if incoming.is_none() {
        APPEARANCE.replace(None);
    }
    if DRAG.replace(incoming) != incoming {
        crate::engine::wake_all_windows();
    }
}

pub(crate) fn incoming_visual<T>(
    window: u64,
    draw: impl FnOnce(&DragAppearance, DragVisual) -> T,
) -> Option<T> {
    let drag = DRAG.get().filter(|drag| drag.destination == window)?;
    APPEARANCE.with_borrow(|appearance| Some(draw(appearance.as_ref()?, drag.visual)))
}

pub(crate) fn shows_elsewhere(window: u64) -> bool {
    DRAG.get().is_some_and(|drag| drag.source == window)
}

pub(super) fn finish(state: &mut AppState, tab: TabId, success: bool, tx: &mut Tx) {
    let Some(drag) = DRAG
        .get()
        .filter(|drag| drag.tab == tab && drag.destination == state.window_id)
    else {
        return;
    };
    if let Phase::Submitted(target) = drag.phase {
        if success {
            state.tab_drag.settle_incoming(drag.visual, target);
        }
        state.tab_drag.set_incoming(None);
        state.tab_drag.set_incoming_visual(None);
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
        publish(None);
    }
}

pub(super) fn poll(state: &mut AppState, tx: &mut Tx) -> bool {
    let mut preview = None;
    if expire(state, tx) {
        return true;
    }
    if let Some(drag) = DRAG.get() {
        if drag.destination == state.window_id {
            if drag.phase == Phase::Hovering
                && state.horizontal_tabs()
                && let Some(x) = state.tab_drag.edge_scroll_target(tx.clock(), drag.point)
            {
                tx.scroll_to_instant(crate::ui::horizontal_tabs::SCROLL, x, 0.0);
                tx.wake_after(Duration::ZERO);
            }
            let target = match drag.phase {
                Phase::Ready(target) | Phase::Submitted(target) => Some(target),
                _ if state.horizontal_tabs() || !state.sidebar.collapsed => state
                    .tab_drag
                    .incoming_target(state.browser.tabs(), drag.point),
                _ => None,
            };
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
            if state.request_tab_transfer(drag.tab, drag.destination, target.section, target.index)
            {
                publish(Some(IncomingDrag {
                    phase: Phase::Submitted(target),
                    ..drag
                }));
            } else {
                publish(None);
            }
        }
    }

    let drag = DRAG.get();
    let visual = drag
        .filter(|drag| drag.destination == state.window_id)
        .map(|drag| drag.visual);
    let layout_changed = state.tab_drag.set_incoming(preview)
        | state.tab_drag.set_outside(shows_elsewhere(state.window_id));
    let visual_changed = state.tab_drag.set_incoming_visual(visual);
    if layout_changed {
        tx.animated();
        tx.motion(crate::ui::TAB_REORDER_MOTION);
    }
    layout_changed || visual_changed
}

fn expire(state: &mut AppState, tx: &mut Tx) -> bool {
    let Some(drag) = DRAG
        .get()
        .filter(|drag| drag.destination == state.window_id || drag.source == state.window_id)
    else {
        return false;
    };
    #[cfg(target_os = "macos")]
    let mut drag = drag;
    #[cfg(target_os = "macos")]
    if drag.phase == Phase::Hovering && drag.source == state.window_id {
        if let Some((destination, point)) =
            DesktopPointer::current().and_then(|pointer| locate_pointer(drag.source, &pointer))
        {
            drag.visual.to = [
                drag.visual.to[0] + point[0] - drag.point[0],
                drag.visual.to[1] + point[1] - drag.point[1],
            ];
            drag.destination = destination;
            drag.point = point;
            publish(Some(drag));
        }
        use objc2_app_kit_02::NSEvent;
        // SAFETY: Rune polls the source window on AppKit's main thread.
        if unsafe { NSEvent::pressedMouseButtons() } & 1 == 0 {
            if drag.deadline.is_none() {
                drag.deadline = Some(Instant::now() + RELEASE_EVENT_GRACE);
                DRAG.set(Some(drag));
            }
        } else {
            drag.deadline = None;
            DRAG.set(Some(drag));
            // Let Rune pace pointer sampling to the active display.
            tx.wake_after(Duration::ZERO);
        }
    }
    let Some(deadline) = drag.deadline else {
        return false;
    };
    if drag.phase == Phase::Hovering && drag.source != state.window_id {
        return false;
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        tx.wake_after(remaining);
        return false;
    }
    #[cfg(target_os = "macos")]
    if drag.phase == Phase::Hovering
        && state.tab_drag.is_dragging()
        && DesktopPointer::current()
            .and_then(|pointer| locate_pointer(drag.source, &pointer))
            .is_some_and(|(window, _)| window == drag.destination)
    {
        state.tab_drag.end(state.browser.tabs(), true);
        drag.phase = Phase::Released;
        drag.deadline = Some(Instant::now() + Duration::from_secs(2));
        publish(Some(drag));
        tx.wake_after(Duration::ZERO);
        return true;
    }
    if matches!(drag.phase, Phase::Submitted(_)) {
        state.cancel_tab_transfer();
    }
    state.cancel_tab_drag();
    state.tab_drag.set_outside(false);
    state.tab_drag.set_incoming(None);
    state.tab_drag.set_incoming_visual(None);
    tx.animated();
    tx.motion(crate::ui::TAB_REORDER_MOTION);
    true
}

fn locate(source: u64, drag: &Drag) -> Option<(u64, [f32; 2])> {
    locate_pointer(source, &DesktopPointer::from_drag(drag)?)
}

fn locate_pointer(source: u64, pointer: &DesktopPointer) -> Option<(u64, [f32; 2])> {
    let windows = WINDOW_REGISTRY
        .get()?
        .lock()
        .expect("window registry poisoned");
    let private = crate::app::is_private_window(source);
    let mut hits = windows
        .iter()
        .filter(|&(&id, _)| {
            crate::app::is_private_window(id) == private && !crate::app::is_mini_window(id)
        })
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
        Self::current()
    }

    fn current() -> Option<Self> {
        use objc2_app_kit_02::NSEvent;
        use objc2_foundation_02::MainThreadMarker;

        let main = MainThreadMarker::new()?;
        // SAFETY: drag handlers run on AppKit's main thread.
        let point = unsafe { NSEvent::mouseLocation() };
        let topmost = crate::drag_overlay_macos::window_below_preview(point, main);
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
    #[test]
    fn horizontal_destination_uses_its_own_drop_geometry() {
        use crate::settings::TabLayout;
        use crate::tab::{Tab, TabSection};

        let source = AppState::new(Some("source.example"));
        let mut destination = AppState::new(Some("destination.example"));
        destination.set_tab_layout(TabLayout::Horizontal);
        let existing = destination.browser.active_tab().map(Tab::id).unwrap();
        let incoming = source.browser.active_tab().map(Tab::id).unwrap();
        let mut runtime = rune::Runtime::new(
            destination,
            crate::ui::view,
            (900.0, 640.0),
            rune::Env::default(),
        );
        runtime.frame(0.0);
        runtime.frame(0.0);
        let [x, y, w, h] = runtime.state.tab_drag.tab_rect(existing).unwrap();
        let point = [x + 0.75 * w, y + 0.5 * h];
        let destination = &mut runtime.state;
        DRAG.set(Some(IncomingDrag {
            source: source.window_id,
            destination: destination.window_id,
            tab: incoming,
            point,
            visual: DragVisual {
                tab: incoming,
                section: TabSection::Regular,
                to: point,
                to_size: [w, h],
                settling: false,
            },
            phase: Phase::Released,
            deadline: Some(Instant::now() + Duration::from_secs(2)),
        }));
        assert!(poll(destination, &mut Tx::default()));
        let ready = DRAG.get().map(|drag| drag.phase);
        clear(source.window_id);
        assert!(matches!(
            ready,
            Some(Phase::Ready(TabDropTarget {
                section: TabSection::Regular,
                index: 1,
            }))
        ));
        assert!(source.browser.tab(incoming).is_some());
        assert!(destination.browser.tab(existing).is_some());
    }

    #[test]
    fn expired_release_clears_preview_without_removing_the_source_tab() {
        let source = AppState::new(Some("one.example"));
        let mut destination = AppState::new(None);
        let tab = source.browser.active_tab().unwrap().id();
        let target = TabDropTarget {
            section: crate::tab::TabSection::Regular,
            index: 0,
        };
        destination.tab_drag.set_incoming(Some((tab, target)));
        DRAG.set(Some(IncomingDrag {
            source: source.window_id,
            destination: destination.window_id,
            tab,
            point: [30.0, 130.0],
            visual: DragVisual {
                tab,
                section: target.section,
                to: [30.0, 130.0],
                to_size: [220.0, 34.0],
                settling: false,
            },
            phase: Phase::Ready(target),
            deadline: Some(Instant::now()),
        }));
        assert!(poll(&mut destination, &mut Tx::default()));
        assert!(DRAG.get().is_none());
        assert!(
            destination
                .tab_drag
                .preview_items(destination.browser.tabs(), target.section)
                .is_empty()
        );
        assert!(source.browser.tab(tab).is_some());
    }
}
