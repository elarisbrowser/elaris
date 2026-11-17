// SPDX-License-Identifier: GPL-3.0-only

//! Resizing the sidebar by dragging its edge. Past its limits the edge stretches with growing
//! resistance, as a scroll view does at its ends, and on release it springs back within them.

use rune::{Cursor, El, Tx, el};

use crate::app::AppState;
use crate::ui::theme;

/// The handle over the sidebar's trailing edge, `width` points from the window's leading one.
/// The window draws it last, so it sits over both the sidebar and the page.
pub(crate) fn handle(width: f32) -> El {
    el().key("sidebar-resize")
        .absolute(width - theme::RESIZE_HANDLE / 2.0, 0.0)
        .w(theme::RESIZE_HANDLE)
        .h_full()
        .cursor(Cursor::ResizeColumn)
        .label("Resize sidebar")
        .on_drag(|state: &mut AppState, drag, tx| match drag.phase {
            // The width at the press, kept for the gesture.
            rune::Phase::Down => drag.data = state.sidebar.width,
            rune::Phase::Move => state.sidebar.width = stretched(drag.data + drag.dx),
            // The spring back waits for the next frame: a drag's commits land at once, and a
            // move in the same frame as the release would land the settle too.
            rune::Phase::Up => {
                state.sidebar.width = stretched(drag.data + drag.dx);
                state.sidebar.settling = true;
                tx.wake_after(std::time::Duration::ZERO);
            }
        })
}

/// Springs a released sidebar back within its limits. The poll calls it every frame; true when
/// the view needs building for it.
pub(crate) fn settle(state: &mut AppState, tx: &mut Tx) -> bool {
    if !std::mem::take(&mut state.sidebar.settling) {
        return false;
    }
    state.sidebar.width = settled(state.sidebar.width);
    tx.animated();
    tx.motion(theme::SIDEBAR_SETTLE);
    true
}

/// The width the sidebar shows while dragged to `wanted`: exact within its limits, and beyond
/// them only part of the way, less the further it is pulled.
pub(crate) fn stretched(wanted: f32) -> f32 {
    let (min, max) = (theme::SIDEBAR_MIN_WIDTH, theme::SIDEBAR_MAX_WIDTH);
    if wanted < min {
        min - rubber_band(min - wanted)
    } else if wanted > max {
        max + rubber_band(wanted - max)
    } else {
        wanted
    }
}

/// The width the sidebar keeps once released at `wanted`.
pub(crate) fn settled(wanted: f32) -> f32 {
    wanted.clamp(theme::SIDEBAR_MIN_WIDTH, theme::SIDEBAR_MAX_WIDTH)
}

/// How far an edge pulled `excess` points past its limit moves: UIKit's scroll-edge curve,
/// which follows the pointer at first and never passes `RUBBER_BAND_REACH`.
fn rubber_band(excess: f32) -> f32 {
    const RESISTANCE: f32 = 0.55;
    let reach = theme::RUBBER_BAND_REACH;
    (1.0 - 1.0 / (excess * RESISTANCE / reach + 1.0)) * reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{RUBBER_BAND_REACH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};

    #[test]
    fn follows_the_pointer_within_its_limits() {
        let middle = (SIDEBAR_MIN_WIDTH + SIDEBAR_MAX_WIDTH) / 2.0;
        assert_eq!(stretched(middle), middle);
        assert_eq!(settled(middle), middle);
    }

    #[test]
    fn stretches_less_the_further_it_is_pulled_past_a_limit() {
        let near = stretched(SIDEBAR_MAX_WIDTH + 20.0) - SIDEBAR_MAX_WIDTH;
        let far = stretched(SIDEBAR_MAX_WIDTH + 2000.0) - SIDEBAR_MAX_WIDTH;
        assert!(0.0 < near && near < 20.0);
        assert!(near < far && far < RUBBER_BAND_REACH);
        assert!(stretched(SIDEBAR_MIN_WIDTH - 20.0) > SIDEBAR_MIN_WIDTH - 20.0);
    }

    #[test]
    fn settles_back_within_its_limits() {
        assert_eq!(settled(SIDEBAR_MAX_WIDTH + 50.0), SIDEBAR_MAX_WIDTH);
        assert_eq!(settled(SIDEBAR_MIN_WIDTH - 50.0), SIDEBAR_MIN_WIDTH);
    }
}
