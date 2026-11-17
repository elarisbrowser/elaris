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

Rune supports macOS and Linux. A working desktop session and GPU driver are required to open the native window on Linux.

## Development setup

See [SETUP.md](SETUP.md) for Ladybird prerequisites, submodule setup, and build commands for macOS and Linux.

## Licensing

Elaris is licensed under [GPL-3.0-only](LICENSE). Contributions are accepted on the understanding that they are provided under the same license; no CLA or copyright assignment is required. Third-party components retain their own licenses, including Rune's MIT license and Ladybird's BSD-2-Clause license. See [TRADEMARKS.md](TRADEMARKS.md) for the separate branding policy.
