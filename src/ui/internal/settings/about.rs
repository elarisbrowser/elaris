// SPDX-License-Identifier: GPL-3.0-only

//! Elaris branding, build versions and open source acknowledgements.

use std::sync::{Arc, OnceLock};

use rune::tok::{space, type_};
use rune::{El, Image, ImageCache, Link, LinkPaint, Span, col, el, rich, row, text};

use crate::app::AppState;
use crate::ui::components::widgets;
use crate::ui::internal::PageContext;
use crate::ui::theme::{self, Theme};

use super::versions;

const LADYBIRD_URL: &str = "https://ladybird.org";
const LOGO_SIZE: f32 = 48.0;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    vec![identity(cx.theme), acknowledgements(cx.theme)]
}

fn identity(theme: &Theme) -> El {
    let brand = row()
        .key("about-brand")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(
            el().size(LOGO_SIZE, LOGO_SIZE)
                .center()
                .pixels(elaris_logo())
                .a11y_hidden(),
        )
        .child(
            col().grow(1.0).min_w(0.0).child(
                text("Elaris")
                    .font(theme::PAGE_TITLE)
                    .color(theme.text)
                    .wrap()
                    .role("heading"),
            ),
        );
    let version = text(versions::summary())
        .key("about-version")
        .font(type_::BODY)
        .color(theme.text_muted)
        .wrap();

    widgets::rows(theme, [brand, version])
}

fn acknowledgements(theme: &Theme) -> El {
    let copy = "Elaris is made possible by the Ladybird open source project and other open source software.";
    let start = copy.find("Ladybird").expect("credit contains Ladybird");
    let end = start + "Ladybird".len();
    let credit = rich(
        copy,
        vec![
            Span::new(start, end)
                .color(theme.accent)
                .underline(theme.accent.alpha(0.45)),
        ],
    )
    .font(type_::BODY)
    .color(theme.text_muted)
    .link(
        Link::new(start..end, LADYBIRD_URL)
            .help("Open Ladybird in your default browser")
            .hover(LinkPaint::new().color(theme.accent).underline(theme.accent)),
    )
    .on_link(|_: &mut AppState, url, tx| {
        let _ = tx.open_url(url);
    });

    widgets::rows(theme, [credit])
}

fn elaris_logo() -> &'static rune::Pixels {
    static LOGO: OnceLock<Image> = OnceLock::new();
    let image = LOGO.get_or_init(|| {
        let cache = ImageCache::new(Arc::new(|| {}));
        let image = cache
            .load(include_bytes!("../../../../assets/brand-kit/logo/logo-color.svg").as_slice());
        image.wait().expect("bundled Elaris logo must decode");
        image
    });
    image.pixels()
}
