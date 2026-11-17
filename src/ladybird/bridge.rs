//! Safe owner-thread wrappers for the optional Ladybird C ABI.

use std::ffi::{CStr, CString, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use super::{Event, Frame, Key, KeyEvent, MouseEvent};

#[repr(C)]
struct RawBridge {
    _private: [u8; 0],
}

#[repr(C)]
struct RawView {
    _private: [u8; 0],
}

#[repr(C)]
struct RawWaker {
    _private: [u8; 0],
}

#[repr(C)]
struct RawEvent {
    kind: i32,
    text: *const c_char,
    value: i32,
}

#[repr(C)]
#[derive(Default)]
struct RawFrame {
    width: i32,
    height: i32,
    stride: i32,
    format: i32,
    device_pixel_ratio: f64,
    damage_x: i32,
    damage_y: i32,
    damage_width: i32,
    damage_height: i32,
    serial: u64,
}

#[repr(C)]
struct RawMouseEvent {
    kind: i32,
    x: i32,
    y: i32,
    button: i32,
    buttons: i32,
    modifiers: i32,
    wheel_delta_x: f64,
    wheel_delta_y: f64,
    wheel_precise: i32,
    wheel_phase: i32,
    click_count: i32,
}

#[repr(C)]
struct RawKeyEvent {
    kind: i32,
    key: i32,
    modifiers: i32,
    code_point: u32,
    repeat: i32,
}

const NO_FRAME: i32 = 1;
const PIXEL_FORMAT_RGBA8: i32 = 1;
const KEY_PRESS: i32 = 1;
const KEY_RELEASE: i32 = 2;

type EventCallback = unsafe extern "C" fn(*mut c_void, *const RawEvent);

unsafe extern "C" {
    fn elaris_ladybird_create(
        resource_dir: *const c_char,
        profile_dir: *const c_char,
        callback: Option<EventCallback>,
        context: *mut c_void,
        out_bridge: *mut *mut RawBridge,
    ) -> i32;
    fn elaris_ladybird_create_view(bridge: *mut RawBridge, out_view: *mut *mut RawView) -> i32;
    fn elaris_ladybird_load(view: *mut RawView, url: *const c_char) -> i32;
    fn elaris_ladybird_resize(view: *mut RawView, width: i32, height: i32, scale: f64) -> i32;
    fn elaris_ladybird_frame(view: *mut RawView, out_frame: *mut RawFrame) -> i32;
    fn elaris_ladybird_copy_frame(view: *mut RawView, rgba: *mut u8, length: usize) -> i32;
    fn elaris_ladybird_mouse_event(view: *mut RawView, event: *const RawMouseEvent) -> i32;
    fn elaris_ladybird_key_event(view: *mut RawView, event: *const RawKeyEvent) -> i32;
    fn elaris_ladybird_set_focus(view: *mut RawView, focused: i32) -> i32;
    fn elaris_ladybird_set_color_scheme(view: *mut RawView, dark: i32) -> i32;
    fn elaris_ladybird_pump(bridge: *mut RawBridge) -> i32;
    fn elaris_ladybird_wait(bridge: *mut RawBridge, timeout_us: i32) -> i32;
    fn elaris_ladybird_create_waker(bridge: *mut RawBridge) -> *mut RawWaker;
    fn elaris_ladybird_wake(waker: *mut RawWaker);
    fn elaris_ladybird_destroy_waker(waker: *mut RawWaker);
    fn elaris_ladybird_destroy_view(view: *mut RawView);
    fn elaris_ladybird_destroy(bridge: *mut RawBridge);
    fn elaris_ladybird_last_error() -> *const c_char;
}

struct CallbackState {
    callback: Box<dyn FnMut(Event)>,
    panicked: bool,
}

// The C++ callback executes synchronously from the owner's event-loop pump.
unsafe extern "C" fn dispatch(context: *mut c_void, raw: *const RawEvent) {
    if context.is_null() || raw.is_null() {
        return;
    }
    // SAFETY: `Owner` keeps this boxed state alive until after the C++ bridge is destroyed, and
    // no Rust reference to it exists while C++ runs the pump that calls back.
    let state = unsafe { &mut *context.cast::<CallbackState>() };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: C++ supplies a valid event and a borrowed, NUL-terminated event string for this call.
        let raw = unsafe { &*raw };
        let text = if raw.text.is_null() {
            String::new()
        } else {
            // SAFETY: Event text is documented by the C API as borrowed through callback return.
            unsafe { CStr::from_ptr(raw.text) }
                .to_string_lossy()
                .into_owned()
        };
        let event = match raw.kind {
            1 => Event::Navigation(text),
            2 => Event::Title(text),
            3 => Event::Loading(raw.value != 0),
            4 => Event::ReadyToPaint,
            _ => return,
        };
        (state.callback)(event);
    }));
    state.panicked |= result.is_err();
}

/// The bridge and its callback state, shared by the application handle and its views so the
/// bridge is destroyed only after the last view.
struct Owner {
    raw: NonNull<RawBridge>,
    callback: NonNull<CallbackState>,
}

impl Owner {
    fn pump(&self) -> Result<(), String> {
        // SAFETY: `Owner` is reachable only through non-Send `Rc`s on the initializing thread.
        status_result(unsafe { elaris_ladybird_pump(self.raw.as_ptr()) })?;
        self.check_callback()
    }

    fn check_callback(&self) -> Result<(), String> {
        // SAFETY: The pump has returned, so no callback holds the state mutably.
        if unsafe { self.callback.as_ref() }.panicked {
            return Err("Ladybird event callback panicked".to_owned());
        }
        Ok(())
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: Every view holds an `Rc<Owner>`, so all were destroyed before this bridge.
        unsafe { elaris_ladybird_destroy(self.raw.as_ptr()) };
        // SAFETY: The bridge is gone, so nothing can call back with this pointer any more.
        drop(unsafe { Box::from_raw(self.callback.as_ptr()) });
    }
}

/// Owns the Ladybird application. It is neither `Send` nor `Sync` because all bridge
/// operations and callbacks must stay on the initializing thread.
pub struct Ladybird {
    owner: Rc<Owner>,
}

impl Ladybird {
    pub fn new(
        resource_dir: &Path,
        profile_dir: &Path,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<Self, String> {
        let resource_dir = path_cstring(resource_dir)?;
        let profile_dir = path_cstring(profile_dir)?;
        let state = NonNull::from(Box::leak(Box::new(CallbackState {
            callback: Box::new(callback),
            panicked: false,
        })));
        let mut raw = std::ptr::null_mut();
        // SAFETY: Paths are valid for the call; the callback context lives until `Owner` drops.
        let status = unsafe {
            elaris_ladybird_create(
                resource_dir.as_ptr(),
                profile_dir.as_ptr(),
                Some(dispatch),
                state.as_ptr().cast(),
                &mut raw,
            )
        };
        let Some(raw) = NonNull::new(raw).filter(|_| status == 0) else {
            // SAFETY: Initialization failed, so C++ kept no reference to the state.
            drop(unsafe { Box::from_raw(state.as_ptr()) });
            return Err(last_error());
        };
        Ok(Self {
            owner: Rc::new(Owner {
                raw,
                callback: state,
            }),
        })
    }

    pub fn create_view(&mut self) -> Result<WebView, String> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: `self.owner.raw` is a live bridge owned on this thread.
        let status = unsafe { elaris_ladybird_create_view(self.owner.raw.as_ptr(), &mut raw) };
        status_result(status)?;
        let raw = NonNull::new(raw).ok_or_else(|| "Ladybird returned a null view".to_owned())?;
        Ok(WebView {
            raw,
            owner: Rc::clone(&self.owner),
        })
    }

    pub fn pump(&mut self) -> Result<(), String> {
        self.owner.pump()
    }

    /// Runs Ladybird work, blocking until there is some, a [`Waker`] wakes it, or `timeout`
    /// passes. `None` waits until there is work or a wake.
    pub fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        let micros = timeout.map_or(-1, |t| i32::try_from(t.as_micros()).unwrap_or(i32::MAX));
        // SAFETY: `self.owner.raw` is a live bridge owned on this thread.
        status_result(unsafe { elaris_ladybird_wait(self.owner.raw.as_ptr(), micros) })?;
        self.owner.check_callback()
    }

    /// A handle any thread can use to end a [`Self::wait`].
    pub fn waker(&mut self) -> Result<Waker, String> {
        // SAFETY: `self.owner.raw` is a live bridge owned on this thread.
        let raw = unsafe { elaris_ladybird_create_waker(self.owner.raw.as_ptr()) };
        NonNull::new(raw)
            .map(|raw| Waker { raw })
            .ok_or_else(last_error)
    }
}

/// Wakes a Ladybird [`Ladybird::wait`] from any thread, for as long as it lives; once the
/// application's event loop is gone, waking does nothing.
pub struct Waker {
    raw: NonNull<RawWaker>,
}

// SAFETY: The C API makes waking and destroying a waker safe from any thread.
unsafe impl Send for Waker {}
// SAFETY: Waking takes no mutable state on the Rust side; C++ synchronises it.
unsafe impl Sync for Waker {}

impl Waker {
    pub fn wake(&self) {
        // SAFETY: The waker is live until this wrapper drops.
        unsafe { elaris_ladybird_wake(self.raw.as_ptr()) };
    }
}

impl Drop for Waker {
    fn drop(&mut self) {
        // SAFETY: The waker is uniquely owned by this wrapper.
        unsafe { elaris_ladybird_destroy_waker(self.raw.as_ptr()) };
    }
}

/// One page view. It keeps its application alive and, like it, stays on the initializing thread.
pub struct WebView {
    raw: NonNull<RawView>,
    owner: Rc<Owner>,
}

impl WebView {
    pub fn load(&mut self, url: &str) -> Result<(), String> {
        let url = CString::new(url).map_err(|_| "URL contains an interior NUL byte".to_owned())?;
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_load(self.raw.as_ptr(), url.as_ptr()) })
    }

    /// Sets the viewport in device pixels and the display scale it is laid out at.
    pub fn resize(&mut self, width: i32, height: i32, scale: f64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_resize(self.raw.as_ptr(), width, height, scale) })
    }

    /// The frame on display, or `None` before the first one.
    pub fn frame(&self) -> Result<Option<Frame>, String> {
        let mut raw = RawFrame::default();
        // SAFETY: The view is live on its owner's thread and `raw` is a valid output.
        match unsafe { elaris_ladybird_frame(self.raw.as_ptr(), &mut raw) } {
            0 => {}
            NO_FRAME => return Ok(None),
            _ => return Err(last_error()),
        }
        if raw.format != PIXEL_FORMAT_RGBA8 || raw.width <= 0 || raw.height <= 0 {
            return Err("Ladybird described an unsupported frame".to_owned());
        }
        Ok(Some(Frame {
            width: raw.width as u32,
            height: raw.height as u32,
            stride: raw.stride as usize,
            scale: raw.device_pixel_ratio,
            damage: [
                raw.damage_x,
                raw.damage_y,
                raw.damage_width,
                raw.damage_height,
            ],
            serial: raw.serial,
        }))
    }

    /// Copies the frame `frame` described into a new RGBA buffer and clears its damage.
    pub fn copy_frame(&mut self, frame: &Frame) -> Result<Vec<u8>, String> {
        // Left uninitialised: zeroing a frame-sized buffer costs as much as a fast copy.
        let length = frame.stride * frame.height as usize;
        let mut rgba = Vec::<u8>::with_capacity(length);
        // SAFETY: The buffer has room for `length` bytes; C++ checks it matches the frame.
        let status =
            unsafe { elaris_ladybird_copy_frame(self.raw.as_ptr(), rgba.as_mut_ptr(), length) };
        status_result(status)?;
        // SAFETY: A successful copy wrote every byte of the frame.
        unsafe { rgba.set_len(length) };
        Ok(rgba)
    }

    pub fn mouse_event(&mut self, event: MouseEvent) -> Result<(), String> {
        let raw = RawMouseEvent {
            kind: event.kind as i32,
            x: event.x,
            y: event.y,
            button: event.button,
            buttons: event.buttons,
            modifiers: event.modifiers,
            wheel_delta_x: event.wheel.0,
            wheel_delta_y: event.wheel.1,
            wheel_precise: event.precise.into(),
            wheel_phase: event.phase as i32,
            click_count: event.clicks,
        };
        // SAFETY: The view is live on its owner's thread and `raw` outlives the call.
        status_result(unsafe { elaris_ladybird_mouse_event(self.raw.as_ptr(), &raw) })
    }

    pub fn key_event(&mut self, event: KeyEvent) -> Result<(), String> {
        let (key, code_point) = match event.key {
            Key::Character(c) => (1, u32::from(c)),
            Key::Return => (2, 0),
            Key::Backspace => (3, 0),
            Key::Delete => (4, 0),
            Key::Tab => (5, 0),
            Key::Escape => (6, 0),
            Key::Left => (7, 0),
            Key::Right => (8, 0),
            Key::Up => (9, 0),
            Key::Down => (10, 0),
            Key::Home => (11, 0),
            Key::End => (12, 0),
            Key::PageUp => (13, 0),
            Key::PageDown => (14, 0),
        };
        let raw = RawKeyEvent {
            kind: if event.pressed {
                KEY_PRESS
            } else {
                KEY_RELEASE
            },
            key,
            modifiers: event.modifiers,
            code_point,
            repeat: event.repeat.into(),
        };
        // SAFETY: The view is live on its owner's thread and `raw` outlives the call.
        status_result(unsafe { elaris_ladybird_key_event(self.raw.as_ptr(), &raw) })
    }

    pub fn set_focus(&mut self, focused: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_set_focus(self.raw.as_ptr(), focused.into()) })
    }

    /// Whether pages see a dark or light `prefers-color-scheme`.
    pub fn set_color_scheme(&mut self, dark: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_set_color_scheme(self.raw.as_ptr(), dark.into()) })
    }

    pub fn pump(&mut self) -> Result<(), String> {
        self.owner.pump()
    }
}

impl Drop for WebView {
    fn drop(&mut self) {
        // SAFETY: The view handle is uniquely owned by this wrapper and its bridge is still alive.
        unsafe { elaris_ladybird_destroy_view(self.raw.as_ptr()) };
    }
}

fn path_cstring(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "path contains an interior NUL byte".to_owned())
}

fn last_error() -> String {
    // SAFETY: C API returns a thread-local NUL-terminated string valid until the next bridge call.
    let error = unsafe { elaris_ladybird_last_error() };
    if error.is_null() {
        "Ladybird bridge failed without an error message".to_owned()
    } else {
        // SAFETY: The pointer is the C API's NUL-terminated last-error string.
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    }
}

fn status_result(status: i32) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(last_error())
    }
}

#[cfg(test)]
mod tests {
    use super::{CallbackState, RawEvent, dispatch};
    use std::ffi::c_void;

    #[test]
    fn callback_panics_are_contained_at_the_ffi_boundary() {
        let mut state = CallbackState {
            callback: Box::new(|_| panic!("callback regression test")),
            panicked: false,
        };
        let event = RawEvent {
            kind: 4,
            text: std::ptr::null(),
            value: 0,
        };
        // SAFETY: Both pointers reference stack values that remain alive through the callback.
        unsafe {
            dispatch((&mut state as *mut CallbackState).cast::<c_void>(), &event);
        }
        assert!(state.panicked);
    }
}
