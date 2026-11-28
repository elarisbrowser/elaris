# Ladybird embedding research

This is the original investigation for issue #230, kept as a design record. It describes the
repository before the bridge existed. For what ships now, see [architecture](architecture.md),
[the bridge guide](../bridge/README.md), and [webview performance](performance/webview.md).

## Recommendation

Use Ladybird's existing browser application and `WebView::ViewImplementation` stack behind a small Elaris-owned C ABI bridge. Keep Rune as the application shell. First render from CPU-visible shared backing stores and copy/import frames into Rune; consider GPU interop only after a working prototype. This uses Ladybird's multiprocess browser rather than embedding `LibWeb` as an in-process renderer.

The vendored source was inspected at the repository's current Ladybird revision. API details can change when the pin moves.

## Integration points

- **Startup and ownership:** `Libraries/LibWebView/Application.h` exposes `Application::create`, `initialize`, and `execute`; Qt starts this in `UI/Qt/main.cpp` and its `Application.cpp`. The application owns profile, process manager, IPC and event-loop state. A bridge must initialize and pump Ladybird's `Core::EventLoop` on the required thread, supply application/platform callbacks, and destroy views before the application. It cannot simply construct a `ViewImplementation` by itself.
- **Views and navigation:** `Libraries/LibWebView/ViewImplementation.h` provides `load(URL)`, `reload`, `stop_loading`, `url`, `title`, `is_loading`, viewport sizing, focus, input enqueueing and frame callbacks. `HeadlessWebView` is a concrete view, but has no native presentation widget. Qt's `RemoteView` (`UI/Qt/RemoteView.h/.cpp`) is a useful reference for the host subclass and callback wiring. View callbacks report navigation/title/loading and frame presentation; adapt those to C callbacks/events for Rust.
- **Frames:** `ViewImplementation` supports `set_presents_from_cpu_memory`, `server_did_paint`, `set_hosts_remote_layer`, `take_frame_snapshot`, and synchronized resize. Qt `RemoteView` installs `Gfx::SharedImage` backing stores and releases them only after presentation; the compositor may reuse a returned store. Its remote-layer path instead lets Ladybird's Compositor present frames itself. The snapshot API returns `Gfx::ShareableBitmap`, suitable for a slow initial proof but not ideal for continuous display. Prefer CPU backing-store presentation for Elaris's first version, with explicit lifetime/copying before release.
- **Input:** `enqueue_input_event(Web::InputEvent)` is the host entry point. Qt's `RemoteView` and page-surface handling show conversion from platform key, pointer, wheel and focus events. Resize through view viewport/window sizing and `begin_synchronized_resize`/`await_synchronized_resize_frame`/`commit_synchronized_resize` to avoid presenting mismatched geometry. The bridge must translate Rune event coordinates, modifiers, key text, wheel units and device-pixel ratio into Ladybird event types; do not expose C++ types across FFI.
- **Processes:** `Application`/`BrowserProcess`, `ProcessManager`, `HelperProcess`, Hub and `WebContentClient` start and supervise WebContent and helper services over Ladybird IPC. `Services/Hub` coordinates UI-to-WebContent view operations; RequestServer, ImageDecoder, AudioServer, Compositor and other services are launched as needed. Let Ladybird own these children and their shutdown. Elaris should own the top-level bridge/application lifetime, ensure its event loop is serviced, and close views before application teardown; do not independently spawn WebContent.

## Rendering and platform constraints

Rune in this checkout is configured with `metal-backend` (`Cargo.toml`). Rune source is pinned in Cargo and should be checked for the exact surface/texture APIs before implementing; the current Elaris UI (`src/ui/mod.rs`) only creates an empty content element and has no browser-frame import path. No confirmed Rune API for importing Ladybird `SharedImage` was found in the app code. CPU pixels copied into a Rune-supported image/texture is the lowest-risk initial path, though it costs a copy. Direct GPU sharing needs compatible device/context ownership and synchronization APIs on both sides; treat it as unproven.

Ladybird's Qt frontend has platform-specific presentation: Wayland uses `UI/Qt/WaylandPageSurface.*` and shared-image buffers; Qt's other surface implementations and `Libraries/LibGfx` handle native/GPU paths. Ladybird's compositor supports Vulkan/Skia paths on Linux and Metal paths on macOS, but its Qt-native surface integration is not a portable Rune surface. X11/Wayland differ in native handles and presentation, while Elaris should avoid binding browser logic to either. Start with CPU presentation on both platforms. Metal texture interop on macOS and Vulkan or shared-memory interop on Linux are later platform-specific options, not established drop-in interfaces.

## Limitations and likely patch boundary

`ViewImplementation` is a broad C++ host abstraction, not a stable public C API. Application creation expects host integration and Qt currently supplies substantial platform services, UI callbacks and process setup. The bridge must provide the minimal required host implementation and may discover missing seams only when compiled against the pinned revision. Keep any necessary Ladybird patch small and documented; likely candidates are exposing an existing CPU frame callback/backing-store access or making application startup usable without Qt. Do not patch rendering internals before a bridge prototype proves the seam is absent.

Building against Ladybird means using its CMake build and generated configuration, not linking a handful of standalone headers. `Libraries/LibWebView/CMakeLists.txt` and the top-level `CMakeLists.txt` define its internal dependency graph and service targets. Build the bridge as a CMake target in Elaris-owned integration code, link the exported/available LibWebView and required Ladybird targets, and arrange runtime service executable/resource paths. A Rust `build.rs` can invoke/configure this build or consume a separately built install/export; avoid hard-coded library lists because Ladybird's transitive dependencies and generated headers are substantial. Confirm target visibility and packaging against the pinned revision before choosing the final mechanism.

## Minimal plan for issue #230

1. Pin and build the existing Ladybird checkout on macOS and Linux; record required build options and service/resource paths.
2. Prototype an Elaris-owned C++ bridge that initializes `Application`, creates one CPU-presenting view, loads a URL, forwards title/loading/frame notifications, and pumps Ladybird's event loop.
3. Copy each presented CPU frame into a Rune-renderable image, handling damage, scale, resize, and backing-store release correctly.
4. Translate Rune keyboard, pointer, wheel and focus events to Ladybird input; exercise close and process shutdown.
5. Only after both OS paths work, assess native GPU sharing. Add a small upstream patch only for a demonstrated missing API, and keep ordinary `cargo run` independent of the optional Ladybird build.
