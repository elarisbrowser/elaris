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

**Steady-state results** (Elaris release build; "before" is the unoptimised Ladybird build with the
old copy):

| Workload | Frames/s before → after | Interval p95 before → after | Copy p50 before → after | Skipped / 5 s after |
|---|---|---|---|---|
| CSS animation | 59.9 → 60.0 | 16.9 → 17.5 ms | 4.27 → 1.54 ms | 0 |
| JS animation | ~60 → 57.0 | 17.4 → 32.4 ms | 6.06 → 1.42 ms | 11 |
| Scrolling | ~60 → 52.6 | 17.0 → 33.4 ms | 6.05 → 0.95 ms | 37 |
| Idle | ~0 frames, 50 ms polls | unchanged | — | 0 |

`Pixels::set` is free (< 0.02 ms): it only swaps the buffer. Debug and release Elaris builds
measured the same; the cost was on the C++ side.

## Remaining bottleneck: present/poll phase

With Ladybird fast, scrolling and JS animation skip frames, which shows as 33 ms gaps. Both
Ladybird's Compositor and Rune tick from the same display, so Ladybird's "presented" IPC message
arrives at about the moment Rune polls. When it lands just after the pump, that refresh shows
nothing new and the next pump finds two frames, so one is dropped. The skipped counter and
interval p95 measure this directly. The simple pages above never miss Ladybird's own deadline,
so this is about timing, not render cost.

Candidate fixes, none done yet:

- **Wake on IPC.** A small thread waits on Ladybird's event-loop file descriptors and calls Rune's
  `AppHandle::wake`, so the UI thread pumps as soon as a frame is presented instead of at the next
  vsync. Lowest latency; needs the descriptors exposed through the bridge and one thread.
- **One-frame queue.** Copy every presented frame (in the bridge's ready-to-paint callback, before
  its backing store is reused) and show one per poll. Even pacing at the cost of one frame of
  latency, which matters for scrolling.
- **Host-driven ticks.** Have Ladybird render on Elaris's vsync rather than its own display link.
  Ladybird has no embedder API for this; it would need upstream discussion.

## Other limits

- **No partial uploads.** `Pixels::set` replaces the whole image, and Rune's atlas copies and
  uploads it whole. Converting only Ladybird's damage rect would still need a full memory copy to
  hand Rune an owned buffer, so damage is not used. GPU texture sharing would remove the copies but
  is deliberately out of scope.
- **Copy scales with pixels.** At 1 ms for 2.5 MP, a full-screen 5K-class window is around 4–6 ms
  on the UI thread.

## Not verified

- Real trackpad scrolling and visual smoothness: the scroll workload is driven by script, and the
  runs were measured, not watched.
- Comparison with Ladybird's native AppKit frontend.
- Linux, resizing and display-scale changes under load.
