// SPDX-License-Identifier: GPL-3.0-only

use elaris::ladybird::{Key, MouseKind};
use rune::{KeyEvent, Mods, PointerButton, WheelEvent};

use super::{Browser, TabId, ViewCommand};
use crate::input;

#[derive(Default)]
pub(super) struct Held {
    drag: Option<(TabId, i32)>,
    keys: Vec<(TabId, Option<rune::PhysicalKeyCode>, Key)>,
}

impl Browser {
    pub(crate) fn drag_pointer(
        &mut self,
        kind: MouseKind,
        point: [f32; 2],
        button: PointerButton,
        mods: Mods,
        clicks: u32,
    ) {
        let bit = input::button_bit(button);
        if bit == 0 {
            return;
        }
        if kind == MouseKind::Down {
            // Pane clicks can be delivered before the next frame's focus poll.
            if self.split.is_some() {
                self.set_viewport_focus(true);
                self.sync_engine();
            }
            self.input.drag = self.active.map(|id| (id, bit));
        }
        let Some((id, held)) = self.input.drag else {
            return;
        };
        if kind == MouseKind::Up || kind == MouseKind::Cancel {
            self.input.drag = None;
        }
        if self.active != Some(id) {
            self.cancel_pointer(id);
            return;
        }
        let changed = if matches!(kind, MouseKind::Down | MouseKind::Up) {
            bit
        } else {
            0
        };
        let buttons = if self.input.drag.is_some() { held } else { 0 };
        let event = input::pointer(
            kind,
            point,
            self.window.scale.get(),
            changed,
            buttons,
            mods,
            clicks,
        );
        self.send(id, ViewCommand::Mouse(event));
    }

    fn cancel_pointer(&mut self, id: TabId) {
        self.input.drag = None;
        self.send(
            id,
            ViewCommand::Mouse(input::pointer(
                MouseKind::Cancel,
                [0.0; 2],
                self.window.scale.get(),
                0,
                0,
                Mods::default(),
                0,
            )),
        );
    }

    pub(crate) fn hover_tab(&self, id: TabId, kind: MouseKind, point: [f32; 2], mods: Mods) {
        if self.input.drag.is_none() && self.visible_tabs().contains(&Some(id)) {
            self.send(
                id,
                ViewCommand::Mouse(input::pointer(
                    kind,
                    point,
                    self.window.scale.get(),
                    0,
                    0,
                    mods,
                    0,
                )),
            );
        }
    }

    pub(crate) fn wheel_tab(&self, id: TabId, wheel: WheelEvent) {
        if !self.visible_tabs().contains(&Some(id)) {
            return;
        }
        let phase = input::wheel_phase(wheel.precise, wheel.phase, wheel.momentum);
        let mut event = input::wheel(
            [wheel.x, wheel.y],
            [wheel.dx, wheel.dy],
            wheel.precise,
            phase,
            self.window.scale.get(),
        );
        event.modifiers = input::modifiers(wheel.mods);
        event.buttons = self
            .input
            .drag
            .filter(|(owner, _)| *owner == id)
            .map_or(0, |(_, bits)| bits);
        self.send(id, ViewCommand::Mouse(event));
    }

    pub(crate) fn key(
        &mut self,
        event: &KeyEvent,
        pressed: bool,
        repeat: bool,
        physical: Option<rune::PhysicalKeyCode>,
    ) -> bool {
        let Some(id) = self.active.filter(|_| self.engine.is_some()) else {
            return false;
        };
        let pressed = if let rune::Key::Modifier { pressed, .. } = event.key {
            pressed
        } else {
            pressed
        };
        let Some(mut key) = input::key(event, pressed, repeat) else {
            return false;
        };
        let held = self.input.keys.iter().position(|(owner, code, logical)| {
            *owner == id
                && match physical {
                    Some(code_now) => *code == Some(code_now),
                    None => *logical == key.key,
                }
        });
        if pressed {
            if held.is_none() {
                if repeat {
                    return false;
                }
                self.input.keys.push((id, physical, key.key));
            }
        } else if let Some(index) = held {
            key.key = self.input.keys.swap_remove(index).2;
        } else {
            return false;
        }
        self.send(id, ViewCommand::Key(key));
        true
    }

    pub(super) fn reset_input(&mut self) {
        if let Some((id, _)) = self.input.drag {
            self.cancel_pointer(id);
        }
        self.input.keys.clear();
    }
}
