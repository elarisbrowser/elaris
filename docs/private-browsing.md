# Private browsing engine investigation

Elaris pins upstream Ladybird at `76bef6eee7bffd40c1895a2ee7332405fa1ef83d`. The checked-out submodule uses the upstream remote and Elaris carries its Peek patch separately. The candidate `elarisbrowser/ladybird` URL returned “Repository not found”. The findings below refer to the pinned source.

| Area | Pinned implementation |
| --- | --- |
| Profile and views | `WebView::Application` owns the ordinary session and holds a weak reference to the current private session. The `HeadlessWebView` constructor accepts `IsPrivate`. Multiple private views can share the current session alongside ordinary views. |
| Cookies and HTTP | `BrowsingSession` creates an in-memory private cookie jar and HSTS store. `RequestServerManager` launches request servers by session and optional site, disables the private disk cache, and registers client IDs with their session for cookie routing. |
| Local and session storage | The private session creates its own memory-backed `StorageJar`. Session storage retains its per-traversable semantics. |
| Blob URLs and workers | The session owns a blob URL store. Shared-worker lookup uses the numeric session ID. BroadcastChannel fanout in `WorkerAgentManager` filters clients by privacy mode. Overlapping private generations therefore need an audit of retained content and worker clients before the host enables reset and immediate reopening. |
| IndexedDB, Cache Storage, service workers | The revision has IndexedDB bindings, Cache Storage code, and agent-local service-worker registration tables. Cross-view availability, process reuse, cleanup, and backing stores still need verification. |
| Authentication and TLS | The request server runs per session, and routes private DNS resolution, AIA and alternative services through private paths. HTTP authentication, client certificate state, and TLS resumption need live checks. |
| Permissions | The private permission manager has no persistence path. It consults the ordinary manager for global defaults, OS restrictions, and site blocks. The Elaris permission coordinator still needs private storage and generation-aware callbacks. |
| Downloads and temporary files | Ladybird excludes private download records from persistence. Its reset operation discards private downloads, so the host needs an explicit active-download close policy. A complete temporary-file audit, including download staging and resource loading, is pending. |

Closing a private view releases its session reference. Request servers and content processes can also hold references. Ladybird exposes `reset_private_browsing_session()` to clear the reusable session reference and remove unowned private workers. A production host must coordinate that operation with window ownership, view teardown, callbacks and downloads. The shared Application and event loop can host ordinary and private views. Elaris currently creates ordinary views and sends download events to a browser-wide observer.

A bridge smoke test can exercise the pinned engine through ordinary and private headless views against one loopback origin. Production window commands should follow verification of the engine boundary and Elaris's own persistence and event routing.
