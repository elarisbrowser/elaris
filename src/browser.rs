// SPDX-License-Identifier: GPL-3.0-only

//! The browser: its tabs, which one is active, and the one engine behind them all.
//!
//! ```ignore
//! let tab = browser.open_tab("https://example.com")?;
//! browser.activate_tab(tab);
//! browser.navigate(tab, "https://ladybird.org")?;
//! browser.reload(tab);
//! browser.close_tab(tab);
//! ```
//!
//! Every operation returns at once. What it asks of the engine happens on the engine's thread,
//! and its outcome (the committed address, title, loading state, errors) arrives in the tab's
//! state through [`Browser::poll`]. Tabs are named by [`TabId`]s, which are never reused: an
//! operation on a tab that has closed does nothing, and nothing the engine reports about a closed
//! tab reaches another one. Visible tabs each have a viewport. The active tab receives keyboard
//! input, and hidden tabs keep their pages but stop painting.

mod accessibility;
mod connection;
mod display;
mod downloads;
mod engine_sync;
mod error;
mod find;
mod input_routing;
mod lifecycle;
mod navigation;
pub(crate) mod peek;
mod selection;
mod sleep;
pub(crate) mod split;
mod state;
pub(crate) mod swipe;
mod tabs;

pub(crate) use error::Error;
pub(crate) use state::{Browser, Polled};

use crate::engine::{Command, ViewCommand};
use crate::tab::{Tab, TabId};

#[cfg(test)]
use crate::engine::{Engine, Surface};
#[cfg(test)]
use elaris_core::internal::{InternalPage, InternalRoute};

#[cfg(test)]
mod tests;
