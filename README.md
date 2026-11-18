<div align="center">
  <img src="assets/brand-kit/logo/logo-color.svg" alt="elaris logo" width="112" />
  <h1>elaris</h1>
  <p>A cross-platform browser built in Rust, powered by Ladybird.</p>
  <p>
    <a href="https://github.com/elarisbrowser/elaris/actions/workflows/ci.yml?query=branch%3Amain">
      <img src="https://github.com/elarisbrowser/elaris/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status" />
    </a>
    <a href="LICENSE">
      <img src="https://img.shields.io/badge/license-GPL--3.0--only-5e5dfb?style=flat" alt="GPL-3.0-only license" />
    </a>
    <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-76b4fb?style=flat" alt="macOS and Linux" />
  </p>
</div>

## Get started

Install Python 3.9 or newer and Git first. On macOS, you also need [Homebrew](https://brew.sh/) and Xcode 26 or newer. On Linux, use a supported package manager (apt, dnf, pacman, or zypper) and a desktop session with a working GPU driver.

From the repository root:

```sh
./elaris setup                         # install prerequisites and set up Ladybird
./elaris build                         # build the engine, bridge, and browser
./elaris run https://example.com       # build if needed, then open a page
```

On Linux, setup uses `sudo` when it needs to install packages, so your terminal will ask for your password. The first engine build downloads and compiles Ladybird's dependencies and can take a while. See [Development setup](SETUP.md) for platform details and troubleshooting.

To work on the browser shell without building Ladybird, run `./elaris build --ui` or `./elaris run --ui`. A direct `cargo run` also stays independent of the engine.

## Development

| Command | Purpose |
| --- | --- |
| `./elaris check` | Run formatting, Rust checks, Clippy, and tests |
| `./elaris format` | Format Rust and Elaris-owned C++ sources |
| `./elaris engine status` | Show the pinned Ladybird revision and local build status |
| `./elaris run --ui` | Launch the Rune interface without Ladybird |

Use `ELARIS_JOBS` or pass `--jobs N` to `build` or `run` to control build parallelism. Use `./elaris setup --skip-system-deps` if you installed system packages yourself; setup still verifies them.

## Documentation

- [Contributing](CONTRIBUTING.md) — workflow, code guidelines, and checks before a pull request.
- [Development setup](SETUP.md) — prerequisites, supported platforms, and build troubleshooting.
- [Architecture](docs/architecture.md) — application modules, engine thread, and bridge.
- [Ladybird integration](docs/ladybird-embedding.md) — embedding constraints and design notes.
- [Bridge development](bridge/README.md) — C++ bridge, FFI, and integration checks.
- [Trademark policy](TRADEMARKS.md) — use of the elaris name and branding.

<details>
<summary>Inspect the Rune UI without opening a window</summary>

Rune can render a screenshot, export the accessibility and layout tree, or run a scripted interaction:

```sh
./elaris run --ui -- --snap /tmp/elaris.png
./elaris run --ui -- --tree /tmp/elaris-tree.json
./elaris run --ui -- --script interaction.json --frames 120 --trace /tmp/elaris-trace.jsonl
```

These tools inspect the UI only; they do not load pages through Ladybird. See [Rune's agent tooling guide](https://github.com/anomalyco/rune/blob/dev/crates/rune/README.md#agent-tooling) for options and script formats.

</details>

elaris is licensed under [GPL-3.0-only](LICENSE). Contributions are accepted under the same license without a CLA. Rune is MIT-licensed and Ladybird is BSD-2-Clause.
