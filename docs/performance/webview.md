# Webview performance

How a Ladybird frame reaches the screen in Elaris, where the time goes, and what is left to do.
Measured on macOS (Apple Silicon, 60 Hz display), default 960×640 pt window at 2× (1920×1280
device pixels).

## Pipeline

Ladybird runs on an engine thread of its own (`src/engine/`); the UI thread never calls it and
never waits on it.

1. WebContent renders; Ladybird's Compositor presents into a shared CPU backing store, paced by
   its own display link (`Services/Compositor/VSyncScheduler.cpp`), and tells the UI process over IPC.
2. The engine thread sleeps in Ladybird's event loop (`elaris_ladybird_wait`) until that IPC, a
   Ladybird timer, or a command from the page wakes it (`elaris_ladybird_wake`, which any thread
   may call). Nothing polls.
3. On `ReadyToPaint` it skips frames already shown (by serial) or laid out at a stale scale, then
   `elaris_ladybird_copy_frame` converts the displayed BGRx bitmap into a new RGBA `Vec`.
4. `Pixels::set`, which Rune allows from any thread, swaps the `Vec` in and wakes the window. Rune
   copies the whole image into its atlas page and uploads it on its next frame. A frame of a new
   size goes to the UI thread instead, which sets it together with its layout size.
5. The UI thread's `Page::poll`, at the start of each Rune frame, forwards viewport, focus and
   navigation as commands, and reads titles, URLs, loading state and errors back from a channel.
   While frames keep coming it asks Rune for the next display frame, so Rune draws at the
   display's rate rather than only when pixels wake it.

The view is not rebuilt for a new frame unless its size changed, so animation costs no layout.

## GPU frames (macOS)

On macOS a frame is never copied. The bridge turns off `presents_from_cpu_memory`, so Ladybird's
Compositor renders into IOSurface-backed backing stores without a CPU readback, and
`elaris_ladybird_gpu_frame` hands out the displayed frame's `IOSurfaceRef`. The engine thread
wraps it in a `PixelBuffer` and pushes it into the tab's Rune `NativeSurface` (`src/engine/gpu.rs`);
the viewport draws that at one texel per device pixel. No RGBA conversion, `Pixels::set` or atlas
upload remains, and the CPU reads pixels back only for the once-a-second tab thumbnail.

Ladybird recycles a backing store as soon as the next frame replaces it, which would let it paint
into a texture Rune is still sampling. The bridge therefore holds each replaced store
(`ElarisWebView::release_backing_store`) until the host reports, through
`elaris_ladybird_release_frames`, that its GPU finished a frame at least as new as the one that
replaced it. Rune reports that per drawn frame through the `NativeSurface` observer
(`FrameTiming::completed_at`, keyed by the frame serial), which also wakes the engine thread. A
frame Rune never draws is covered by any later one, and a hidden tab paints nothing, so nothing
waits on it.

`ELARIS_CPU_FRAMES=1` selects the copying path described below, which Linux always uses.

## Measuring

`ELARIS_PERF=1 ./elaris run <url>` prints, every 5 s and per thread:

- **main** (UI): `Drawn`, the interval between frames Rune drew, and `Poll`, between polls.
- **elaris-engine**: frames shown per second; duplicate, skipped (presented but replaced before
  the engine took them) and stale frames; and n/mean/p50/p95/max for handling one batch of
  Ladybird work (`Pump`), frame interval, copy, `Pixels::set`, input to the next new frame, and
  a new viewport size to the first frame at it (`ResizeToFrame`).

Off, it costs a cached branch. The workload pages are in [`benchmarks/`](../../benchmarks): a CSS
animation, a `requestAnimationFrame` animation, a 3000-row page that scrolls itself every frame,
and an idle page. slaterlabs.dev (a Next.js site) was used with trackpad scrolling.

## Findings, in order

**1. The Ladybird build was unoptimised.** `./elaris cpp-configure` set no `CMAKE_BUILD_TYPE`, so
the bridge and every Ladybird library and helper compiled without `-O`. It now passes
`RelWithDebInfo`, as Ladybird's own Release preset does.

**2. Frame copy cost 4–6 ms per frame** on the UI thread, about a third of a 60 Hz frame. The
bridge now converts a pixel per word store instead of a byte at a time, and Rust no longer
zero-fills a frame-sized buffer before the copy overwrites it: about 1 ms now.

**3. Pumping on vsync dropped frames.** With Ladybird optimised, scrolling and JS animation
skipped frames, showing as 33 ms gaps. Ladybird's Compositor presents on the same display's vsync
that Rune polls on, so its IPC arrived at about the moment the UI thread pumped; a frame landing
just after the pump arrived a refresh later with the next one, which replaced it. A first fix
waited up to 2 ms on the UI thread for the late frame. The engine thread replaces it: it handles
each present as it arrives, so phase no longer matters, and the UI thread does not wait.

**4. Ladybird blocked the UI thread.** Startup (~130 ms, mostly waiting for helper processes), the
first pump of a load (~30 ms), the frame wait above (about 1 ms on nearly every animated frame),
and the 100 ms close grace all ran on the UI thread. All now run on the engine thread. Rune
reported these blocks in debug builds; with the engine thread it reports none.

**5. Touchpad scrolls had no gesture phase.** Elaris now passes Rune's phase and momentum to
Ladybird as ongoing, momentum or ended, as Ladybird's Qt frontend does, so a gesture stays on
one scroller and momentum is told apart from finger scrolling. With phases reported, the bridge
also turns on Ladybird's elastic overscroll on macOS (`platform_scrolls_elastically`), which needs
to know when the fingers lift.

**6. Live resizing.** The UI sends the viewport size every frame of a live resize. The engine
thread now applies only the newest size from each batch of commands, so Ladybird lays out the
size the window has now instead of working through stale ones.

## Results

Steady-state 5 s windows after startup, release Elaris build unless noted; "first" is the
starting point (unoptimised Ladybird, old copy), "UI-thread" is optimised Ladybird with the 2 ms
wait, "engine thread" is the current design.

| Workload | Frames/s first → UI-thread → engine thread | Skipped / 5 s | Frame interval p95 (ms) | Rune drawn / 5 s |
|---|---|---|---|---|
| CSS animation | 59.9 → 60.0 → 60.0 | 0 → 0 → 0 | 16.9 → 17.7 → 17.8 | 300 |
| JS animation | ~60 → 58.4–59.2 → 59.0 | — → 0–4 → 0 | 17.4 → 17.8 → 17.8 | 297 |
| Scrolling | ~60 → 60.0 → 60.0 | — → 0 → 0 | 17.0 → 17.7 → 21.0 | 300 |
| Scrolling, debug Elaris | ~60 → — → 60.0 | — → — → 0 | 17.0 → — → 19.0 | 300 |
| slaterlabs.dev | — → 53.6–60.0 → 56.0–60.2 | — → 0–12 → 0 | — → 17.7–32.5 → 18.8 | 300 |
| Idle | no frames; UI polled every 50 ms → UI sleeps until something changes | 0 | — | — |

Continuous resizing (the window grown and shrunk through 82 sizes, four times, by script):

| Build, page | Rune drawn / 5 s | Drawn p95 / max (ms) | New size on screen p50 / p95 / max (ms) |
|---|---|---|---|
| Release, `scroll.html` | 299 | 17.5 / 36.5 | 12.3 / 15.7 / 17.7 |
| Debug, slaterlabs.dev | 300 | 18.8 / 19.9 | 10.7 / 16.2 / 21.2 |

On the UI thread a poll now does no engine work, and copy (0.5–1.3 ms) and `Pixels::set`
(< 0.1 ms) run on the engine thread. Input reaches a new frame in about 4–13 ms, measured from
when the engine thread receives it. Quitting with ⌘Q exits cleanly in about 50 ms with no helper
processes left, five runs out of five.

## Design notes

- **Thread affinity.** Ladybird requires every call on the thread that initialised it, not the
  main thread; the engine thread is that thread. The bridge checks it on every call.
- **Shutdown.** Rune ends the process from inside its event loop, so no Elaris code runs after
  the window goes except exit handlers. Closing the window only asks the engine to close. An
  `atexit` handler then sends the close if it was not sent, and waits for the engine thread, so no
  Ladybird code runs while the process tears down Ladybird's statics; it runs before them because
  they were registered first. The wait is after the window has gone, so nothing is seen to block.
- **Rune pacing.** A `Pixels::set` alone wakes Rune, but in debug builds that drew only every
  other refresh (30 frames/s). Asking for the next frame (`Tx::wake_after(ZERO)`) while the
  engine's frame count rises keeps Rune on the display link, as the old per-frame pump did.

**Rejected:**

- *Pumping between vsyncs from a timer thread* via Rune's `AppHandle::wake`. Rune paces those
  wakes to the display too, so polls stayed on vsync and it made things worse (35 frames/s).
- *Waking on Ladybird's IPC file descriptors.* `EventLoopImplementationUnix::pump` downcasts the
  installed manager to the `final` `EventLoopManagerUnix`, so a host cannot wrap it to learn the
  notifier descriptors, and the Mach port transport keeps its notify pipe private.

## Remaining limits

- **Ladybird stalls on heavy pages.** slaterlabs.dev pauses for 200–550 ms while it loads,
  with React errors (#425, #422) in the console. Elaris's own work in those windows is about 1 ms
  per frame; the pause is in Ladybird. Comparing with Ladybird's own browser would confirm it.
- **No partial uploads.** `Pixels::set` replaces the whole image, and Rune's atlas copies and
  uploads it whole. Converting only Ladybird's damage rect would still need a full memory copy to
  hand Rune an owned buffer, so damage is not used. GPU texture sharing would remove the copies but
  is deliberately out of scope.
- **Copy scales with pixels.** About 1 ms for 2.5 MP; a full-screen 5K-class window would be
  around 4–6 ms, now on the engine thread rather than the UI thread.
- **A live resize can temporarily scale the last frame.** Rune keeps that frame pinned to the
  viewport while Ladybird lays out and presents the new size; the next complete frame replaces it.

## Not verified

- The GPU frame path (macOS) is untested at runtime. Check that pages draw with the right colours
  and no tearing, that tabs keep painting while scrolling, and compare `ELARIS_PERF=1` frame
  intervals against `ELARIS_CPU_FRAMES=1`. If a page freezes after a few frames, the backing
  stores are not being released.
- Frames' alpha is assumed opaque; the CPU path forced it, the GPU path samples what Ladybird wrote.

- Visual smoothness of real trackpad scrolling beyond the user's report on slaterlabs.dev; the
  test pages are driven by script and were measured, not watched.
- Resizing by dragging the window edge (measured resizes were scripted), display-scale changes,
  and displays above 60 Hz.
- Elastic overscroll: it needs a real touchpad, so it is enabled but unmeasured.
- Comparison with Ladybird's own browser.
- Linux.

See the [memory audit](memory-audit.md) for process RSS and retained frame payload measurements.
