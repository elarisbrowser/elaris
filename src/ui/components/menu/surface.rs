// SPDX-License-Identifier: GPL-3.0-only

//! The panel every menu, popover and dropdown draws, its entrance, where it sits beside its
//! trigger, and how it is dismissed.

use rune::{
    Align, Color, El, Key, KeyEvent, Side, Spring, SpringPair, Transition, Tx, Vec2, col, el,
};

use super::Apply;
use crate::ui::theme::{self, MENU_SCALE_FROM, Theme};

/// Rows rise and fade in after the panel, one after another; they keep their place under reduced
/// motion, where the stagger is dropped.
pub(super) fn item_transition(reduced: bool) -> Transition {
    if reduced {
        Transition::none()
    } else {
        Transition::offset(0.0, theme::MENU_ROW_OFFSET).combined(Transition::opacity())
    }
}

/// What a menu or popover shares with its trigger: where it sits and how it is dismissed.
pub(super) struct Anchored {
    pub(super) key: String,
    pub(super) trigger: Option<El>,
    pub(super) open: bool,
    pub(super) at: Option<(f32, f32)>,
    pub(super) side: Side,
    pub(super) align: Align,
    pub(super) dismiss: Option<(El, Apply)>,
}

impl Anchored {
    /// Returns the trigger in place, and adds the scrim and `content` to `layer` when open.
    pub(super) fn attach(mut self, layer: &mut Vec<El>, content: impl FnOnce(Vec2) -> El) -> El {
        let anchor_key = format!("{}-anchor", self.key);
        let pointer = self.at.is_some();
        let trigger = self
            .trigger
            .take()
            .map_or_else(el, |trigger| trigger.expanded(self.open && !pointer));
        let anchor = el().key(anchor_key.clone()).child(trigger);
        if !self.open {
            return anchor;
        }

        let (anchor_at, gap) = match self.at {
            Some((x, y)) => {
                let point = format!("{}-point", self.key);
                layer.push(el().key(point.clone()).absolute(x, y).size(0.0, 0.0));
                (point, 0.0)
            }
            None => (anchor_key.clone(), theme::MENU_GAP),
        };
        let (scrim, escape) = match self.dismiss.take() {
            Some((scrim, escape)) => (Some(scrim), Some(escape)),
            None => (None, None),
        };
        layer.extend(scrim);

        let mut panel = content(origin(self.side, self.align));
        if let Some(escape) = escape {
            panel = escape(panel);
        }
        layer.push(
            panel
                .anchored_fit(anchor_at, self.side, self.align, gap)
                .focus_return(anchor_key),
        );
        anchor
    }
}

/// A scrim that closes the menu on a press outside it, and the Escape handler for its panel.
pub(crate) fn dismissal<S: 'static>(
    key: &str,
    close: impl Fn(&mut S) + Clone + 'static,
) -> (El, Apply) {
    let scrim = el()
        .key(format!("{key}-scrim"))
        .role("none")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .on_press(close.clone());
    let escape: Apply = Box::new(move |panel: El| {
        panel.on_key(move |state: &mut S, event: &KeyEvent, _: &mut Tx| {
            if event.key == Key::Escape {
                close(state);
                true
            } else {
                false
            }
        })
    });
    (scrim, escape)
}

/// The rounded container every menu, popover and dropdown panel draws.
pub(crate) fn surface(
    key: &str,
    role: &'static str,
    min_width: f32,
    reveal: Transition,
    fill: Color,
    theme: &Theme,
    content: El,
) -> El {
    col()
        .key(format!("{key}-menu"))
        .role(role)
        .focus_group()
        .min_w(min_width)
        .items_stretch()
        .padding(theme::MENU_INSET)
        .radius(theme::MENU_RADIUS)
        .continuous_corners()
        .fill(fill)
        .border(1.0, theme.overlay_border)
        .shadow(
            theme::MENU_SHADOW_Y,
            theme::MENU_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .transition(reveal)
        .child(content)
}

/// Where a panel grows from: the edge that touches its trigger, and the side of that edge it
/// aligns to. Matches Rune's overlay origin so a menu opens from its trigger.
pub(crate) fn origin(side: Side, align: Align) -> Vec2 {
    let along = match align {
        Align::Start => 0.0,
        Align::Center => 0.5,
        Align::End => 1.0,
    };
    match side {
        Side::Top => Vec2::new(along, 1.0),
        Side::Bottom => Vec2::new(along, 0.0),
        Side::Left => Vec2::new(1.0, along),
        Side::Right => Vec2::new(0.0, along),
    }
}

/// A panel scales in from 0.97 at its trigger edge while it fades in, and fades out in place.
/// The rows follow it, so the two move the same way.
pub(crate) fn reveal(reduced: bool, origin: Vec2) -> Transition {
    if reduced {
        return Transition::none();
    }
    Transition::asymmetric(
        Transition::scale_anchor(MENU_SCALE_FROM, origin),
        Transition::opacity(),
    )
    .springs(SpringPair::new(
        Spring::new(0.2, 0.0),
        Spring::new(0.15, 0.0),
    ))
}
