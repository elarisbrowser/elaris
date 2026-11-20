# Quality audit

| Priority and evidence | Affected code and impact | Improvement | Risk | Status |
|---|---|---|---|---|
| High · Reproduced on macOS | `ui/mod.rs` applies native Glass to the window. The forced-light sidebar renders muted labels on a grey backdrop substantially darker than the palette's window colour. | Give chrome a stable background and check layered foreground contrast. | Medium · changes window material | Addressed |
| High · Verified in code | `ui/sidebar/mod.rs` identifies the address launcher with a low-alpha fill alone. Find and the omnibox use different focus and selection treatments. | Add a control boundary and shared focus and selection colours. | Low | Addressed |
| High · Reproduced on macOS | A fast close and quit left an incomplete session temp file and restored the just-closed tab. | Wait for the storage queue before the native host exits and keep the existing atomic replacement. | Low · up to two seconds during shutdown | Addressed and retested |
| Medium · Verified in code | Five UI call sites derive Rune Kit palettes independently. Rune Kit checks secondary text against the seed but paints some controls on derived surfaces. | Build Kit from semantic surface and text colours and verify the rendered combinations. | Low | Addressed |
| Medium · Verified in code | `app::start_engine` installs the storage wake callback only in bridge builds. Settings completion should wake Rune even without a web engine. | Register the storage wake callback in both build modes. | Low | Addressed |
| Medium · Measured in a headless burst | 128 history visits took 75 ms; 128 queued session snapshots took 1.04 s in a debug build on the local filesystem. | Coalesce adjacent fire-and-forget session snapshots to the latest full multi-window state; keep acknowledged commits and other storage commands as ordering barriers. | Low · intermediate unacknowledged snapshots may be skipped | Addressed; same burst's session writes took 10 ms |
| Low · Verified in code | Settings duplicated choice labels and the Linux XDG parser scanned all entries to return the last valid one. | Derive picker labels from typed choices and traverse XDG entries from the end. | Low | Addressed |
| Low · Potential improvement | Sidebar reordering and overlays use separate springs with different jobs. | Keep pointer tracking direct and review reduced-motion paths without retiming drag geometry. | Medium | Reviewed |

Browser owns stable tab IDs and section ordering. App coordinates overlays, session snapshots and
engine updates. View and frame ownership stay on the engine thread, C++ event payloads are copied
during the Rust callback, and worker shutdown drains queued persistence. The audit found no reason
to move these responsibilities or change the session format.

The storage burst test uses an isolated temporary profile and no native windows. It queues 128
history visits and then 128 complete session snapshots, draining each batch before timing the
next. The local debug-build result is a focused before-and-after measurement, not a cross-platform
benchmark; filesystem speed and build mode will change the absolute times.

For a real browsing run, set `ELARIS_STORAGE_PERF=1` before launching the integrated build. Each
session replacement then reports the cumulative write-attempt count, current and peak queued
worker commands, and worst observed enqueue-to-save latency. This opt-in output is intended for
measurement and is silent by default.

On macOS Apple Silicon, an integrated debug build loaded a local page in Ladybird that changed its
document title 100 times at 75 ms intervals. Across two runs, Elaris recorded 102 session write
attempts per run, the peak worker queue depth was 2 commands, and worst enqueue-to-save latency
ranged from 11.17 to 12.73 ms. After quit, each isolated profile stored the final
`Storage pressure 100` title. This exercises the actual engine and storage worker with a repeatable
page workload; it does not stand in for measurements on ordinary sites or other platforms.

The concurrent headless registry stress ran six window producers through 384 create/transfer/close
updates and verified all 384 distinct tab URLs remained in the final restored snapshot. Across
three runs, peak queue depth ranged from 174 to 329 commands, the worker performed three session
writes, and worst save latency ranged from 17.20 to 23.93 ms. The final-tab merge integration test
also passed with the transfer session committed before source-window closure.

See [Design and visual verification](ui-design.md) for the review matrix and observed timings.
