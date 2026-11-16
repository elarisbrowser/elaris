<p align="center"><img src="crates/elaris-brand/assets/brand-kit/logo/logo.svg" alt="Elaris" width="96" /></p>

<h1 align="center">Elaris</h1>

<p align="center"><strong>A browser built on <a href="https://ladybird.org">Ladybird</a>.</strong></p>

<p align="center">
  <a href="#run-from-source">Run from source</a> ·
  <a href="Documentation/Architecture.md">Architecture</a> ·
  <a href="Documentation/Roadmap.md">Roadmap</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<img width="2592" height="1644" alt="image" src="https://github.com/user-attachments/assets/59500315-d983-4af4-b105-4d102498ccc7" />


## Run from source

Install the platform prerequisites in [contributor setup](CONTRIBUTOR_SETUP.md)
first, including Rust through rustup. `./elaris` runs Cargo; it does not install
Rust.

```sh
git clone --recurse-submodules https://github.com/PhotonBrowser/photon.git
cd photon
```

On macOS, run `./elaris setup --install-system-deps`; on Linux, install the
packages listed in [contributor setup](CONTRIBUTOR_SETUP.md), then run
`./elaris setup`. Follow the platform-specific build and run guidance there
before launching Elaris.

Setup installs or refreshes the `elaris` command and `el` alias in Cargo's bin
directory. Full Linux setup also registers a per-user desktop launcher and web
URL handlers; UI-only setup does not install or update them. `elaris install`
refreshes only the CLI.
Make sure `~/.cargo/bin` is on your `PATH` to use `elaris` without the `./`
prefix.

To work on the GPUI shell on Linux without setting up Ladybird Engine, clone
without submodules and use the UI-only path:

```sh
git clone https://github.com/PhotonBrowser/photon.git
cd photon
./elaris setup --ui-only
elaris run --ui
```

This opens the New Tab page and Elaris-owned settings pages; web pages require
the full Engine setup.

## Develop Elaris

Start with [contributor setup](CONTRIBUTOR_SETUP.md), then use the [build guide](Documentation/Building.md) and [architecture guide](Documentation/Architecture.md). See [contribution guidance](CONTRIBUTING.md) for pull requests and the [roadmap](Documentation/Roadmap.md) for planned work.

```mermaid
flowchart LR
    User[You] --> App[elaris-app]
    App --> Shell[elaris-shell<br/>windows, tabs, and UI]
    Shell --> Core[elaris-core<br/>browser rules and state]
    Shell --> Storage[elaris-storage<br/>profile files]
    Shell --> Embedder[Elaris Engine API]
    Embedder --> Engine[Ladybird Engine<br/>web pages]
    Shell --> Platform[Platform adapters<br/>macOS · Linux · Windows]
    Shell --> PerfUI[elaris-performance-ui<br/>GPUI charts and overlays]
    PerfUI --> Perf[elaris-performance<br/>sampling and diagnostics]
    CLI[elaris-cli<br/>setup and developer commands]
```

The shell connects the browser interface to framework-independent browser
rules, profile storage, and the Ladybird Engine. See the
[architecture guide](Documentation/Architecture.md) for component boundaries
and common change locations.

<details>
<summary>Rust crate guides</summary>

- [elaris-app](crates/elaris-app/README.md) · [elaris-brand](crates/elaris-brand/README.md) · [elaris-cli](crates/elaris-cli/README.md) · [elaris-core](crates/elaris-core/README.md)
- [elaris-performance](crates/elaris-performance/README.md) · [elaris-performance-ui](crates/elaris-performance-ui/README.md)
- [elaris-presentation-broker](crates/elaris-presentation-broker/README.md) · [elaris-presentation-ipc](crates/elaris-presentation-ipc/README.md) · [elaris-shell](crates/elaris-shell/README.md)
- [elaris-storage](crates/elaris-storage/README.md) · [gpui-motion](crates/gpui-motion/README.md)

</details>

## Star History

<a href="https://www.star-history.com/?repos=photonbrowser%2Fphoton&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=photonbrowser/photon&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=photonbrowser/photon&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=photonbrowser/photon&type=date&legend=top-left" />
 </picture>
</a>
