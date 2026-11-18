# Elaris

Elaris is a cross-platform browser with a Rust and [Rune](https://github.com/anomalyco/rune) desktop interface and the upstream [Ladybird](https://github.com/LadybirdBrowser/ladybird) web engine. Ladybird is pinned as a Git submodule; Elaris keeps its C++ integration in `bridge/`.

## Get started

You need Python 3.9 or newer and Git to start `./elaris`. On macOS, install Homebrew and Xcode 26 or newer first. On Linux, use a supported package manager (apt, dnf, pacman, or zypper) and a desktop session with a working GPU driver to open the app.

From the repository root:

```sh
./elaris setup       # install build prerequisites, Rust toolchains, and Ladybird
./elaris build       # build the Ladybird engine, bridge, and Elaris
./elaris run         # build what is needed, then launch Elaris
./elaris run https://example.com
```

The first engine build downloads and compiles Ladybird's dependencies and can take a while. On Linux, setup asks for your password through `sudo` when package installation requires it. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full setup, platform details, checks, and project layout.

To work on the browser shell without building Ladybird, use `./elaris build --ui` and `./elaris run --ui`. A direct `cargo run` also remains independent of Ladybird.

## Common commands

| Command | Purpose |
| --- | --- |
| `./elaris setup` | Install host prerequisites, Rust toolchains, and the pinned Ladybird submodule |
| `./elaris build` | Build Ladybird dependencies, the Elaris bridge, and the Rust app |
| `./elaris run [URL]` | Build the integrated browser and launch it, optionally at a URL |
| `./elaris check` | Run Rust formatting, check, Clippy, and tests |
| `./elaris format` | Format Elaris Rust and C++ sources |
| `./elaris engine status` | Show the pinned Ladybird revision and build status |

Set `ELARIS_JOBS` or pass `--jobs N` to `build` or `run` to control parallel builds. `./elaris setup --skip-system-deps` skips package-manager installs when prerequisites are already managed separately, but still verifies them.

## UI tooling

Rune includes an offscreen renderer and command-line tools for inspecting the UI without opening a window. Put Rune's flags after `--`:

```sh
# Render the initial browser window to a PNG.
./elaris run --ui -- --snap /tmp/elaris.png

# Save the UI element tree, including labels, roles, text, and layout rectangles.
./elaris run --ui -- --tree /tmp/elaris-tree.json

# Drive the app with a Rune input script and save a trace.
./elaris run --ui -- --script interaction.json --frames 120 --trace /tmp/elaris-trace.jsonl
```

Headless runs can check layout, visual output, and scripted UI behavior. They do not load pages through Ladybird or measure native window presentation. See [Rune's agent tooling guide](https://github.com/anomalyco/rune/blob/dev/crates/rune/README.md#agent-tooling) for the complete flag and script reference.

Rune also accepts options such as `--size 960x640`, `--scale 2`, `--theme dark`, and `--at 0.5` (virtual time in seconds) with `--snap` and `--tree`. Combine `--script` with `--frames`, `--fps`, `--out`, or `--trace` to inspect a sequence of frames. Use `--native --trace` on a real display to inspect window presentation timings.

## Project notes

- [Architecture](docs/architecture.md) explains the UI, engine thread, and bridge.
- [Ladybird embedding notes](docs/ladybird-embedding.md) record the integration constraints.
- [Bridge development and integration checks](bridge/README.md) cover focused C++ and FFI work.
- Elaris is GPL-3.0-only; contributions are accepted under that license without a CLA. Rune is MIT-licensed and Ladybird is BSD-2-Clause. See [TRADEMARKS.md](TRADEMARKS.md) for branding policy.
