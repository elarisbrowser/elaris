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
const BUILD_BADGE_HEIGHT: f32 = 18.0;
const BUILD_BADGE_RIGHT_OVERHANG: f32 = 3.0;
const BUILD_BADGE_BOTTOM_OVERHANG: f32 = 2.0;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    vec![identity(cx.theme), acknowledgements(cx.theme)]
}

fn identity(theme: &Theme) -> El {
    let channel = BuildChannel::current();
    let brand = row()
        .key("about-brand")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(brand_mark(channel, theme))
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

fn brand_mark(channel: BuildChannel, theme: &Theme) -> El {
    let label = channel.badge_label();
    let description = label.map_or_else(
        || "Elaris logo".to_owned(),
        |label| format!("Elaris {label} build logo"),
    );
    let mut mark = el()
        .key("about-logo")
        .size(LOGO_SIZE, LOGO_SIZE)
        .center()
        .pixels(elaris_logo(channel.uses_yellow_logo()))
        .role("img")
        .label(description);
    if let Some(label) = label {
        mark = mark.child(
            row()
                .key("about-build-badge")
                .pin(
                    None,
                    None,
                    Some(-BUILD_BADGE_RIGHT_OVERHANG),
                    Some(-BUILD_BADGE_BOTTOM_OVERHANG),
                )
                .h(BUILD_BADGE_HEIGHT)
                .px(space::S2)
                .center_y()
                .justify_center()
                .radius(theme::PAGE_GROUP_RADIUS)
                .continuous_corners()
                .fill(theme.build_badge_fill)
                .border(1.0, theme.build_badge_edge)
                .a11y_hidden()
                .child(
                    text(label)
                        .font(rune::TextStyle::new(10.0, 700.0).lh(12.0))
                        .color(theme.build_badge_text)
                        .single_line(),
                ),
        );
    }
    mark
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

fn elaris_logo(yellow: bool) -> &'static rune::Pixels {
    static BLUE_LOGO: OnceLock<Image> = OnceLock::new();
    static YELLOW_LOGO: OnceLock<Image> = OnceLock::new();
    let logo = if yellow {
        YELLOW_LOGO.get_or_init(|| load_logo(include_bytes!(
            "../../../../assets/brand-kit/apple/Icon Exports/Icon-Dev.icon/Assets/logo-color.svg"
        )))
    } else {
        BLUE_LOGO.get_or_init(|| {
            load_logo(include_bytes!(
                "../../../../assets/brand-kit/logo/logo-color.svg"
            ))
        })
    };
    logo.pixels()
}

fn load_logo(bytes: &[u8]) -> Image {
    let cache = ImageCache::new(Arc::new(|| {}));
    let image = cache.load(bytes);
    image.wait().expect("bundled Elaris logo must decode");
    image
}

#[derive(Clone, Copy)]
enum BuildChannel {
    Stable,
    Alpha,
    Dev,
    Canary,
}

impl BuildChannel {
    fn current() -> Self {
        #[cfg(target_os = "macos")]
        if let Some(channel) = crate::platform::macos::dock_tag::channel().and_then(Self::from_name)
        {
            return channel;
        }

        Self::from_name(env!("ELARIS_CHANNEL")).unwrap_or(Self::Stable)
    }

    fn from_name(channel: &str) -> Option<Self> {
        match channel {
            "stable" => Some(Self::Stable),
            "alpha" => Some(Self::Alpha),
            "dev" => Some(Self::Dev),
            "canary" => Some(Self::Canary),
            _ => None,
        }
    }

    fn uses_yellow_logo(self) -> bool {
        matches!(self, Self::Dev | Self::Canary)
    }

    fn badge_label(self) -> Option<&'static str> {
        match self {
            Self::Alpha => Some("ALPHA"),
            Self::Dev => Some("DEV"),
            Self::Stable | Self::Canary => None,
        }
    }
}
