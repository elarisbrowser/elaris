# Contributing to Elaris

## Get running

From the repository root, run `./elaris setup --ui --run` for interface work, or `./elaris setup --run` to run with Ladybird. See [Development setup](SETUP.md) for prerequisites and Docker alternatives.

## Check your change

```sh
./elaris check
```

This runs Rust formatting checks, type checking, Clippy, Rust tests, and the developer CLI unit tests. For C++ bridge changes, run `./elaris cpp-format` and `./elaris cpp-tidy` after configuring the bridge, then follow the [bridge test guide](bridge/README.md).

Explain what changed and how you checked it in your pull request. Mention which platforms you ran on and any limitations. Include screenshots for visible UI changes if useful.

## Work in the right place

- `src/`: Rust application, UI, browser state, and engine-thread integration.
- `crates/`: shared Rust libraries.
- `bridge/`: Elaris's C ABI bridge to Ladybird.
- `tools/elaris_cli/`: the `./elaris` developer commands.

Keep browser state separate from Rune rendering, and isolate platform-specific code. Reuse existing Rune APIs and avoid unrelated refactors. Keep ordinary `cargo run` working without Ladybird.

Ladybird follows the revision pinned by the repository. Don't edit `vendor/ladybird` directly; keep Elaris's integration and any documented compatibility patches in this repository. See [Architecture](docs/architecture.md) and [Ladybird integration](docs/ladybird-embedding.md).

## License

Elaris accepts contributions under [GPL-3.0-only](LICENSE), without a CLA or copyright assignment. Third-party components keep their own licenses. See [Trademarks](TRADEMARKS.md) for branding guidance.
