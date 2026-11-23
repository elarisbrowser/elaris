// SPDX-License-Identifier: GPL-3.0-only

//! The search providers' coloured brand marks. They are bitmaps, not strokes, so they are decoded once and
//! shared.

use std::sync::{Arc, OnceLock};

use rune::{El, Image, ImageCache, Pixels, el};

use elaris_core::address::SearchEngine;

static SEARCH_ENGINE_IMAGES: OnceLock<[Image; 4]> = OnceLock::new();

pub(crate) fn search_engine(engine: SearchEngine, size: f32) -> El {
    el().size(size, size)
        .center()
        .pixels(search_engine_pixels(engine))
}

fn search_engine_pixels(engine: SearchEngine) -> &'static Pixels {
    let images = SEARCH_ENGINE_IMAGES.get_or_init(|| {
        let cache = ImageCache::new(Arc::new(|| {}));
        let images = [
            cache.load(include_bytes!("../../../assets/search-engines/google.svg").as_slice()),
            cache.load(include_bytes!("../../../assets/search-engines/bing.svg").as_slice()),
            cache.load(include_bytes!("../../../assets/search-engines/duckduckgo.svg").as_slice()),
            cache.load(include_bytes!("../../../assets/search-engines/brave.png").as_slice()),
        ];
        for image in &images {
            image
                .wait()
                .expect("bundled search engine icon must decode");
        }
        images
    });

    let index = match engine {
        SearchEngine::Google => 0,
        SearchEngine::Bing => 1,
        SearchEngine::DuckDuckGo => 2,
        SearchEngine::Brave => 3,
    };
    images[index].pixels()
}
