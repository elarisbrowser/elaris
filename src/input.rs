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

pub(crate) fn button_bit(button: rune::PointerButton) -> i32 {
    match button {
        rune::PointerButton::Primary => button::PRIMARY,
        rune::PointerButton::Secondary => button::SECONDARY,
        rune::PointerButton::Middle => button::MIDDLE,
        _ => 0,
    }
}

pub(crate) fn pointer(
    kind: MouseKind,
    point: [f32; 2],
    scale: f32,
    button: i32,
    buttons: i32,
    mods: Mods,
    clicks: u32,
) -> MouseEvent {
    MouseEvent {
        kind,
        x: device(point[0], scale),
        y: device(point[1], scale),
        button,
        buttons,
        modifiers: modifiers(mods),
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
        ..pointer(MouseKind::Wheel, point, scale, 0, 0, Mods::default(), 0)
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

/// A single logical transition. Committed input-method strings travel separately.
pub(crate) fn key(event: &RuneKeyEvent, pressed: bool, repeat: bool) -> Option<KeyEvent> {
    let named = match &event.key {
        RuneKey::Text(text) => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Character(ch)
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
        RuneKey::Function(n @ 1..=12) => Key::Function(*n),
        RuneKey::Insert => Key::Insert,
        RuneKey::PrintScreen => Key::PrintScreen,
        RuneKey::ScrollLock => Key::ScrollLock,
        RuneKey::Pause => Key::Pause,
        RuneKey::Modifier { modifier, .. } => match modifier {
            rune::Modifier::ShiftLeft => Key::ShiftLeft,
            rune::Modifier::ShiftRight => Key::ShiftRight,
            rune::Modifier::ControlLeft => Key::ControlLeft,
            rune::Modifier::ControlRight => Key::ControlRight,
            rune::Modifier::AltLeft => Key::AltLeft,
            rune::Modifier::AltRight => Key::AltRight,
            rune::Modifier::SuperLeft => Key::SuperLeft,
            rune::Modifier::SuperRight => Key::SuperRight,
            _ => return None,
        },
        _ => return None,
    };
    Some(KeyEvent {
        key: named,
        pressed,
        modifiers: modifiers(event.mods),
        repeat: pressed && repeat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_points_to_device_pixels() {
        let event = pointer(MouseKind::Move, [10.25, 3.0], 2.0, 0, 0, Mods::default(), 0);
        assert_eq!((event.x, event.y), (21, 6));
    }

    #[test]
    fn a_click_presses_then_releases_the_primary_button() {
        let down = pointer(MouseKind::Down, [0.0, 0.0], 1.0, 1, 1, Mods::default(), 2);
        assert_eq!(
            (down.button, down.buttons, down.clicks),
            (button::PRIMARY, button::PRIMARY, 2)
        );
        let drag = pointer(MouseKind::Move, [1.0, 0.0], 1.0, 0, 1, Mods::default(), 0);
        assert_eq!((drag.button, drag.buttons), (0, button::PRIMARY));
        let up = pointer(MouseKind::Up, [1.0, 0.0], 1.0, 1, 0, Mods::default(), 2);
        assert_eq!((up.button, up.buttons), (button::PRIMARY, 0));
        let hover = pointer(MouseKind::Move, [2.0, 0.0], 1.0, 0, 0, Mods::default(), 0);
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
        let down = key(&event, true, true).unwrap();
        let up = key(&event, false, false).unwrap();
        assert_eq!(down.key, Key::Return);
        assert_eq!(down.modifiers, modifier::CTRL);
        assert!(down.pressed && down.repeat && !up.pressed && !up.repeat);
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
    fn composed_strings_are_not_split_into_keystrokes() {
        let event = RuneKeyEvent {
            key: RuneKey::Text("hé".into()),
            mods: Mods::default(),
        };
        assert!(key(&event, true, false).is_none());
    }

    #[test]
    fn shortcuts_stay_with_the_app() {
        let event = RuneKeyEvent {
            key: RuneKey::Command('q'),
            mods: Mods::default(),
        };
        assert!(key(&event, true, false).is_none());
    }
}
