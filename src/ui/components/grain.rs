// SPDX-License-Identifier: GPL-3.0-only

//! Still grain over the glass: noise generated at the window's device size, so each speck is one
//! device pixel and nothing is stretched, at a low opacity.

use std::cell::RefCell;

use rune::{El, Pixels, el};

/// The opacity of the grain at full strength.
pub(crate) const MAX_OPACITY: f32 = 0.08;
/// A fixed seed, so the grain is the same on every launch and never moves.
pub(crate) const SEED: u32 = 0x2545_F491;

thread_local! {
    /// The last noise made, with its device size: a resize regenerates it, a repaint reuses it.
    static NOISE: RefCell<Option<((u32, u32), Pixels)>> = const { RefCell::new(None) };
}

/// The grain over a `width` by `height` window in logical pixels at `scale`, `grain` out of 100.
pub(crate) fn layer(grain: u8, width: f32, height: f32, scale: f32) -> El {
    el().key("window-grain")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .opacity(f32::from(grain.min(100)) / 100.0 * MAX_OPACITY)
        .a11y_hidden()
        .pixels(&noise(device_size(width, height, scale)))
}

pub(crate) fn device_size(width: f32, height: f32, scale: f32) -> (u32, u32) {
    let device = |logical: f32| (logical * scale).round().max(1.0) as u32;
    (device(width), device(height))
}

/// The noise at `size`, reused while the size holds.
pub(crate) fn noise(size: (u32, u32)) -> Pixels {
    NOISE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, pixels)) = cache.as_ref()
            && *cached == size
        {
            return pixels.clone();
        }
        let pixels = noise_pixels(size);
        *cache = Some((size, pixels.clone()));
        pixels
    })
}

/// Black and white pixels from a xorshift generator, taking each pixel's shade from the top bit:
/// the low bits of xorshift repeat patterns.
pub(crate) fn noise_pixels((width, height): (u32, u32)) -> Pixels {
    let mut state = SEED;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let shade = if state >> 31 == 0 { 0 } else { 255 };
        rgba.extend_from_slice(&[shade, shade, shade, 255]);
    }
    let pixels = Pixels::new();
    pixels.set(width, height, rgba);
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_matches_the_window_and_is_the_same_every_time() {
        let first = noise_pixels((64, 48));
        assert_eq!(first.size(), (64, 48));
        assert_eq!(first.rgba(), noise_pixels((64, 48)).rgba());
    }

    #[test]
    fn device_size_follows_the_scale_and_never_reaches_zero() {
        assert_eq!(device_size(800.0, 600.0, 2.0), (1600, 1200));
        assert_eq!(device_size(0.2, 0.0, 1.0), (1, 1));
    }
}
