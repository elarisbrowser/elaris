// SPDX-License-Identifier: GPL-3.0-only

//! Page translation: the button in the address field, and the choices it and the page menu offer.

use rune::{El, icon};

use crate::app::AppState;
use crate::engine::ViewCommand;
use crate::tab::Tab;
use crate::ui::components::controls;
use crate::ui::components::menu::{Menu, MenuItem};
use crate::ui::icons;
use crate::ui::theme::{self, Theme};

/// One thing the user can do with the page's translation.
pub(in crate::ui) struct Choice {
    pub(in crate::ui) label: String,
    pub(in crate::ui) checked: bool,
    pub(in crate::ui) enabled: bool,
    /// The language to show the page in; empty for the original.
    language: String,
    /// Download what translating into the language needs, rather than translating.
    download: bool,
}

impl Choice {
    pub(in crate::ui) fn command(&self) -> ViewCommand {
        ViewCommand::Translate {
            language: self.language.clone(),
            download: self.download,
        }
    }
}

/// Whether the page can be translated, or is.
pub(in crate::ui) fn offered(tab: &Tab) -> bool {
    tab.translation.offered || tab.translation.translated
}

/// Translate to each language Ladybird offers, the downloads some need, and Show Original once
/// the page is translated.
pub(in crate::ui) fn choices(tab: &Tab) -> Vec<Choice> {
    let mut choices: Vec<Choice> = tab
        .translation
        .items
        .iter()
        .map(|item| Choice {
            label: item.label.clone(),
            checked: item.checked,
            enabled: item.enabled,
            language: item.language.clone(),
            download: item.download,
        })
        .collect();
    if tab.translation.translated {
        choices.push(Choice {
            label: "Show Original".to_owned(),
            checked: false,
            enabled: true,
            language: String::new(),
            download: false,
        });
    }
    choices
}

/// The address field's translate button, while the active page is in a language the user doesn't
/// read or is translated. It shows in the accent colour while the page is translated.
pub(in crate::ui) fn button(state: &AppState, theme: &Theme, overlays: &mut Vec<El>) -> Option<El> {
    let tab = state
        .browser
        .active_tab()
        .filter(|tab| !tab.is_internal() && offered(tab))?;
    let id = tab.id();
    let translated = tab.translation.translated;
    let trigger = controls::icon_button_frame(
        "translate-button",
        if translated {
            "Translated page"
        } else {
            "Translate page"
        },
        theme::ICON_BUTTON,
        theme,
    )
    .help(if translated {
        "Showing a translation"
    } else {
        "Translate this page"
    })
    .child(icon(&icons::TRANSLATE, theme::ICON).color(if translated {
        theme.accent
    } else {
        theme.icon
    }))
    .on_press(move |state: &mut AppState| {
        state.dismiss_context_menu();
        state.navigation_menu = None;
        state.page_menu = Default::default();
        state.zoom_menu = None;
        state.translate_menu = (state.translate_menu != Some(id)).then_some(id);
    });
    let mut menu = Menu::new("translate-menu")
        .trigger(trigger)
        .open(state.translate_menu == Some(id))
        .on_close(|state: &mut AppState| state.translate_menu = None);
    for choice in choices(tab) {
        menu = menu.item(
            MenuItem::new(choice.label.clone())
                .checked(choice.checked)
                .disabled(!choice.enabled)
                .on_select(move |state: &mut AppState| {
                    state.translate_menu = None;
                    if state.browser.active_tab().is_some_and(|tab| tab.id() == id) {
                        state.browser.send(id, choice.command());
                    }
                }),
        );
    }
    Some(menu.build(state, theme, overlays))
}

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::*;
    use crate::engine::Command;

    #[test]
    fn the_address_field_offers_translation_only_while_the_page_can_be_translated() {
        let mut state = AppState::new(Some("https://example.fr/"));
        let (engine, fake) = crate::engine::Engine::fake();
        state.browser.connect(engine);
        let tab = state.browser.active_tab().unwrap().id();
        let mut runtime = Runtime::new(state, super::super::view, (1000.0, 640.0), Env::default());
        runtime.poll = Some(crate::app::poll);
        runtime.frame(0.0);
        assert!(runtime.presented("translate-button").is_none());

        runtime.update(|state, _| {
            state.browser.tab_mut(tab).unwrap().translation = serde_json::from_str(
                r#"{"offered":true,"translated":false,"items":[
                    {"language":"en","label":"Translate to English","checked":false,"download":false,"enabled":true}]}"#,
            )
            .unwrap();
        });
        runtime.frame(0.016);
        let ([x, y, w, h], _) = runtime
            .presented("translate-button")
            .expect("the button shows for a page in another language");
        let (x, y) = (x + w / 2.0, y + h / 2.0);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.032);
        assert_eq!(runtime.state.translate_menu, Some(tab));

        let choices = choices(runtime.state.browser.tab(tab).unwrap());
        assert_eq!(choices.len(), 1);
        fake.commands.try_iter().count();
        runtime.update(|state, _| state.browser.send(tab, choices[0].command()));
        assert!(fake.commands.try_iter().any(|command| command
            == Command::View(
                tab,
                ViewCommand::Translate {
                    language: "en".into(),
                    download: false
                }
            )));
    }

    #[test]
    fn a_translated_page_offers_its_original() {
        let mut tab = Tab::new(crate::tab::TabId::from_raw(1), "https://example.fr/".into());
        tab.translation.translated = true;
        let choices = choices(&tab);
        assert_eq!(choices.last().unwrap().label, "Show Original");
        assert_eq!(
            choices.last().unwrap().command(),
            ViewCommand::Translate {
                language: String::new(),
                download: false
            }
        );
    }
}
