// SPDX-License-Identifier: GPL-3.0-only

//! Rebinding keyboard shortcuts from Settings: which action is listening for its new chord, and
//! saving what it hears to `[keybindings]`.

use rune::{Key, KeyEvent};

use super::AppState;
use crate::shortcuts::{self, Shortcut};
use elaris_core::keybindings::{self, KeyBindings};
use elaris_core::settings::SettingsChange;

#[derive(Default)]
pub(crate) struct ShortcutRecorder {
    /// The action waiting for its new chord. The macOS menu bar's keys pause meanwhile.
    pub(crate) recording: Option<Shortcut>,
    /// What the last recording changed, or why a chord was refused.
    pub(crate) notice: Option<String>,
}

impl AppState {
    /// Listens for `shortcut`'s new chord, or stops if it was already listening.
    pub(crate) fn toggle_recording(&mut self, shortcut: Shortcut) {
        let recorder = &mut self.shortcut_recorder;
        recorder.notice = None;
        recorder.recording = (recorder.recording != Some(shortcut)).then_some(shortcut);
    }

    pub(crate) fn stop_recording(&mut self) {
        self.shortcut_recorder.recording = None;
    }

    /// Takes a key while an action is listening: a chord becomes its only key, Backspace or
    /// Delete removes its keys, and Escape stops listening. Returns whether the key was used.
    pub(crate) fn record_key(&mut self, event: &KeyEvent) -> bool {
        let Some(shortcut) = self.shortcut_recorder.recording else {
            return false;
        };
        match event.key {
            Key::Escape => {
                self.stop_recording();
                return true;
            }
            Key::Tab => return false,
            Key::Backspace | Key::Delete => {
                self.stop_recording();
                self.clear_shortcut(shortcut);
                return true;
            }
            _ => {}
        }
        let Some(chord) = shortcuts::chord_of(event) else {
            let modifier = if cfg!(target_os = "macos") {
                "⌘ Command"
            } else {
                "Ctrl"
            };
            self.shortcut_recorder.notice = Some(format!("Hold {modifier} with the key."));
            return true;
        };
        let label = shortcuts::chord_label(chord);
        if let Some(owner) = shortcuts::reserved_by(chord) {
            self.shortcut_recorder.notice = Some(format!("{label} is reserved for {owner}."));
            return true;
        }
        let Some(name) = shortcut.name() else {
            self.stop_recording();
            return true;
        };
        let previous = shortcuts::owner(chord).filter(|owner| *owner != shortcut);
        let mut bindings = self.settings.keybindings.clone();
        keybindings::assign(&mut bindings, name, chord);
        self.set_keybindings(bindings);
        self.stop_recording();
        self.shortcut_recorder.notice = previous.map(|previous| {
            format!(
                "{label} no longer runs {}.",
                shortcuts::title(previous).to_lowercase()
            )
        });
        true
    }

    /// Leaves `shortcut` with no keys at all.
    pub(crate) fn clear_shortcut(&mut self, shortcut: Shortcut) {
        if let Some(name) = shortcut.name() {
            let mut bindings = self.settings.keybindings.clone();
            bindings.insert(name.to_owned(), Vec::new());
            self.set_keybindings(bindings);
        }
    }

    /// Gives `shortcut` its default keys back.
    pub(crate) fn reset_shortcut(&mut self, shortcut: Shortcut) {
        if let Some(name) = shortcut.name() {
            let mut bindings = self.settings.keybindings.clone();
            bindings.remove(name);
            self.set_keybindings(bindings);
        }
    }

    pub(crate) fn reset_all_shortcuts(&mut self) {
        self.shortcut_recorder = ShortcutRecorder::default();
        self.set_keybindings(KeyBindings::new());
    }

    /// Whether `shortcut` has keys of its own in `[keybindings]`.
    pub(crate) fn shortcut_customized(&self, shortcut: Shortcut) -> bool {
        shortcut
            .name()
            .is_some_and(|name| self.settings.keybindings.contains_key(name))
    }

    fn set_keybindings(&mut self, bindings: KeyBindings) {
        if self.settings.keybindings == bindings {
            return;
        }
        let issues = shortcuts::apply_keybindings(&bindings);
        if !issues.is_empty() {
            self.configuration_issue = Some(issues.join("\n"));
        }
        self.settings.keybindings = bindings.clone();
        self.save_setting(SettingsChange::KeyBindings(bindings));
    }
}
