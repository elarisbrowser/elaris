// SPDX-License-Identifier: GPL-3.0-only

use elaris::ladybird::{Edit, PageMenu};
use rune::{El, PointerPress, Tx, el};
use rune_kit::{MenuItem, context_menu};

use crate::{app::AppState, engine::ViewCommand, tab::TabId, ui::theme::Theme};

#[derive(Default)]
pub(crate) struct PageMenuState {
    pub(super) anchor: Option<(TabId, PointerPress)>,
    current: Option<(TabId, PointerPress, PageMenu)>,
}
impl PageMenuState {
    pub(crate) fn receive(&mut self, id: TabId, menu: PageMenu) {
        if let Some((owner, point)) = self.anchor.take()
            && owner == id
        {
            self.current = Some((id, point, menu));
        }
    }
    pub(crate) fn open(&self) -> bool {
        self.current.is_some()
    }
}

fn item(id: TabId, label: &str, action: impl Fn(&mut AppState, &mut Tx) + 'static) -> MenuItem {
    MenuItem::new(label).on_select_tx(move |s: &mut AppState, tx| {
        if s.browser.active_tab().is_some_and(|tab| tab.id() == id) && !s.omnibox.open {
            tx.focus(crate::ui::viewport::focus_key(&s.browser), false);
            action(s, tx);
        }
        s.page_menu = Default::default();
    })
}

pub(crate) fn view(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    let Some((id, point, info)) = &state.page_menu.current else {
        return;
    };
    let Some(tab) = state.browser.active_tab().filter(|tab| tab.id() == *id) else {
        return;
    };
    let id = *id;
    let request = info.request;
    let mut menu = context_menu("page-context-menu", el(), Some(*point))
        .on_close(|s: &mut AppState| s.page_menu = Default::default());
    if !info.url.is_empty() {
        if !info.image {
            menu = menu.item(item(id, "Open Link", move |s, _| {
                s.browser.send(
                    id,
                    ViewCommand::OpenContext {
                        request,
                        new_tab: false,
                    },
                )
            }));
        }
        menu = menu.item(item(
            id,
            if info.image {
                "Open Image in New Tab"
            } else {
                "Open Link in New Tab"
            },
            move |s, _| {
                s.browser.send(
                    id,
                    ViewCommand::OpenContext {
                        request,
                        new_tab: true,
                    },
                )
            },
        ));
        let url = info.url.clone();
        menu = menu.item(item(
            id,
            if info.image {
                "Copy Image Address"
            } else {
                "Copy Link Address"
            },
            move |_, tx| tx.copy(&url),
        ));
    } else {
        if tab.can_go_back {
            menu = menu.item(item(id, "Back", move |s, _| s.browser.go_back(id)));
        }
        if tab.can_go_forward {
            menu = menu.item(item(id, "Forward", move |s, _| s.browser.go_forward(id)));
        }
        menu = menu
            .item(item(id, "Reload", move |s, _| s.browser.reload(id)))
            .item(item(id, "Reload Without Cache", move |s, _| {
                s.browser.reload_bypassing_cache(id);
            }));
    }
    if !info.selection.is_empty() {
        let text = info.selection.clone();
        menu = menu.item(item(id, "Copy", move |_, tx| tx.copy(&text)));
        if info.editable {
            menu = menu.item(item(id, "Cut", move |s, _| {
                s.browser.send(id, ViewCommand::Edit(Edit::Cut))
            }));
        }
    }
    if info.editable {
        menu = menu.item(item(id, "Paste", move |_, tx| super::paste(id, tx)));
    }
    if info.url.is_empty() {
        menu = menu.item(item(id, "Select All", move |s, _| {
            s.browser.send(id, ViewCommand::Edit(Edit::SelectAll))
        }));
    }
    crate::ui::hover_highlight::build_context_menu(state, menu, theme, layer);
}
