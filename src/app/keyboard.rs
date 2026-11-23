// SPDX-License-Identifier: GPL-3.0-only

//! Rebinding keyboard shortcuts from Settings: which action is listening for its new chord, and
//! saving what it hears to `[keybindings]`.

use rune::{Key, KeyEvent};

use super::AppState;
use crate::shortcuts::{self, Shortcut};
use elaris_core::keybindings::{self, Chord, KeyBindings};
use elaris_core::settings::SettingsChange;

#[derive(Default)]
pub(crate) struct ShortcutRecorder {
    /// The action waiting for its new chord. The macOS menu bar's keys pause meanwhile.
    pub(crate) recording: Option<Shortcut>,
    /// What the last recording changed, or why a chord was refused.
    pub(crate) notice: Option<String>,
    /// Whether the notice is a problem to fix rather than news.
    pub(crate) problem: bool,
    /// A chord another action uses, pressed once: pressing it again moves it to the recording
    /// action. Both show the conflict until then.
    pub(crate) pending: Option<(Chord, Shortcut)>,
}

impl AppState {
    /// Listens for `shortcut`'s new chord, or stops if it was already listening.
    pub(crate) fn toggle_recording(&mut self, shortcut: Shortcut) {
        let recording = self.shortcut_recorder.recording != Some(shortcut);
        self.shortcut_recorder = ShortcutRecorder {
            recording: recording.then_some(shortcut),
            ..ShortcutRecorder::default()
        };
    }

    pub(crate) fn stop_recording(&mut self) {
        let recorder = &mut self.shortcut_recorder;
        recorder.recording = None;
        recorder.pending = None;
        if recorder.problem {
            recorder.notice = None;
            recorder.problem = false;
        }
    }

    fn refuse(&mut self, notice: String) {
        self.shortcut_recorder.notice = Some(notice);
        self.shortcut_recorder.problem = true;
    }

    /// Takes every key while an action is listening, so none runs anything else: a chord
    /// becomes the action's only key, Backspace or Delete removes its keys, and Escape stops
    /// listening. A chord another action uses waits for a second press to move it. Returns
    /// whether an action was listening.
    pub(crate) fn record_key(&mut self, event: &KeyEvent) -> bool {
        let Some(shortcut) = self.shortcut_recorder.recording else {
            return false;
        };
        match event.key {
            Key::Escape => {
                self.stop_recording();
                return true;
            }
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
            self.shortcut_recorder.pending = None;
            self.refuse(format!("Hold {modifier} with the key."));
            return true;
        };
        let label = shortcuts::chord_label(chord);
        if let Some(owner) = shortcuts::reserved_by(chord) {
            self.shortcut_recorder.pending = None;
            self.refuse(format!("{label} is reserved for {owner}."));
            return true;
        }
        let Some(name) = shortcut.name() else {
            self.stop_recording();
            return true;
        };
        let other = shortcuts::owner(chord).filter(|owner| *owner != shortcut);
        if let Some(other) = other
            && self.shortcut_recorder.pending != Some((chord, other))
        {
            self.shortcut_recorder.pending = Some((chord, other));
            let title = shortcuts::title(other).to_lowercase();
            self.refuse(format!(
                "{label} is already used by {title}. Press it again to use it here instead."
            ));
            return true;
        }
        let mut bindings = self.settings.keybindings.clone();
        keybindings::assign(&mut bindings, name, chord);
        self.set_keybindings(bindings);
        self.shortcut_recorder = ShortcutRecorder {
            notice: other.map(|other| {
                let title = shortcuts::title(other).to_lowercase();
                format!("{label} moved here from {title}.")
            }),
            ..ShortcutRecorder::default()
        };
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
