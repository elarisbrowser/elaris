<div align="center">
  <img src="assets/brand-kit/logo/logo-color.svg" alt="" width="112" />
  <h1>elaris</h1>
  <p><strong>A cross-platform browser written in Rust, powered by <a href="https://ladybird.org/">Ladybird</a>.</strong></p>
  <p>
    <a href="https://github.com/elarisbrowser/elaris/actions/workflows/ci.yml?query=branch%3Amain"><img src="https://github.com/elarisbrowser/elaris/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--only-5e5dfb?style=flat" alt="GPL-3.0-only" /></a>
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-76b4fb?style=flat" alt="macOS and Linux" />
    <img src="https://img.shields.io/badge/status-pre--alpha-orange?style=flat" alt="Pre-alpha" />
  </p>
</div>

<p align="center">
  <img src="assets/elaris_demo.png" alt="Elaris browser window" width="900" />
</p>

> **Pre-alpha.** Elaris is under active development. Expect rough edges and breaking changes.

## Features

- **Ladybird engine.** Web content runs in Ladybird's multiprocess engine, with Rune for the browser UI.
- **Native on macOS and Linux.** Both platforms are first-class targets.
- **Tabs and windows.** Each window keeps its own tabs, and session state is restored.
- **Downloads.** Track progress, pause, resume, and retry. Downloads survive restarts.
- **History.** Search past visits, or remove one address or everything.

See [Multi-window state](docs/multi-window.md) and [Downloads](docs/downloads.md) for details.

## Quick start

You need Python 3.9+ and Git. macOS also needs [Homebrew](https://brew.sh/) and Xcode 26+.

```sh
./elaris setup             # install prerequisites and initialize Ladybird
./elaris build             # build the engine and browser (first build is slow)
./elaris run https://example.com
```

`setup` installs system packages and Rust toolchains. On Linux it supports apt, dnf, pacman, and zypper, and may ask for your password through `sudo`.

For UI work without building the engine, use `./elaris run --ui`. Add `--release` to `build` or `run` for an optimised build (`./elaris run --release`), which launches from `build/ElarisRelease.app` on macOS.

### Shortcuts

| Action | macOS | Linux |
| --- | --- | --- |
| Open history | <kbd>Command</kbd> + <kbd>Y</kbd> | <kbd>Control</kbd> + <kbd>H</kbd> |

On Linux, the toolbar slides in when the pointer reaches the top of the window. Set `ELARIS_LINUX_TITLEBAR=always` to keep it visible.

## Development

```sh
./elaris check             # fmt, clippy, and tests
./elaris format            # format Rust and bridge C++
./elaris linux-build       # build for Linux in Docker
./elaris bundle            # release build with its icon: build/ElarisRelease.app (macOS) or build/linux/Elaris (Linux)
```

On Linux, copy `build/linux/Elaris` into `~/.local` (or `/usr`) to install the launcher entry and icon.

Run `./elaris check` before opening a pull request. See [CONTRIBUTING.md](CONTRIBUTING.md) for workflow and [SETUP.md](SETUP.md) for full environment details.

## Documentation

| Topic | Read |
| --- | --- |
| Architecture | [docs/architecture.md](docs/architecture.md) |
| Ladybird integration | [docs/ladybird-embedding.md](docs/ladybird-embedding.md) |
| Development setup | [SETUP.md](SETUP.md) |
| C++ bridge | [bridge/README.md](bridge/README.md) |
| Contributing | [CONTRIBUTING.md](CONTRIBUTING.md) |

## License

Elaris is licensed under [GPL-3.0-only](LICENSE). Contributions are welcome under the same license. See [TRADEMARKS.md](TRADEMARKS.md) for branding guidance.
