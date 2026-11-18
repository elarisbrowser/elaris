# Contributing to Elaris

Thanks for helping improve Elaris. Keep changes focused, preserve the project's cross-platform behavior, and keep browser state separate from Rune rendering.

## Start here

Read [Development setup](SETUP.md), then use the root command wrapper:

```sh
./elaris setup
./elaris build
./elaris run
```

To work in a consistent Linux environment, open the repository in the optional [Linux devcontainer](.devcontainer/devcontainer.json) or run `./elaris linux-build` from the host terminal. The CLI command streams the container build output and keeps Linux artifacts separate from host builds.

`setup` installs supported system packages, Rust toolchains, and the repository-pinned Ladybird submodule. Linux package installation uses `sudo` and may ask for your password. The first `build` compiles Ladybird's dependencies and can take a while. `./elaris run --ui` and `./elaris build --ui` let you work on the Rune shell without building the engine.

## Before opening a pull request

Run the project's standard Rust checks:

```sh
./elaris check
```

This runs `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked`. If you changed bridge C++, also run:

```sh
./elaris cpp-format
./elaris cpp-tidy
```

`cpp-tidy` uses the compilation database from the Ladybird bridge build. Bridge integration and FFI checks are documented in [bridge/README.md](bridge/README.md); the full Linux bridge workflow is available as a manual GitHub Actions workflow.

In the pull request description, summarize the user-visible or internal change and list the checks you ran. Include screenshots for visible UI changes when useful. Call out platform-specific behavior or limitations.

## Code and repository boundaries

- Follow the surrounding Rust, Python, and C++ style. Prefer small modules and existing Rune APIs over new abstractions or dependencies.
- Keep shared browser behavior platform-neutral. Isolate unavoidable operating-system integration behind narrow boundaries.
- Keep Ladybird pinned to the revision recorded by the submodule pointer. Do not maintain a fork or edit `vendor/ladybird`; keep Elaris integration and any documented compatibility code in this repository.
- Keep ordinary `cargo run` working without Ladybird. The bridge is opt-in at the Cargo feature boundary; `./elaris run --ui` is the supported shell-only path.
- Avoid unrelated formatting or refactors in a focused change.

Useful entry points:

| Path | Purpose |
| --- | --- |
| `src/` | Rust application, Rune UI, browser state, and engine-thread integration |
| `bridge/` | Elaris-owned C ABI bridge to Ladybird |
| `scripts/elaris_cli/` | `./elaris` setup, build, check, and development commands |
| `docs/architecture.md` | Module map and UI-to-engine data flow |
| `docs/ladybird-embedding.md` | Ladybird integration notes and constraints |

## Licensing

Elaris is licensed under [GPL-3.0-only](LICENSE). Contributions are accepted under the same license; no CLA or copyright assignment is required. Third-party components keep their own licenses. See [TRADEMARKS.md](TRADEMARKS.md) for branding guidance.
