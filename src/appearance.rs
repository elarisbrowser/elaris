// SPDX-License-Identifier: GPL-3.0-only

//! Whether the interface is light or dark: following the system, or fixed either way.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// The material behind the window when glass is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GlassStyle {
    /// The popover vibrancy material.
    Frosted,
    /// The Liquid Glass material on macOS 26 and later.
    #[default]
    Liquid,
}

impl GlassStyle {
    /// Every style, in the order the settings page shows them.
    pub(crate) const ALL: [Self; 2] = [Self::Frosted, Self::Liquid];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Frosted => "Frosted",
            Self::Liquid => "Liquid",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Frosted => "frosted",
            Self::Liquid => "liquid",
        }
    }

    pub(crate) fn from_config_key(value: &str) -> Option<Self> {
        match value {
            "frosted" => Some(Self::Frosted),
            "liquid" => Some(Self::Liquid),
            _ => None,
        }
    }
}

/// The interface accent used for focus, selection and emphasized controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AccentColor {
    #[default]
    Blue,
    Purple,
    Pink,
    Orange,
    Green,
    Gray,
}

impl AccentColor {
    /// Every available accent, in the order the settings page shows them.
    pub(crate) const ALL: [Self; 6] = [
        Self::Blue,
        Self::Purple,
        Self::Pink,
        Self::Orange,
        Self::Green,
        Self::Gray,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Orange => "Orange",
            Self::Green => "Green",
            Self::Gray => "Gray",
        }
    }

    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Orange => "orange",
            Self::Green => "green",
            Self::Gray => "gray",
        }
    }

    pub(crate) fn from_config_key(value: &str) -> Option<Self> {
        match value {
            "blue" => Some(Self::Blue),
            "purple" => Some(Self::Purple),
            "pink" => Some(Self::Pink),
            "orange" => Some(Self::Orange),
            "green" => Some(Self::Green),
            "gray" => Some(Self::Gray),
            _ => None,
        }
    }
}

impl Appearance {
    /// Every choice, in the order the picker shows them.
    /// The appearance after this one, for a control that cycles through them.
    pub(crate) fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Whether to draw dark, given whether the system is dark.
    pub(crate) fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Self::System => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_every_appearance() {
        let mut seen = vec![Appearance::System];
        while seen.len() < 4 {
            seen.push(seen.last().unwrap().next());
        }
        assert_eq!(
            seen,
            [
                Appearance::System,
                Appearance::Light,
                Appearance::Dark,
                Appearance::System
            ]
        );
    }

    #[test]
    fn system_follows_the_system_and_the_others_do_not() {
        assert!(Appearance::System.is_dark(true));
        assert!(!Appearance::System.is_dark(false));
        assert!(!Appearance::Light.is_dark(true));
        assert!(Appearance::Dark.is_dark(false));
    }

    #[test]
    fn glass_styles_round_trip_through_config_keys() {
        for style in GlassStyle::ALL {
            assert_eq!(GlassStyle::from_config_key(style.config_key()), Some(style));
        }
        assert_eq!(GlassStyle::default(), GlassStyle::Liquid);
        assert_eq!(GlassStyle::from_config_key("clear"), None);
    }

    #[test]
    fn accent_colors_have_unique_config_keys_and_labels() {
        let accents = AccentColor::ALL;
        for (index, accent) in accents.into_iter().enumerate() {
            assert_eq!(
                AccentColor::from_config_key(accent.config_key()),
                Some(accent)
            );
            assert!(!accent.label().is_empty());
            assert!(!accents[index + 1..].contains(&accent));
        }
        assert_eq!(AccentColor::default(), AccentColor::Blue);
        assert_eq!(AccentColor::from_config_key("invalid"), None);
    }
}
