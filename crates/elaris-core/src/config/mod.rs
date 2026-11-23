// SPDX-License-Identifier: GPL-3.0-only

//! Typed settings and comment-preserving TOML configuration persistence.

mod load;
mod write;

#[cfg(test)]
mod tests;

pub use load::{ConfigLoad, load_or_create, parse};
pub use write::{write_autoplay, write_change, write_permission};

const DEFAULT_CONFIG: &str = "# Elaris Browser configuration\n\n[appearance]\ntheme = \"system\"\naccent = \"blue\"\nglass = true\nglass_style = \"liquid\"\ntint_opacity = 100\ngrain_enabled = false\ngrain = 40\ntab_layout = \"vertical\"\naddress_bar = \"sidebar\"\ncompact_tabs = false\nsidebar_side = \"left\"\nnew_tab_position = \"top\"\npage_style = \"card\"\n\n[split_view]\nmax_panes = 2\n\n[search]\nengine = \"duckduckgo\"\n\n# Add your own search engines, then name one above:\n# [search.engines.kagi]\n# name = \"Kagi\"\n# url = \"https://kagi.com/search?q=%s\"\n\n[downloads]\ndirectory = \"default\"\n\n[startup]\nmode = \"restore\"\n\n[pages]\nzoom = 100\n\n[performance]\nreduce_motion = false\nmemory_saver = \"balanced\"\n";

const AUTOPLAY_KEY: &str = "autoplay";
