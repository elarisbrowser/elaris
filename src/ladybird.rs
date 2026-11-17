//! Safe owner-thread wrappers for the optional Ladybird C ABI.

use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

#[repr(C)]
struct RawBridge {
    _private: [u8; 0],
}

#[repr(C)]
struct RawView {
    _private: [u8; 0],
}

#[repr(C)]
struct RawEvent {
    kind: i32,
    text: *const c_char,
    value: i32,
}

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
    fn elaris_ladybird_resize(view: *mut RawView, width: i32, height: i32) -> i32;
    fn elaris_ladybird_pump(bridge: *mut RawBridge) -> i32;
    fn elaris_ladybird_destroy_view(view: *mut RawView);
    fn elaris_ladybird_destroy(bridge: *mut RawBridge);
    fn elaris_ladybird_last_error() -> *const c_char;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Navigation(String),
    Title(String),
    Loading(bool),
    ReadyToPaint,
}

struct CallbackState<F> {
    callback: F,
    panicked: bool,
}

// The C++ callback executes synchronously from the owner's event-loop pump.
unsafe extern "C" fn dispatch<F: FnMut(Event)>(context: *mut c_void, raw: *const RawEvent) {
    if context.is_null() || raw.is_null() {
        return;
    }
    // SAFETY: `Ladybird` keeps this boxed state alive until after the C++ bridge is destroyed.
    let state = unsafe { &mut *context.cast::<CallbackState<F>>() };
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

/// Owns the Ladybird application. It is neither `Send` nor `Sync` because all
/// bridge operations and callbacks must stay on the initializing thread.
pub struct Ladybird<F: FnMut(Event)> {
    raw: NonNull<RawBridge>,
    callback: Box<CallbackState<F>>,
    _owner_thread: PhantomData<Rc<()>>,
}

impl<F: FnMut(Event)> Ladybird<F> {
    pub fn new(resource_dir: &Path, profile_dir: &Path, callback: F) -> Result<Self, String> {
        let resource_dir = path_cstring(resource_dir)?;
        let profile_dir = path_cstring(profile_dir)?;
        let mut state = Box::new(CallbackState {
            callback,
            panicked: false,
        });
        let mut raw = std::ptr::null_mut();
        // SAFETY: Paths and callback context stay valid through the bridge lifetime.
        let status = unsafe {
            elaris_ladybird_create(
                resource_dir.as_ptr(),
                profile_dir.as_ptr(),
                Some(dispatch::<F>),
                (&mut *state as *mut CallbackState<F>).cast(),
                &mut raw,
            )
        };
        if status != 0 {
            return Err(last_error());
        }
        let raw = NonNull::new(raw).ok_or_else(|| "Ladybird returned a null bridge".to_owned())?;
        Ok(Self {
            raw,
            callback: state,
            _owner_thread: PhantomData,
        })
    }

    pub fn create_view(&mut self) -> Result<WebView<'_, F>, String> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: `self.raw` is a live bridge owned on this thread.
        let status = unsafe { elaris_ladybird_create_view(self.raw.as_ptr(), &mut raw) };
        if status != 0 {
            return Err(last_error());
        }
        let raw = NonNull::new(raw).ok_or_else(|| "Ladybird returned a null view".to_owned())?;
        Ok(WebView {
            raw,
            bridge: self.raw,
            owner: PhantomData,
        })
    }

    pub fn pump(&mut self) -> Result<(), String> {
        // SAFETY: Called only through this non-Send owner wrapper.
        let status = unsafe { elaris_ladybird_pump(self.raw.as_ptr()) };
        if status != 0 {
            return Err(last_error());
        }
        if self.callback.panicked {
            return Err("Ladybird event callback panicked".to_owned());
        }
        Ok(())
    }
}

impl<F: FnMut(Event)> Drop for Ladybird<F> {
    fn drop(&mut self) {
        // SAFETY: Views borrow `self`, so Rust drops them before this application owner.
        unsafe { elaris_ladybird_destroy(self.raw.as_ptr()) };
    }
}

pub struct WebView<'a, F: FnMut(Event)> {
    raw: NonNull<RawView>,
    bridge: NonNull<RawBridge>,
    owner: PhantomData<&'a mut Ladybird<F>>,
}

impl<F: FnMut(Event)> WebView<'_, F> {
    pub fn load(&mut self, url: &str) -> Result<(), String> {
        let url = CString::new(url).map_err(|_| "URL contains an interior NUL byte".to_owned())?;
        // SAFETY: The view is live and borrowed from its application owner.
        let status = unsafe { elaris_ladybird_load(self.raw.as_ptr(), url.as_ptr()) };
        status_result(status)
    }

    pub fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
        // SAFETY: The view is live and borrowed from its application owner.
        let status = unsafe { elaris_ladybird_resize(self.raw.as_ptr(), width, height) };
        status_result(status)
    }

    pub fn pump(&mut self) -> Result<(), String> {
        // SAFETY: The view's exclusive borrow keeps its application owner alive and on this thread.
        let status = unsafe { elaris_ladybird_pump(self.bridge.as_ptr()) };
        status_result(status)
    }
}

impl<F: FnMut(Event)> Drop for WebView<'_, F> {
    fn drop(&mut self) {
        // SAFETY: The view handle is uniquely owned by this wrapper.
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
    use super::{CallbackState, Event, RawEvent, dispatch};
    use std::ffi::c_void;

    #[test]
    fn callback_panics_are_contained_at_the_ffi_boundary() {
        let callback: fn(Event) = |_| panic!("callback regression test");
        let mut state = CallbackState {
            callback,
            panicked: false,
        };
        let event = RawEvent {
            kind: 4,
            text: std::ptr::null(),
            value: 0,
        };
        // SAFETY: Both pointers reference stack values that remain alive through the callback.
        unsafe {
            dispatch::<fn(Event)>(
                (&mut state as *mut CallbackState<fn(Event)>).cast::<c_void>(),
                &event,
            );
        }
        assert!(state.panicked);
    }
}
