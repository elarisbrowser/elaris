// SPDX-License-Identifier: GPL-3.0-only

//! Still grain over the glass: one small noise tile, held once and drawn at device pixels across
//! the window. Random noise has no edge to match, so the repeats show no seam, and at a low
//! opacity the tile's period is hard to see.

use std::cell::OnceCell;

use rune::{El, Pixels, el};

/// The opacity of the grain at full strength.
const MAX_OPACITY: f32 = 0.08;
/// A fixed seed, so the grain is the same on every launch and never moves.
const SEED: u32 = 0x2545_F491;
/// The tile's side in device pixels: 256 KiB of RGBA, shared by every grain element.
const TILE: u32 = 256;

thread_local! {
    /// The one noise tile, made on first use and reused by every window and repaint.
    static TILE_PIXELS: OnceCell<Pixels> = const { OnceCell::new() };
}

/// The grain over a `width` by `height` window in logical pixels at `scale`, `grain` out of 100.
pub(crate) fn layer(grain: u8, width: f32, height: f32, scale: f32) -> El {
    let tile = TILE_PIXELS.with(|cell| cell.get_or_init(|| noise_pixels(TILE)).clone());
    let step = TILE as f32 / scale;
    let (columns, rows) = tile_count(width, height, scale);
    let mut layer = el()
        .key("window-grain")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .opacity(f32::from(grain.min(100)) / 100.0 * MAX_OPACITY)
        .a11y_hidden();
    for row in 0..rows {
        for column in 0..columns {
            let cell = el()
                .absolute(column as f32 * step, row as f32 * step)
                .size(step, step)
                .pixels(&tile);
            layer = layer.child(cell);
        }
    }
    layer
}

/// How many tiles cover a `width` by `height` window in logical pixels at `scale`, as columns and rows.
fn tile_count(width: f32, height: f32, scale: f32) -> (u32, u32) {
    let step = TILE as f32 / scale;
    (
        (width / step).ceil().max(1.0) as u32,
        (height / step).ceil().max(1.0) as u32,
    )
}

/// Black and white pixels from a xorshift generator, taking each pixel's shade from the top bit:
/// the low bits of xorshift repeat patterns.
fn noise_pixels(side: u32) -> Pixels {
    let mut state = SEED;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for _ in 0..side * side {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let shade = if state >> 31 == 0 { 0 } else { 255 };
        rgba.extend_from_slice(&[shade, shade, shade, 255]);
    }
    let pixels = Pixels::new();
    pixels.set(side, side, rgba);
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_is_square_and_the_same_every_time() {
        let first = noise_pixels(TILE);
        assert_eq!(first.size(), (TILE, TILE));
        assert_eq!(first.rgba(), noise_pixels(TILE).rgba());
    }

    #[test]
    fn tiles_cover_the_window_at_device_pixels() {
        // At 2x, each tile spans 128 logical points, so 800 by 600 needs 7 by 5 tiles.
        assert_eq!(tile_count(800.0, 600.0, 2.0), (7, 5));
        assert_eq!(tile_count(0.0, 0.0, 1.0), (1, 1));
    }
}
