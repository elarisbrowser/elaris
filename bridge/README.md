# Ladybird bridge

This opt-in bridge builds against the pinned `vendor/ladybird` checkout. It owns no Ladybird code and adds no files to that submodule. The bridge keeps the existing Rust/Rune binary independent; it is built separately with CMake.

```sh
cmake -S bridge -B build/ladybird-bridge -G Ninja -DENABLE_GUI_TARGETS=OFF
cmake --build build/ladybird-bridge --target elaris_ladybird
```

The developer CLI provides `./elaris cpp-format`, `./elaris cpp-configure`, and
`./elaris cpp-tidy`. The configuration exports `compile_commands.json` under
`build/cpp`; clangd reads that database through the root `.clangd` config. CMake
supplies Ladybird's target include paths, generated-header directories, and
compiler definitions to clangd. The database and build directory are ignored by
git. Formatting and tidy inputs are limited to files under `bridge/`.

Ladybird's full native build prerequisites and resource packaging still apply. The runtime `resource_dir` must point to Ladybird's installed resource directory (the one containing `themes/Default.ini`), and the profile path must be absolute. Ladybird launches its own helper processes using the configured install/build paths. Build and package those helpers with Ladybird; the bridge does not spawn them itself.

The C API requires one owner thread for initialization, view operations, event callbacks, pumping, and shutdown. `elaris_ladybird_pump` polls without waiting and should be called from the host event loop so Rune remains responsive. Destroying the bridge also destroys any views still owned by it. The event text is borrowed only for the callback duration. Call `elaris_ladybird_last_error` immediately after a failing function.

## Current boundary

This milestone exposes initialization, one or more headless views, navigation, title/loading notifications, a ready-to-paint notification, polling, and orderly view/application teardown. It does not copy or expose frame pixels, connect the library to Rust's build, or render into Rune. The `FRAME` event means Ladybird reports the view ready to paint; it does not provide a presentable frame buffer. These boundaries keep normal `cargo run` independent while the C++ host seams are validated against the pinned revision.

The bridge has not yet been verified end to end. On the current macOS checkout, CMake configuration reaches Ladybird's dependency checks but stops because the `mimalloc` CMake package is missing. Linux and runtime process lifecycle verification remain outstanding.
