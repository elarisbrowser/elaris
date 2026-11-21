// SPDX-License-Identifier: GPL-3.0-only

//! Development timing for the page pipeline, on only when `ELARIS_PERF` is set. Every few
//! seconds it prints, per stage, the sample count, mean, median, p95 and worst time to stderr.
//! Off, each call is a branch on a cached flag. Each thread keeps and prints its own timings:
//! the UI thread's polls, and the engine thread's work and frames.
// Most stages are timed on the engine thread, which a build without an engine lacks.
#![cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How often a summary prints.
const REPORT_EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Stage {
    /// Handling one batch of Ladybird work on the engine thread.
    Pump,
    /// Between two frames Rune drew: what reaches the screen.
    Drawn,
    /// Between two polls of the page: Rune's frame cadence while it keeps waking us.
    Poll,
    /// Work done inside Elaris' per-frame poll.
    PollWork,
    /// Building Elaris' view tree.
    BuildView,
    /// Between two frames Ladybird presented.
    FrameInterval,
    /// Copying and converting one frame out of Ladybird.
    Copy,
    /// One `Pixels::set`.
    Set,
    /// Restoring an evicted tab from Ladybird when it becomes visible.
    RestoreFrame,
    /// Replacing a hidden tab's full-size frame with its fallback.
    EvictFrame,
    /// From an input event to the first new frame after it.
    InputToFrame,
    /// From a new viewport size to the first frame at that size.
    ResizeToFrame,
    /// From a tab close request to the tab leaving the UI, including the page's close decision.
    TabClose,
}

/// Counters that are not durations.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Count {
    /// Frames shown.
    Shown,
    /// Ready-to-paint events with no new frame behind them.
    Duplicate,
    /// Frames Ladybird presented that were replaced before a poll could take them.
    Skipped,
    /// Frames dropped for a stale display scale.
    Stale,
}

#[derive(Default)]
struct Recorder {
    samples: Vec<(Stage, Duration)>,
    counts: [u64; 4],
    since: Option<Instant>,
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::default();
}

pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ELARIS_PERF").is_some_and(|v| v != "0"))
}

/// Times `work` as `stage`.
pub(crate) fn time<T>(stage: Stage, work: impl FnOnce() -> T) -> T {
    if !enabled() {
        return work();
    }
    let start = Instant::now();
    let result = work();
    record(stage, start.elapsed());
    result
}

pub(crate) fn record(stage: Stage, duration: Duration) {
    if enabled() {
        RECORDER.with_borrow_mut(|r| r.samples.push((stage, duration)));
        report_if_due();
    }
}

/// Rune drew a frame: records the time since the last one.
pub(crate) fn frame_drawn() {
    thread_local! {
        static LAST: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    }
    if enabled() {
        let now = Instant::now();
        if let Some(last) = LAST.replace(Some(now)) {
            record(Stage::Drawn, now - last);
        }
    }
}

pub(crate) fn count(counter: Count, by: u64) {
    if enabled() {
        RECORDER.with_borrow_mut(|r| r.counts[counter as usize] += by);
    }
}

fn report_if_due() {
    RECORDER.with_borrow_mut(|r| {
        let since = *r.since.get_or_insert_with(Instant::now);
        if since.elapsed() < REPORT_EVERY {
            return;
        }
        eprintln!("{}", summary(&mut r.samples, &r.counts, since.elapsed()));
        *r = Recorder {
            since: Some(Instant::now()),
            ..Recorder::default()
        };
    });
}

fn summary(samples: &mut [(Stage, Duration)], counts: &[u64; 4], over: Duration) -> String {
    samples.sort_unstable();
    let mut out = format!(
        "[perf {}] {:.1}s  shown {} ({:.1}/s)  duplicate {}  skipped {}  stale {}\n",
        std::thread::current().name().unwrap_or("?"),
        over.as_secs_f64(),
        counts[Count::Shown as usize],
        counts[Count::Shown as usize] as f64 / over.as_secs_f64(),
        counts[Count::Duplicate as usize],
        counts[Count::Skipped as usize],
        counts[Count::Stale as usize],
    );
    out += "[perf]   stage              n     mean      p50      p95      max  (ms)";
    for chunk in samples.chunk_by(|a, b| a.0 == b.0) {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let at = |q: f64| ms(chunk[((chunk.len() - 1) as f64 * q).round() as usize].1);
        let mean = chunk.iter().map(|s| ms(s.1)).sum::<f64>() / chunk.len() as f64;
        out += &format!(
            "\n[perf]   {:<14} {:>5} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            format!("{:?}", chunk[0].0),
            chunk.len(),
            mean,
            at(0.5),
            at(0.95),
            at(1.0),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_groups_stages_and_orders_percentiles() {
        let mut samples: Vec<_> = (1..=100)
            .map(|ms| (Stage::Pump, Duration::from_millis(ms)))
            .chain([(Stage::Copy, Duration::from_millis(7))])
            .collect();
        let text = summary(&mut samples, &[60, 1, 2, 0], Duration::from_secs(1));
        assert!(text.contains("shown 60 (60.0/s)"));
        assert!(text.contains("Pump             100    50.50    51.00    95.00   100.00"));
        assert!(text.contains("Copy               1     7.00"));
    }
}
