# Architecture

Elaris is one Rust package. Rune owns the window and interface on the UI thread. Ladybird runs on
its own thread behind the Elaris-owned C++ bridge. A separate storage worker owns the SQLite
connection and writes session files, so neither page work nor browser-data writes block Rune.

## Modules

| Module | Responsibility |
|---|---|
| `src/main.rs` | Builds the Rune app and installs its lifecycle hooks. |
| `src/app.rs` | App state, tab actions, omnibox actions, startup restoration, persistence scheduling, polling and shutdown. |
| `src/browser.rs` | Ordered tabs, active-tab selection, input routing and commands sent to the engine. |
| `src/tab.rs` | Per-tab address, committed address, title, favicon, page state and engine state. |
| `src/omnibox.rs` | Omnibox state, local suggestion ranking and typed actions. |
| `src/history.rs` | Public synchronous SQLite history API; the app uses it through the storage worker. |
| `src/storage.rs` | Native data paths, asynchronous history access, session JSON and atomic session replacement. |
| `src/engine/` | The engine thread and everything it owns (see below). |
| `src/input.rs` | Translates Rune pointer, wheel and key input into Ladybird input. |
| `src/address.rs` | Classifies omnibox input and builds navigation or search URLs. |
| `src/appearance.rs` | System, light and dark appearance preference. |
| `src/perf.rs` | Development timing, printed when `ELARIS_PERF=1`. |
| `src/ui/` | Rune components for the toolbar, sidebar, omnibox, viewport and shared theme. |
| `src/ladybird/` (library) | Safe Rust wrappers around the bridge's C ABI. Plain event, frame and input types build without the bridge feature. |
| `bridge/` | The C++ bridge and its headless Ladybird views. |

The main dependency direction is `ui` → `app` → `browser` → `engine` → `ladybird` → bridge.
Storage is an app-owned boundary: the app queues committed visits and session snapshots, and the
storage worker handles disk access. The UI knows tabs and pixels, not SQLite or C++.

## Storage and privacy

Release data lives in the native application-data directory:

| Platform | Release data directory |
|---|---|
| macOS | `~/Library/Application Support/Elaris/` |
| Linux | `$XDG_DATA_HOME/Elaris/`, or `~/.local/share/Elaris/` when `XDG_DATA_HOME` is unset or relative |

Debug builds use a `development/` subdirectory under that root, keeping development profiles
separate from release data. Set `ELARIS_DATA_DIR` to select an exact application-data root for
development or tests; it must be absolute and bypasses the platform default and build-mode suffix.
The default Ladybird profile is `profile/` under the selected data root. The existing
`ELARIS_LADYBIRD_PROFILE_DIR` override still selects Ladybird's profile directory independently;
history and session files remain under `ELARIS_DATA_DIR` or the native application-data root.
`ELARIS_LADYBIRD_RESOURCE_DIR` continues to select Ladybird resources.

Elaris stores:

- `history.sqlite`: one row per visited HTTP or HTTPS URL, with its latest title, visit time and
  visit count. Internal, invalid and non-web URLs are excluded. Typed omnibox queries are never
  recorded.
- `session.json`: a versioned list of open tab URLs and titles plus the active tab index. It is
  written to a temporary file and atomically renamed into place.
- `profile/`: Ladybird's engine-owned profile data, separate from Elaris history and session
  metadata.

The storage worker owns the database connection. It serves recent and frequent snapshots and
search results to the UI from memory; searches are sent to SQLite off the UI thread. The local
history API supports recording visits, title updates, recent/frequent lists, search, individual
deletion and clearing. No remote autocomplete requests, telemetry or analytics are used.

History currently keeps the latest row for each URL and increments its visit count on each
committed visit. Elaris does not yet expose a history-management screen. Session metadata does not
include scroll position, form state, cookies or Ladybird's internal per-view navigation stack.
Restored background tabs are created in order but are not laid out or loaded until selected.

## The engine

`src/engine/` is the only Rust code that talks to Ladybird:

- `mod.rs`: the protocol. The browser sends `Command`s (open, load, resize, input, focus, close)
  and reads `Update`s (display URL, committed navigation, title, loading, history, favicon and
  error).
- `thread.rs`: starts the thread, runs its loop and joins it during process exit.
- `runtime.rs`: Ladybird and its views. They stay on the engine thread and are neither `Send` nor
  `Sync`.
- `frames.rs`: which presented frames are worth copying.

Without `ladybird-bridge`, `Engine::start` reports the missing feature and `cargo run` still works
without Ladybird.

Ladybird permits one application per process, so Elaris uses one engine thread with one view per
tab. Commands and updates carry a stable tab ID. Closing a tab drops later updates for that ID.
Only the active tab receives the viewport size and focus; hidden tabs retain their page. Restored
background tabs are an exception to the usual eager first layout: their load remains pending until
they become active.

## Navigation and history

Ladybird's display-URL callback can report an in-progress address, so it is used only to update the
omnibox and tab label. The bridge also reports a distinct committed-navigation event from
Ladybird's top-level commit and history-traversal callbacks. `app::poll` records only those events
in SQLite. A title updates history only after the displayed address agrees with the last committed
URL, so a provisional page title cannot overwrite the prior entry. This keeps typed queries and
provisional navigations out of persistent history.

The omnibox combines a compact recent list or local database matches with existing open-tab
suggestions. Matching considers title and URL/domain, then favors stronger text matches, visit
count and recency. Query work runs on the storage worker; the UI reads a small in-memory result
snapshot and never waits for SQLite.

## From Rune to Ladybird

1. Rune calls `app::poll`. The browser sends changed viewport, focus, visibility and first-load
   state to the engine without waiting for a reply.
2. Commands travel over a channel. They wake Ladybird's event loop with a bridge waker.
3. Ladybird callbacks are collected during the engine wait, tagged with a tab ID and sent back to
   the app. Committed navigation events and title changes are queued to the storage worker.
4. On ready to paint, the engine skips frames already shown or laid out at an old scale, then
   copies the displayed frame into a new RGBA buffer.
5. Frames go into the tab's `rune::Pixels`, which Rune can draw from any thread and which wakes the
   window. Rune scales the latest complete frame to the viewport.

## Startup and shutdown

At app construction, Elaris resolves its data directory and reads the small session file. It
recreates tabs in saved order and activates the saved tab before the engine starts. The active
view's first load waits for the viewport and colour scheme; restored background views stay lazy.
History database opening and subsequent queries/writes happen on the storage worker.

On normal close, the current tab list and active index are queued before Ladybird shutdown. Session
replacement is atomic. A process-exit handler drains and joins the storage worker after queued
writes, while the existing Ladybird exit handler still closes views, gives helper processes its
short grace period and joins the engine thread. A forced process termination can still lose work
that had not reached the storage worker; this milestone does not add crash recovery.
