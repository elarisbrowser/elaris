// SPDX-License-Identifier: GPL-3.0-only

//! The engine thread: starting it, its loop, and finishing it as the process exits.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, Once};
use std::thread::JoinHandle;

use super::runtime::Runtime;
use super::{Command, Engine, Notify, Update, Wake, drain};
use crate::perf::{self, Stage};

/// What the process needs to finish the engine as it exits. Rune ends the process from within
/// its event loop once the window has gone, so the only place left to wait for the engine
/// without the UI waiting too is an exit handler. Ladybird allows one application per process,
/// so there is at most one engine.
struct Running {
    thread: JoinHandle<()>,
    commands: Sender<Command>,
    wake: Wake,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> i32;
}

/// Stops the engine if the browser has not, and waits for it, so its helper processes exit and no
/// Ladybird code is running when the process tears down Ladybird's statics. Exit handlers run in
/// reverse order of registration, so this runs before those, which Ladybird registered as it
/// started, earlier than this.
extern "C" fn finish_at_exit() {
    let running = RUNNING
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    if let Some(running) = running {
        // Rune's quit hook can queue shutdown before this exit handler runs. Repeating it is
        // safe, and covers OS exits that did not run the hook.
        request_shutdown(&running.commands, &running.wake);
        let _ = running.thread.join();
    }
}

/// Wake Ladybird if startup has installed its waker. Otherwise the queued command is seen
/// before the engine's first wait.
fn wake(wake_handle: &Wake) {
    if let Some(wake) = wake_handle.get() {
        wake();
    }
}

fn request_shutdown(commands: &Sender<Command>, wake_handle: &Wake) {
    if commands.send(Command::Shutdown).is_ok() {
        wake(wake_handle);
    }
}

pub(super) fn start(notify: Notify) -> Result<Engine, String> {
    let (commands, received) = channel();
    let (sender, updates) = channel();
    let wake = Wake::default();
    let thread = {
        let wake = Arc::clone(&wake);
        std::thread::Builder::new()
            .name("elaris-engine".to_owned())
            .spawn(move || run(&received, &sender, &wake, &*notify))
            .map_err(|e| format!("cannot start the engine thread: {e}"))?
    };
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        thread,
        commands: commands.clone(),
        wake: Arc::clone(&wake),
    });
    static AT_EXIT: Once = Once::new();
    // SAFETY: `finish_at_exit` is a plain function that stays valid for the process.
    AT_EXIT.call_once(|| unsafe {
        atexit(finish_at_exit);
    });
    Ok(Engine {
        commands,
        updates,
        wake,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn shutdown_wakes_a_waiting_engine_and_joins_cleanly() {
        let (commands, received) = channel();
        let wake: Wake = Arc::new(std::sync::OnceLock::new());
        let woke = Arc::new(AtomicBool::new(false));
        let wake_flag = Arc::clone(&woke);
        wake.set(Box::new(move || wake_flag.store(true, Ordering::SeqCst)))
            .unwrap();

        let thread = std::thread::spawn(move || {
            received.recv_timeout(Duration::from_secs(1)).unwrap();
            !drain(&received, |_| {})
        });
        request_shutdown(&commands, &wake);
        assert!(woke.load(Ordering::SeqCst));
        assert!(thread.join().unwrap());
    }

    #[test]
    fn shutdown_before_waker_installation_stays_queued() {
        let (commands, received) = channel();
        let wake: Wake = Arc::new(std::sync::OnceLock::new());
        request_shutdown(&commands, &wake);
        assert!(matches!(received.try_recv(), Ok(Command::Shutdown)));
    }
}

/// The engine thread: starts Ladybird, runs its work as it comes, between commands, and closes
/// every view gracefully however it stops.
fn run(
    commands: &Receiver<Command>,
    updates: &Sender<Update>,
    wake: &Wake,
    notify: &(dyn Fn() + Send + Sync),
) {
    let report = |update| {
        if updates.send(update).is_ok() {
            notify();
        }
    };
    let mut runtime = match Runtime::start() {
        Ok(runtime) => runtime,
        Err(error) => return report(Update::Failed(error)),
    };
    if let Err(error) = serve(&mut runtime, commands, wake, &report) {
        report(Update::Failed(error));
    }
    runtime.close();
}

/// Applies commands and runs Ladybird until the browser shuts it down or Ladybird fails.
fn serve(
    runtime: &mut Runtime,
    commands: &Receiver<Command>,
    wake: &Wake,
    report: &impl Fn(Update),
) -> Result<(), String> {
    let waker = runtime.waker()?;
    let _ = wake.set(Box::new(move || waker.wake()));
    loop {
        let open = drain(commands, |command| {
            if let Some(error) = runtime.apply(command) {
                report(error);
            }
        });
        if !open {
            return Ok(());
        }
        runtime.wait()?;
        for update in perf::time(Stage::Pump, || runtime.handle_events()) {
            report(update);
        }
    }
}
