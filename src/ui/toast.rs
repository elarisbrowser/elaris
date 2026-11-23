// SPDX-License-Identifier: GPL-3.0-only

//! The toast stack, over the window's bottom edge. Toasts slide up and pile behind the newest,
//! peeking out above it. Hovering the stack spreads them into a column; leaving it gathers them
//! back.

use rune::tok::type_;
use rune::{El, Spring, Transition, col, el, icon, row, text};

use crate::app::AppState;
use crate::toast::{Shown, Toasts};
use crate::ui::theme::{self, Theme};

/// How many toasts the collapsed stack shows; older ones sit hidden behind the deepest of these.
const COLLAPSED_SHOWN: usize = 3;

/// The stack for the toasts on screen, or `None` when there are none.
pub(crate) fn view(
    toasts: &Toasts,
    theme: &Theme,
    window_width: f32,
    reduced_motion: bool,
) -> Option<El> {
    let count = toasts.shown.len();
    if count == 0 {
        return None;
    }
    let spring = if reduced_motion {
        Spring::NONE
    } else {
        theme::TOAST_STACK
    };
    let mut stack = el()
        .key("toasts")
        .w(theme::TOAST_WIDTH)
        .h(stack_height(count, toasts.hovered))
        .pin(
            Some((window_width - theme::TOAST_WIDTH) / 2.0),
            None,
            None,
            Some(theme::TOAST_INSET),
        )
        .token_motion(spring)
        .on_hover_change(|state: &mut AppState, hovered, tx| {
            state.toasts.hover(hovered, tx.clock());
            tx.animated();
        });
    // Oldest first, so the newest draws on top.
    for (index, shown) in toasts.shown.iter().enumerate() {
        let depth = count - 1 - index;
        stack = stack.child(card(shown, depth, toasts.hovered, theme, reduced_motion));
    }
    Some(stack)
}

/// The stack's height: a column of toasts when hovered, else the newest with the rest peeking.
fn stack_height(count: usize, hovered: bool) -> f32 {
    let shown = count.min(COLLAPSED_SHOWN) as f32;
    if hovered {
        count as f32 * theme::TOAST_HEIGHT + (count - 1) as f32 * theme::TOAST_GAP
    } else {
        theme::TOAST_HEIGHT + (shown - 1.0) * theme::TOAST_PEEK
    }
}

/// Where a toast sits: its height above the stack's bottom and its size, by its depth (0 is the
/// newest). A hovered stack spreads them out at full size; a collapsed one stacks them.
fn placement(depth: usize, hovered: bool) -> (f32, f32) {
    if hovered {
        (depth as f32 * (theme::TOAST_HEIGHT + theme::TOAST_GAP), 1.0)
    } else {
        let step = depth.min(COLLAPSED_SHOWN - 1);
        (
            step as f32 * theme::TOAST_PEEK,
            1.0 - step as f32 * theme::TOAST_STACK_SHRINK,
        )
    }
}

fn card(shown: &Shown, depth: usize, hovered: bool, theme: &Theme, reduced_motion: bool) -> El {
    let id = shown.id;
    let toast = &shown.toast;
    let (lift, scale) = placement(depth, hovered);

    let mut lines = col().grow(1.0).min_w(0.0).gap(theme::TOAST_LINE_GAP).child(
        text(toast.message.clone())
            .font(type_::BODY)
            .color(theme.text)
            .single_line()
            .truncate(),
    );
    if let Some(detail) = &toast.detail {
        lines = lines.child(
            text(detail.clone())
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate(),
        );
    }
    if let Some(content) = &toast.content {
        lines = lines.child(content(theme));
    }

    let mut body = row()
        .w_full()
        .h_full()
        .center_y()
        .px(theme::TOAST_PAD_X)
        .gap(theme::TOAST_GAP);
    if let Some(glyph) = toast.icon {
        body = body.child(icon(glyph, theme::TOAST_ICON).color(theme.icon));
    }

    el().key(format!("toast-{id}"))
        .pin(Some(0.0), None, Some(0.0), Some(lift))
        .h(theme::TOAST_HEIGHT)
        .scale(scale)
        .radius(theme::TOAST_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(theme::OMNIBOX_BORDER_WIDTH, theme.overlay_border)
        .shadow(
            theme::TOAST_SHADOW_Y,
            theme::TOAST_SHADOW_BLUR,
            theme.overlay_shadow,
        )
        .role("status")
        .label(toast.message.clone())
        .help("Dismiss")
        .press(theme.press)
        .on_press_tx(move |state: &mut AppState, tx| {
            state.toasts.dismiss(id);
            tx.animated();
        })
        .transition(presence(reduced_motion))
        .token_motion(if reduced_motion {
            Spring::NONE
        } else {
            theme::TOAST_STACK
        })
        .child(body.child(lines))
}

/// Rising in from below and shrinking away, with the toast's springs. Reduced motion shows and
/// hides it at once.
fn presence(reduced_motion: bool) -> Transition {
    if reduced_motion {
        return Transition::none();
    }
    Transition::offset(0.0, theme::TOAST_RISE)
        .combined(Transition::scale(theme::TOAST_SCALE))
        .springs(theme::TOAST_PRESENCE)
}

#[cfg(test)]
mod tests {
    use elaris_core::appearance::AccentColor;
    use rune::Tx;

    use super::*;

    #[test]
    fn shows_a_stack_only_while_toasts_are_up() {
        let theme = theme::palette(false, false, 100, AccentColor::default());
        let mut toasts = Toasts::default();
        assert!(view(&toasts, &theme, 800.0, false).is_none());

        let toast = crate::toast::Toast::new("Copied address").detail("https://example.com");
        toasts.show(&mut Tx::default(), toast);
        toasts.show(&mut Tx::default(), "No address to copy");
        assert!(view(&toasts, &theme, 800.0, false).is_some());
        assert!(view(&toasts, &theme, 800.0, true).is_some());
    }

    #[test]
    fn collapsed_stack_peeks_and_hover_spreads_it() {
        let (newest_lift, newest_scale) = placement(0, false);
        assert_eq!((newest_lift, newest_scale), (0.0, 1.0));
        // Deeper toasts sit higher and smaller, and stop at the deepest visible one.
        assert!(placement(1, false).0 > newest_lift && placement(1, false).1 < 1.0);
        assert_eq!(placement(2, false), placement(7, false));

        assert_eq!(placement(1, true).1, 1.0);
        assert!(placement(1, true).0 > placement(1, false).0);
        assert!(stack_height(4, true) > stack_height(4, false));
        assert_eq!(stack_height(1, false), theme::TOAST_HEIGHT);
    }
}
