// SPDX-License-Identifier: GPL-3.0-only

//! The toast stack, centred over the window's bottom edge. Each toast rises in, and the rest of
//! the stack glides to close a gap when one leaves.

use rune::tok::space;
use rune::tok::type_;
use rune::{El, Transition, col, el, icon, row, text};

use crate::app::AppState;
use crate::toast::{Shown, Toasts};
use crate::ui::theme::{self, Theme};

/// The stack for the toasts on screen, or `None` when there are none.
pub(crate) fn view(
    toasts: &Toasts,
    theme: &Theme,
    window_width: f32,
    reduced_motion: bool,
) -> Option<El> {
    if toasts.shown.is_empty() {
        return None;
    }
    let mut stack = col()
        .key("toasts")
        .w(theme::TOAST_WIDTH)
        .gap(theme::TOAST_GAP)
        .pin(
            Some((window_width - theme::TOAST_WIDTH) / 2.0),
            None,
            None,
            Some(theme::TOAST_INSET),
        );
    for shown in &toasts.shown {
        stack = stack.child(card(shown, theme, reduced_motion));
    }
    Some(stack)
}

fn card(shown: &Shown, theme: &Theme, reduced_motion: bool) -> El {
    let id = shown.id;
    let toast = &shown.toast;

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

    let mut body = row().w_full().center_y().gap(theme::TOAST_GAP);
    if let Some(glyph) = toast.icon {
        body = body.child(icon(glyph, theme::TOAST_ICON).color(theme.icon));
    }

    el().key(format!("toast-{id}"))
        .w_full()
        .px(space::S3)
        .py(space::S2)
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
            rune::Spring::NONE
        } else {
            theme::TOAST_REFLOW
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
}
