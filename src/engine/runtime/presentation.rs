// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsStr;
use std::time::{Duration, Instant};

use elaris::ladybird::Frame;

use super::Verdict;
use super::View;
use super::thumbnails::{self, Sample};
use crate::perf::{self, Count, Stage};

pub(super) const RETAINED_HIDDEN_FRAMES: usize = 2;
const FALLBACK_MAX_SIDE: u32 = 160;
const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(1);

pub(super) fn evict_frame(view: &mut View) -> bool {
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
    let thumbnail = if sample
        && view.visible
        && view.surface.preview_source.revision() == view.preview_revision
        && view
            .last_thumbnail
            .is_none_or(|last| last.elapsed() >= THUMBNAIL_INTERVAL)
    {
        view.last_thumbnail = Some(Instant::now());
        thumbnails::sample(frame.width, frame.height, &rgba)
    } else {
        None
    };
    view.surface.count_frame();
    timing.shown(&frame, skipped);
    view.gate.shown(frame.serial);
    view.showing_fallback = false;
    perf::time(Stage::Set, || {
        view.surface.pixels.set(frame.width, frame.height, rgba)
    });
    Ok(thumbnail)
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

    fn shown(&mut self, frame: &Frame, skipped: u64) {
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
            && size == (frame.width, frame.height)
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
