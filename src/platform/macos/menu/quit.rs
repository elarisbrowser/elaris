// SPDX-License-Identifier: GPL-3.0-only

//! Coordinates a quit across every window, so each page can confirm it may unload first.

use rune::Tx;

use super::{APP_HANDLES, QUIT_PENDING};

/// Rune's quit guard: a quit from the menu, the Dock or the system waits for every window's
/// pages to agree to unload, then quits from the last approval.
pub(super) fn quit_guard() -> bool {
    let handles: Vec<_> = APP_HANDLES.with(|handles| handles.borrow().values().cloned().collect());
    if handles.is_empty() {
        return true;
    }
    let started = QUIT_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.is_some() {
            return false;
        }
        *pending = Some(APP_HANDLES.with(|handles| handles.borrow().keys().copied().collect()));
        true
    });
    if started {
        for handle in handles {
            handle.update(|state, tx| state.request_quit_close(tx));
        }
    }
    false
}

/// Records one window's approval and quits once every window has approved.
pub(crate) fn quit_window_approved(id: u64, tx: &mut Tx) {
    let finished = QUIT_PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let Some(windows) = pending.as_mut() else {
            return false;
        };
        windows.remove(&id);
        let finished = windows.is_empty();
        if finished {
            *pending = None;
        }
        finished
    });
    if finished {
        tx.quit();
    }
}

/// A page kept its window open, so the whole quit is abandoned.
pub(crate) fn quit_cancelled() {
    QUIT_PENDING.with(|pending| *pending.borrow_mut() = None);
}
