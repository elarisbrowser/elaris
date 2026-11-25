// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsStr;
use std::time::{Duration, Instant};

use elaris::ladybird::GpuPoll;

use super::Verdict;
use super::View;
use super::thumbnails::{self, Sample};
use elaris_core::perf::{self, Count, Stage};

pub(super) const RETAINED_HIDDEN_FRAMES: usize = 2;
const FALLBACK_MAX_SIDE: u32 = 160;
const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(1);

pub(super) fn evict_frame(view: &mut View) -> bool {
    // A GPU frame costs no CPU memory to keep, and has no pixels to shrink.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if view.surface.gpu.is_some() {
        return false;
    }
    let Ok(Some(frame)) = view.view.frame() else {
        return false;
    };
    let Ok(rgba) = view.view.copy_frame(&frame) else {
        return false;
    };
    let (width, height, fallback) = frame_fallback(frame.width, frame.height, &rgba);
    view.surface.pixels.set(width, height, fallback);
    view.showing_fallback = true;
    view.gate.forget_frame();
    true
}

fn frame_fallback(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let longest = u64::from(width.max(height));
    let side = u64::from(FALLBACK_MAX_SIDE).min(longest);
    let scaled = |size| (u64::from(size) * side / longest).max(1) as usize;
    let (w, h) = (scaled(width), scaled(height));
    let (width, height) = (width as usize, height as usize);
    let mut fallback = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = ((2 * y + 1) * height / (2 * h)) * width;
        for x in 0..w {
            let offset = (row + (2 * x + 1) * width / (2 * w)) * 4;
            fallback.extend_from_slice(&rgba[offset..offset + 4]);
        }
    }
    (w as u32, h as u32, fallback)
}

/// Shows the view's newest frame in its surface if the gate lets it through.
pub(super) fn show_frame(
    view: &mut View,
    timing: &mut Timing,
    sample: bool,
) -> Result<Option<Sample>, String> {
    if view.showing_fallback && !view.visible {
        return Ok(None);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if view.surface.gpu.is_some() {
        return show_gpu_frame(view, timing, sample);
    }
    let Some(frame) = view.view.frame()? else {
        return Ok(None);
    };
    let skipped = match view.gate.check(frame.serial, frame.scale) {
        Verdict::Show { skipped } => skipped,
        Verdict::Duplicate => {
            perf::count(Count::Duplicate, 1);
            return Ok(None);
        }
        Verdict::Stale => {
            perf::count(Count::Stale, 1);
            return Ok(None);
        }
    };
    let rgba = perf::time(Stage::Copy, || view.view.copy_frame(&frame))?;
    let thumbnail = if sample && wants_thumbnail(view) {
        view.last_thumbnail = Some(Instant::now());
        thumbnails::sample(frame.width, frame.height, &rgba)
    } else {
        None
    };
    view.surface.count_frame();
    timing.shown(frame.width, frame.height, skipped);
    view.gate.shown(frame.serial);
    view.showing_fallback = false;
    perf::time(Stage::Set, || {
        view.surface.pixels.set(frame.width, frame.height, rgba)
    });
    Ok(thumbnail)
}

/// Shows the view's newest frame by handing Rune a GPU copy of it; no pixel reaches the CPU. Only
/// the thumbnail sample, at most once a second, reads them back.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn show_gpu_frame(
    view: &mut View,
    timing: &mut Timing,
    sample: bool,
) -> Result<Option<Sample>, String> {
    if view.hosted {
        show_hosted_frame(view, timing, sample)?;
        return Ok(None);
    }
    if view.gpu_waiting {
        return Ok(None);
    }
    // Whether the frame is new comes before the copy, which takes one of few surfaces.
    let Some(cpu) = view.view.frame()? else {
        return Ok(None);
    };
    let skipped = match view.gate.check(cpu.serial, cpu.scale) {
        Verdict::Show { skipped } => skipped,
        Verdict::Duplicate => {
            perf::count(Count::Duplicate, 1);
            return Ok(None);
        }
        Verdict::Stale => {
            perf::count(Count::Stale, 1);
            return Ok(None);
        }
    };
    let frame = match perf::time(Stage::Copy, || view.view.gpu_frame())? {
        GpuPoll::Frame(frame) => frame,
        GpuPoll::NoFrame => return Ok(None),
        GpuPoll::Busy => {
            // Rune's GPU still holds every surface; the engine tries again as it finishes one.
            view.gpu_waiting = true;
            return Ok(None);
        }
    };
    let thumbnail = if sample && wants_thumbnail(view) {
        view.last_thumbnail = Some(Instant::now());
        // A frame in GPU memory may have no pixels the CPU can read; then it has no thumbnail.
        view.view
            .copy_frame(&cpu)
            .ok()
            .and_then(|rgba| thumbnails::sample(cpu.width, cpu.height, &rgba))
    } else {
        None
    };
    let Some(gpu) = view.surface.gpu.as_ref() else {
        return Err("a GPU frame arrived for a surface without a GPU".to_owned());
    };
    let (width, height, serial) = (frame.width, frame.height, frame.serial);
    perf::time(Stage::Set, || gpu.show(frame))?;
    view.surface.count_frame();
    timing.shown(width, height, skipped);
    view.gate.shown(serial);
    view.showing_fallback = false;
    Ok(thumbnail)
}

/// The Compositor showed a frame in the view's own layer, which the window server shows: the
/// window has nothing to draw. A thumbnail, at most once a second, comes as a snapshot
/// (`Event::Snapshot`), since the frame is in no memory the engine can read.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn show_hosted_frame(view: &mut View, timing: &mut Timing, sample: bool) -> Result<(), String> {
    if let Some(frame) = view.view.frame()? {
        timing.shown(frame.width, frame.height, 0);
    }
    if sample && wants_thumbnail(view) {
        view.last_thumbnail = Some(Instant::now());
        view.view.request_snapshot()?;
    }
    Ok(())
}

/// Whether the view's newest frame should refresh its thumbnail.
fn wants_thumbnail(view: &View) -> bool {
    view.visible
        && view.surface.preview_source.revision() == view.preview_revision
        && view
            .last_thumbnail
            .is_none_or(|last| last.elapsed() >= THUMBNAIL_INTERVAL)
}

/// Development timing (`ELARIS_PERF`): the last frame shown, the oldest input not yet on screen,
/// and the newest size not yet on screen.
#[derive(Default)]
pub(super) struct Timing {
    last_frame: Option<Instant>,
    pending_input: Option<Instant>,
    pending_resize: Option<((u32, u32), Instant)>,
}

impl Timing {
    pub(super) fn input(&mut self) {
        if perf::enabled() {
            self.pending_input.get_or_insert_with(Instant::now);
        }
    }

    pub(super) fn resized(&mut self, size: (u32, u32)) {
        if perf::enabled() {
            self.pending_resize = Some((size, Instant::now()));
        }
    }

    fn shown(&mut self, width: u32, height: u32, skipped: u64) {
        if !perf::enabled() {
            return;
        }
        let now = Instant::now();
        perf::count(Count::Shown, 1);
        perf::count(Count::Skipped, skipped);
        if let Some(last) = self.last_frame.replace(now) {
            perf::record(Stage::FrameInterval, now - last);
        }
        if let Some(input) = self.pending_input.take() {
            perf::record(Stage::InputToFrame, now - input);
        }
        if let Some((size, since)) = self.pending_resize
            && size == (width, height)
        {
            perf::record(Stage::ResizeToFrame, now - since);
            self.pending_resize = None;
        }
    }
}

pub(super) fn eviction_enabled(setting: Option<&OsStr>) -> bool {
    setting.is_some_and(|value| value != "0")
}

#[cfg(test)]
mod tests {
    use super::eviction_enabled;
    use std::ffi::OsStr;

    #[test]
    fn hidden_frame_eviction_requires_a_nonzero_opt_in() {
        assert!(!eviction_enabled(None));
        assert!(!eviction_enabled(Some(OsStr::new("0"))));
        assert!(eviction_enabled(Some(OsStr::new(""))));
        assert!(eviction_enabled(Some(OsStr::new("1"))));
        assert!(eviction_enabled(Some(OsStr::new("true"))));
    }
}
