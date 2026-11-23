# Rune fork

Elaris maintains a Rune fork so framework fixes and platform improvements can be tested
against Elaris before release. Everything lives on the fork's `dev` branch: upstream's
history plus Elaris's commits on top. Commit Rune changes straight to `dev` (no long-lived
feature branches), keep them useful to any Rune app, and avoid browser concepts in Rune
APIs. Contribute generally useful fixes upstream when the project accepts them.

- Official upstream: <https://github.com/anomalyco/rune>
- Elaris fork: <https://github.com/elarisbrowser/rune>
- Branch: `dev` on both

## Synchronise upstream

From a clone of the fork, remotes are `origin` (the fork) and `upstream` (official Rune),
and `dev` tracks `origin/dev`. Review what upstream added, then merge it:

```sh
git fetch upstream
git log --oneline dev..upstream/dev
git diff --stat dev...upstream/dev
git switch dev
git merge --no-ff upstream/dev
```

Resolve conflicts by keeping Elaris's reviewed changes and the intended upstream behaviour.
Run Rune's checks (`bash scripts/check-local.sh`, and the tests its `AGENTS.md` lists), then
Elaris's validation below. Push `dev` only after both projects pass. Never rebase or
force-push `dev`; apps pin its commits.

## Update Elaris's pin

After the fork's `dev` branch passes both projects' checks, pin it:

```sh
./elaris pin              # the head of the fork's dev branch
./elaris pin <branch|tag|full-commit>
```

It rewrites the shared revision of `rune`, `rune-kit` and every Rune-owned
`[patch.crates-io]` entry in `Cargo.toml`, re-locks only those sources, verifies
`cargo metadata --locked`, and runs `cargo check --locked` (skip with `--no-check`). It
refuses while a local `.cargo/config.toml` patches Rune.

It also pins Ladybird: it stages `vendor/ladybird` at its current checkout, or at an
upstream ref fetched with `--ladybird <branch|tag|commit>`, after checking the checkout
is clean and Elaris's patches in `bridge/patches` still apply. Then run the remaining checks:

```sh
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
```

To develop against a local clone first, add a git-ignored `.cargo/config.toml`:

```toml
[patch."https://github.com/elarisbrowser/rune"]
rune = { path = "../rune/crates/rune" }
rune-kit = { path = "../rune/crates/rune-kit" }
```

The hot-reload launcher lookup in `tools/elaris_cli/project.py` reads the pinned
revision from `cargo metadata`, so it follows the pin.

## Window API investigation

The current Elaris pin (`cca5df6`) already exports `rune::open_window(WindowSpec)` for
runtime creation and `WindowHandle` for opening/closing a window. It also has
`App::window` for windows registered with the app. Therefore, the claim that this pin
lacks runtime window creation is not borne out by its source. Elaris does not use those
APIs today. Before adopting them, review Rune's generic window queue and ownership in
`crates/rune/src/window/spec.rs` and `window.rs`, event-loop integration in
`window/event_loop.rs`, per-window host construction in `window/host.rs`, and the native
platform window implementations. Test lifecycle, focus/activation, Wayland recreation,
macOS AppKit behavior, and Intel macOS support. Keep any future API changes useful to
general Rune applications.

## Contributing upstream

Rune's README says it does not take pull requests. For a useful general fix or idea, open
an issue at <https://github.com/anomalyco/rune/issues> describing the problem or proposal;
Rune's agents usually implement it. Keep Elaris-specific integration changes on the fork.
