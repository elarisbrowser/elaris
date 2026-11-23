// SPDX-License-Identifier: GPL-3.0-only

//! Raw C ABI types, status codes and `extern "C"` declarations for the Ladybird bridge.

use std::ffi::{CStr, c_char, c_void};

use super::callback::EventCallback;

#[repr(C)]
pub(super) struct RawBridge {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct RawView {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct RawWaker {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct RawGpuFrame {
    pub(super) io_surface: *mut c_void,
    pub(super) dmabuf_fd: i32,
    pub(super) dmabuf_offset: u32,
    pub(super) dmabuf_stride: u32,
    pub(super) dmabuf_format: u32,
    pub(super) dmabuf_modifier: u64,
    pub(super) bitmap_id: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) device_pixel_ratio: f64,
    pub(super) serial: u64,
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
pub(super) struct RawFrame {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) stride: i32,
    pub(super) format: i32,
    pub(super) device_pixel_ratio: f64,
    pub(super) damage_x: i32,
    pub(super) damage_y: i32,
    pub(super) damage_width: i32,
    pub(super) damage_height: i32,
    pub(super) serial: u64,
}

#[repr(C)]
pub(super) struct RawMouseEvent {
    pub(super) kind: i32,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) button: i32,
    pub(super) buttons: i32,
    pub(super) modifiers: i32,
    pub(super) wheel_delta_x: f64,
    pub(super) wheel_delta_y: f64,
    pub(super) wheel_precise: i32,
    pub(super) wheel_phase: i32,
    pub(super) click_count: i32,
}

#[repr(C)]
pub(super) struct RawKeyEvent {
    pub(super) kind: i32,
    pub(super) key: i32,
    pub(super) modifiers: i32,
    pub(super) code_point: u32,
    pub(super) repeat: i32,
}

unsafe extern "C" {
    pub(super) fn elaris_ladybird_set_dev_options(options: u32);
    pub(super) fn elaris_ladybird_create(
        resource_dir: *const c_char,
        profile_dir: *const c_char,
        callback: Option<EventCallback>,
        context: *mut c_void,
        out_bridge: *mut *mut RawBridge,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_download_directory(
        bridge: *mut RawBridge,
        path: *const c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_start_download(
        bridge: *mut RawBridge,
        url: *const c_char,
        suggested_filename: *const c_char,
        out_download_id: *mut u64,
    ) -> i32;
    pub(super) fn elaris_ladybird_cancel_download(bridge: *mut RawBridge, download_id: u64) -> i32;
    pub(super) fn elaris_ladybird_cancel_active_downloads(bridge: *mut RawBridge) -> i32;
    pub(super) fn elaris_ladybird_pause_download(
        bridge: *mut RawBridge,
        id: u64,
        pause: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_suspend_downloads(bridge: *mut RawBridge) -> i32;
    pub(super) fn elaris_ladybird_create_view(
        bridge: *mut RawBridge,
        view_id: u64,
        out_view: *mut *mut RawView,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_display_metadata(
        view: *mut RawView,
        display_id: u32,
        refresh_rate: f64,
    ) -> i32;
    pub(super) fn elaris_ladybird_load(view: *mut RawView, url: *const c_char) -> i32;
    pub(super) fn elaris_ladybird_navigation_items(view: *mut RawView, direction: i32) -> i32;
    pub(super) fn elaris_ladybird_traverse_to_step(view: *mut RawView, step: i32) -> i32;
    pub(super) fn elaris_ladybird_reload(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_reload_bypassing_cache(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_set_zoom(view: *mut RawView, zoom: f64) -> i32;
    pub(super) fn elaris_ladybird_stop_loading(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_traverse_history(view: *mut RawView, delta: i32) -> i32;
    pub(super) fn elaris_ladybird_set_visible(view: *mut RawView, visible: i32) -> i32;
    pub(super) fn elaris_ladybird_resize(
        view: *mut RawView,
        width: i32,
        height: i32,
        scale: f64,
    ) -> i32;
    pub(super) fn elaris_ladybird_frame(view: *mut RawView, out_frame: *mut RawFrame) -> i32;
    pub(super) fn elaris_ladybird_copy_frame(
        view: *mut RawView,
        rgba: *mut u8,
        length: usize,
    ) -> i32;
    pub(super) fn elaris_ladybird_gpu_frames_supported() -> i32;
    pub(super) fn elaris_ladybird_set_gpu_frames(view: *mut RawView, enabled: i32) -> i32;
    pub(super) fn elaris_ladybird_gpu_frame(view: *mut RawView, out_frame: *mut RawGpuFrame)
    -> i32;
    pub(super) fn elaris_ladybird_release_frames(view: *mut RawView, completed_serial: u64) -> i32;
    pub(super) fn elaris_ladybird_mouse_event(
        view: *mut RawView,
        event: *const RawMouseEvent,
    ) -> i32;
    pub(super) fn elaris_ladybird_discard_prepared(view: *mut RawView, request: u64) -> i32;
    pub(super) fn elaris_ladybird_load_prepared(
        view: *mut RawView,
        source: u64,
        request: u64,
    ) -> i32;
    pub(super) fn elaris_ladybird_open_context_target(
        view: *mut RawView,
        request: u64,
        new_tab: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_media_action(view: *mut RawView, action: i32) -> i32;
    pub(super) fn elaris_ladybird_find(
        view: *mut RawView,
        query: *const c_char,
        request_id: u64,
    ) -> i32;
    pub(super) fn elaris_ladybird_find_next(view: *mut RawView, request_id: u64) -> i32;
    pub(super) fn elaris_ladybird_find_previous(view: *mut RawView, request_id: u64) -> i32;
    pub(super) fn elaris_ladybird_adopt_popup(
        bridge: *mut RawBridge,
        source: u64,
        request_id: u64,
        id: u64,
        view: *mut *mut RawView,
    ) -> i32;
    pub(super) fn elaris_ladybird_exit_web_fullscreen(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_create_private_view(
        bridge: *mut RawBridge,
        id: u64,
        view: *mut *mut RawView,
    ) -> i32;
    pub(super) fn elaris_ladybird_reset_private_session(bridge: *mut RawBridge) -> i32;
    pub(super) fn elaris_ladybird_set_autoplay(
        bridge: *mut RawBridge,
        default_policy: i32,
        json: *const c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_content_blocking(
        bridge: *mut RawBridge,
        enabled: i32,
        json: *const c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_processes(
        bridge: *mut RawBridge,
        out_json: *mut *mut c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_end_process(bridge: *mut RawBridge, child_id: u64) -> i32;
    pub(super) fn elaris_ladybird_free_string(string: *mut c_char);
    pub(super) fn elaris_ladybird_clear_browsing_data(
        bridge: *mut RawBridge,
        since_ms: i64,
        flags: i32,
        request_id: u64,
    ) -> i32;
    pub(super) fn elaris_ladybird_reply_to_external_url(
        view: *mut RawView,
        request_id: u64,
        open: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_accessibility(view: *mut RawView, enabled: i32) -> i32;
    pub(super) fn elaris_ladybird_accessibility_action(
        view: *mut RawView,
        node: i64,
        action: *const c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_accessibility_select(
        view: *mut RawView,
        node: i64,
        start: i32,
        end: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_reply_to_permission(
        view: *mut RawView,
        request_id: u64,
        choice: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_permission_default(
        bridge: *mut RawBridge,
        key: *const c_char,
        blocked: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_permissions(
        bridge: *mut RawBridge,
        json: *const c_char,
    ) -> i32;
    pub(super) fn elaris_ladybird_set_permission(
        bridge: *mut RawBridge,
        origin: *const c_char,
        key: *const c_char,
        choice: i32,
    ) -> i32;
    pub(super) fn elaris_ladybird_reply_to_dialog(
        view: *mut RawView,
        request_id: u64,
        accepted: i32,
        response: *const u8,
        response_size: usize,
    ) -> i32;
    pub(super) fn elaris_ladybird_reply_to_picker(
        view: *mut RawView,
        request_id: u64,
        response: *const u8,
        response_size: usize,
    ) -> i32;
    pub(super) fn elaris_ladybird_preflight_close(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_find_end(view: *mut RawView) -> i32;
    pub(super) fn elaris_ladybird_selected_text(view: *mut RawView, request_id: u64) -> i32;
    pub(super) fn elaris_ladybird_edit(
        view: *mut RawView,
        command: i32,
        bytes: *const u8,
        len: usize,
    ) -> i32;
    pub(super) fn elaris_ladybird_key_event(view: *mut RawView, event: *const RawKeyEvent) -> i32;
    pub(super) fn elaris_ladybird_set_focus(view: *mut RawView, focused: i32) -> i32;
    pub(super) fn elaris_ladybird_set_color_scheme(bridge: *mut RawBridge, dark: i32) -> i32;
    pub(super) fn elaris_ladybird_pump(bridge: *mut RawBridge) -> i32;
    pub(super) fn elaris_ladybird_wait(bridge: *mut RawBridge, timeout_us: i32) -> i32;
    pub(super) fn elaris_ladybird_create_waker(bridge: *mut RawBridge) -> *mut RawWaker;
    pub(super) fn elaris_ladybird_wake(waker: *mut RawWaker);
    pub(super) fn elaris_ladybird_destroy_waker(waker: *mut RawWaker);
    pub(super) fn elaris_ladybird_destroy_view(view: *mut RawView);
    pub(super) fn elaris_ladybird_destroy(bridge: *mut RawBridge);
    pub(super) fn elaris_ladybird_last_error() -> *const c_char;
}

pub(super) fn last_error() -> String {
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

pub(super) fn permission_choice(choice: Option<bool>) -> i32 {
    match choice {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}

pub(super) fn status_result(status: i32) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(last_error())
    }
}
