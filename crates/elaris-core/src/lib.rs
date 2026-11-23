// SPDX-License-Identifier: GPL-3.0-only

//! Elaris's UI-free core: settings and their TOML file, website permissions, address rules, the history and
//! download stores, internal routes, storage paths, and development timing. Nothing here depends on Rune or the
//! engine, so the browser's state and its stores can be tested without a window.

pub mod address;
pub mod appearance;
pub mod config;
pub mod dev;
pub mod download_history;
pub mod downloads;
pub mod history;
pub mod internal;
pub mod paths;
pub mod perf;
pub mod permissions;
pub mod settings;
