// SPDX-License-Identifier: GPL-3.0-only

//! Safe owner-thread wrappers for the optional Ladybird C ABI.

mod app;
mod callback;
mod ffi;
mod view;

pub use app::{Ladybird, Waker, gpu_frames_supported};
pub use view::WebView;
