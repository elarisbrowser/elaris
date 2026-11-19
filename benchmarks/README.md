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

## Memory audit

Run with an isolated data directory so restored tabs and browsing data do not affect the baseline.

```sh
audit_dir=$(mktemp -d)
ELARIS_DATA_DIR="$audit_dir/profile" ELARIS_PERF=1 \
  ./elaris run "file://$PWD/benchmarks/idle.html" 2>"$audit_dir/perf.log"
```

In a second terminal, find the actual Elaris PID with `ps -ax -o pid,ppid,comm`.
Use the PID of the Elaris binary, excluding the launcher and Cargo. Set `audit_dir`
to the path created above and `elaris_pid` to that PID. Sample each phase into its own CSV.

```sh
python3 benchmarks/sample-processes.py "$elaris_pid" idle_1 \
  >"$audit_dir/idle_1.csv"
```

Keep the window size, display scale, and refresh rate fixed. Record them alongside the
Elaris commit, OS, and build profile. Allow ten seconds to settle before sampling each
steady state. Use the same address for each idle tab, open each in the foreground, and
wait for its first frame before opening the next. Repeat the sequence in a fresh profile.

| Phase | Procedure |
|---|---|
| `idle_1` | Sample one loaded idle page. |
| `idle_10`, `idle_20` | Sample at ten and twenty loaded idle pages. |
| `switch` | Start sampling, then cycle through ten loaded tabs, one per second. |
| `after_close` | Close ten loaded tabs and sample once settled. Compare with the earlier ten-tab sample. |
| `split_before`, `split_active`, `split_after` | In a fresh run, load the CSS and JS animation pages. Sample both tabs in single-pane mode, then use the tab context menu to split and select the other tab, then sample after exiting Split View. Compare `Drawn`, `Copy`, and `Set` in the timing log. |
| `peek_before`, `peek_cycles`, `peek_after` | In a fresh run, open `links.html`. Shift-click its link to open Peek and press Escape to dismiss. Sample before opening Peek, during five open/close cycles of two seconds each (use `--samples 20`), and after dismissal and settling. |

CSV rows keep Elaris separate from each descendant. Identify Ladybird helpers by the
command column. RSS includes shared resident mappings, so summing helper RSS can
double-count memory. `ps_cpu_percent` is the OS's reported CPU estimate, which may
reflect usage outside the sampling interval. The sampler observes child processes
while they remain descendants of Elaris.

The `[perf surfaces]` lines record visible and hidden engine views and the RGBA payload
of their latest Rune frames. They appear on view lifecycle and visibility commands and
when frame dimensions change. Counts cover live engine views, including Peek views,
and exclude lazy tabs that have yet to open a view. Payload bytes exclude thumbnail
cache, allocator overhead, Rune atlases, GPU textures and Ladybird-owned buffers.
Compare these counters with the corresponding RSS samples to investigate which
memory changes track frame retention.

### Hidden-frame eviction

Elaris keeps the two most recently hidden full-resolution frames by default. Older
hidden views keep a full-page fallback with a maximum side of 160 pixels. Set
`ELARIS_EVICT_HIDDEN_FRAMES=0` for a baseline run with eviction disabled.

Run the same sequence in both modes with fresh profiles and `ELARIS_PERF=1`. Confirm the
number of live views in the log before comparing RSS. Add `fallback_rgba_bytes` to
the full-resolution payload when accounting for all Rune page images. With eviction
enabled, `EvictFrame` measures copying and downsampling a hidden frame and
`RestoreFrame` measures the engine's attempt to restore it on activation. Neither
timing includes UI input dispatch or presentation. Use contrasting pages during rapid
switching to check for blank frames and confirm both Split View panes and Peek still
paint and dismiss.
