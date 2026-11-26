// SPDX-License-Identifier: GPL-3.0-only

//! Toasts: short confirmations that float over the window and leave on their own. A handler that
//! has the app state and a `Tx` shows one with [`Toasts::show`]:
//!
//! ```ignore
//! state.toasts.show(tx, "Saved");
//! state.toasts.show(tx, Toast::new("Address copied").icon(&icons::COPY).detail(url));
//! ```
//!
//! The newest toast shows last, at the bottom. A toast leaves once its time is up or when clicked.

use std::time::Duration;

use rune::{Icon, Tx};

use crate::ui::icons;

/// How long a toast stays up, in seconds on the window clock.
const LIFETIME: f64 = 2.5;
/// The most toasts kept at once. A new one past this pushes out the oldest.
pub(crate) const MAX_VISIBLE: usize = 5;

/// What a toast says. Only the message is required; the rest is optional.
pub(crate) struct Toast {
    pub(crate) message: String,
    pub(crate) icon: Option<&'static Icon>,
    pub(crate) detail: Option<String>,
}

impl Toast {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            icon: None,
            detail: None,
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
}

impl From<&str> for Toast {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<String> for Toast {
    fn from(message: String) -> Self {
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
    /// The pointer is over the stack, which keeps its toasts up until it leaves.
    pub(crate) hovered: bool,
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

    /// Copies `url` and confirms it with a toast. Every copy of the page address goes through here.
    pub(crate) fn copy_url(&mut self, tx: &mut Tx, url: &str) {
        tx.copy(url);
        self.show(
            tx,
            Toast::new("Copied address").icon(&icons::COPY).detail(url),
        );
    }

    /// Notes whether the pointer is over the stack. Leaving it lets the toasts whose time is up go.
    pub(crate) fn hover(&mut self, hovered: bool, clock: f64) {
        self.hovered = hovered;
        if !hovered {
            self.settle(clock);
        }
    }

    /// Drops the toasts whose time is up at `clock`, unless the pointer holds the stack. Returns
    /// whether any left.
    pub(crate) fn settle(&mut self, clock: f64) -> bool {
        let before = self.shown.len();
        let hovered = self.hovered;
        self.shown.retain(|shown| hovered || clock < shown.expires);
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
        for n in 0..=MAX_VISIBLE {
            toasts.show(&mut tx, format!("toast {n}"));
        }
        assert_eq!(toasts.shown.len(), MAX_VISIBLE);
        assert_eq!(toasts.shown[0].toast.message, "toast 1");
    }

    #[test]
    fn hovered_stack_holds_its_toasts_until_the_pointer_leaves() {
        let mut toasts = Toasts::default();
        toasts.show(&mut Tx::default(), "Copied address");
        toasts.hover(true, LIFETIME * 2.0);
        assert!(!toasts.settle(LIFETIME * 2.0));
        assert_eq!(toasts.shown.len(), 1);
        toasts.hover(false, LIFETIME * 2.0);
        assert!(toasts.shown.is_empty());
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
}
