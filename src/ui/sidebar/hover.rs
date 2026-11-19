// SPDX-License-Identifier: GPL-3.0-only

use std::cell::Cell;

use rune::{El, Tx};

use crate::{app::AppState, tab::TabId};

const OPEN_DELAY: f64 = 0.4;
const SWITCH_DELAY: f64 = 0.06;
const LEAVE_DELAY: f64 = 0.10;
pub(in crate::ui) const FADE_DURATION: f32 = 0.12;

#[derive(Default)]
pub(in crate::ui) struct Hover {
    candidate: Option<TabId>,
    shown: Option<TabId>,
    due: Option<f64>,
    fading: bool,
    pub(in crate::ui) reduced: Cell<bool>,
}

impl Hover {
    fn enter(&mut self, id: TabId, now: f64) {
        self.candidate = Some(id);
        let delay = match self.shown {
            Some(shown) if shown == id => 0.0,
            Some(_) => SWITCH_DELAY,
            None => OPEN_DELAY,
        };
        self.due = Some(now + delay);
    }

    fn leave(&mut self, id: TabId, now: f64) {
        if self.candidate == Some(id) {
            self.candidate = None;
            if !self.fading {
                self.due = self.shown.map(|_| now + LEAVE_DELAY);
            }
        }
    }

    fn step(&mut self, now: f64, reduced: bool) -> bool {
        if self.due.is_none_or(|due| now < due) {
            return false;
        }
        self.due = None;
        let before = self.display();
        if let Some(id) = self.candidate {
            self.shown = Some(id);
            self.fading = false;
        } else if !reduced && self.shown.is_some() && !self.fading {
            self.fading = true;
            self.due = Some(now + f64::from(FADE_DURATION));
        } else {
            self.shown = None;
            self.fading = false;
        }
        self.display() != before
    }

    pub(in crate::ui) fn display(&self) -> Option<(TabId, bool)> {
        self.shown.map(|id| (id, self.fading))
    }

    fn dismiss(&mut self) -> bool {
        let changed = self.shown.is_some();
        self.candidate = None;
        self.shown = None;
        self.due = None;
        self.fading = false;
        changed
    }

    fn wake(&self, tx: &mut Tx) {
        if let Some(due) = self.due {
            tx.wake_at(due);
        }
    }
}

pub(in crate::ui) fn allowed(state: &AppState) -> bool {
    state.browser.window_focused()
        && !state.sidebar.collapsed
        && !state.tab_drag.is_dragging()
        && state.split_preview.is_none()
        && !state.omnibox.open
        && !state.find_open
        && state.tab_switcher.is_none()
        && state.context_menu().is_none()
        && !state.page_menu.open()
        && !state.site_permissions_open
        && !state.downloads_open
}

pub(in crate::ui) fn attach(tab: El, id: TabId) -> El {
    tab.on_hover_change(move |state: &mut AppState, inside, tx| {
        if inside && allowed(state) {
            state.sidebar.hover.enter(id, tx.clock());
        } else {
            state.sidebar.hover.leave(id, tx.clock());
        }
        state.sidebar.hover.wake(tx);
        tx.live_only();
    })
}

pub(crate) fn displayed(state: &AppState) -> Option<TabId> {
    state.sidebar.hover.display().map(|(id, _)| id)
}

pub(crate) fn dismiss(state: &mut AppState) -> bool {
    state.sidebar.hover.dismiss()
}

pub(crate) fn poll(state: &mut AppState, now: f64, tx: &mut Tx) -> bool {
    let exists = |id| state.browser.tab(id).is_some();
    let hover = &state.sidebar.hover;
    if !allowed(state)
        || hover.shown.is_some_and(|id| !exists(id))
        || hover.candidate.is_some_and(|id| !exists(id))
    {
        return dismiss(state);
    }
    let hover = &mut state.sidebar.hover;
    let changed = hover.step(now, hover.reduced.get());
    if changed && !hover.reduced.get() && hover.display().is_some() {
        tx.animated();
    }
    if changed {
        hover.wake(tx);
    }
    changed
}
