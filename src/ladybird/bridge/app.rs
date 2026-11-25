// SPDX-License-Identifier: GPL-3.0-only

//! The Ladybird application: its bridge handle, the owner shared with its views, and its waker.

use std::ffi::CString;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use super::callback::{CallbackState, dispatch};
use super::ffi::*;
use super::view::WebView;
use crate::ladybird::{DisplayMetadata, Event};

// The bridge's development and test switches, matching `ELARIS_LADYBIRD_DEV_*` in `elaris_ladybird.h`.
const DEV_SANDBOX_OFF: u32 = 1;
const DEV_TEST_LOCATION: u32 = 2;
const DEV_MICROPHONE_DENIED: u32 = 4;
const DEV_GPU_FRAMES: u32 = 8;

/// The switches the environment sets. Only debug builds read them (see `elaris_core::dev`).
fn dev_options() -> u32 {
    let enabled = |name: &str| elaris_core::dev::var(name).is_some_and(|value| value == "1");
    let mut options = 0;
    if enabled("ELARIS_TEST_DISABLE_SANDBOX") {
        options |= DEV_SANDBOX_OFF;
    }
    if enabled("ELARIS_TEST_LOCATION") {
        options |= DEV_TEST_LOCATION;
    }
    if enabled("ELARIS_TEST_MICROPHONE_DENIED") {
        options |= DEV_MICROPHONE_DENIED;
    }
    if elaris_core::dev::var("ELARIS_GPU_FRAMES").is_some_and(|value| value != "0") {
        options |= DEV_GPU_FRAMES;
    }
    options
}

/// The bridge and its callback state, shared by the application handle and its views so the
/// bridge is destroyed only after the last view.
pub(super) struct Owner {
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

/// Whether this build of the bridge can share frames with the host's GPU.
pub fn gpu_frames_supported() -> bool {
    // SAFETY: A pure query with no arguments.
    unsafe { elaris_ladybird_gpu_frames_supported() != 0 }
}

/// Asks Ladybird for frames the host shows from GPU memory, before [`Ladybird::new`]: on Linux,
/// dmabufs Rune or the window system reads in place.
pub fn request_gpu_frames(requested: bool) {
    // SAFETY: The setter only stores a value.
    unsafe { elaris_ladybird_request_gpu_frames(requested.into()) };
}

/// Names the DRM device (`dev_t`) the window system presents on, before [`Ladybird::new`], so
/// Ladybird's Compositor paints its GPU frames there (Linux).
pub fn set_presentation_device(device: u64) {
    // SAFETY: The setter only stores a value.
    unsafe { elaris_ladybird_set_presentation_device(device) };
}

impl Ladybird {
    /// Starts Ladybird. `callback` receives every view's events with the id the view was
    /// created with, synchronously while [`Self::wait`] or [`Self::pump`] runs.
    pub fn new(
        resource_dir: Option<&Path>,
        profile_dir: &Path,
        callback: impl FnMut(u64, Event) + 'static,
    ) -> Result<Self, String> {
        let resource_dir = resource_dir.map(path_cstring).transpose()?;
        let profile_dir = path_cstring(profile_dir)?;
        let state = NonNull::from(Box::leak(Box::new(CallbackState {
            callback: Box::new(callback),
            panicked: false,
        })));
        let mut raw = std::ptr::null_mut();
        // SAFETY: The setter only stores a value, and it runs before the bridge is created.
        unsafe { elaris_ladybird_set_dev_options(dev_options()) };
        // SAFETY: Paths are valid for the call; the callback context lives until `Owner` drops.
        let status = unsafe {
            elaris_ladybird_create(
                resource_dir
                    .as_ref()
                    .map_or(std::ptr::null(), |path| path.as_ptr()),
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

    /// A new view, whose events carry `id`.
    pub fn create_view(&mut self, id: u64, display_refresh_rate: f64) -> Result<WebView, String> {
        self.create_view_in(id, display_refresh_rate, false)
    }

    /// Creates a view in the ordinary session, or in the shared private session when `private`.
    pub fn create_view_in(
        &mut self,
        id: u64,
        display: impl Into<DisplayMetadata>,
        private: bool,
    ) -> Result<WebView, String> {
        let mut raw = std::ptr::null_mut();
        let create = if private {
            elaris_ladybird_create_private_view
        } else {
            elaris_ladybird_create_view
        };
        // SAFETY: `self.owner.raw` is a live bridge owned on this thread.
        let status = unsafe { create(self.owner.raw.as_ptr(), id, &mut raw) };
        status_result(status)?;
        let raw = NonNull::new(raw).ok_or_else(|| "Ladybird returned a null view".to_owned())?;
        let view = WebView {
            raw,
            _owner: Rc::clone(&self.owner),
        };
        view.set_display_metadata(display.into())?;
        Ok(view)
    }

    /// Takes ownership of a popup opened by `source` and gives it the host's view id.
    pub fn adopt_popup(
        &mut self,
        source: u64,
        request_id: u64,
        id: u64,
        display: impl Into<DisplayMetadata>,
    ) -> Result<Option<WebView>, String> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: The bridge is live on this thread and the output pointer is writable.
        status_result(unsafe {
            elaris_ladybird_adopt_popup(self.owner.raw.as_ptr(), source, request_id, id, &mut raw)
        })?;
        let Some(raw) = NonNull::new(raw) else {
            return Ok(None);
        };
        let view = WebView {
            raw,
            _owner: Rc::clone(&self.owner),
        };
        view.set_display_metadata(display.into())?;
        Ok(Some(view))
    }

    /// Replaces the autoplay default and site exceptions. Policies are 0 allow, 1 block audio,
    /// 2 block all.
    pub fn set_autoplay(
        &mut self,
        default_policy: i32,
        sites: &[(&str, i32)],
    ) -> Result<(), String> {
        let json = serde_json::to_string(sites).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|error| error.to_string())?;
        // SAFETY: The bridge is live on its owner thread and borrows the string for this call.
        status_result(unsafe {
            elaris_ladybird_set_autoplay(self.owner.raw.as_ptr(), default_policy, json.as_ptr())
        })
    }

    /// Turns force dark mode on or off for every page without a per-site choice.
    pub fn set_force_dark(&mut self, enabled: bool) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_set_force_dark(self.owner.raw.as_ptr(), i32::from(enabled))
        })
    }

    /// Turns the content blocker on or off and enables the built-in lists named by identifier.
    pub fn set_content_blocking(&mut self, enabled: bool, lists: &[&str]) -> Result<(), String> {
        let json = serde_json::to_string(lists).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|error| error.to_string())?;
        // SAFETY: The bridge is live on its owner thread and borrows the string for this call.
        status_result(unsafe {
            elaris_ladybird_set_content_blocking(
                self.owner.raw.as_ptr(),
                i32::from(enabled),
                json.as_ptr(),
            )
        })
    }

    /// Clears engine data last used at or after `since` (Unix milliseconds), or for all time.
    pub fn clear_browsing_data(
        &mut self,
        since_ms: Option<i64>,
        data: crate::ladybird::BrowsingData,
        request_id: u64,
    ) -> Result<(), String> {
        let flags = i32::from(data.cache)
            | i32::from(data.site_data) << 1
            | i32::from(data.download_history) << 2;
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_clear_browsing_data(
                self.owner.raw.as_ptr(),
                since_ms.unwrap_or(-1),
                flags,
                request_id,
            )
        })
    }

    /// Every Ladybird process, as Ladybird's Task Manager describes them.
    pub fn processes(&mut self) -> Result<Vec<elaris_core::processes::Process>, String> {
        let mut json = std::ptr::null_mut();
        // SAFETY: The bridge is live on its owner thread and writes an owned string on success.
        status_result(unsafe { elaris_ladybird_processes(self.owner.raw.as_ptr(), &mut json) })?;
        // SAFETY: The bridge returned a NUL-terminated string that stays valid until it is freed below.
        let text = unsafe { std::ffi::CStr::from_ptr(json) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: The string came from the bridge, which frees it, and is not used after.
        unsafe { elaris_ladybird_free_string(json) };
        elaris_core::processes::parse(&text)
    }

    /// Ends the process with Ladybird's child ID `id`. One that is already gone counts as ended.
    pub fn end_process(&mut self, id: u64) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe { elaris_ladybird_end_process(self.owner.raw.as_ptr(), id) })
    }

    /// Forgets what the sleeping tab with this view ID had, as it closed without waking.
    pub fn forget_discarded(&mut self, view_id: u64) {
        // SAFETY: Runs on the bridge's owner thread, which keeps the discarded tabs.
        unsafe { elaris_ladybird_forget_discarded(view_id) }
    }

    /// Discards the private session once no private view remains, so the next one starts empty.
    pub fn reset_private_session(&mut self) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe { elaris_ladybird_reset_private_session(self.owner.raw.as_ptr()) })
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

    /// Whether every page sees a dark or light `prefers-color-scheme`, now and once created.
    pub fn set_color_scheme(&mut self, dark: bool) -> Result<(), String> {
        // SAFETY: `self.owner.raw` is a live bridge owned on this thread.
        status_result(unsafe {
            elaris_ladybird_set_color_scheme(self.owner.raw.as_ptr(), dark.into())
        })
    }

    /// Uses `directory` instead of the operating system Downloads directory.
    ///
    /// This is primarily useful for isolated profiles and integration tests.
    pub fn set_download_directory(&mut self, directory: &Path) -> Result<(), String> {
        let directory = path_cstring(directory)?;
        // SAFETY: The bridge is live on its owner thread and the path is borrowed only for this call.
        status_result(unsafe {
            elaris_ladybird_set_download_directory(self.owner.raw.as_ptr(), directory.as_ptr())
        })
    }

    /// Replaces the ordinary permission decisions with a serialized list of origin, key and choice rows.
    pub fn set_permissions(&mut self, rows: &[(&str, &str, i32)]) -> Result<(), String> {
        let json = serde_json::to_string(rows).map_err(|error| error.to_string())?;
        let json = CString::new(json).map_err(|error| error.to_string())?;
        // SAFETY: The bridge is live on the owner thread and borrows the string for this call.
        status_result(unsafe {
            elaris_ladybird_set_permissions(self.owner.raw.as_ptr(), json.as_ptr())
        })
    }

    /// Sets whether sites without a decision are blocked by default.
    pub fn set_permission_default(&mut self, key: &str, blocked: bool) -> Result<(), String> {
        let key = CString::new(key).map_err(|error| error.to_string())?;
        // SAFETY: The bridge is live on the owner thread and borrows the key for this call.
        status_result(unsafe {
            elaris_ladybird_set_permission_default(
                self.owner.raw.as_ptr(),
                key.as_ptr(),
                i32::from(blocked),
            )
        })
    }

    /// Updates a site override. An empty choice resets it.
    pub fn set_permission(
        &mut self,
        origin: &str,
        key: &str,
        choice: Option<bool>,
    ) -> Result<(), String> {
        let origin = CString::new(origin).map_err(|error| error.to_string())?;
        let key = CString::new(key).map_err(|error| error.to_string())?;
        // SAFETY: The bridge is live on the owner thread and borrows both strings for this call.
        status_result(unsafe {
            elaris_ladybird_set_permission(
                self.owner.raw.as_ptr(),
                origin.as_ptr(),
                key.as_ptr(),
                permission_choice(choice),
            )
        })
    }

    /// Restores Ladybird's platform Downloads directory.
    pub fn clear_download_directory(&mut self) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread; null resets the process override.
        status_result(unsafe {
            elaris_ladybird_set_download_directory(self.owner.raw.as_ptr(), std::ptr::null())
        })
    }

    /// Starts a direct HTTP(S) download and returns Ladybird's transfer identifier.
    pub fn start_download(
        &mut self,
        url: &str,
        suggested_filename: Option<&str>,
    ) -> Result<u64, String> {
        let url = CString::new(url).map_err(|_| "URL contains an interior NUL byte".to_owned())?;
        let filename = suggested_filename
            .map(CString::new)
            .transpose()
            .map_err(|_| "suggested filename contains an interior NUL byte".to_owned())?;
        let empty = CString::default();
        let mut id = 0;
        // SAFETY: All strings and the output stay live through this owner-thread call.
        status_result(unsafe {
            elaris_ladybird_start_download(
                self.owner.raw.as_ptr(),
                url.as_ptr(),
                filename.as_ref().unwrap_or(&empty).as_ptr(),
                &mut id,
            )
        })?;
        Ok(id)
    }

    /// Starts a direct HTTP(S) download to a destination the user chose, replacing a file already there.
    pub fn start_download_to(&mut self, url: &str, destination: &Path) -> Result<u64, String> {
        let url = CString::new(url).map_err(|_| "URL contains an interior NUL byte".to_owned())?;
        let destination = CString::new(destination.as_os_str().as_encoded_bytes())
            .map_err(|_| "destination contains an interior NUL byte".to_owned())?;
        let mut id = 0;
        // SAFETY: Both strings and the output stay live through this owner-thread call.
        status_result(unsafe {
            elaris_ladybird_start_download_to(
                self.owner.raw.as_ptr(),
                url.as_ptr(),
                destination.as_ptr(),
                &mut id,
            )
        })?;
        Ok(id)
    }

    /// Cancels a Ladybird transfer by its bridge-side identifier.
    pub fn cancel_download(&mut self, id: u64) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe { elaris_ladybird_cancel_download(self.owner.raw.as_ptr(), id) })
    }

    /// Cancels every in-flight transfer.
    pub fn cancel_active_downloads(&mut self) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe { elaris_ladybird_cancel_active_downloads(self.owner.raw.as_ptr()) })
    }

    /// Pauses (`true`) or resumes a transfer by its bridge-side identifier.
    pub fn pause_download(&mut self, id: u64, pause: bool) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_pause_download(self.owner.raw.as_ptr(), id, i32::from(pause))
        })
    }

    /// Before shutdown: pauses resumable transfers, which Ladybird restores next launch, and
    /// cancels the rest.
    pub fn suspend_downloads(&mut self) -> Result<(), String> {
        // SAFETY: The bridge is live on its owner thread.
        status_result(unsafe { elaris_ladybird_suspend_downloads(self.owner.raw.as_ptr()) })
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

fn path_cstring(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "path contains an interior NUL byte".to_owned())
}
