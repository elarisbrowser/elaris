// SPDX-License-Identifier: GPL-3.0-only

//! The Settings form that adds the user's own search engines, and removing them again.

use rune::{TextEdit, Tx};

use super::AppState;
use elaris_core::address::SearchEngine;
use elaris_core::settings::SettingsChange;

/// What is being typed into the Add search engine form, and why the last attempt failed.
#[derive(Default)]
pub(crate) struct SearchEngineForm {
    pub(crate) name: TextEdit,
    pub(crate) url: TextEdit,
    pub(crate) error: Option<String>,
}

impl AppState {
    /// Adds the engine the form describes, or says what is wrong with it.
    pub(crate) fn add_search_engine(&mut self) {
        self.add_search_engine_change();
    }

    pub(crate) fn add_search_engine_with_toast(&mut self, tx: &mut Tx) {
        if self.add_search_engine_change() {
            self.toasts.show(tx, "Search engine added");
        }
    }

    fn add_search_engine_change(&mut self) -> bool {
        let form = &self.search_engine_form;
        let added = self
            .settings
            .add_custom_search_engine(&form.name.text, &form.url.text);
        match added {
            Ok(_) => {
                self.search_engine_form = SearchEngineForm::default();
                self.save_search_engines();
                true
            }
            Err(error) => {
                self.search_engine_form.error = Some(error);
                false
            }
        }
    }

    /// Removes one of the user's engines. If it was the chosen one, the default takes over.
    #[cfg(test)]
    pub(crate) fn remove_search_engine(&mut self, index: usize) {
        self.remove_search_engine_change(index);
    }

    pub(crate) fn remove_search_engine_with_toast(&mut self, index: usize, tx: &mut Tx) {
        if self.remove_search_engine_change(index) {
            self.toasts.show(tx, "Search engine removed");
        }
    }

    fn remove_search_engine_change(&mut self, index: usize) -> bool {
        if index >= self.settings.custom_search_engines.len() {
            return false;
        }
        let was_chosen = self.settings.search_engine == SearchEngine::Custom(index);
        self.settings.remove_custom_search_engine(index);
        self.save_search_engines();
        if was_chosen {
            self.save_setting(self.settings.search_engine_change());
        }
        true
    }

    fn save_search_engines(&mut self) {
        let engines = self.settings.custom_search_engines.clone();
        self.save_setting(SettingsChange::SearchEngines(engines));
    }
}
