// SPDX-License-Identifier: GPL-3.0-only

//! Typed settings and comment-preserving TOML configuration persistence.

mod load;
mod write;

#[cfg(test)]
mod tests;

pub use load::{ConfigLoad, load_or_create, parse};
pub use write::{write_autoplay, write_change, write_permission};

const DEFAULT_CONFIG: &str = "# Elaris Browser configuration\n\n[appearance]\ntheme = \"system\"\naccent = \"blue\"\nglass = true\nglass_style = \"liquid\"\ntint_opacity = 100\ngrain = 0\ntab_layout = \"vertical\"\nsidebar_side = \"left\"\nnew_tab_position = \"top\"\npage_style = \"card\"\n\n[split_view]\nmax_panes = 2\n\n[search]\nengine = \"duckduckgo\"\n\n[downloads]\ndirectory = \"default\"\n\n[startup]\nmode = \"restore\"\n\n[pages]\nzoom = 100\n\n[performance]\nreduce_motion = false\n";

const AUTOPLAY_KEY: &str = "autoplay";
