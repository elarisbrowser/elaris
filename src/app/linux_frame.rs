// SPDX-License-Identifier: GPL-3.0-only

//! Linux window frame and titlebar: who draws the frame, and when the toolbar is revealed.

#[cfg(elaris_ui = "linux")]
#[cfg(not(test))]
use crate::storage;
#[cfg(elaris_ui = "linux")]
use elaris_core::settings::WindowDecorations;

/// How the Linux toolbar stays visible while browsing.
#[cfg(elaris_ui = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LinuxTitlebarMode {
    /// Reveal the toolbar while the pointer is in the window's top band.
    #[default]
    Hover,
    /// Keep the toolbar visible regardless of pointer position.
    Always,
}

#[cfg(elaris_ui = "linux")]
impl LinuxTitlebarMode {
    fn from_environment() -> Self {
        // With a system frame there is no edge to reveal from, so the toolbar stays put.
        if LinuxDecorations::current() == LinuxDecorations::System {
            return Self::Always;
        }
        match std::env::var("ELARIS_LINUX_TITLEBAR") {
            Ok(mode) if mode.eq_ignore_ascii_case("always") => Self::Always,
            _ => Self::Hover,
        }
    }
}

/// Who draws the Linux window frame and its controls.
#[cfg(elaris_ui = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinuxDecorations {
    /// The compositor's frame, or none at all on tiling window managers.
    System,
    /// A borderless window with Elaris's own controls.
    Elaris,
}

/// The frame saved in config.toml. The first window opens before storage loads, so this reads
/// the file directly.
#[cfg(elaris_ui = "linux")]
fn saved_window_decorations() -> WindowDecorations {
    #[cfg(not(test))]
    if let Ok(paths) = storage::Paths::resolve()
        && let Ok(contents) = std::fs::read_to_string(&paths.config_file)
        && let Ok(load) = elaris_core::config::parse(&contents)
    {
        return load.settings.window_decorations;
    }
    WindowDecorations::Auto
}

/// Desktops that tile windows and draw no titlebar, where custom window controls only get in the way.
#[cfg(elaris_ui = "linux")]
const TILING_DESKTOPS: &[&str] = &[
    "sway",
    "hyprland",
    "i3",
    "niri",
    "river",
    "bspwm",
    "dwm",
    "qtile",
    "awesome",
    "herbstluftwm",
    "xmonad",
    "wayfire",
    "labwc",
    "cosmic",
];

#[cfg(elaris_ui = "linux")]
impl LinuxDecorations {
    /// The mode for this process, chosen once since a window's frame is fixed when it opens.
    pub(crate) fn current() -> Self {
        static MODE: std::sync::OnceLock<LinuxDecorations> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            let var = |name| std::env::var(name).ok();
            Self::resolve(
                Self::override_setting().unwrap_or_else(saved_window_decorations),
                var("XDG_CURRENT_DESKTOP").as_deref(),
                var("XDG_SESSION_DESKTOP").as_deref(),
            )
        })
    }

    /// The `ELARIS_LINUX_DECORATIONS` choice, which wins over the saved setting.
    pub(crate) fn override_setting() -> Option<WindowDecorations> {
        WindowDecorations::from_config_key(&std::env::var("ELARIS_LINUX_DECORATIONS").ok()?)
    }

    /// The mode `setting` would give on this desktop.
    pub(crate) fn for_setting(setting: WindowDecorations) -> Self {
        let var = |name| std::env::var(name).ok();
        Self::resolve(
            setting,
            var("XDG_CURRENT_DESKTOP").as_deref(),
            var("XDG_SESSION_DESKTOP").as_deref(),
        )
    }

    fn resolve(setting: WindowDecorations, current: Option<&str>, session: Option<&str>) -> Self {
        match setting {
            WindowDecorations::System => return Self::System,
            WindowDecorations::Elaris => return Self::Elaris,
            WindowDecorations::Auto => {}
        }
        let tiling = [current, session]
            .into_iter()
            .flatten()
            .flat_map(|desktops| desktops.split(':'))
            .any(|desktop| {
                TILING_DESKTOPS
                    .iter()
                    .any(|tiling| desktop.eq_ignore_ascii_case(tiling))
            });
        if tiling { Self::System } else { Self::Elaris }
    }
}

/// Linux titlebar visibility and its current pointer state.
#[cfg(elaris_ui = "linux")]
pub(crate) struct LinuxTitlebar {
    mode: LinuxTitlebarMode,
    top_hovered: bool,
    toolbar_hovered: bool,
}

#[cfg(elaris_ui = "linux")]
impl Default for LinuxTitlebar {
    fn default() -> Self {
        Self {
            mode: LinuxTitlebarMode::from_environment(),
            top_hovered: false,
            toolbar_hovered: false,
        }
    }
}

#[cfg(elaris_ui = "linux")]
impl LinuxTitlebar {
    /// Whether the toolbar should be drawn now.
    pub(crate) fn is_visible(&self) -> bool {
        self.mode == LinuxTitlebarMode::Always || self.top_hovered || self.toolbar_hovered
    }

    /// Updates hover visibility and reports whether the view needs an animated commit.
    pub(crate) fn set_top_hovered(&mut self, hovered: bool) -> bool {
        if self.mode == LinuxTitlebarMode::Always || self.top_hovered == hovered {
            return false;
        }
        self.top_hovered = hovered;
        true
    }

    /// Keeps the toolbar open while the pointer moves from the top reveal strip into the row.
    pub(crate) fn set_toolbar_hovered(&mut self, hovered: bool) -> bool {
        if self.mode == LinuxTitlebarMode::Always || self.toolbar_hovered == hovered {
            return false;
        }
        self.toolbar_hovered = hovered;
        true
    }

    /// Whether the custom minimize control shows. Rune ignores minimize for borderless windows on
    /// macOS, where the native traffic lights do the job, so the `linux-ui` preview there has none.
    pub(crate) fn can_minimize(&self) -> bool {
        !cfg!(target_os = "macos")
    }

    /// Whether the thin strip at the top edge reveals the toolbar.
    pub(crate) fn reveals_on_hover(&self) -> bool {
        self.mode == LinuxTitlebarMode::Hover
    }
}

#[cfg(all(test, elaris_ui = "linux"))]
mod linux_decorations_tests {
    use super::LinuxDecorations;
    use elaris_core::settings::WindowDecorations::{Auto, Elaris, System};

    #[test]
    fn explicit_setting_wins() {
        assert_eq!(
            LinuxDecorations::resolve(System, Some("GNOME"), None),
            LinuxDecorations::System
        );
        assert_eq!(
            LinuxDecorations::resolve(Elaris, Some("sway"), None),
            LinuxDecorations::Elaris
        );
    }

    #[test]
    fn auto_detects_tiling_desktops() {
        assert_eq!(
            LinuxDecorations::resolve(Auto, Some("Hyprland"), None),
            LinuxDecorations::System
        );
        assert_eq!(
            LinuxDecorations::resolve(Auto, Some("ubuntu:GNOME"), Some("sway")),
            LinuxDecorations::System
        );
        assert_eq!(
            LinuxDecorations::resolve(Auto, Some("KDE"), None),
            LinuxDecorations::Elaris
        );
        assert_eq!(
            LinuxDecorations::resolve(Auto, None, None),
            LinuxDecorations::Elaris
        );
    }
}
