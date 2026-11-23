// SPDX-License-Identifier: GPL-3.0-only

//! A page view: its navigation, frame, input and accessibility commands.

use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::fd::FromRawFd;
use std::ptr::NonNull;
use std::rc::Rc;

use super::app::Owner;
use super::ffi::*;
use crate::ladybird::{DisplayMetadata, Frame, GpuFrame, GpuPoll, Key, KeyEvent, MouseEvent};

const NO_FRAME: i32 = 1;
const BUSY: i32 = 2;
const PIXEL_FORMAT_RGBA8: i32 = 1;
const KEY_PRESS: i32 = 1;
const KEY_RELEASE: i32 = 2;

/// One page view. It keeps its application alive and, like it, stays on the initializing thread.
pub struct WebView {
    pub(super) raw: NonNull<RawView>,
    /// Keeps the bridge alive until after this view is destroyed.
    pub(super) _owner: Rc<Owner>,
}

impl WebView {
    pub(super) fn set_display_metadata(&self, display: DisplayMetadata) -> Result<(), String> {
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
            dmabuf: crate::ladybird::DmabufPlane {
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

    pub fn edit(&mut self, edit: &crate::ladybird::Edit) -> Result<(), String> {
        use crate::ladybird::Edit::*;
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

    pub fn media_action(&mut self, action: crate::ladybird::MediaAction) -> Result<(), String> {
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
