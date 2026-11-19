# Elaris Performance & Memory Audit

The [benchmark protocol](../../benchmarks/README.md#memory-audit) combines Rune frame payload
counters with per-process RSS and CPU samples. The first pass used a debug build with Ladybird
enabled on macOS 27.2.1, with an isolated profile and the local idle fixture. The source was
commit `9bad70a` plus the audit instrumentation. Each full-size frame
held 6,973,312 bytes (6.65 MiB). The steady-state rows are medians of five samples taken about
one second apart after ten seconds of settling.

| Phase | Elaris RSS (MiB) | Descendant RSS sum (MiB) | Elaris CPU estimate | Hidden RGBA (MiB) |
|---|---:|---:|---:|---:|
| 1 loaded tab | 168.9 | 249.4 | 0.0% | 0.0 |
| 10 loaded tabs | 158.9 | 761.2 | 0.0% | 59.9 |
| 20 loaded tabs | 179.5 | 1300.6 | 6.8% | 126.4 |
| Switching among 20 loaded tabs | 191.4 | 1222.2 | 11.0% | 126.4 after switch |
| Back to 10 loaded tabs | 104.8 | 656.4 | 1.1% | 59.9 |

The switching row includes 12 samples taken while switching. The OS CPU estimate is smoothed,
and shared resident pages can appear in multiple processes' RSS. Descendant medians are
computed by summing the processes within each sample first. Per-process data remains separate
in the CSV.

A hidden view retains its latest full-size Rune frame. Closing ten views reduced the counted
payload by 66.5 MiB. Elaris RSS moved independently of the frame totals even at the same tab
count, so the payload gives the potential size of this optimization and the RSS samples describe
this particular run. The counters exclude Rune's renderer-side allocations and Ladybird's
buffers. Use a release-build comparison with the same profile and window geometry to measure the
resident memory benefit and the time to show a frame after switching back to a tab.

## Peek

Five Shift-click and Escape cycles added 8.47 MiB of live frame payload while Peek was open.
After each dismissal, the engine returned to a single visible view with 9.75 MiB of frame
payload and zero hidden frame bytes.

| Phase | Elaris RSS (MiB) | Descendant RSS sum (MiB) |
|---|---:|---:|
| Before Peek | 155.5 | 202.6 |
| During open and close cycles | 209.7 | 209.1 |
| After dismissal and settling | 139.4 | 181.5 |

## Split View

The CSS and JS animation fixtures ran in a fresh profile. Before splitting, JS was the
visible tab. Both panes rendered while split, and CSS was the visible tab after exiting.
Frame timings are p95 values from one five-second report per state. `Shown/s` is the
aggregate rate of frames accepted from Ladybird across both panes.

| Phase | Elaris RSS (MiB) | Descendant RSS sum (MiB) | Shown/s | Drawn (ms) | Copy (ms) | Set (ms) |
|---|---:|---:|---:|---:|---:|---:|
| Single pane before split | 224.2 | 272.9 | 58.6 | 32.76 | 2.32 | 0.08 |
| Both panes visible | 196.7 | 246.4 | 118.5 | 30.89 | 0.87 | 0.04 |
| Single pane after exit | 198.8 | 241.3 | 59.7 | 32.09 | 1.72 | 0.16 |

The split held 9.66 MiB across its two visible frames. After exit, the visible page returned
to 9.75 MiB and the hidden pane retained its last 4.83 MiB frame until its view closed.
`Set` measures the CPU-side handoff to Rune. GPU upload time and GPU residency were outside
the scope of this pass. Linux and release builds remain unmeasured.
