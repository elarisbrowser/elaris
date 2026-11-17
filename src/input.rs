// SPDX-License-Identifier: GPL-3.0-only

//! Translates Rune input into Ladybird page input. Rune reports points; pages take device pixels.

use elaris::ladybird::{Key, KeyEvent, MouseEvent, MouseKind, WheelPhase, button, modifier};
use rune::{GesturePhase, Key as RuneKey, KeyEvent as RuneKeyEvent, Mods};

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

/// A pointer event at `point`, in the viewport's own points. Rune reports the primary button
/// only: `Down` and `Up` change it, and `held` says whether it is down after the event.
pub(crate) fn pointer(
    kind: MouseKind,
    point: [f32; 2],
    scale: f32,
    held: bool,
    clicks: u32,
) -> MouseEvent {
    let pressed = |yes| if yes { button::PRIMARY } else { 0 };
    MouseEvent {
        kind,
        x: device(point[0], scale),
        y: device(point[1], scale),
        button: pressed(matches!(kind, MouseKind::Down | MouseKind::Up)),
        buttons: pressed(held),
        modifiers: 0,
        wheel: (0.0, 0.0),
        precise: false,
        phase: WheelPhase::None,
        clicks: clicks.try_into().unwrap_or(i32::MAX),
    }
}

/// A wheel event. Rune's delta moves content (positive right and down); a page's scrolls
/// toward its end, so the signs flip.
pub(crate) fn wheel(
    point: [f32; 2],
    delta: [f32; 2],
    precise: bool,
    phase: WheelPhase,
    scale: f32,
) -> MouseEvent {
    MouseEvent {
        wheel: (-f64::from(delta[0]), -f64::from(delta[1])),
        precise,
        phase,
        ..pointer(MouseKind::Wheel, point, scale, false, 0)
    }
}

/// Where a Rune scroll is in its touchpad gesture. Notched wheels have no gesture; momentum
/// keeps its own phase until it ends.
pub(crate) fn wheel_phase(precise: bool, phase: GesturePhase, momentum: bool) -> WheelPhase {
    match (precise, phase, momentum) {
        (false, _, _) => WheelPhase::None,
        (true, GesturePhase::Ended, _) => WheelPhase::Ended,
        (true, _, true) => WheelPhase::Momentum,
        (true, _, false) => WheelPhase::Ongoing,
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
        let event = pointer(MouseKind::Move, [10.25, 3.0], 2.0, false, 0);
        assert_eq!((event.x, event.y), (21, 6));
    }

    #[test]
    fn a_click_presses_then_releases_the_primary_button() {
        let down = pointer(MouseKind::Down, [0.0, 0.0], 1.0, true, 2);
        assert_eq!(
            (down.button, down.buttons, down.clicks),
            (button::PRIMARY, button::PRIMARY, 2)
        );
        let drag = pointer(MouseKind::Move, [1.0, 0.0], 1.0, true, 0);
        assert_eq!((drag.button, drag.buttons), (0, button::PRIMARY));
        let up = pointer(MouseKind::Up, [1.0, 0.0], 1.0, false, 2);
        assert_eq!((up.button, up.buttons), (button::PRIMARY, 0));
        let hover = pointer(MouseKind::Move, [2.0, 0.0], 1.0, false, 0);
        assert_eq!((hover.button, hover.buttons), (0, 0));
    }

    #[test]
    fn named_keys_and_modifiers_reach_the_page() {
        let event = RuneKeyEvent {
            key: RuneKey::Enter,
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
        };
        let keys = keys(&event);
        assert_eq!(keys.len(), 2);
        assert!(
            keys.iter()
                .all(|k| k.key == Key::Return && k.modifiers == modifier::CTRL)
        );
        assert!(keys[0].pressed && !keys[1].pressed);
    }

    #[test]
    fn wheel_scrolls_toward_the_page_end_when_content_moves_up() {
        let event = wheel([0.0, 0.0], [0.0, -30.0], true, WheelPhase::Ongoing, 1.0);
        assert_eq!(event.wheel, (-0.0, 30.0));
        assert!(event.precise);
        assert_eq!(event.phase, WheelPhase::Ongoing);
    }

    #[test]
    fn maps_touchpad_gesture_phases() {
        use GesturePhase::{Began, Ended, Moved};
        assert_eq!(wheel_phase(false, Moved, false), WheelPhase::None);
        assert_eq!(wheel_phase(true, Began, false), WheelPhase::Ongoing);
        assert_eq!(wheel_phase(true, Moved, false), WheelPhase::Ongoing);
        assert_eq!(wheel_phase(true, Moved, true), WheelPhase::Momentum);
        assert_eq!(wheel_phase(true, Ended, false), WheelPhase::Ended);
        assert_eq!(wheel_phase(true, Ended, true), WheelPhase::Ended);
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
