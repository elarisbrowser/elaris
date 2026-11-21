// SPDX-License-Identifier: GPL-3.0-only

//! Page frames Rune's GPU samples in place (macOS): the bridge copies each frame on the GPU into an
//! IOSurface of its own, the surface is wrapped without a further copy, and Rune draws it as a
//! texture. Rune keeps every surface it has drawn pinned, so it never gets Ladybird's own backing
//! stores, which the Compositor does not repaint while another process holds them. The bridge
//! reuses a surface once Rune reports it has drawn a newer frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use elaris::ladybird::GpuFrame;
use rune::NativeSurface;
use rune::native_surface::macos::PixelBuffer;
use rune::native_surface::{ColorInfo, FrameTiming};

/// Set to anything but `0` to show frames through the GPU. Off until the path is verified.
const GPU_FRAMES_VARIABLE: &str = "ELARIS_GPU_FRAMES";

/// Whether new tabs show their frames through the GPU.
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(GPU_FRAMES_VARIABLE).is_some_and(|value| value != "0"))
}

/// Where the engine shows a tab's GPU frames, shared with the window that draws them.
#[derive(Clone)]
pub(crate) struct GpuSurface {
    native: NativeSurface,
    /// The newest frame serial Rune's GPU has finished drawing.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    completed: Arc<AtomicU64>,
}

impl GpuSurface {
    pub(super) fn new() -> Self {
        Self {
            native: NativeSurface::new(),
            completed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn native(&self) -> &NativeSurface {
        &self.native
    }

    /// The newest frame serial Rune's GPU has finished drawing.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn completed(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    /// Runs `wake` whenever the GPU finishes a frame that is newer than any it finished before.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn on_completed(&self, wake: impl Fn() + Send + Sync + 'static) {
        let completed = Arc::clone(&self.completed);
        self.native
            .set_observer(Arc::new(move |timing: FrameTiming| {
                if timing.completed_at.is_some()
                    && completed.fetch_max(timing.frame_id, Ordering::AcqRel) < timing.frame_id
                {
                    wake();
                }
            }));
    }

    /// Shows `frame` once Rune next draws, replacing any frame it has not drawn yet.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn show(&self, frame: &GpuFrame) -> Result<(), String> {
        // SAFETY: The bridge finished its copy before returning the surface, and does not write
        // it again until Rune reports a frame newer than this one drawn.
        let buffer =
            unsafe { PixelBuffer::from_iosurface(frame.io_surface, ColorInfo::default())? };
        self.native.push(buffer, frame.serial)?;
        // `Pixels::set` wakes the window for a new frame; pushing a surface frame does not, and
        // without it the page only redraws when something else, such as the pointer, does.
        super::wake_all_windows();
        Ok(())
    }
}
