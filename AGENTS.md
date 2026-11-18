# Elaris — Agent Guidelines

Elaris is a cross-platform browser written in Rust. It uses Rune for its desktop UI and will embed upstream Ladybird as its web engine. macOS and Linux are both first-class targets.

## Principles

- Keep the code simple, readable, and maintainable.
- Prefer small, focused modules over unnecessary crates or abstractions.
- Follow existing conventions and use Rune's built-in APIs where possible.
- Keep application state separate from UI rendering.
- Avoid platform-specific code in shared browser logic.
- Make focused changes; don't refactor unrelated code.

## Ladybird

- Use a pinned upstream Ladybird revision; do not maintain a separate fork.
- Keep integration code in Elaris, outside Ladybird's source tree.
- Only add small, documented patches when strictly necessary.
- Do not modify vendored dependencies directly.

## Workflow

- Check the existing code and relevant documentation before changing anything.
- Keep `cargo run` working without requiring Ladybird until integration is explicitly enabled.
- Keep `Cargo.toml` valid TOML 1.0: use dependency subtables or single-line inline tables, not multiline inline tables. Validate manifest edits with `cargo metadata --locked --no-deps --format-version 1` before finishing.
- Run `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked` after changes.
- Test on macOS and Linux where possible; clearly state what was not tested.
- Summarize what changed and note any limitations or follow-up work.

Don't add features, dependencies, or complexity beyond what's needed for the task.

## Commit Messages

- Use Conventional Commits for every commit so the history can support automated changelogs and versioning.
- Format: `<type>[optional scope][!]: <description>`. The type is required; use a concise description immediately after the colon and space.
- Use `feat` for a new feature and `fix` for a bug fix. Other types such as `build`, `chore`, `ci`, `docs`, `perf`, `refactor`, `style`, and `test` are allowed when they describe the change.
- A scope is optional and should name the affected area, for example `feat(parser): add array parsing`.
- A body is optional and starts after a blank line. Footers are optional, follow a blank line, and use trailer syntax such as `Refs: #123`.
- Mark breaking API changes with `!` before the colon, a `BREAKING CHANGE: <description>` footer, or both. Keep `BREAKING CHANGE` uppercase.
- `feat` normally indicates a MINOR version change, `fix` a PATCH, and a breaking change a MAJOR. Other types have no implied version effect unless they include a breaking change.

Examples:

```text
feat: add tab search
fix(parser): handle empty input
docs!: restructure configuration guide

BREAKING CHANGE: configuration keys have moved under `browser`.
```

After each implementation, commit all changes in logical groups.
