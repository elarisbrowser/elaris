# Elaris Browser

Elaris is a small cross-platform browser shell built with Rust and [Rune](https://github.com/anomalyco/rune). The app opens a dark native window with an editable address bar and an empty content area. Ladybird is pinned as an independent engine dependency; it is not linked to the app yet.

## Run Elaris

Install the stable Rust toolchain, then run:

```sh
cargo run
```

Rune supports macOS and Linux. A working desktop session and GPU driver are required to open the native window on Linux.

## Development setup

See [SETUP.md](SETUP.md) for Ladybird prerequisites, submodule setup, and build commands for macOS and Linux.
