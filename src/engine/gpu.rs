// SPDX-License-Identifier: GPL-3.0-only

//! Page frames Rune's GPU samples in place (macOS): Ladybird presents each frame in an IOSurface,
//! the surface is wrapped without a copy, and Rune draws it as a texture. Ladybird may paint into a
//! frame's backing store again only once Rune's GPU has finished drawing the frame after it, which
//! Rune reports for each frame it draws.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use elaris::ladybird::GpuFrame;
use rune::NativeSurface;
use rune::native_surface::macos::PixelBuffer;
use rune::native_surface::{ColorInfo, FrameTiming};

/// Set to anything but `0` to copy frames through the CPU instead, as other platforms do.
const CPU_FRAMES_VARIABLE: &str = "ELARIS_CPU_FRAMES";

/// Whether new tabs show their frames through the GPU.
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(CPU_FRAMES_VARIABLE).is_none_or(|value| value == "0"))
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
        // SAFETY: Ladybird presented the surface complete, and does not paint into its backing
        // store again until `completed` reaches a frame newer than this one.
        let buffer =
            unsafe { PixelBuffer::from_iosurface(frame.io_surface, ColorInfo::default())? };
        self.native.push(buffer, frame.serial)
    }
}
