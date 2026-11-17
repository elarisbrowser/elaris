// SPDX-License-Identifier: GPL-3.0-only

//! Whether the interface is light or dark: following the system, or fixed either way.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Appearance {
    #[default]
    System,
    Light,
    Dark,
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
}
