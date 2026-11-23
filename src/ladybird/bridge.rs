//! Safe owner-thread wrappers for the optional Ladybird C ABI.

use std::ffi::{CStr, CString, c_char, c_void};
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use super::{DisplayMetadata, Event, Favicon, Frame, GpuFrame, GpuPoll, Key, KeyEvent, MouseEvent};
use elaris_core::downloads::{Status as DownloadStatus, Update as DownloadUpdate};

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
    view_id: u64,
    text: *const c_char,
    value: i32,
    favicon_rgba: *const u8,
    favicon_width: i32,
    favicon_height: i32,
    favicon_stride: i32,
    request_id: u64,
    selection: *const c_char,
    caret: [i32; 4],
    download_id: u64,
    bytes_downloaded: u64,
    total_bytes: u64,
    has_total_bytes: i32,
    download_status: i32,
    destination: *const c_char,
    error: *const c_char,
    current_match_index: u64,
    total_match_count: u64,
    has_total_match_count: i32,
}

#[repr(C)]
struct RawGpuFrame {
    io_surface: *mut c_void,
    dmabuf_fd: i32,
    dmabuf_offset: u32,
    dmabuf_stride: u32,
    dmabuf_format: u32,
    dmabuf_modifier: u64,
    bitmap_id: i32,
    width: i32,
    height: i32,
    device_pixel_ratio: f64,
    serial: u64,
}

impl Default for RawGpuFrame {
    fn default() -> Self {
        Self {
            io_surface: std::ptr::null_mut(),
            dmabuf_fd: -1,
            dmabuf_offset: 0,
            dmabuf_stride: 0,
            dmabuf_format: 0,
            dmabuf_modifier: 0,
            bitmap_id: -1,
            width: 0,
            height: 0,
            device_pixel_ratio: 1.0,
            serial: 0,
        }
    }
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
const BUSY: i32 = 2;
const PIXEL_FORMAT_RGBA8: i32 = 1;
const KEY_PRESS: i32 = 1;
const KEY_RELEASE: i32 = 2;
const HISTORY_BACK: i32 = 1;
const HISTORY_FORWARD: i32 = 2;
const FIND_RESULT: i32 = 13;
const SELECTED_TEXT: i32 = 14;

type EventCallback = unsafe extern "C" fn(*mut c_void, *const RawEvent);

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

unsafe extern "C" {
    fn elaris_ladybird_set_dev_options(options: u32);
    fn elaris_ladybird_create(
        resource_dir: *const c_char,
        profile_dir: *const c_char,
        callback: Option<EventCallback>,
        context: *mut c_void,
        out_bridge: *mut *mut RawBridge,
    ) -> i32;
    fn elaris_ladybird_set_download_directory(bridge: *mut RawBridge, path: *const c_char) -> i32;
    fn elaris_ladybird_start_download(
        bridge: *mut RawBridge,
        url: *const c_char,
        suggested_filename: *const c_char,
        out_download_id: *mut u64,
    ) -> i32;
    fn elaris_ladybird_cancel_download(bridge: *mut RawBridge, download_id: u64) -> i32;
    fn elaris_ladybird_cancel_active_downloads(bridge: *mut RawBridge) -> i32;
    fn elaris_ladybird_pause_download(bridge: *mut RawBridge, id: u64, pause: i32) -> i32;
    fn elaris_ladybird_suspend_downloads(bridge: *mut RawBridge) -> i32;
    fn elaris_ladybird_create_view(
        bridge: *mut RawBridge,
        view_id: u64,
        out_view: *mut *mut RawView,
    ) -> i32;
    fn elaris_ladybird_set_display_metadata(
        view: *mut RawView,
        display_id: u32,
        refresh_rate: f64,
    ) -> i32;
    fn elaris_ladybird_load(view: *mut RawView, url: *const c_char) -> i32;
    fn elaris_ladybird_navigation_items(view: *mut RawView, direction: i32) -> i32;
    fn elaris_ladybird_traverse_to_step(view: *mut RawView, step: i32) -> i32;
    fn elaris_ladybird_reload(view: *mut RawView) -> i32;
    fn elaris_ladybird_reload_bypassing_cache(view: *mut RawView) -> i32;
    fn elaris_ladybird_set_zoom(view: *mut RawView, zoom: f64) -> i32;
    fn elaris_ladybird_stop_loading(view: *mut RawView) -> i32;
    fn elaris_ladybird_traverse_history(view: *mut RawView, delta: i32) -> i32;
    fn elaris_ladybird_set_visible(view: *mut RawView, visible: i32) -> i32;
    fn elaris_ladybird_resize(view: *mut RawView, width: i32, height: i32, scale: f64) -> i32;
    fn elaris_ladybird_frame(view: *mut RawView, out_frame: *mut RawFrame) -> i32;
    fn elaris_ladybird_copy_frame(view: *mut RawView, rgba: *mut u8, length: usize) -> i32;
    fn elaris_ladybird_gpu_frames_supported() -> i32;
    fn elaris_ladybird_set_gpu_frames(view: *mut RawView, enabled: i32) -> i32;
    fn elaris_ladybird_gpu_frame(view: *mut RawView, out_frame: *mut RawGpuFrame) -> i32;
    fn elaris_ladybird_release_frames(view: *mut RawView, completed_serial: u64) -> i32;
    fn elaris_ladybird_mouse_event(view: *mut RawView, event: *const RawMouseEvent) -> i32;
    fn elaris_ladybird_discard_prepared(view: *mut RawView, request: u64) -> i32;
    fn elaris_ladybird_load_prepared(view: *mut RawView, source: u64, request: u64) -> i32;
    fn elaris_ladybird_open_context_target(view: *mut RawView, request: u64, new_tab: i32) -> i32;
    fn elaris_ladybird_media_action(view: *mut RawView, action: i32) -> i32;
    fn elaris_ladybird_find(view: *mut RawView, query: *const c_char, request_id: u64) -> i32;
    fn elaris_ladybird_find_next(view: *mut RawView, request_id: u64) -> i32;
    fn elaris_ladybird_find_previous(view: *mut RawView, request_id: u64) -> i32;
    fn elaris_ladybird_adopt_popup(
        bridge: *mut RawBridge,
        source: u64,
        request_id: u64,
        id: u64,
        view: *mut *mut RawView,
    ) -> i32;
    fn elaris_ladybird_exit_web_fullscreen(view: *mut RawView) -> i32;
    fn elaris_ladybird_create_private_view(
        bridge: *mut RawBridge,
        id: u64,
        view: *mut *mut RawView,
    ) -> i32;
    fn elaris_ladybird_reset_private_session(bridge: *mut RawBridge) -> i32;
    fn elaris_ladybird_set_autoplay(
        bridge: *mut RawBridge,
        default_policy: i32,
        json: *const c_char,
    ) -> i32;
    fn elaris_ladybird_set_content_blocking(
        bridge: *mut RawBridge,
        enabled: i32,
        json: *const c_char,
    ) -> i32;
    fn elaris_ladybird_clear_browsing_data(
        bridge: *mut RawBridge,
        since_ms: i64,
        flags: i32,
        request_id: u64,
    ) -> i32;
    fn elaris_ladybird_reply_to_external_url(view: *mut RawView, request_id: u64, open: i32)
    -> i32;
    fn elaris_ladybird_set_accessibility(view: *mut RawView, enabled: i32) -> i32;
    fn elaris_ladybird_accessibility_action(
        view: *mut RawView,
        node: i64,
        action: *const c_char,
    ) -> i32;
    fn elaris_ladybird_accessibility_select(
        view: *mut RawView,
        node: i64,
        start: i32,
        end: i32,
    ) -> i32;
    fn elaris_ladybird_reply_to_permission(view: *mut RawView, request_id: u64, choice: i32)
    -> i32;
    fn elaris_ladybird_set_permission_default(
        bridge: *mut RawBridge,
        key: *const c_char,
        blocked: i32,
    ) -> i32;
    fn elaris_ladybird_set_permissions(bridge: *mut RawBridge, json: *const c_char) -> i32;
    fn elaris_ladybird_set_permission(
        bridge: *mut RawBridge,
        origin: *const c_char,
        key: *const c_char,
        choice: i32,
    ) -> i32;
    fn elaris_ladybird_reply_to_dialog(
        view: *mut RawView,
        request_id: u64,
        accepted: i32,
        response: *const u8,
        response_size: usize,
    ) -> i32;
    fn elaris_ladybird_reply_to_picker(
        view: *mut RawView,
        request_id: u64,
        response: *const u8,
        response_size: usize,
    ) -> i32;
    fn elaris_ladybird_preflight_close(view: *mut RawView) -> i32;
    fn elaris_ladybird_find_end(view: *mut RawView) -> i32;
    fn elaris_ladybird_selected_text(view: *mut RawView, request_id: u64) -> i32;
    fn elaris_ladybird_edit(view: *mut RawView, command: i32, bytes: *const u8, len: usize) -> i32;
    fn elaris_ladybird_key_event(view: *mut RawView, event: *const RawKeyEvent) -> i32;
    fn elaris_ladybird_set_focus(view: *mut RawView, focused: i32) -> i32;
    fn elaris_ladybird_set_color_scheme(bridge: *mut RawBridge, dark: i32) -> i32;
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
    callback: Box<dyn FnMut(u64, Event)>,
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
            5 => Event::History {
                back: raw.value & HISTORY_BACK != 0,
                forward: raw.value & HISTORY_FORWARD != 0,
            },
            6 => Event::Favicon(copy_favicon(raw)),
            7 => Event::NewTab {
                url: text,
                request: raw.request_id,
                activate: raw.value != 0,
            },
            8 => Event::ContextMenu(super::PageMenu {
                request: raw.request_id,
                url: text,
                image: raw.value & 1 != 0,
                editable: raw.value & 2 != 0,
                media: (raw.value & 4 != 0).then_some(super::MediaMenu {
                    playing: raw.value & 8 != 0,
                    muted: raw.value & 16 != 0,
                    can_enter_picture_in_picture: raw.value & 32 != 0,
                    in_picture_in_picture: raw.value & 64 != 0,
                }),
                selection: if raw.selection.is_null() {
                    String::new()
                } else {
                    // SAFETY: The bridge lends this string for the duration of the callback.
                    unsafe { CStr::from_ptr(raw.selection) }
                        .to_string_lossy()
                        .into_owned()
                },
            }),
            9 => Event::Clipboard(text),
            10 => Event::ImeCaret((raw.value != 0).then_some(raw.caret)),
            11 => Event::NavigationCommitted(text),
            15 => Event::Peek {
                url: text,
                request: raw.request_id,
            },
            16 => Event::LoadError(text),
            21 => Event::CloseDecision(raw.value != 0),
            23 => Event::NavigationItems(serde_json::from_str(&text).unwrap_or_default()),
            22 => Event::FormPicker(
                serde_json::from_str(&text)
                    .ok()
                    .map(|picker| (raw.request_id, picker)),
            ),
            17 => Event::HoveredLink(text),
            18 => Event::Tooltip(text),
            19 => Event::Cursor(text),
            20 => {
                use super::{PageDialog, PageDialogKind};
                let kind = match raw.value {
                    1 => Some(PageDialogKind::Alert),
                    2 => Some(PageDialogKind::Confirm),
                    3 => Some(PageDialogKind::Prompt),
                    4 => Some(PageDialogKind::BeforeUnload),
                    _ => None,
                };
                Event::PageDialog(kind.map(|kind| PageDialog {
                    request_id: raw.request_id,
                    kind,
                    message: text,
                    default_text: if raw.selection.is_null() {
                        String::new()
                    } else {
                        // SAFETY: The bridge lends this string for the duration of the callback.
                        unsafe { CStr::from_ptr(raw.selection) }
                            .to_string_lossy()
                            .into_owned()
                    },
                }))
            }
            24 | 25 => {
                let key = if raw.selection.is_null() {
                    String::new()
                } else {
                    // SAFETY: The bridge lends this NUL-terminated string through callback return.
                    unsafe { CStr::from_ptr(raw.selection) }
                        .to_string_lossy()
                        .into_owned()
                };
                // Elaris names Ladybird's geolocation permission "location".
                let key = if key == "geolocation" {
                    "location".into()
                } else {
                    key
                };
                if raw.kind == 24 {
                    Event::PermissionPrompt((raw.value != 0).then_some(super::PermissionPrompt {
                        request_id: raw.request_id,
                        requesting_origin: if raw.destination.is_null() {
                            text.clone()
                        } else {
                            // SAFETY: The bridge lends this NUL-terminated string through callback return.
                            unsafe { CStr::from_ptr(raw.destination) }
                                .to_string_lossy()
                                .into_owned()
                        },
                        origin: text,
                        key,
                        os_denied: raw.value == 2,
                    }))
                } else {
                    Event::PermissionSaved {
                        origin: text,
                        key,
                        allowed: raw.value == 1,
                    }
                }
            }
            26 => Event::PermissionUse {
                key: text,
                visible: raw.value != 0,
            },
            27 => Event::WebFullscreen(raw.value != 0),
            33 => Event::WheelResult {
                cancelled: raw.value != 0,
            },
            28 => Event::Popup {
                request: raw.request_id,
                activate: raw.value != 0,
            },
            29 => Event::PopupClosed,
            32 => match super::AccessibilityEvent::parse(raw.value, &text) {
                Some(event) => Event::Accessibility(event),
                None => return,
            },
            31 => Event::BrowsingDataCleared {
                request: raw.request_id,
                cleared: raw.value != 0,
            },
            30 => Event::ExternalUrl((raw.value != 0).then(|| super::ExternalUrl {
                request_id: raw.request_id,
                url: text,
                // SAFETY: The bridge lends these NUL-terminated strings through callback return.
                origin: unsafe { borrowed_text(raw.destination) },
                application: unsafe { borrowed_text(raw.selection) },
                unavailable: raw.value == 2,
            })),
            12 => {
                let status = match raw.download_status {
                    1 => DownloadStatus::Downloading,
                    2 => DownloadStatus::Paused,
                    3 => DownloadStatus::Completed,
                    4 => DownloadStatus::Cancelled,
                    5 => DownloadStatus::Failed,
                    _ => return,
                };
                let filename = if raw.selection.is_null() {
                    "download".to_owned()
                } else {
                    // SAFETY: Download strings are borrowed until the event callback returns.
                    unsafe { CStr::from_ptr(raw.selection) }
                        .to_string_lossy()
                        .into_owned()
                };
                let destination = if raw.destination.is_null() {
                    None
                } else {
                    // SAFETY: Download strings are borrowed until the event callback returns.
                    let path = unsafe { CStr::from_ptr(raw.destination) }
                        .to_string_lossy()
                        .into_owned();
                    (!path.is_empty()).then(|| PathBuf::from(path))
                };
                let error = if raw.error.is_null() {
                    None
                } else {
                    // SAFETY: Download strings are borrowed until the event callback returns.
                    Some(
                        unsafe { CStr::from_ptr(raw.error) }
                            .to_string_lossy()
                            .into_owned(),
                    )
                };
                Event::Download(DownloadUpdate {
                    filename,
                    source_url: text,
                    destination,
                    status,
                    bytes_downloaded: raw.bytes_downloaded,
                    total_bytes: (raw.has_total_bytes != 0).then_some(raw.total_bytes),
                    error,
                    private: raw.value & 1 != 0,
                    can_resume: raw.value & 2 != 0,
                    retrying: raw.value & 4 != 0,
                    native_id: raw.download_id,
                })
            }
            FIND_RESULT => Event::FindResult {
                request_id: raw.request_id,
                current_match_index: raw.current_match_index,
                total_match_count: (raw.has_total_match_count != 0)
                    .then_some(raw.total_match_count),
            },
            SELECTED_TEXT => Event::SelectedText {
                request_id: raw.request_id,
                text,
            },
            _ => return,
        };
        (state.callback)(raw.view_id, event);
    }));
    state.panicked |= result.is_err();
}

fn copy_favicon(raw: &RawEvent) -> Option<Favicon> {
    let width = u32::try_from(raw.favicon_width).ok()?;
    let height = u32::try_from(raw.favicon_height).ok()?;
    let stride = usize::try_from(raw.favicon_stride).ok()?;
    if width == 0 || height == 0 || raw.favicon_rgba.is_null() {
        return None;
    }
    let row_bytes = usize::try_from(width).ok()?.checked_mul(4)?;
    if stride < row_bytes {
        return None;
    }
    let byte_len = row_bytes.checked_mul(usize::try_from(height).ok()?)?;
    let height_usize = usize::try_from(height).ok()?;
    let source_len = stride.checked_mul(height_usize)?;
    if source_len > isize::MAX as usize {
        return None;
    }
    // SAFETY: The C bridge keeps favicon bytes alive through this callback and supplies `source_len` bytes.
    let rows = unsafe { std::slice::from_raw_parts(raw.favicon_rgba, source_len) };
    let mut rgba = Vec::with_capacity(byte_len);
    for row in rows.chunks_exact(stride).take(height_usize) {
        rgba.extend_from_slice(&row[..row_bytes]);
    }
    Some(Favicon {
        width,
        height,
        rgba,
    })
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

/// Whether this build of the bridge can share frames with the host's GPU.
pub fn gpu_frames_supported() -> bool {
    // SAFETY: A pure query with no arguments.
    unsafe { elaris_ladybird_gpu_frames_supported() != 0 }
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
        data: super::BrowsingData,
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

/// One page view. It keeps its application alive and, like it, stays on the initializing thread.
pub struct WebView {
    raw: NonNull<RawView>,
    /// Keeps the bridge alive until after this view is destroyed.
    _owner: Rc<Owner>,
}

impl WebView {
    fn set_display_metadata(&self, display: DisplayMetadata) -> Result<(), String> {
        // SAFETY: The view is live and owned by this thread.
        status_result(unsafe {
            elaris_ladybird_set_display_metadata(
                self.raw.as_ptr(),
                display.id.unwrap_or(0),
                display.refresh_rate,
            )
        })
    }

    /// Searches the page case-insensitively and highlights all matches.
    pub fn find(&mut self, query: &str, request_id: u64) -> Result<(), String> {
        let query = CString::new(query)
            .map_err(|_| "find query contains an interior NUL byte".to_owned())?;
        // SAFETY: The view is live on its owner's thread and the query outlives the call.
        status_result(unsafe {
            elaris_ladybird_find(self.raw.as_ptr(), query.as_ptr(), request_id)
        })
    }

    /// Selects the next highlighted match and reports its position asynchronously.
    pub fn find_next(&mut self, request_id: u64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_find_next(self.raw.as_ptr(), request_id) })
    }

    /// Selects the previous highlighted match and reports its position asynchronously.
    pub fn find_previous(&mut self, request_id: u64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_find_previous(self.raw.as_ptr(), request_id) })
    }

    /// Asks the active document to exit element fullscreen.
    pub fn exit_web_fullscreen(&mut self) -> Result<(), String> {
        // SAFETY: The view is live on its owner thread.
        status_result(unsafe { elaris_ladybird_exit_web_fullscreen(self.raw.as_ptr()) })
    }

    /// Starts or stops reporting the page's accessibility tree. Starting asks for it whole.
    pub fn set_accessibility(&mut self, enabled: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_set_accessibility(self.raw.as_ptr(), i32::from(enabled))
        })
    }

    /// Asks the page to `"press"`, `"focus"` or `"scroll_into_view"` an accessibility node.
    pub fn accessibility_action(&mut self, node: i64, action: &str) -> Result<(), String> {
        let action = CString::new(action).map_err(|error| error.to_string())?;
        // SAFETY: The view is live on its owner thread and borrows the string for this call.
        status_result(unsafe {
            elaris_ladybird_accessibility_action(self.raw.as_ptr(), node, action.as_ptr())
        })
    }

    /// Sets the caret (`start == end`) or selection in a text node, in character offsets.
    pub fn accessibility_select(&mut self, node: i64, start: i32, end: i32) -> Result<(), String> {
        // SAFETY: The view is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_accessibility_select(self.raw.as_ptr(), node, start, end)
        })
    }

    /// Answers the view's current external link request; stale ids are ignored.
    pub fn reply_to_external_url(&mut self, request_id: u64, open: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_reply_to_external_url(self.raw.as_ptr(), request_id, i32::from(open))
        })
    }

    /// Replies to the active permission prompt if its document still owns it.
    pub fn reply_to_permission(
        &mut self,
        request_id: u64,
        choice: Option<bool>,
    ) -> Result<(), String> {
        // SAFETY: The view is live on its owner thread.
        status_result(unsafe {
            elaris_ladybird_reply_to_permission(
                self.raw.as_ptr(),
                request_id,
                permission_choice(choice),
            )
        })
    }

    /// Replies to the requested page dialog if its document still owns it.
    pub fn reply_to_dialog(
        &mut self,
        request_id: u64,
        response: Option<&str>,
    ) -> Result<(), String> {
        let text = response.unwrap_or_default();
        // SAFETY: The view is live on its owner thread and the text is borrowed for the call.
        status_result(unsafe {
            elaris_ladybird_reply_to_dialog(
                self.raw.as_ptr(),
                request_id,
                i32::from(response.is_some()),
                text.as_ptr(),
                text.len(),
            )
        })
    }

    /// Replies to the picker that still owns the request ID.
    pub fn reply_to_picker(
        &mut self,
        request_id: u64,
        response: Option<&[u8]>,
    ) -> Result<(), String> {
        let (bytes, size) =
            response.map_or((std::ptr::null(), 0), |bytes| (bytes.as_ptr(), bytes.len()));
        // SAFETY: The view belongs to this thread and the response is borrowed for the call.
        status_result(unsafe {
            elaris_ladybird_reply_to_picker(self.raw.as_ptr(), request_id, bytes, size)
        })
    }

    /// Asks the live page whether the tab can close, including its beforeunload handler.
    pub fn preflight_close(&mut self) -> Result<(), String> {
        // SAFETY: The handle belongs to this live view on its owner thread.
        status_result(unsafe { elaris_ladybird_preflight_close(self.raw.as_ptr()) })
    }

    pub fn find_end(&mut self) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_find_end(self.raw.as_ptr()) })
    }

    /// Reads the selected webpage text asynchronously without blocking the owner thread.
    pub fn selected_text(&mut self, request_id: u64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_selected_text(self.raw.as_ptr(), request_id) })
    }

    pub fn load(&mut self, url: &str) -> Result<(), String> {
        let url = CString::new(url).map_err(|_| "URL contains an interior NUL byte".to_owned())?;
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_load(self.raw.as_ptr(), url.as_ptr()) })
    }

    pub fn reload(&mut self) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_reload(self.raw.as_ptr()) })
    }

    /// Reloads the current page without using cached resources.
    pub fn reload_bypassing_cache(&mut self) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_reload_bypassing_cache(self.raw.as_ptr()) })
    }

    /// Sets the page zoom factor.
    pub fn set_zoom(&mut self, zoom: f64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread; the bridge validates zoom.
        status_result(unsafe { elaris_ladybird_set_zoom(self.raw.as_ptr(), zoom) })
    }

    pub fn stop_loading(&mut self) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_stop_loading(self.raw.as_ptr()) })
    }

    /// Requests the targets in one direction of the view's session history.
    pub fn navigation_items(&mut self, direction: i32) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread and the bridge validates the direction.
        status_result(unsafe { elaris_ladybird_navigation_items(self.raw.as_ptr(), direction) })
    }

    /// Traverses to a step reported by the view's session history.
    pub fn traverse_to_step(&mut self, step: i32) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_traverse_to_step(self.raw.as_ptr(), step) })
    }

    /// Moves through the session history: -1 goes back, 1 forward.
    pub fn traverse_history(&mut self, delta: i32) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_traverse_history(self.raw.as_ptr(), delta) })
    }

    /// Whether the view is on screen; a hidden one stops painting.
    pub fn set_visible(&mut self, visible: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_set_visible(self.raw.as_ptr(), visible.into()) })
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
        // Copying sizes its buffer from these, so anything but the documented layout is refused.
        if raw.format != PIXEL_FORMAT_RGBA8
            || raw.width <= 0
            || raw.height <= 0
            || i64::from(raw.stride) != i64::from(raw.width) * 4
        {
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

    /// Makes [`Self::gpu_frame`] available (macOS and Linux). Once on, call [`Self::release_frames`] as
    /// the host's GPU finishes frames.
    pub fn set_gpu_frames(&mut self, enabled: bool) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe { elaris_ladybird_set_gpu_frames(self.raw.as_ptr(), enabled.into()) })
    }

    /// Copies the frame on display into a GPU surface of the bridge's own.
    pub fn gpu_frame(&self) -> Result<GpuPoll, String> {
        let mut raw = RawGpuFrame::default();
        // SAFETY: The view is live on its owner's thread and `raw` is a valid output.
        match unsafe { elaris_ladybird_gpu_frame(self.raw.as_ptr(), &mut raw) } {
            0 => {}
            NO_FRAME => return Ok(GpuPoll::NoFrame),
            BUSY => return Ok(GpuPoll::Busy),
            _ => return Err(last_error()),
        }
        #[cfg(target_os = "linux")]
        let owned_fd = (raw.dmabuf_fd >= 0).then(|| {
            // SAFETY: The bridge duplicated this descriptor for the host, which owns it now.
            unsafe { std::os::fd::OwnedFd::from_raw_fd(raw.dmabuf_fd) }
        });
        #[cfg(target_os = "macos")]
        let shared = !raw.io_surface.is_null();
        #[cfg(target_os = "linux")]
        let shared = owned_fd.is_some();
        if !shared || raw.width <= 0 || raw.height <= 0 {
            return Err("Ladybird described an unsupported GPU frame".to_owned());
        }
        Ok(GpuPoll::Frame(GpuFrame {
            #[cfg(target_os = "macos")]
            io_surface: raw.io_surface,
            #[cfg(target_os = "linux")]
            dmabuf: super::DmabufPlane {
                fd: owned_fd.ok_or_else(|| {
                    "Ladybird sent a DMA-BUF frame without a descriptor".to_owned()
                })?,
                offset: raw.dmabuf_offset,
                stride: raw.dmabuf_stride,
                format: raw.dmabuf_format,
                modifier: raw.dmabuf_modifier,
            },
            #[cfg(target_os = "linux")]
            bitmap_id: raw.bitmap_id,
            width: raw.width as u32,
            height: raw.height as u32,
            scale: raw.device_pixel_ratio,
            serial: raw.serial,
        }))
    }

    /// Tells the bridge the host's GPU has finished drawing the frame `completed_serial`, so the
    /// surfaces of older frames can take new ones.
    pub fn release_frames(&mut self, completed_serial: u64) -> Result<(), String> {
        // SAFETY: The view is live on its owner's thread.
        status_result(unsafe {
            elaris_ladybird_release_frames(self.raw.as_ptr(), completed_serial)
        })
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

    pub fn edit(&mut self, edit: &super::Edit) -> Result<(), String> {
        use super::Edit::*;
        let (command, text): (i32, &str) = match edit {
            Copy => (0, ""),
            Cut => (1, ""),
            Paste(s) => (2, s),
            SelectAll => (3, ""),
            Mark(s) => (4, s),
            Commit(s) => (5, s),
        };
        // SAFETY: Text is borrowed only during this call on the view's owner thread.
        status_result(unsafe {
            elaris_ladybird_edit(self.raw.as_ptr(), command, text.as_ptr(), text.len())
        })
    }

    pub fn load_prepared(&mut self, source: u64, request: u64) -> Result<(), String> {
        // SAFETY: The bridge resolves both identifiers on the owner thread.
        status_result(unsafe { elaris_ladybird_load_prepared(self.raw.as_ptr(), source, request) })
    }

    /// Releases a prepared navigation Elaris chose not to open.
    pub fn discard_prepared(&mut self, request: u64) -> Result<(), String> {
        // SAFETY: The view lives on the calling engine thread.
        status_result(unsafe { elaris_ladybird_discard_prepared(self.raw.as_ptr(), request) })
    }

    pub fn open_context_target(&mut self, request: u64, new_tab: bool) -> Result<(), String> {
        // SAFETY: The bridge checks that this request is still current.
        status_result(unsafe {
            elaris_ladybird_open_context_target(self.raw.as_ptr(), request, new_tab.into())
        })
    }

    pub fn media_action(&mut self, action: super::MediaAction) -> Result<(), String> {
        // SAFETY: The view lives on the calling engine thread.
        status_result(unsafe { elaris_ladybird_media_action(self.raw.as_ptr(), action as i32) })
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
            Key::Function(n @ 1..=12) => (32 + i32::from(n) - 1, 0),
            Key::Function(_) => return Err("unsupported function key".into()),
            Key::Insert => (15, 0),
            Key::PrintScreen => (16, 0),
            Key::ScrollLock => (17, 0),
            Key::Pause => (18, 0),
            Key::ShiftLeft => (19, 0),
            Key::ShiftRight => (20, 0),
            Key::ControlLeft => (21, 0),
            Key::ControlRight => (22, 0),
            Key::AltLeft => (23, 0),
            Key::AltRight => (24, 0),
            Key::SuperLeft => (25, 0),
            Key::SuperRight => (26, 0),
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

/// Copies a string the bridge lends for the duration of a callback.
///
/// # Safety
/// `text` must be null or a NUL-terminated string valid for this call.
unsafe fn borrowed_text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: The caller guarantees a live NUL-terminated string.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

fn permission_choice(choice: Option<bool>) -> i32 {
    match choice {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
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
            callback: Box::new(|_, _| panic!("callback regression test")),
            panicked: false,
        };
        let event = RawEvent {
            kind: 4,
            view_id: 1,
            text: std::ptr::null(),
            value: 0,
            favicon_rgba: std::ptr::null(),
            favicon_width: 0,
            favicon_height: 0,
            favicon_stride: 0,
            request_id: 0,
            selection: std::ptr::null(),
            caret: [0; 4],
            download_id: 0,
            bytes_downloaded: 0,
            total_bytes: 0,
            has_total_bytes: 0,
            download_status: 0,
            destination: std::ptr::null(),
            error: std::ptr::null(),
            current_match_index: 0,
            total_match_count: 0,
            has_total_match_count: 0,
        };
        // SAFETY: Both pointers reference stack values that remain alive through the callback.
        unsafe {
            dispatch((&mut state as *mut CallbackState).cast::<c_void>(), &event);
        }
        assert!(state.panicked);
    }
}
