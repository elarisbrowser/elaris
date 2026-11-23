// SPDX-License-Identifier: GPL-3.0-only

//! Toasts: short confirmations that float over the window and leave on their own. A handler that
//! has the app state and a `Tx` shows one with [`Toasts::show`]:
//!
//! ```ignore
//! state.toasts.show(tx, "Saved");
//! state.toasts.show(tx, Toast::new("Address copied").icon(&icons::COPY).detail(url));
//! state.toasts.show(tx, Toast::new("Downloaded").content(|theme| my_row(theme)));
//! ```
//!
//! The newest toast shows last, at the bottom. A toast leaves once its time is up or when clicked.

use std::time::Duration;

use rune::{El, Icon, Tx};

use crate::ui::theme::Theme;

/// How long a toast stays up, in seconds on the window clock.
const LIFETIME: f64 = 2.5;
/// The most toasts on screen at once. A new one past this pushes out the oldest.
const MAX_VISIBLE: usize = 3;

/// Builds a toast's extra content from the theme, each time the window draws.
type Content = Box<dyn Fn(&Theme) -> El>;

/// What a toast says. Only the message is required; the rest is optional.
pub(crate) struct Toast {
    pub(crate) message: String,
    pub(crate) icon: Option<&'static Icon>,
    pub(crate) detail: Option<String>,
    pub(crate) content: Option<Content>,
}

impl Toast {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            icon: None,
            detail: None,
            content: None,
        }
    }

    /// An icon before the message.
    pub(crate) fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A quieter line under the message, truncated to fit.
    pub(crate) fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Anything else to show under the message. The closure runs each time the window draws, so
    /// it takes the current theme.
    // Public API for callers that need more than a message; none needs it yet.
    #[allow(dead_code)]
    pub(crate) fn content(mut self, content: impl Fn(&Theme) -> El + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }
}

impl From<&str> for Toast {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// A toast on screen, with its id and the window clock time it leaves.
pub(crate) struct Shown {
    pub(crate) id: u64,
    pub(crate) toast: Toast,
    expires: f64,
}

/// The toasts on screen, oldest first.
#[derive(Default)]
pub(crate) struct Toasts {
    next_id: u64,
    pub(crate) shown: Vec<Shown>,
}

impl Toasts {
    /// Shows `toast`, animating it in, and makes sure the window wakes to take it away.
    pub(crate) fn show(&mut self, tx: &mut Tx, toast: impl Into<Toast>) {
        self.next_id += 1;
        self.shown.push(Shown {
            id: self.next_id,
            toast: toast.into(),
            expires: tx.clock() + LIFETIME,
        });
        let overflow = self.shown.len().saturating_sub(MAX_VISIBLE);
        self.shown.drain(..overflow);
        tx.animated();
        tx.wake_after(Duration::from_secs_f64(LIFETIME));
    }

    /// Takes one toast away, as clicking it does. Returns whether it was still shown.
    pub(crate) fn dismiss(&mut self, id: u64) -> bool {
        let before = self.shown.len();
        self.shown.retain(|shown| shown.id != id);
        self.shown.len() != before
    }

    /// Drops the toasts whose time is up at `clock`. Returns whether any left.
    pub(crate) fn settle(&mut self, clock: f64) -> bool {
        let before = self.shown.len();
        self.shown.retain(|shown| clock < shown.expires);
        self.shown.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(toasts: &Toasts) -> Vec<&str> {
        toasts
            .shown
            .iter()
            .map(|shown| shown.toast.message.as_str())
            .collect()
    }

    #[test]
    fn leaves_once_its_time_is_up() {
        let mut toasts = Toasts::default();
        toasts.show(&mut Tx::default(), "Copied address");
        assert!(!toasts.settle(LIFETIME - 0.1));
        assert_eq!(messages(&toasts), ["Copied address"]);
        assert!(toasts.settle(LIFETIME));
        assert!(toasts.shown.is_empty());
    }

    #[test]
    fn keeps_only_the_newest_visible() {
        let mut toasts = Toasts::default();
        let mut tx = Tx::default();
        for message in ["one", "two", "three", "four"] {
            toasts.show(&mut tx, message);
        }
        assert_eq!(messages(&toasts), ["two", "three", "four"]);
    }

    #[test]
    fn dismisses_one_by_id() {
        let mut toasts = Toasts::default();
        let mut tx = Tx::default();
        toasts.show(&mut tx, "one");
        toasts.show(&mut tx, "two");
        let first = toasts.shown[0].id;
        assert!(toasts.dismiss(first));
        assert!(!toasts.dismiss(first));
        assert_eq!(messages(&toasts), ["two"]);
    }

    #[test]
    fn takes_optional_content() {
        let plain = Toast::new("Saved");
        assert!(plain.icon.is_none() && plain.detail.is_none() && plain.content.is_none());

        let toast = Toast::new("Saved")
            .detail("notes.md")
            .content(|_| rune::el());
        assert_eq!(toast.detail.as_deref(), Some("notes.md"));
        assert!(toast.content.is_some());
    }
}
