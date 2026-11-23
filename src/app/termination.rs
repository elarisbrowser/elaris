// SPDX-License-Identifier: GPL-3.0-only

//! Quit on SIGTERM the way the Quit command does, so the session and transfers are saved. The signal handler
//! only sets a flag, which is all a signal handler may safely do. A watcher thread then wakes the event loop, and
//! its poll sees the flag and quits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const SIGTERM: i32 = 15;
static REQUESTED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn on_sigterm(_: i32) {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Routes SIGTERM to the app. `wake` must wake the event loop, so the next poll can see the request.
pub(crate) fn install(wake: impl Fn() + Send + 'static) {
    // SAFETY: the handler is a plain function that touches only an atomic flag.
    unsafe {
        signal(SIGTERM, on_sigterm);
    }
    std::thread::spawn(move || {
        while !REQUESTED.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        wake();
    });
}

/// Whether a SIGTERM has arrived since the last call. Clears the request.
pub(crate) fn take_request() -> bool {
    REQUESTED.swap(false, Ordering::SeqCst)
}
