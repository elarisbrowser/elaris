# Architecture

Elaris is one Rust package. Rune draws the window on the UI thread; upstream Ladybird runs on a
thread of its own behind a small C++ bridge. Nothing on the UI thread waits for Ladybird.

## Modules

| Module | Responsibility |
|---|---|
| `src/main.rs` | Hands Rune the app's state, view and hooks. |
| `src/app.rs` | Application state (address bar, page, appearance) and the Rune hooks: `setup` starts the engine, `poll` runs each frame, and window focus and shutdown go to the page. |
| `src/page.rs` | One page: its latest frame, loading state and error. It turns UI changes (viewport size, scale, focus, colour scheme, input) into engine commands, sending only what changed, and reads back engine updates. |
| `src/engine/` | The engine thread and everything it owns (see below). |
| `src/input.rs` | Translates Rune pointer, wheel and key input into Ladybird's, from points to device pixels. |
| `src/address.rs` | Turns what was typed into a URL to load, or says why it cannot. |
| `src/appearance.rs` | The System/Light/Dark preference. |
| `src/perf.rs` | Development timing, printed when `ELARIS_PERF=1`. |
| `src/ui/` | Rune components: `toolbar`, `appearance_picker`, `viewport` (the page's frame and its input), and `theme` (every colour and measurement). |
| `src/ladybird/` (library) | Safe Rust over the bridge's C ABI. The plain event, frame and input types are always built; the FFI wrappers need the `ladybird-bridge` feature. |
| `bridge/` | The C++ bridge: a headless Ladybird view whose frames are read from CPU memory, behind `elaris_ladybird.h`. |
| `scripts/elaris_cli/` | The `./elaris` developer commands. |

Dependencies point one way: `ui` → `app` → `page` → `engine` → `ladybird` → bridge. The UI knows
pages and their pixels, not the engine or C++; the engine knows nothing of the UI's layout.

## The engine

`src/engine/` is the only code that talks to Ladybird:

- `mod.rs`: the protocol. The page sends `Command`s (load, resize, input, focus, colour scheme,
  close) and reads `Update`s (navigation, title, loading, error, a frame of a new size). `Engine`
  is the page's handle on both channels.
- `thread.rs`: starts the thread, runs its loop and finishes it as the process exits.
- `runtime.rs`: Ladybird and its view. Ladybird requires every call on the thread that started it,
  so only the engine thread holds these; they are neither `Send` nor `Sync`.
- `frames.rs`: which presented frames are worth copying.

Without the `ladybird-bridge` feature, `Engine::start` returns an error the toolbar shows, so
`cargo run` works without Ladybird.

## From the UI to Ladybird

1. Rune calls `app::poll` at the start of every frame. The page sends the engine whatever changed
   since the last poll: viewport size and scale, focus, and the first load once both size and
   colour scheme are known. Input handlers send pointer, wheel and key commands immediately.
2. Sending queues the command on a channel and wakes Ladybird's event loop (`elaris_ladybird_wake`,
   safe from any thread). Nothing waits for a reply.
3. The engine thread sleeps in `elaris_ladybird_wait` until Ladybird has work or a command wakes
   it. It applies queued commands in order, except that of several resizes only the newest is
   applied, so a live resize lays out the size the window has now.

## From Ladybird to the screen

1. Ladybird's callbacks (navigation, title, loading, ready to paint) are collected during a wait
   and handled after it returns.
2. On ready to paint, the engine skips frames already shown or laid out at an old scale, then
   copies the displayed frame into a new RGBA buffer (`elaris_ladybird_copy_frame`).
3. A frame of the size already on show goes straight into the page's `rune::Pixels`, which Rune
   allows from any thread and which wakes the window. No UI state changes, so the view is not
   rebuilt.
4. A frame of a new size, and every other update, goes through the update channel, followed by
   the waker Rune gave `setup`. The next poll applies them together, so the frame's pixels and its
   layout size never disagree.
5. While frames keep arriving, `poll` asks Rune for the next display frame, which keeps Rune
   drawing at the display's rate.

## Ownership and shutdown

- `Page` owns the `Engine` handle and the `Pixels` the engine draws into.
- The engine thread owns Ladybird and the view. Each `WebView` holds a reference to the bridge, so
  the bridge is destroyed only after its last view.
- Closing or quitting only sends `Close`. The engine closes the view, keeps Ladybird running for
  a short grace period so helper processes see it go, and exits. It does this on every exit path,
  including errors.
- Rune ends the process from inside its event loop, so an `atexit` handler sends `Close` if it was
  not sent and joins the engine thread. It runs before Ladybird's own exit handlers, so no
  Ladybird code runs while its statics are torn down. Ladybird's helper processes exit with it.

## Where tabs fit

Ladybird allows one application per process, and the bridge, the colour scheme and the `atexit`
handler are all process-wide. Tabs therefore mean one engine thread with many views, not many
engines:

- `AppState` would own the `Engine` and a list of pages; each page would own its `Pixels`,
  frame size, loading state, error and address text, and the active page would be the one the
  viewport shows.
- `Command` and `Update` would carry a view id, and `Runtime` would hold a view per id with a
  `FrameGate` each. The colour scheme is already applied to the whole application, not a view.
- The bridge's events do not yet say which view raised them: `ElarisLadybirdEvent` would need the
  view (or each view its own callback context) before a second view can be told apart.
