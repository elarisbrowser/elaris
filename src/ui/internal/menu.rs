// SPDX-License-Identifier: GPL-3.0-only

//! The context menu for native pages: navigation, the page's own actions, and shortcuts to the
//! other internal pages.

use rune::{El, PointerPress, Tx, el};
use rune_kit::{Menu, MenuItem, context_menu as menu};

use crate::app::{self, AppState};
use crate::internal::{InternalPage, InternalRoute};
use crate::tab::TabId;
use crate::ui::theme::Theme;

/// Shares the web page menu's key, so it gets the same hover highlight and dismissal.
const KEY: &str = "page-context-menu";

/// Internal pages the menu links to, except the one it is on.
type Open = fn(&mut AppState, &mut Tx);

const SHORTCUTS: [(InternalPage, &str, Open); 2] = [
    (InternalPage::Settings, "Settings", app::open_settings),
    (InternalPage::History, "History", app::open_history),
];

/// Builds the menu for the internal page in tab `id`, opened at `point`.
pub(crate) fn context_menu(
    state: &AppState,
    id: TabId,
    point: PointerPress,
    theme: &Theme,
    layer: &mut Vec<El>,
) {
    let Some(tab) = state.browser.active_tab().filter(|tab| tab.id() == id) else {
        return;
    };
    let Some(route) = &tab.internal else {
        return;
    };
    let current = match route {
        InternalRoute::Page(page) => Some(*page),
        InternalRoute::NotFound(_) => None,
    };
    let mut menu = menu(KEY, el(), Some(point))
        .on_close(|state: &mut AppState| state.page_menu = Default::default());

    let mut sections: Vec<Vec<MenuItem>> = Vec::new();
    let mut navigation = Vec::new();
    if tab.can_go_back {
        navigation.push(item(id, "Back", move |state, _| state.browser.go_back(id)));
    }
    if tab.can_go_forward {
        navigation.push(item(id, "Forward", move |state, _| {
            state.browser.go_forward(id)
        }));
    }
    sections.push(navigation);

    if let Some(page) = current {
        sections.push(
            super::page(page)
                .actions
                .iter()
                .map(|action| item(id, action.label, action.run).disabled(!(action.enabled)(state)))
                .collect(),
        );
    }

    let address = current.map_or_else(|| tab.url.clone(), |page| page.address().to_owned());
    sections.push(vec![item(id, "Copy Page Address", move |_, tx| {
        tx.copy(&address)
    })]);

    sections.push(
        SHORTCUTS
            .into_iter()
            .filter(|(page, ..)| Some(*page) != current)
            .map(|(_, label, open)| item(id, label, open))
            .collect(),
    );

    menu = join(menu, sections);
    crate::ui::hover_highlight::build_context_menu(state, menu, theme, layer);
}

/// Adds each non-empty section, with a separator between neighbours.
fn join(mut menu: Menu, sections: Vec<Vec<MenuItem>>) -> Menu {
    let mut first = true;
    for section in sections.into_iter().filter(|section| !section.is_empty()) {
        if !first {
            menu = menu.separator();
        }
        first = false;
        for entry in section {
            menu = menu.item(entry);
        }
    }
    menu
}

/// An item that acts only while its tab is still the active one, then closes the menu.
fn item(id: TabId, label: &str, action: impl Fn(&mut AppState, &mut Tx) + 'static) -> MenuItem {
    MenuItem::new(label).on_select_tx(move |state: &mut AppState, tx| {
        state.page_menu = Default::default();
        if state.browser.active_tab().is_some_and(|tab| tab.id() == id) {
            action(state, tx);
        }
    })
}
