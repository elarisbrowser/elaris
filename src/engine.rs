// SPDX-License-Identifier: GPL-3.0-only

//! The Ladybird engine behind the page view. Without the `ladybird-bridge` feature, starting it
//! reports why, so `cargo run` works without Ladybird.

#[cfg(feature = "ladybird-bridge")]
pub(crate) use bridged::Engine;
#[cfg(not(feature = "ladybird-bridge"))]
pub(crate) use unavailable::Engine;

#[cfg(feature = "ladybird-bridge")]
mod bridged {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use elaris::ladybird::{Event, Frame, KeyEvent, Ladybird, MouseEvent, WebView};

    /// How long shutdown keeps pumping so helper processes see the view close.
    const CLOSE_GRACE: Duration = Duration::from_millis(100);
    const CLOSE_PUMP_INTERVAL: Duration = Duration::from_millis(5);

    /// One Ladybird application with one view. Every call stays on the thread that started it.
    pub(crate) struct Engine {
        view: Option<WebView>,
        ladybird: Option<Ladybird>,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl Engine {
        pub(crate) fn start() -> Result<Self, String> {
            let resources = directory("ELARIS_LADYBIRD_RESOURCE_DIR", "vendor/ladybird/Base/res");
            let profile = directory("ELARIS_LADYBIRD_PROFILE_DIR", "build/elaris-profile");
            std::fs::create_dir_all(&profile)
                .map_err(|e| format!("cannot create profile {}: {e}", profile.display()))?;
            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = Rc::clone(&events);
            let mut ladybird =
                Ladybird::new(&resources, &profile, move |e| sink.borrow_mut().push(e))?;
            let view = ladybird.create_view()?;
            Ok(Self {
                view: Some(view),
                ladybird: Some(ladybird),
                events,
            })
        }

        fn view(&mut self) -> &mut WebView {
            self.view
                .as_mut()
                .expect("the view lives until the engine drops")
        }

        pub(crate) fn load(&mut self, url: &str) -> Result<(), String> {
            self.view().load(url)
        }

        pub(crate) fn resize(&mut self, width: i32, height: i32, scale: f64) -> Result<(), String> {
            self.view().resize(width, height, scale)
        }

        pub(crate) fn mouse(&mut self, event: MouseEvent) -> Result<(), String> {
            self.view().mouse_event(event)
        }

        pub(crate) fn key(&mut self, event: KeyEvent) -> Result<(), String> {
            self.view().key_event(event)
        }

        pub(crate) fn set_focus(&mut self, focused: bool) -> Result<(), String> {
            self.view().set_focus(focused)
        }

        /// Runs the work Ladybird has ready without waiting, and returns the events it raised.
        pub(crate) fn pump(&mut self) -> Result<Vec<Event>, String> {
            self.view().pump()?;
            Ok(std::mem::take(&mut *self.events.borrow_mut()))
        }

        /// Runs Ladybird work until a new frame is presented or `timeout` passes, and returns the
        /// events it raised.
        pub(crate) fn wait_for_frame(&mut self, timeout: Duration) -> Result<Vec<Event>, String> {
            self.view().wait_for_frame(timeout)?;
            Ok(std::mem::take(&mut *self.events.borrow_mut()))
        }

        /// The frame on display, if any.
        pub(crate) fn frame(&mut self) -> Result<Option<Frame>, String> {
            self.view().frame()
        }

        /// A copy of the pixels of `frame`, which [`Self::frame`] just described.
        pub(crate) fn copy_frame(&mut self, frame: &Frame) -> Result<Vec<u8>, String> {
            self.view().copy_frame(frame)
        }
    }

    impl Drop for Engine {
        fn drop(&mut self) {
            drop(self.view.take());
            if let Some(ladybird) = self.ladybird.as_mut() {
                let deadline = Instant::now() + CLOSE_GRACE;
                while Instant::now() < deadline && ladybird.pump().is_ok() {
                    std::thread::sleep(CLOSE_PUMP_INTERVAL);
                }
            }
        }
    }

    /// A directory from `variable`, or `fallback` within the source tree for development builds.
    fn directory(variable: &str, fallback: &str) -> PathBuf {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback))
    }
}

#[cfg(not(feature = "ladybird-bridge"))]
mod unavailable {
    use std::time::Duration;

    use elaris::ladybird::{Event, Frame, KeyEvent, MouseEvent};

    /// Never constructed: this build has no engine.
    pub(crate) enum Engine {}

    impl Engine {
        pub(crate) fn start() -> Result<Self, String> {
            Err("This build has no web engine. Rebuild with --features ladybird-bridge.".to_owned())
        }

        pub(crate) fn load(&mut self, _: &str) -> Result<(), String> {
            match *self {}
        }

        pub(crate) fn resize(&mut self, _: i32, _: i32, _: f64) -> Result<(), String> {
            match *self {}
        }

        pub(crate) fn mouse(&mut self, _: MouseEvent) -> Result<(), String> {
            match *self {}
        }

        pub(crate) fn key(&mut self, _: KeyEvent) -> Result<(), String> {
            match *self {}
        }

        pub(crate) fn set_focus(&mut self, _: bool) -> Result<(), String> {
            match *self {}
        }

        pub(crate) fn pump(&mut self) -> Result<Vec<Event>, String> {
            match *self {}
        }

        pub(crate) fn wait_for_frame(&mut self, _: Duration) -> Result<Vec<Event>, String> {
            match *self {}
        }

        pub(crate) fn frame(&mut self) -> Result<Option<Frame>, String> {
            match *self {}
        }

        pub(crate) fn copy_frame(&mut self, _: &Frame) -> Result<Vec<u8>, String> {
            match *self {}
        }
    }
}
