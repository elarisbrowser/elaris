# Rune fork

Elaris maintains a Rune fork so framework fixes and platform improvements can be tested
against Elaris before release. Keep it close to upstream: `dev` is the clean upstream
reference; `elaris` is the integration branch. Put generally useful changes on `elaris`
and avoid browser concepts in Rune APIs. Contribute generally useful fixes upstream when
the project accepts them.

- Official upstream: <https://github.com/anomalyco/rune>
- Elaris fork: <https://github.com/elarisbrowser/rune>
- Upstream development branch: `dev`
- Elaris integration branch: `elaris`

## Synchronise upstream

From a clone of the Rune fork, remotes are `origin` (the Elaris fork) and `upstream`
(official Rune). The local `dev` branch tracks `upstream/dev`; `elaris` tracks
`origin/elaris`.

```sh
git fetch upstream
git log --oneline --left-right elaris...upstream/dev
git diff --stat elaris...upstream/dev
git diff elaris...upstream/dev
```

Review the commits and source changes first. Integrate approved changes explicitly:

```sh
git switch dev
git merge --ff-only upstream/dev
git push origin dev
git switch elaris
git merge --no-ff dev
```

Resolve conflicts on `elaris` by preserving Elaris's reviewed changes and the intended
upstream behavior. Run Rune's documented checks from its `AGENTS.md`/CI, then run Elaris's
validation below. Push `elaris` only after both projects pass. Do not rebase or force-push
published branches, and do not merge upstream changes without review.

## Update Elaris's pin

After the fork's `elaris` branch passes both projects' checks, pin it:

```sh
./elaris pin              # the head of the fork's elaris branch
./elaris pin <branch|tag|full-commit>
```

It rewrites the shared revision of `rune`, `rune-kit` and every Rune-owned
`[patch.crates-io]` entry in `Cargo.toml`, re-locks only those sources, verifies
`cargo metadata --locked`, and runs `cargo check --locked` (skip with `--no-check`). It
refuses while a local `.cargo/config.toml` patches Rune. Then run the remaining checks:

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

The hot-reload launcher lookup in `scripts/elaris_cli/project.py` reads the pinned
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
