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
- Run `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked` after changes.
- Test on macOS and Linux where possible; clearly state what was not tested.
- Summarize what changed and note any limitations or follow-up work.

Don't add features, dependencies, or complexity beyond what's needed for the task.

After each implementation you should commit all your changes in groups.
