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

See [Design and visual verification](ui-design.md) for the review matrix and observed timings.
