# Webview performance

How a Ladybird frame reaches the screen in Elaris, where the time goes, and what is left to do.
Measured on macOS (Apple Silicon, 60 Hz display), default 960×640 pt window at 2× (1920×1280
device pixels).

## Pipeline

1. WebContent renders; Ladybird's Compositor presents into a shared CPU backing store, paced by
   its own display link (`Services/Compositor/VSyncScheduler.cpp`), and tells the UI process over IPC.
2. Rune calls `app::poll` once per display frame. `Page::poll` pumps Ladybird's event loop once
   without waiting (`elaris_ladybird_pump`), which handles that IPC and raises `ReadyToPaint`.
3. `Page::take_frame` skips frames already shown (by serial) or laid out at a stale scale, then
   `elaris_ladybird_copy_frame` converts the displayed BGRx bitmap into a new RGBA `Vec`.
4. `Pixels::set` swaps the `Vec` in and wakes the window; Rune copies the whole image into its
   atlas page and uploads the dirty rows (the whole frame) on its next frame.

The view is not rebuilt for a new frame unless its size changed, so animation costs no layout.

## Measuring

`ELARIS_PERF=1 ./elaris run <url>` prints, every 5 s, frames shown per second, duplicate, skipped
(presented but replaced before a poll took them) and stale frames, and n/mean/p50/p95/max for the
pump, poll cadence, frame interval, copy, `Pixels::set` and input-to-frame latency. Off, it costs a
cached branch. Workloads: a CSS `left` animation, a `requestAnimationFrame` animation, a 3000-row
page scrolled by `scrollBy` each animation frame, and an idle page.

## Findings

**The Ladybird build was unoptimised.** `./elaris cpp-configure` set no `CMAKE_BUILD_TYPE`, so the
bridge and every Ladybird library and helper compiled without `-O`. It now passes
`RelWithDebInfo`, as Ladybird's own Release preset does. This is the largest single change: every
page's layout, paint and script ran at `-O0`.

**Polling is not the bottleneck.** Rune collapses every wake into its next display frame, so the
4 ms active interval already means one poll per vsync. Pumps take < 0.1 ms. Input reaches a new
frame in one display frame (p50 16.6 ms). The pump interval was left as is.

**Frame copy was the main UI-thread cost:** 4–6 ms per frame before, about a third of a 60 Hz
frame, growing with window size. The bridge now converts a pixel per word store instead of a byte
at a time, and Rust no longer zero-fills a frame-sized buffer before the copy overwrites it.

**Steady-state results** (Elaris release build, 5 s windows after startup; "before" is the
unoptimised Ladybird build with the old copy, "optimised" adds the `RelWithDebInfo` build and the
faster copy, "final" adds the frame wait below):

| Workload | Frames/s before → optimised → final | Interval p95 (ms) | Copy p50 (ms) | Skipped / 5 s |
|---|---|---|---|---|
| CSS animation | 59.9 → 60.0 → 60.0 | 16.9 → 17.5 → 17.7 | 4.27 → 1.54 → 0.95 | 0 → 0 → 0 |
| JS animation | ~60 → 57.0 → 58.4–59.2 | 17.4 → 32.4 → 17.8 | 6.06 → 1.42 → 0.81 | — → 11 → 0–4 |
| Scrolling | ~60 → 52.6 → 60.0 | 17.0 → 33.4 → 17.7 | 6.05 → 0.95 → 1.54 | — → 37 → 0 |
| Idle | 0 frames, 50 ms polls throughout | — | — | 0 |

`Pixels::set` is free (< 0.02 ms): it only swaps the buffer. Debug and release Elaris builds
measured the same; the cost was on the C++ side. Copy times vary with how much of the page the
bitmap's memory has warm in cache, not with the workload's damage.

## Present/poll phase

With Ladybird optimised, scrolling and JS animation skipped frames, showing as 33 ms gaps. Ladybird's
Compositor presents on a display link (`Services/Compositor/VSyncScheduler.cpp`) tied to the same
display Rune polls on, so its "presented" IPC message arrives at about the moment Elaris pumps. A
frame that lands just after the pump waits a whole refresh and arrives with the next one, which
replaces it.

**Fix:** while frames are flowing (the previous poll showed a new one) and the pump found none,
`Page::poll` calls `elaris_ladybird_wait_for_frame`, which pumps Ladybird, blocking, until the view
presents or 2 ms pass. That moves the boundary away from vsync. Measured cost: mean 0.5–0.7 ms per
poll that waits, p95 1.2–2.1 ms, worst about 3 ms; it never runs on still or idle pages.

**Rejected:**

- *Pumping between vsyncs from a timer thread* via Rune's `AppHandle::wake`. Rune also paces
  those wakes to the display, so polls stayed on vsync and the extra wakes made it worse
  (35 frames/s, 62 skipped per 5 s).
- *Waking on Ladybird's IPC file descriptors.* `EventLoopImplementationUnix::pump` downcasts the
  installed manager to the `final` `EventLoopManagerUnix`, so a host cannot wrap it to learn the
  notifier descriptors, and the Mach port transport keeps its notify pipe private.

**Remaining:** the JS animation still skips 0–4 frames per 5 s, where Ladybird itself presents
late. The clean fix is a host-driven tick (Ladybird rendering on Elaris's frame clock), which has no
embedder API; it would need upstream discussion.

## Other limits

- **No partial uploads.** `Pixels::set` replaces the whole image, and Rune's atlas copies and
  uploads it whole. Converting only Ladybird's damage rect would still need a full memory copy to
  hand Rune an owned buffer, so damage is not used. GPU texture sharing would remove the copies but
  is deliberately out of scope.
- **Waiting assumes 60 Hz-ish refresh.** The 2 ms bound is a fraction of any common refresh
  interval, but on a 120 Hz display it is a quarter of the frame; it was only measured at 60 Hz.
- **Copy scales with pixels.** At 1 ms for 2.5 MP, a full-screen 5K-class window is around 4–6 ms
  on the UI thread.

## Not verified

- Real trackpad scrolling and visual smoothness: the scroll workload is driven by script, and the
  runs were measured, not watched.
- Comparison with Ladybird's native AppKit frontend.
- Linux, resizing and display-scale changes under load.
