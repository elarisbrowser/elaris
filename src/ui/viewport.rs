// SPDX-License-Identifier: GPL-3.0-only

//! Rounded webpage panes and their pointer, wheel, and keyboard input.

mod crash;
mod divider;
mod input;
mod layout;
pub(crate) mod menu;
mod overlay;
mod page;
mod panes;
mod picker;

pub(crate) use input::ime;
use input::paste;
#[cfg(target_os = "macos")]
pub(crate) use input::{PageEdit, menu_edit};
pub(super) use input::{mouse_kind, page_key};
pub(crate) use layout::view;
pub(crate) use overlay::page_origin;
pub(super) use page::{page_hover, page_surface};
pub(crate) use panes::{drop_side, focus_key, page_focus_key};
pub(crate) use picker::PICKER;
