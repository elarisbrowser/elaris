// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::time::Duration;

use objc2_app_kit_02::{NSEvent, NSScreen, NSWindow};
use objc2_foundation_02::{MainThreadMarker, NSPoint};
use rune::{
    App, Availability, Cx, El, OverlayHandle, OverlayInput, OverlayLevel, OverlayOptions,
    OverlayPlacement, OverlaySpec, Tx,
};

use crate::app::{AppState, cross_window_drag::DragAppearance};
use crate::ui::{DragVisual, theme};
use elaris_core::settings::Settings;

const PADDING: f32 = theme::TAB_DRAG_SHADOW_BLUR * 2.0;

struct Preview {
    appearance: DragAppearance,
    visual: DragVisual,
    settings: Settings,
}

struct Overlay {
    handle: OverlayHandle<Option<Preview>>,
    shown: Option<(u64, DragVisual)>,
    placement: Option<OverlayPlacement>,
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

pub(crate) fn install(app: &mut App<AppState>) {
    let spec = OverlaySpec {
        name: "Elaris tab drag preview",
        size: (theme::SIDEBAR_WIDTH, theme::ROW_HEIGHT + PADDING * 2.0),
        edge: rune::ScreenEdge::Top,
        margin: 0.0,
        align: rune::Align::Start,
        inset: 0.0,
        interactive: false,
        view,
    }
    .options(OverlayOptions {
        input: OverlayInput::None,
        level: OverlayLevel::AboveOverlays,
        prewarm: true,
        ..OverlayOptions::default()
    });
    let handle = app.overlay(spec, None);
    OVERLAY.replace(Some(Overlay {
        handle,
        shown: None,
        placement: None,
    }));
}

fn view(preview: &Option<Preview>, cx: &mut Cx<Option<Preview>>) -> El {
    let Some(preview) = preview else {
        return rune::el();
    };
    let settings = &preview.settings;
    let palette = theme::palette(
        settings.appearance.is_dark(cx.system_dark()),
        settings.glass_enabled,
        settings.tint_opacity,
        settings.accent_color,
    );
    crate::ui::floating_drag_preview(&preview.appearance, preview.visual, &palette, true)
}

pub(crate) fn shown() -> bool {
    OVERLAY.with_borrow(|overlay| {
        overlay.as_ref().is_some_and(|overlay| {
            overlay.shown.is_some() && overlay.handle.availability() != Availability::Unavailable
        })
    })
}

pub(crate) fn hide(source: u64) {
    OVERLAY.with_borrow_mut(|overlay| {
        if let Some(overlay) = overlay
            && overlay.shown.is_some_and(|(window, _)| window == source)
        {
            overlay.shown = None;
            overlay.placement = None;
            overlay.handle.hide();
            overlay.handle.update(|preview, _| *preview = None);
        }
    });
}

/// AppKit may report a click-through panel in the window stack. Skip our panel for drop targeting.
pub(crate) fn window_below_preview(point: NSPoint, main: MainThreadMarker) -> isize {
    let panel = OVERLAY.with_borrow(|overlay| overlay.as_ref()?.handle.window_number());
    // SAFETY: the supplied marker proves this is AppKit's main thread.
    let hit = unsafe { NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, main) };
    if u32::try_from(hit)
        .ok()
        .is_some_and(|hit| Some(hit) == panel)
    {
        // SAFETY: same main-thread query, starting immediately below the preview panel.
        unsafe { NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, hit, main) }
    } else {
        hit
    }
}

pub(crate) fn poll(state: &AppState, tx: &mut Tx) -> bool {
    let was_shown = shown();
    update(state, tx);
    let changed = was_shown != shown();
    if changed {
        crate::engine::wake_all_windows();
    }
    changed
}

fn update(state: &AppState, tx: &mut Tx) {
    let id = state.window_id;
    let Some((mut visual, grab)) = state.tab_drag.single_drag() else {
        hide(id);
        return;
    };
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: this poll runs on AppKit's main thread.
    if unsafe { NSEvent::pressedMouseButtons() } & 1 == 0 {
        hide(id);
        return;
    }
    tx.wake_after(Duration::ZERO);
    if let Some((pointer, primary_top_left)) = sample_outside_source(id, main) {
        let Some(tab) = state.browser.tab(visual.tab) else {
            hide(id);
            return;
        };
        let placement = preview_placement(pointer, primary_top_left, grab, visual.to_size);
        visual.to = [PADDING; 2];
        OVERLAY.with_borrow_mut(|overlay| {
            let Some(overlay) = overlay else { return };
            if overlay.placement != Some(placement) {
                overlay.placement = Some(placement);
                overlay.handle.set_placement(Some(placement));
            }
            if overlay.shown != Some((id, visual)) {
                overlay.shown = Some((id, visual));
                let preview = Preview {
                    appearance: DragAppearance {
                        label: tab.label().to_owned(),
                        favicon: tab.favicon.clone(),
                    },
                    visual,
                    settings: state.settings.clone(),
                };
                overlay
                    .handle
                    .update(move |state, _| *state = Some(preview));
                overlay.handle.show();
            }
        });
    } else {
        hide(id);
    }
}

fn sample_outside_source(id: u64, main: MainThreadMarker) -> Option<(NSPoint, NSPoint)> {
    // SAFETY: this poll runs on AppKit's main thread.
    let pointer = unsafe { NSEvent::mouseLocation() };
    let window = crate::app::native_window(id)?;
    let frame = crate::platform::macos::window::with_window(&window, |native| native.frame())?;
    if pointer.x >= frame.origin.x
        && pointer.x < frame.origin.x + frame.size.width
        && pointer.y >= frame.origin.y
        && pointer.y < frame.origin.y + frame.size.height
        && u32::try_from(window_below_preview(pointer, main)).ok() == window.window_number()
    {
        return None;
    }
    let primary = NSScreen::screens(main).iter().next()?.frame();
    Some((
        pointer,
        NSPoint::new(primary.origin.x, primary.origin.y + primary.size.height),
    ))
}

fn preview_placement(
    pointer: NSPoint,
    primary_top_left: NSPoint,
    grab: [f32; 2],
    [width, height]: [f32; 2],
) -> OverlayPlacement {
    OverlayPlacement::Rect {
        x: (pointer.x - primary_top_left.x) as f32 - grab[0] - PADDING,
        y: (primary_top_left.y - pointer.y) as f32 - grab[1] - PADDING,
        width: width + PADDING * 2.0,
        height: height + PADDING * 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_preserves_grab_offset_on_a_display_above_and_left() {
        assert_eq!(
            preview_placement(
                NSPoint::new(-120.0, 950.0),
                NSPoint::new(0.0, 900.0),
                [14.0, 9.0],
                [230.0, 34.0]
            ),
            OverlayPlacement::Rect {
                x: -134.0 - PADDING,
                y: -59.0 - PADDING,
                width: 230.0 + 2.0 * PADDING,
                height: 34.0 + 2.0 * PADDING,
            }
        );
    }
}
