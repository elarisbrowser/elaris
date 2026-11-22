// SPDX-License-Identifier: GPL-3.0-only

//! The context menu for native pages: navigation, the page's own actions, and shortcuts to the
//! other internal pages.

use crate::ui::animated_icon::{self, AnimatedIcon};
use crate::ui::menu::{Menu, MenuItem, context_menu as menu};
use rune::{El, PointerPress, Tx, el};

use crate::app::{self, AppState};
use crate::internal::{InternalPage, InternalRoute};
use crate::tab::TabId;
use crate::ui::theme::Theme;

use crate::ui::viewport::menu::{KEY, with_header};

/// Internal pages the menu links to, except the one it is on.
type Open = fn(&mut AppState, &mut Tx);

const SHORTCUTS: [(InternalPage, &str, Open, Option<&AnimatedIcon>); 2] = [
    (InternalPage::Settings, "Settings", app::open_settings, None),
    (
        InternalPage::History,
        "History",
        app::open_history,
        Some(&animated_icon::HISTORY),
    ),
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
    let mut menu = with_header(
        menu(KEY, el(), Some(point))
            .on_close(|state: &mut AppState| state.page_menu = Default::default()),
        state,
        tab,
        theme,
    );

    let mut sections: Vec<Vec<MenuItem>> = Vec::new();
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
    sections.push(vec![
        item(id, "Copy Page Address", move |_, tx| tx.copy(&address))
            .animated(&animated_icon::COPY),
    ]);

    sections.push(
        SHORTCUTS
            .into_iter()
            .filter(|(page, ..)| Some(*page) != current)
            .map(|(_, label, open, icon)| {
                let entry = item(id, label, open);
                match icon {
                    Some(icon) => entry.animated(icon),
                    None => entry,
                }
            })
            .collect(),
    );

    menu = join(menu, sections);
    menu.build(state, theme, layer);
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
