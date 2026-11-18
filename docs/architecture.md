# Architecture

Elaris is one Rust package. Rune owns the window and interface on the UI thread. Ladybird runs on
its own thread behind the Elaris-owned C++ bridge. A separate storage worker owns the SQLite
connection and writes session files, so neither page work nor browser-data writes block Rune.

## Modules

| Module | Responsibility |
|---|---|
| `src/main.rs` | Builds the Rune app and installs its lifecycle hooks. |
| `src/app.rs` | App state, settings actions, tab actions, omnibox and find actions, startup restoration, persistence scheduling, polling and shutdown. |
| `src/browser.rs` | Ordered web and native tabs, their Favourite/Pinned/Regular classification, active-tab selection, input routing and commands sent to the engine, including tab-targeted find requests. |
| `src/tab.rs` | Per-tab address, committed address, title, favicon, section, page state, native route, engine state and transient find query/results. |
| `src/omnibox.rs` | Omnibox state, local suggestion ranking and typed actions. |
| `src/history.rs` | Synchronous SQLite history API used by the storage worker. |
| `src/app/history.rs` | Native History search, pagination, mutation acknowledgements and navigation. |
| `src/storage.rs` | Native data/config paths, asynchronous history and configuration access, session JSON and atomic replacement. |
| `src/config.rs` | Typed TOML loading, validation, comment-preserving edits and atomic configuration writes. |
| `src/settings.rs` | The single typed settings model shared by the app, settings page and TOML file. |
| `src/internal.rs` | Explicit `elaris://` route parsing for Elaris-owned native pages. |
| `src/engine/` | The engine thread and everything it owns (see below). |
| `src/input.rs` | Translates Rune pointer, wheel and key input into Ladybird input. |
| `src/address.rs` | Classifies omnibox input and builds navigation or search URLs. |
| `src/appearance.rs` | System, light and dark appearance preference. |
| `src/perf.rs` | Development timing, printed when `ELARIS_PERF=1`. |
| `src/ui/` | Rune components for the toolbar, sidebar, omnibox, floating find bar, viewport, native pages and shared theme. |
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

The user-editable `config.toml` lives at `~/Library/Application Support/Elaris/config.toml` on macOS
and `$XDG_CONFIG_HOME/elaris/config.toml` on Linux, falling back to `~/.config/elaris/config.toml`.
Debug builds place each profile under its own `development/` directory. `ELARIS_DATA_DIR` is an
explicit test/development override and places the config beside the selected data root.

Set `ELARIS_DATA_DIR` to select an exact application-data root for
development or tests; it must be absolute and bypasses the platform default and build-mode suffix.
The default Ladybird profile is `profile/` under the selected data root. The existing
`ELARIS_LADYBIRD_PROFILE_DIR` override still selects Ladybird's profile directory independently;
history and session files remain under `ELARIS_DATA_DIR` or the native application-data root.
`ELARIS_LADYBIRD_RESOURCE_DIR` continues to select Ladybird resources.

Elaris stores:

- `history.sqlite`: one row per visited HTTP or HTTPS URL, with its latest title, visit time and
  visit count. Internal, invalid and non-web URLs are excluded. Typed omnibox queries are never
  recorded.
- `session.json`: a versioned list of open tab URLs and titles plus the active tab index and the
  indices classified as Favourite or Pinned. Unmarked tabs are Regular. Older sessions without
  section indices restore as Regular tabs. The file is written to a temporary file and atomically
  renamed into place.
- `profile/`: Ladybird's engine-owned profile data, separate from Elaris history and session
  metadata.
- `config.toml`: appearance, search provider, download directory and startup mode. The settings
  page and file use the same typed `Settings` value. The storage worker edits one key at a time,
  preserves unrelated TOML comments and keys, and replaces the file atomically. A malformed file
  is reported and never replaced by defaults. Reloading from the settings page reads the file
  again without restarting the browser.

The storage worker owns the database connection. It serves recent and frequent snapshots and
search results to the UI from memory; searches are sent to SQLite off the UI thread. The local
history API supports recording visits, title updates, recent/frequent lists, search, individual
deletion and clearing. No remote autocomplete requests, telemetry or analytics are used.

Session metadata does not include scroll position, form state, cookies or Ladybird's internal
per-view navigation stack.
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

### Native History

`elaris://history` is a Rune page. Command+Y on macOS and Control+H on Linux activate an existing
History tab or open one. Find focuses the native History search field. Both the route and the
active tab restore through the ordinary session file.

The database stores the most recent committed visit to each HTTP or HTTPS URL and a count of
visits to that address. The page shows one result per URL, most recent first, and groups results
by the local date of that visit. Date and time conversion uses the host timezone for each stored
timestamp. Today and Yesterday are based on local calendar dates and refresh at local midnight.

The storage worker searches title and full URL, including the domain, as the user types.
Substring matching is literal, with SQLite's default case folding for ASCII. The composite
timestamp and URL index provides stable cursor pagination in batches of 40. Newer and Older
replace the visible batch and keep the Rune tree bounded. Arbitrary substring searches can scan
the index when matches are sparse. Request revisions let the worker skip superseded searches and
let the app discard results for older queries.

Opening a result replaces the current History tab. Middle-click or a platform shortcut-modifier
click opens a background tab. Adding Shift to the shortcut-modifier click opens a foreground tab.
History uses favicons from matching live tabs and a generic website icon for other entries.
Persisting favicons would require extending the history store.

Removing a result deletes its URL row and the visit count for that address. Clear History requires
confirmation and deletes all rows in a single SQLite transaction. The worker refreshes the
omnibox snapshots before acknowledging a successful mutation. The History page keeps the search
term and refreshes the results. Open tabs, Favourites, Pinned Tabs, session metadata and
Ladybird's profile data are outside these operations.

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

Tabs remain in one stable-ID browser collection, grouped for keyboard navigation in Favourite,
Pinned, then Regular order. Moving a tab only changes its section and position; it keeps its
existing Ladybird view and page state. Rune's sidebar owns the transient drag preview and shared
row-to-tile transition, while the browser commits a move only on release or a context-menu action.
Only those completed moves queue a session snapshot. With the blank-window startup preference,
restored Favourite and Pinned tabs remain in the sidebar without activating a page; regular tabs
from the prior session are not restored into that blank window.

At app construction, Elaris resolves its data/config directories, loads the small settings file,
and reads the session file. It recreates tabs in saved order and activates the saved tab before the
engine starts. `elaris://` URLs restore as Elaris-owned native tabs and never create Ladybird views.
Only explicitly supported routes are rendered; unknown internal routes show a native not-found
page. A webpage's navigation to an `elaris:` URL is rejected at the browser/engine boundary.
The active web view's first load waits for the viewport and colour scheme; restored background web
views stay lazy. History database opening, configuration saves/reloads and subsequent history
queries/writes happen on the storage worker.

## Find in Page

The floating bar is rendered through Rune's root overlay layer. Its anchor uses the viewport's
sidebar and platform header geometry, so its right and top insets are measured from the webpage
area while it remains above the Ladybird surface. `Cmd+F` on macOS and `Ctrl+F` on Linux focus that field;
`Enter`/`Shift+Enter` and the platform `Cmd+G`/`Ctrl+G` shortcuts route to the active tab's next
or previous match. The omnibox and page context menu dismiss the bar before taking focus.

Each web `Tab` owns its last query, latest Ladybird result, and a monotonically advanced request
generation. `Browser` validates the stable `TabId` and sends typed find commands through the engine
queue. `Runtime` invokes `WebView::find_in_page`, `find_in_page_next_match`,
`find_in_page_previous_match`, and `find_in_page_end`; Ladybird performs text matching, paints all
match highlights, distinguishes the active range, and scrolls it into view. The Elaris bridge
reports Ladybird's current index and optional total asynchronously. The UI clears displayed counts
as soon as a query or page changes, and drops results whose generation no longer matches.

Ladybird's existing callback does not include a request token. The Elaris bridge tags its result
callbacks with each view's queued request IDs; the pinned engine processes these requests and
callbacks in order on the view's IPC path. Navigation invalidates pending Rust generations before
the retained query is issued again after commit. Closing a tab drops every later event for its
never-reused ID. Switching tabs clears the previous page's highlights while keeping its query in
memory, then restores the new active tab's query. Dismissing Find clears highlights and counts but
keeps that tab's query for the next open. Queries are never written to disk or history.

When Find opens with an empty query, the bridge reads the selected webpage text through
`ViewImplementation::selected_text()`'s asynchronous promise. The app accepts it only if the same
tab and request are still active and the user has not started typing. History routes Find to its
own search field. Other internal pages show the Find unavailable message.

Rune connects the storage wake callback when the window is ready, including UI-only builds.
The callback schedules a poll after the worker publishes configuration or history updates.

On close or quit, App queues the current session and waits up to two seconds for the storage worker
to acknowledge the queue. The worker still owns all disk I/O and replaces the session atomically.
The acknowledgement means earlier commands finished processing. Write errors are logged by the worker.
Process-exit handlers still request worker shutdown and join the storage and engine threads when the
host runs those handlers. A forced exit or stalled disk can still leave pending work.

See [Design and visual verification](ui-design.md) for native UI conventions and checks.

### Quick Tab Switcher

Control+Tab previews the next recently used tab, and Control+Shift+Tab
cycles backwards. Rune's desktop host delivers native modifier changes to
the focused switcher. Releasing Control or pressing Enter commits the
selection. Escape, an outside click, and window deactivation cancel it.
The switcher ignores the chord during an active sidebar drag.

Browser records recency at activation using stable tab IDs. Tabs without an
activation record follow in sidebar order. A restored session starts with
its saved active tab followed by the saved sidebar order. The switcher
snapshots that order when opened and keeps its preview separate from
Browser, so cycling leaves the page, session, and lazy loads untouched.
