# 001 — Shorten Shared Highlight Motion

- **Status**: DONE
- **Commit**: a0f2da3
- **Severity**: LOW
- **Category**: Purpose & frequency
- **Estimated scope**: 1 file, 1 line

## Problem

The shared highlight spring is used for pointer hover highlights and selection changes. Both recur during ordinary browsing, so the current 0.14 spring can make small pointer movements more noticeable than needed.

`src/ui/theme.rs:254` currently contains:

```rust
pub(super) const HIGHLIGHT_MOTION: Spring = Spring::new(0.14, 0.0);
```

The token is used by `src/ui/hover_highlight.rs` for the moving control/tab highlight and selection highlight, and by `src/ui/tab_switcher.rs` for quick-switch selection feedback.

## Target

Keep the same shared token and spring shape, but change its first parameter from `0.14` to `0.10`. This shortens settling time while preserving spatial feedback and keeping reduced-motion branches unchanged.

```rust
pub(super) const HIGHLIGHT_MOTION: Spring = Spring::new(0.10, 0.0);
```

## Repo conventions to follow

- Shared motion values live in `src/ui/theme.rs` as named constants.
- Components use `theme::HIGHLIGHT_MOTION` and retain their existing reduced-motion branches.

## Steps

1. In `src/ui/theme.rs`, update only `HIGHLIGHT_MOTION` from `Spring::new(0.14, 0.0)` to `Spring::new(0.10, 0.0)`.

## Boundaries

- Do not change component behavior, reduced-motion handling, or any other motion token.
- Do not edit unrelated files or add dependencies.
- If the constant no longer matches the excerpt, stop and report the drift.

## Verification

- **Mechanical**: run `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked`.
- **Feel check**: move the pointer across the toolbar and tab rows, then cycle tabs with the quick switcher. Confirm highlights still track the target but settle more promptly. Enable reduced motion and confirm the highlights still snap immediately.
- **Done when**: the token is 0.10, all verification commands pass, and the interaction remains clear at normal speed.
