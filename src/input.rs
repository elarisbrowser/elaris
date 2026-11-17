// SPDX-License-Identifier: GPL-3.0-only

//! Translates Rune input into Ladybird page input. Rune reports points; pages take device pixels.

use elaris::ladybird::{Key, KeyEvent, MouseEvent, MouseKind, modifier};
use rune::{Key as RuneKey, KeyEvent as RuneKeyEvent, Mods};

pub(crate) fn modifiers(mods: Mods) -> i32 {
    [
        (mods.alt, modifier::ALT),
        (mods.ctrl, modifier::CTRL),
        (mods.shift, modifier::SHIFT),
        (mods.logo, modifier::SUPER),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(0, |bits, (_, bit)| bits | bit)
}

pub(crate) fn device(points: f32, scale: f32) -> i32 {
    (points * scale).round() as i32
}

/// A pointer event at `point`, in the viewport's own points.
pub(crate) fn mouse(kind: MouseKind, point: [f32; 2], scale: f32, mods: Mods) -> MouseEvent {
    MouseEvent {
        kind,
        x: device(point[0], scale),
        y: device(point[1], scale),
        button: 0,
        buttons: 0,
        modifiers: modifiers(mods),
        wheel: (0.0, 0.0),
        precise: false,
        clicks: 0,
    }
}

/// A wheel event. Rune's delta moves content (positive right and down); a page's scrolls
/// toward its end, so the signs flip.
pub(crate) fn wheel(point: [f32; 2], delta: [f32; 2], precise: bool, scale: f32) -> MouseEvent {
    MouseEvent {
        wheel: (-f64::from(delta[0]), -f64::from(delta[1])),
        precise,
        ..mouse(MouseKind::Wheel, point, scale, Mods::default())
    }
}

/// The page keys a Rune key press types, each pressed and released: Rune reports presses only.
/// Shortcuts (Rune's `Command` keys) stay with the app.
pub(crate) fn keys(event: &RuneKeyEvent) -> Vec<KeyEvent> {
    let named = match &event.key {
        RuneKey::Text(text) => {
            return text
                .chars()
                .flat_map(|c| tap(Key::Character(c), event.mods))
                .collect();
        }
        RuneKey::Enter => Key::Return,
        RuneKey::Backspace => Key::Backspace,
        RuneKey::Delete => Key::Delete,
        RuneKey::Tab => Key::Tab,
        RuneKey::Escape => Key::Escape,
        RuneKey::Left => Key::Left,
        RuneKey::Right => Key::Right,
        RuneKey::Up => Key::Up,
        RuneKey::Down => Key::Down,
        RuneKey::Home => Key::Home,
        RuneKey::End => Key::End,
        RuneKey::PageUp => Key::PageUp,
        RuneKey::PageDown => Key::PageDown,
        _ => return Vec::new(),
    };
    tap(named, event.mods).to_vec()
}

fn tap(key: Key, mods: Mods) -> [KeyEvent; 2] {
    let modifiers = modifiers(mods);
    let event = |pressed| KeyEvent {
        key,
        pressed,
        modifiers,
        repeat: false,
    };
    [event(true), event(false)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_points_to_device_pixels() {
        let event = mouse(MouseKind::Move, [10.25, 3.0], 2.0, Mods::default());
        assert_eq!((event.x, event.y), (21, 6));
    }

    #[test]
    fn wheel_scrolls_toward_the_page_end_when_content_moves_up() {
        let event = wheel([0.0, 0.0], [0.0, -30.0], true, 1.0);
        assert_eq!(event.wheel, (-0.0, 30.0));
        assert!(event.precise);
    }

    #[test]
    fn maps_modifiers() {
        let mods = Mods {
            shift: true,
            logo: true,
            ..Mods::default()
        };
        assert_eq!(modifiers(mods), modifier::SHIFT | modifier::SUPER);
    }

    #[test]
    fn text_presses_and_releases_each_character() {
        let event = RuneKeyEvent {
            key: RuneKey::Text("hé".into()),
            mods: Mods::default(),
        };
        let keys: Vec<_> = keys(&event).iter().map(|k| (k.key, k.pressed)).collect();
        assert_eq!(
            keys,
            [
                (Key::Character('h'), true),
                (Key::Character('h'), false),
                (Key::Character('é'), true),
                (Key::Character('é'), false),
            ]
        );
    }

    #[test]
    fn shortcuts_stay_with_the_app() {
        let event = RuneKeyEvent {
            key: RuneKey::Command('q'),
            mods: Mods::default(),
        };
        assert!(keys(&event).is_empty());
    }
}
