// SPDX-License-Identifier: GPL-3.0-only

//! The one-pixel rule that separates groups of rows.

use rune::{Color, El, el};

/// A full-width hairline filled with `color`. Callers set its padding or key as needed.
pub(crate) fn hairline(color: Color) -> El {
    el().w_full().h(1.0).fill(color)
}
