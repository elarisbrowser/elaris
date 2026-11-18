# Quality audit

| Priority and evidence | Affected code and impact | Improvement | Risk | Status |
|---|---|---|---|---|
| High · Reproduced on macOS | `ui/mod.rs` applies native Glass to the window. The forced-light sidebar renders muted labels on a grey backdrop substantially darker than the palette's window colour. | Give chrome a stable background and check layered foreground contrast. | Medium · changes window material | Addressed |
| High · Verified in code | `ui/sidebar/mod.rs` identifies the address launcher with a low-alpha fill alone. Find and the omnibox use different focus and selection treatments. | Add a control boundary and shared focus and selection colours. | Low | Addressed |
| Medium · Verified in code | Five UI call sites derive Rune Kit palettes independently. Rune Kit checks secondary text against the seed but paints some controls on derived surfaces. | Build Kit from semantic surface and text colours and verify the rendered combinations. | Low | Planned |
| Medium · Verified in code | `app::start_engine` installs the storage wake callback only in bridge builds. Settings completion should wake Rune even without a web engine. | Register the storage wake callback in both build modes. | Low | Planned |
| Medium · Potential improvement | Storage refreshes history snapshots after writes, while app actions can queue multiple session snapshots. Both are off the UI thread. | Measure queue pressure before introducing coalescing that could change shutdown or error ordering. | Medium | Deferred |
| Low · Potential improvement | Sidebar reordering and overlays use separate springs with different jobs. | Keep pointer tracking direct and review reduced-motion paths without retiming drag geometry. | Medium | Reviewed |

Browser owns stable tab IDs and section ordering. App coordinates overlays, session snapshots and
engine updates. View and frame ownership stay on the engine thread, C++ event payloads are copied
during the Rust callback, and worker shutdown drains queued persistence. The audit found no reason
to move these responsibilities or change the session format.
