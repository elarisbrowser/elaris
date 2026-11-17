# Benchmarks

Pages for measuring how smoothly Elaris shows web content. Run one with timing on:

```sh
ELARIS_PERF=1 ./elaris run "file://$PWD/benchmarks/scroll.html"
```

Every 5 seconds Elaris prints timings to the terminal, separately for the UI thread (`main`) and
the engine thread (`elaris-engine`). See `docs/performance/webview.md` for what each line means
and for past results.

| Page | What it exercises | Healthy result (60 Hz display) |
|---|---|---|
| `css-animation.html` | A CSS animation | `shown` ≈ 60/s, `skipped 0`, `Drawn` p50 ≈ 16.7 ms |
| `js-animation.html` | A `requestAnimationFrame` animation driven by script | Same as above |
| `scroll.html` | The page scrolling itself every frame | Same as above |
| `idle.html` | Nothing moving | Almost no lines: both threads sleep |
| `color-scheme.html` | `prefers-color-scheme`, by CSS and by script | Both lines match the appearance picker, and change with it |

Real sites are worth checking too, by scrolling them with a touchpad. `https://slaterlabs.dev` (a
Next.js site) was used for the measurements in the performance notes.

Keep the window in front while measuring: Rune stops drawing a covered window.
