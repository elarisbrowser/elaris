// SPDX-License-Identifier: GPL-3.0-only

//! Event callback trampolines: decodes the bridge's C event stream into Rust events.

use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use crate::ladybird::{Event, Favicon};
use elaris_core::downloads::{Status as DownloadStatus, Update as DownloadUpdate};

#[repr(C)]
pub(super) struct RawEvent {
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

const HISTORY_BACK: i32 = 1;
const HISTORY_FORWARD: i32 = 2;
const FIND_RESULT: i32 = 13;
const SELECTED_TEXT: i32 = 14;

pub(super) type EventCallback = unsafe extern "C" fn(*mut c_void, *const RawEvent);

pub(super) struct CallbackState {
    pub(super) callback: Box<dyn FnMut(u64, Event)>,
    pub(super) panicked: bool,
}

// The C++ callback executes synchronously from the owner's event-loop pump.
pub(super) unsafe extern "C" fn dispatch(context: *mut c_void, raw: *const RawEvent) {
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
            8 => Event::ContextMenu(crate::ladybird::PageMenu {
                request: raw.request_id,
                url: text,
                image: raw.value & 1 != 0,
                editable: raw.value & 2 != 0,
                media: (raw.value & 4 != 0).then_some(crate::ladybird::MediaMenu {
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
                use crate::ladybird::{PageDialog, PageDialogKind};
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
                    Event::PermissionPrompt((raw.value != 0).then_some(
                        crate::ladybird::PermissionPrompt {
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
                        },
                    ))
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
            32 => match crate::ladybird::AccessibilityEvent::parse(raw.value, &text) {
                Some(event) => Event::Accessibility(event),
                None => return,
            },
            31 => Event::BrowsingDataCleared {
                request: raw.request_id,
                cleared: raw.value != 0,
            },
            30 => Event::ExternalUrl((raw.value != 0).then(|| crate::ladybird::ExternalUrl {
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
