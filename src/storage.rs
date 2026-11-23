// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    test,
    expect(
        dead_code,
        reason = "App tests disable storage startup to keep tests out of user profiles"
    )
)]

//! Elaris-owned data paths, history worker, and versioned session files.

mod history;
mod history_cache;
mod queue;
mod session;
mod store;
mod worker;

#[cfg(feature = "ladybird-bridge")]
pub(crate) use elaris_core::paths::display_path;
pub(crate) use elaris_core::paths::{Paths, default_download_directory};
pub(crate) use history::{HistoryEvent, HistoryMutation};
pub(crate) use session::{Session, SessionFavicon, SessionSplit, SessionTab};
pub(crate) use store::{ConfigEvent, Storage};

#[cfg(test)]
mod tests;
