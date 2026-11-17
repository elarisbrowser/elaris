# Elaris Browser

Elaris is a small cross-platform browser shell built with Rust and [Rune](https://github.com/anomalyco/rune). The app opens a dark native window with an editable address bar and an empty content area. Ladybird is pinned as an independent engine dependency; it is not linked to the app yet.

## Developer CLI

Use `./elaris` (Python 3 standard library) for common development tasks:

```sh
./elaris setup          # Initialise submodules and check tools
./elaris build          # Build the Rust application
./elaris run [URL]      # Build if needed and launch Elaris with Ladybird
./elaris run --ui       # Launch the interface alone, without Ladybird
./elaris check          # Format, lint, and test
./elaris engine status  # Inspect the pinned Ladybird checkout
./elaris engine build   # Build Ladybird with CMake and Ninja
```

Ladybird builds are explicit and are not triggered by `build` or `run`; `run` needs a built Ladybird bridge (see `bridge/README.md`) unless given `--ui`. The engine build directory defaults to `vendor/ladybird/Build/release`; set `LADYBIRD_BUILD_DIR` to override it.

Install the stable Rust toolchain before using the Rust commands. Rune supports macOS and Linux. A working desktop session and GPU driver are required to open the native window on Linux.

## Headless UI checks and benchmarks

Rune includes an offscreen renderer and command-line tools for checking the UI without opening a
window. Elaris enables Rune's `tooling` feature in all builds, so these commands work with both
debug and release builds. The existing `--ui` option skips the Ladybird bridge; put Rune's flags
after `--` so the Elaris CLI passes them through:

```sh
# Render the initial browser window to a PNG.
./elaris run --ui -- --snap /tmp/elaris.png

# Save the UI element tree, including labels, roles, text and layout rectangles.
./elaris run --ui -- --tree /tmp/elaris-tree.json

# Drive the app with a Rune input script, render 120 frames and save a trace.
./elaris run --ui -- --script interaction.json --frames 120 --trace /tmp/elaris-trace.jsonl
```

Rune also accepts options such as `--size 960x640`, `--scale 2`, `--theme dark`, and `--at 0.5`
(virtual time in seconds) with `--snap` and `--tree`. `--script` files describe synthetic pointer
and keyboard input; combine them with `--frames`, `--fps`, `--out`, or `--trace` to inspect a
sequence. For example, `--script interaction.json --frames 120 --fps 60 --out /tmp/frames` saves
sampled frames. See [Rune's agent tooling guide](https://github.com/anomalyco/rune/blob/dev/crates/rune/README.md#agent-tooling)
for the full flag and script format reference.

Headless runs render Elaris's Rune interface through an offscreen GPU renderer. They can check
layout, visual output, and scripted UI behavior without showing a window. They do not load pages
through Ladybird, measure browser-engine performance, or include native window presentation. For
on-screen presentation timings, use Rune's `--native --trace` on a real display.

Rune supports macOS and Linux. A working desktop session and GPU driver are required to open the native window on Linux.

## Development setup

See [SETUP.md](SETUP.md) for Ladybird prerequisites, submodule setup, and build commands for macOS and Linux. [docs/architecture.md](docs/architecture.md) explains how the code is organised and how pages reach the screen.

## Licensing

Elaris is licensed under [GPL-3.0-only](LICENSE). Contributions are accepted on the understanding that they are provided under the same license; no CLA or copyright assignment is required. Third-party components retain their own licenses, including Rune's MIT license and Ladybird's BSD-2-Clause license. See [TRADEMARKS.md](TRADEMARKS.md) for the separate branding policy.
