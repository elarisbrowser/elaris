// SPDX-License-Identifier: GPL-3.0-only

//! Small previews sampled from RGBA frames already copied for a visible viewport.

use std::collections::VecDeque;
use std::sync::{Arc, Weak};

use rune::Pixels;

use super::THUMBNAIL_ASPECT;
use crate::tab::TabId;

const MAX_WIDTH: usize = 320;
const CAPACITY: usize = 12;

/// Only the engine holds cache entries strongly. Pending UI events hold weak
/// references, so a blocked UI cannot accumulate a queue of image buffers.
#[derive(Default)]
pub(super) struct Cache(VecDeque<(TabId, Arc<Pixels>)>);

impl Cache {
    pub(super) fn remove(&mut self, tab: TabId) {
        self.0.retain(|(id, _)| *id != tab);
    }

    /// Announces a new image handle. Existing handles wake Rune directly when displayed.
    pub(super) fn store(&mut self, tab: TabId, sample: Sample) -> Option<Weak<Pixels>> {
        let (entry, new) = if let Some(index) = self.0.iter().position(|(id, _)| *id == tab) {
            (self.0.remove(index).unwrap(), false)
        } else {
            if self.0.len() == CAPACITY {
                self.0.pop_front();
            }
            ((tab, Arc::new(Pixels::new())), true)
        };
        entry.1.set(sample.width, sample.height, sample.rgba);
        let announced = new.then(|| Arc::downgrade(&entry.1));
        self.0.push_back(entry);
        announced
    }
}

pub(super) struct Sample {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

pub(super) fn sample(width: u32, height: u32, rgba: &[u8]) -> Option<Sample> {
    let (width, height) = (width as usize, height as usize);
    let (aspect_w, aspect_h) = (THUMBNAIL_ASPECT.0 as usize, THUMBNAIL_ASPECT.1 as usize);
    let crop_units = (width / aspect_w).min(height / aspect_h);
    if crop_units == 0 {
        return None;
    }
    let units = crop_units.min(MAX_WIDTH / aspect_w);
    let (w, h) = (units * aspect_w, units * aspect_h);
    let (crop_w, crop_h) = (crop_units * aspect_w, crop_units * aspect_h);
    let left = (width - crop_w) / 2;

    // Frame the top of a tall page, centre a wide page, and reuse the source column
    // offsets for every row. Sampling work depends on thumbnail size alone.
    let mut preview = Vec::with_capacity(w * h * 4);
    let row_start = |y| (2 * y + 1) * crop_h / (2 * h) * width * 4 + left * 4;
    if crop_w == w {
        for y in 0..h {
            let start = row_start(y);
            preview.extend_from_slice(&rgba[start..start + w * 4]);
        }
    } else {
        let mut columns = [0; MAX_WIDTH];
        for (x, column) in columns[..w].iter_mut().enumerate() {
            *column = (2 * x + 1) * crop_w / (2 * w) * 4;
        }
        for y in 0..h {
            let row = row_start(y);
            let source = &rgba[row..row + crop_w * 4];
            for &column in &columns[..w] {
                preview.extend_from_slice(&source[column..column + 4]);
            }
        }
    }
    Some(Sample {
        width: w as u32,
        height: h as u32,
        rgba: preview,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_tall_panes_from_the_top_and_centres_wide_panes() {
        let mut tall = vec![0; 320 * 720 * 4];
        tall[..320 * 180 * 4].fill(127);
        let tall = sample(320, 720, &tall).unwrap();
        assert_eq!((tall.width, tall.height), (320, 180));
        assert_eq!(tall.rgba, vec![127; 320 * 180 * 4]);

        let mut wide = vec![0; 32 * 9 * 4];
        for row in wide.chunks_exact_mut(32 * 4) {
            row[8 * 4..24 * 4].fill(127);
        }
        let wide = sample(32, 9, &wide).unwrap();
        assert_eq!((wide.width, wide.height), (16, 9));
        assert_eq!(wide.rgba, vec![127; 16 * 9 * 4]);
        let large = sample(640, 480, &vec![75; 640 * 480 * 4]).unwrap();
        assert_eq!((large.width, large.height), (320, 180));
        assert_eq!(large.rgba, vec![75; 320 * 180 * 4]);
        assert!(sample(1, 2, &[0; 8]).is_none());
    }

    #[test]
    fn refreshes_the_same_handle_and_evicts_the_oldest_sample() {
        let mut cache = Cache::default();
        let put = |cache: &mut Cache, id| {
            cache.store(
                TabId::from_raw(id),
                sample(16, 9, &[0; 16 * 9 * 4]).unwrap(),
            )
        };
        let first = put(&mut cache, 0).unwrap();
        let second = put(&mut cache, 1).unwrap();
        for id in 2..CAPACITY as u64 {
            put(&mut cache, id).unwrap();
        }
        assert!(put(&mut cache, 0).is_none());
        put(&mut cache, CAPACITY as u64).unwrap();
        assert!(first.upgrade().is_some());
        assert!(second.upgrade().is_none());
        assert_eq!(cache.0.len(), CAPACITY);
        cache.remove(TabId::from_raw(0));
        assert!(first.upgrade().is_none());
    }
}
