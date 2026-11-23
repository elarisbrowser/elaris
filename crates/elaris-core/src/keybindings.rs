// SPDX-License-Identifier: GPL-3.0-only

//! The user's keyboard shortcut overrides from `[keybindings]`, as chords such as `"mod+shift+t"`.
//! `mod` is the platform's shortcut modifier: Command on macOS and Control elsewhere. Which action
//! names exist is the browser's business; this only reads and checks the chords.

use std::collections::BTreeMap;
use std::fmt;

/// One key combination that runs a browser shortcut. Every chord includes the shortcut modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The unshifted key, lowercased, as menus show it.
    pub key: char,
    pub shift: bool,
    /// Option on macOS, Alt elsewhere.
    pub option: bool,
}

impl Chord {
    pub const fn new(key: char, shift: bool, option: bool) -> Self {
        Self { key, shift, option }
    }

    /// Reads a chord such as `"mod+t"`, `"mod+shift+["` or `"mod+alt+b"`. Case and spaces around
    /// `+` are ignored, and the `+` key itself is written as `plus`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts.pop().unwrap_or_default().to_ascii_lowercase();
        let (mut primary, mut shift, mut option) = (false, false, false);
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "mod" => &mut primary,
                "shift" => &mut shift,
                "alt" | "option" => &mut option,
                "cmd" | "command" | "ctrl" | "control" => {
                    return Err(format!(
                        "{text:?} uses {part:?}; write mod, which is Command on macOS and Control elsewhere"
                    ));
                }
                _ => return Err(format!("{text:?} has an unknown modifier {part:?}")),
            };
            if *flag {
                return Err(format!("{text:?} repeats {part:?}"));
            }
            *flag = true;
        }
        if !primary {
            return Err(format!("{text:?} must include mod"));
        }
        let key = match key.as_str() {
            "plus" => '=',
            _ => {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(key), None) if key.is_ascii_graphic() => key,
                    _ => return Err(format!("{text:?} must end in a single key")),
                }
            }
        };
        Ok(Self { key, shift, option })
    }
}

/// Written the way `[keybindings]` reads it back.
impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("mod")?;
        if self.shift {
            f.write_str("+shift")?;
        }
        if self.option {
            f.write_str("+alt")?;
        }
        write!(f, "+{}", self.key)
    }
}

/// Each overridden action's chords, by action name. An empty list removes the action's keys.
pub type KeyBindings = BTreeMap<String, Vec<Chord>>;

/// Makes `chord` the only key for `action`, taking it from any other action that lists it.
pub fn assign(bindings: &mut KeyBindings, action: &str, chord: Chord) {
    for chords in bindings.values_mut() {
        chords.retain(|bound| *bound != chord);
    }
    bindings.insert(action.to_owned(), vec![chord]);
}

#[cfg(test)]
mod tests {
    use super::{Chord, KeyBindings, assign};

    #[test]
    fn assigning_a_chord_takes_it_from_other_actions() {
        let k = Chord::new('k', false, false);
        let j = Chord::new('j', false, false);
        let mut bindings = KeyBindings::from([("find".to_owned(), vec![k, j])]);
        assign(&mut bindings, "new_tab", k);
        assert_eq!(bindings["find"], [j]);
        assert_eq!(bindings["new_tab"], [k]);
    }

    #[test]
    fn parses_modifiers_in_any_order_and_case() {
        assert_eq!(Chord::parse("mod+t"), Ok(Chord::new('t', false, false)));
        assert_eq!(
            Chord::parse("Shift + MOD + T"),
            Ok(Chord::new('t', true, false))
        );
        assert_eq!(Chord::parse("mod+alt+b"), Ok(Chord::new('b', false, true)));
        assert_eq!(
            Chord::parse("mod+option+["),
            Ok(Chord::new('[', false, true))
        );
        assert_eq!(Chord::parse("mod+plus"), Ok(Chord::new('=', false, false)));
    }

    #[test]
    fn rejects_chords_that_cannot_reach_the_browser() {
        for bad in [
            "t",
            "shift+t",
            "mod+",
            "mod+tab",
            "mod+mod+t",
            "mod+hyper+t",
            "mod+ ",
        ] {
            assert!(Chord::parse(bad).is_err(), "{bad}");
        }
        let error = Chord::parse("cmd+t").unwrap_err();
        assert!(error.contains("write mod"));
    }

    #[test]
    fn displays_as_it_parses() {
        for text in ["mod+t", "mod+shift+[", "mod+alt+b", "mod+shift+alt+="] {
            assert_eq!(Chord::parse(text).unwrap().to_string(), text);
        }
    }
}
