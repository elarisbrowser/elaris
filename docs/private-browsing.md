# Private browsing engine investigation

Elaris pins upstream Ladybird at `76bef6eee7bffd40c1895a2ee7332405fa1ef83d`. The checked-out submodule uses the upstream remote and Elaris carries its Peek patch separately. The maintained Ladybird-derived repository is `elarisbrowser/photon-engine`. Its master tip `0692c24b` contains Photon embedder and core engine changes, while Elaris still builds its pinned upstream submodule. Both revisions implement the same private reset path. The runtime findings below refer to the revision Elaris builds.

| Area | Pinned implementation |
| --- | --- |
| Profile and views | `WebView::Application` owns the ordinary session and holds a weak reference to the current private session. The `HeadlessWebView` constructor accepts `IsPrivate`. Multiple private views can share the current session alongside ordinary views. |
| Cookies and HTTP | `BrowsingSession` creates an in-memory private cookie jar and HSTS store. `RequestServerManager` launches request servers by session and optional site, disables the private disk cache, and registers client IDs with their session for cookie routing. |
| Local and session storage | The private session creates its own memory-backed `StorageJar`. Session storage retains its per-traversable semantics. |
| Blob URLs and workers | The session owns a blob URL store. Shared-worker lookup uses the numeric session ID. BroadcastChannel fanout in `WorkerAgentManager` filters clients by privacy mode. Overlapping private generations therefore need an audit of retained content and worker clients before the host enables reset and immediate reopening. |
| IndexedDB, Cache Storage, service workers | IndexedDB stores databases in memory per agent thread in `indexed_db/internal/storage.rs`. Cache Storage uses an ephemeral object-local map pending UI-process serialization, and service-worker registrations are agent-local. Private top-level pages start in separate content clients. These implementations do not provide a session-wide backing store for independent views. |
| Authentication and TLS | The request server runs per session, and routes private DNS resolution, AIA and alternative services through private paths. Fetch leaves creation and reuse of HTTP authentication entries unimplemented. Client certificate state and TLS resumption need live checks. |
| Permissions | The private permission manager has no persistence path. It consults the ordinary manager for global defaults, OS restrictions, and site blocks. The Elaris permission coordinator still needs private storage and generation-aware callbacks. |
| Downloads and temporary files | Ladybird excludes private download records from persistence. Its reset operation discards private downloads, so the host needs an explicit active-download close policy. A complete temporary-file audit, including download staging and resource loading, is pending. |

Closing a private view releases its session reference. Request servers and content processes can also hold references. Ladybird exposes `reset_private_browsing_session()` to clear the reusable session reference and remove unowned private workers. A production host must coordinate that operation with window ownership, view teardown, callbacks and downloads. The shared Application and event loop can host ordinary and private views. Elaris currently creates ordinary views and sends download events to a browser-wide observer.

## Verified on Apple Silicon macOS

The download smoke executable also runs a privacy probe against a loopback origin and an isolated profile. The server reports the HTTP request cookie and the page reports local and session storage. The probe verifies two-way isolation, private cookie and local storage sharing across views, per-view session storage, survival after closing one view, and fresh storage after closing all private views and explicitly resetting the session. It also checks that the test bridge rejects reset while a private view remains open.

## RequestServer retention after reset

Before the Elaris patch, Ladybird kept browser-level private RequestServers in the manager until application shutdown. Each held a strong reference to the old session. The bridge build now retires every RequestServer for the ending session, disconnects its clients, and rejects subsequent attempts to launch a RequestServer for that session. The bridge smoke test requires zero private RequestServers after each reset and exercises fresh cookies and storage after reopening.

The host bridge also rejects prepared navigations between distinct Ladybird session IDs before consuming the source descriptor. Its ordinary download event stream filters out private records. The live tests verify that a rejected prepared link can still be loaded in its own context, private download records never reach that stream, and the completed file remains after reset. Three successive close/reset/reopen cycles verify that the next session sees fresh cookies, local storage, and session storage.

## Release status

Elaris ships private windows (File › New Private Window, ⌘⇧N / Ctrl+Shift+N). Every page in a private
window, including popups and Peek, opens in the shared private session. Private windows record no
history and save no session, tabs cannot be dragged or detached between private and ordinary windows,
and the engine resets the private session when the last private view closes, so the next private
window starts empty.

Private downloads appear only in private windows' download lists and are forgotten with the
session, whose reset cancels any still running. Retry and the image context menu's Save Image As
are not available in private windows for now. Worker and content-client
isolation across successive private generations, permission prompts in private windows, disk cache
behavior and TLS cleanup remain unverified.
