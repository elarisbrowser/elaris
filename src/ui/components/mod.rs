// SPDX-License-Identifier: GPL-3.0-only

//! Shared interface components. Native pages, the sidebar, the viewport and the menus build from these
//! instead of styling their own controls, so the same control looks and behaves the same everywhere.

pub(crate) mod animated_icon;
pub(crate) mod controls;
pub(crate) mod grain;
pub(crate) mod hover_highlight;
pub(crate) mod menu;
pub(crate) mod rule;
pub(crate) mod widgets;
