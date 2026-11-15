<p align="center"><img src="../elaris-brand/assets/logo.svg" alt="Photon" width="72" /></p>

# `elaris-cli`

The command-line interface for setting up pinned checkouts, building and running Photon, and maintaining upstream dependencies.

## Use

Install [Rust with rustup](https://rustup.rs) first; `./elaris` runs Cargo and cannot install it. See the [contributor setup guide](../../CONTRIBUTOR_SETUP.md) for platform prerequisites. Then bootstrap from the repository root:

```sh
./elaris setup
```

Setup installs or refreshes `elaris` and its `el` alias in Cargo's bin directory. Full Linux setup also registers the per-user desktop entry and web URL handlers; `--ui-only` does not install or update them. Make sure `~/.cargo/bin` is on `PATH`, then use either command inside the checkout:

```sh
elaris doctor
elaris run --url https://ladybird.org
elaris check
elaris test
elaris deps status
```

For example, `el check` and `el run` work the same as `elaris check` and `elaris run`.

Build and run commands use optimized builds by default; pass `--debug` for
faster, unoptimized builds. `elaris setup --install-system-deps` on macOS
installs build packages with Homebrew; on Linux, install packages from the
contributor guide first. `elaris install` refreshes only the CLI binaries. Full
Linux setup registers desktop integration automatically; the `elaris desktop`
commands are for manual refresh or removal. `elaris desktop uninstall` removes
the launcher and icon without removing the CLI. `elaris deps status`,
`elaris deps sync`, `elaris deps restore`, and `elaris deps pin` cover the
common submodule workflow. Command behavior lives in one module per command
under [`src/commands`](src/commands/).
