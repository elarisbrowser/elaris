// SPDX-License-Identifier: GPL-3.0-only

use rune::tok::{radius, space, type_};
use rune::{
    El, Key, KeyEvent, PhysicalInput, ScrollSpec, Spring, SpringPair, Transition, Tx, col, el,
    icon, row, text,
};

use crate::app::AppState;
use crate::internal::{InternalPage, InternalRoute};
use crate::tab::{Tab, TabId};
use crate::ui::{VIEWPORT, icons, theme::Theme};

const PANEL: &str = "quick-tab-switcher";
const LIST: &str = "quick-tab-list";
pub(super) const ROW: f32 = 46.0;
const WIDTH: f32 = 384.0;
const VISIBLE_ROWS: usize = 6;
const MOTION: Spring = Spring::new(0.14, 0.0);

pub(crate) struct Switcher {
    origin: Option<TabId>,
    previous_focus: Option<String>,
    candidates: Vec<TabId>,
    selected: TabId,
}

impl Switcher {
    fn advance(&mut self, backwards: bool) {
        let count = self.candidates.len();
        let index = self
            .candidates
            .iter()
            .position(|&id| id == self.selected)
            .unwrap_or(0);
        self.selected = self.candidates[if backwards {
            (index + count - 1) % count
        } else {
            (index + 1) % count
        }];
    }
}

pub(crate) fn cycle_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if event.key != Key::Tab
        || !event.mods.ctrl
        || event.mods.alt
        || event.mods.logo
        || (cfg!(target_os = "macos") && event.mods.cmd)
    {
        return false;
    }
    if state.tab_drag.is_dragging() {
        return true;
    }
    reconcile(state, tx);
    if let Some(switcher) = &mut state.tab_switcher {
        switcher.advance(event.mods.shift);
    } else {
        let candidates = state.browser.recent_tabs();
        if candidates.len() < 2 {
            return true;
        }
        let origin = state.browser.active_tab().map(Tab::id);
        let mut switcher = Switcher {
            origin,
            previous_focus: tx.focused().map(str::to_owned),
            selected: candidates[0],
            candidates,
        };
        if origin.is_some() || event.mods.shift {
            switcher.advance(event.mods.shift);
        }
        state.dismiss_context_menu();
        state.page_menu = Default::default();
        state.tab_switcher = Some(switcher);
        tx.focus(PANEL, false);
    }
    if let Some(switcher) = &state.tab_switcher {
        tx.scroll_into_view_in(LIST, entry_key(switcher.selected));
    }
    animate(tx);
    true
}

fn animate(tx: &mut Tx) {
    tx.animated();
    tx.motion(MOTION);
}

pub(crate) fn finish(state: &mut AppState, target: Option<TabId>, tx: &mut Tx) {
    let Some(switcher) = state.tab_switcher.take() else {
        return;
    };
    if let Some(target) = target.filter(|&id| {
        switcher.origin == state.browser.active_tab().map(Tab::id)
            && switcher.candidates.contains(&id)
            && state.browser.tab(id).is_some()
    }) {
        state.omnibox.close();
        state.dismiss_find();
        state.activate(target);
        tx.focus(crate::ui::viewport::focus_key(&state.browser), false);
    } else {
        tx.focus(
            switcher.previous_focus.as_deref().unwrap_or(VIEWPORT),
            false,
        );
    }
    animate(tx);
}

pub(crate) fn reconcile(state: &mut AppState, tx: &mut Tx) -> bool {
    let Some(switcher) = &mut state.tab_switcher else {
        return false;
    };
    let before = switcher.candidates.len();
    switcher
        .candidates
        .retain(|&id| state.browser.tab(id).is_some());
    if switcher.origin != state.browser.active_tab().map(Tab::id)
        || switcher.candidates.len() < 2
        || !switcher.candidates.contains(&switcher.selected)
    {
        finish(state, None, tx);
        true
    } else {
        before != switcher.candidates.len()
    }
}

fn physical(state: &mut AppState, input: &PhysicalInput, tx: &mut Tx) -> bool {
    match input {
        PhysicalInput::Modifiers(mods) if !mods.ctrl => {
            let Some(target) = state
                .tab_switcher
                .as_ref()
                .map(|switcher| switcher.selected)
            else {
                return false;
            };
            finish(state, Some(target), tx);
            true
        }
        _ => false,
    }
}

fn key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    if cycle_key(state, event, tx) {
        return true;
    }
    match event.key {
        Key::Escape => finish(state, None, tx),
        Key::Enter => {
            let selected = state
                .tab_switcher
                .as_ref()
                .map(|switcher| switcher.selected);
            finish(state, selected, tx);
        }
        _ if event.mods.cmd || event.mods.logo => return false,
        _ => {}
    }
    true
}

fn entry_key(id: TabId) -> String {
    format!("quick-tab-{}", id.raw())
}

pub(crate) fn view(state: &AppState, theme: &Theme, height: f32, reduced: bool) -> Option<El> {
    let switcher = state.tab_switcher.as_ref()?;
    let index = switcher
        .candidates
        .iter()
        .position(|&id| id == switcher.selected)?;
    let motion = if reduced { Spring::NONE } else { MOTION };
    let mut items = col().key("quick-tab-items").w_full().child(
        el().key("quick-tab-highlight")
            .pin(Some(0.0), Some(0.0), Some(0.0), None)
            .h(ROW)
            .offset(0.0, index as f32 * ROW)
            .offset_motion(motion)
            .radius(radius::MD)
            .fill(theme.overlay_selected),
    );
    for &id in &switcher.candidates {
        if let Some(tab) = state.browser.tab(id) {
            items = items.child(entry(tab, id == switcher.selected, theme));
        }
    }
    let list_height = (switcher.candidates.len().min(VISIBLE_ROWS) as f32 * ROW)
        .min((height - 3.0 * space::S6).max(0.0));

    let panel = tab_panel(PANEL, theme)
        .shadow(8.0, 26.0, theme.overlay_shadow)
        .focusable(true)
        .role("listbox")
        .label("Recently used tabs")
        .active_descendant(entry_key(switcher.selected))
        .description_static(
            "Release Control or press Enter to switch tabs. Press Escape to cancel.",
        )
        .preserve_focus_on_press(true)
        .on_physical_input(physical)
        .on_key(key)
        .transition(if reduced {
            Transition::none()
        } else {
            Transition::scale(0.97).springs(SpringPair::new(MOTION, Spring::new(0.11, 0.0)))
        })
        .token_motion(motion)
        .child(
            col()
                .key(LIST)
                .w_full()
                .h(list_height)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(items),
        );

    Some(
        col()
            .key("quick-tab-overlay")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .px(space::S6)
            .pb(space::S6)
            .center()
            .child(
                el().key("quick-tab-dismiss")
                    .absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .preserve_focus_on_press(true)
                    .on_press_tx(|state: &mut AppState, tx| finish(state, None, tx))
                    .on_wheel(|_: &mut AppState, _, _| true),
            )
            .child(panel),
    )
}

pub(super) fn tab_panel(key: &str, theme: &Theme) -> El {
    col()
        .key(key)
        .w_full()
        .max_w(WIDTH)
        .padding(space::S2)
        .radius(radius::LG)
        .continuous_corners()
        .fill(theme.overlay_panel)
        .border(1.0, theme.overlay_border)
}

fn entry(tab: &Tab, selected: bool, theme: &Theme) -> El {
    let id = tab.id();
    tab_entry(entry_key(id), tab, selected, theme)
        .on_pointer_move_local(move |state: &mut AppState, _, tx| {
            if let Some(switcher) = &mut state.tab_switcher
                && switcher.selected != id
            {
                switcher.selected = id;
                animate(tx);
            }
        })
        .on_press_tx(move |state: &mut AppState, tx| finish(state, Some(id), tx))
}

pub(super) fn tab_entry(key: String, tab: &Tab, selected: bool, theme: &Theme) -> El {
    let address = if tab.is_internal() {
        &tab.url
    } else {
        crate::address::host_for_address(&tab.url)
    };
    let title = if tab.title.trim().is_empty() {
        if address.is_empty() {
            "Untitled tab"
        } else {
            address
        }
    } else {
        &tab.title
    };
    let glyph = match tab.internal {
        Some(InternalRoute::Page(InternalPage::History)) => &icons::HISTORY,
        Some(InternalRoute::Page(InternalPage::Downloads)) => &icons::DOWNLOAD,
        Some(InternalRoute::Page(InternalPage::Settings)) => &icons::SYSTEM,
        _ => &icons::PAGE,
    };
    let symbol = tab.favicon.as_ref().map_or_else(
        || icon(glyph, 18.0).color(theme.icon),
        |pixels| el().size(18.0, 18.0).pixels(pixels),
    );
    let mut labels = col().min_w(0.0).grow(1.0).child(
        text(title)
            .font(type_::BODY)
            .color(theme.text)
            .single_line()
            .truncate(),
    );
    if !address.is_empty() && address != title {
        labels = labels.child(
            text(address)
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate(),
        );
    }
    row()
        .key(key)
        .w_full()
        .h(ROW)
        .min_h(ROW)
        .px(space::S3)
        .gap(space::S3)
        .center_y()
        .role("option")
        .label(if selected {
            format!("{title}, selected")
        } else {
            title.to_owned()
        })
        .preserve_focus_on_press(true)
        .child(symbol)
        .child(labels)
}

#[cfg(test)]
mod tests {
    use rune::{Env, Mods, Runtime};

    use super::*;
    use crate::tab::TabSection;

    fn runtime() -> (Runtime<AppState>, Vec<TabId>) {
        let mut state = AppState::new(Some("first.example"));
        let ids = [
            state.browser.active_tab().unwrap().id(),
            state
                .browser
                .restore_tab_in_section("elaris://history", "", true, TabSection::Favourite)
                .unwrap(),
            state
                .browser
                .restore_tab_in_section("pinned.example", "Pinned", true, TabSection::Pinned)
                .unwrap(),
        ];
        state.activate(ids[1]);
        state.activate(ids[0]);
        let mut runtime =
            Runtime::new(state, crate::ui::view, (960.0, 640.0), Env::new(false, 1.0));
        runtime.frame(0.0);
        runtime.update(|_, tx| tx.focus(VIEWPORT, false));
        runtime.frame(0.0);
        (runtime, ids.to_vec())
    }

    fn cycle(runtime: &mut Runtime<AppState>, shift: bool) {
        runtime.key(KeyEvent {
            key: Key::Tab,
            mods: Mods {
                ctrl: true,
                shift,
                ..Mods::default()
            },
        });
        runtime.frame(0.0);
    }

    fn release(runtime: &mut Runtime<AppState>) {
        runtime.physical_input(PhysicalInput::Modifiers(Mods::default()));
        runtime.frame(0.0);
    }

    #[test]
    fn keyboard_previews_stable_entries_and_release_commits_mru() {
        let (mut runtime, ids) = runtime();
        assert_eq!(runtime.state.browser.recent_tabs(), ids);
        cycle(&mut runtime, false);
        assert_eq!(
            runtime.state.tab_switcher.as_ref().unwrap().selected,
            ids[1]
        );
        assert!(
            runtime
                .elements()
                .iter()
                .any(|node| node.focused && node.key.as_deref() == Some(PANEL))
        );
        assert_eq!(
            runtime
                .elements()
                .iter()
                .find(|node| node.key.as_deref() == Some(LIST) && !node.exiting)
                .unwrap()
                .rect[3],
            3.0 * ROW
        );
        for _ in 0..16 {
            cycle(&mut runtime, true);
            cycle(&mut runtime, false);
        }
        cycle(&mut runtime, false);
        assert!(
            runtime
                .state
                .browser
                .tab(ids[2])
                .unwrap()
                .engine
                .deferred_load
        );
        assert_eq!(
            runtime.state.tab_switcher.as_ref().unwrap().selected,
            ids[2]
        );
        assert_eq!(runtime.state.browser.recent_tabs(), ids);
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), ids[0]);
        assert_eq!(
            runtime
                .state
                .browser
                .tabs()
                .iter()
                .map(Tab::id)
                .collect::<Vec<_>>(),
            [ids[1], ids[2], ids[0]]
        );
        release(&mut runtime);
        assert!(runtime.state.tab_switcher.is_none());
        assert_eq!(
            runtime.state.browser.recent_tabs(),
            [ids[2], ids[0], ids[1]]
        );
        assert!(
            runtime
                .elements()
                .iter()
                .any(|node| node.focused && node.key.as_deref() == Some(VIEWPORT))
        );
        cycle(&mut runtime, false);
        release(&mut runtime);
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), ids[0]);
    }

    #[test]
    fn escape_outside_click_and_window_blur_cancel_before_modifier_release() {
        let (mut runtime, ids) = runtime();
        cycle(&mut runtime, true);
        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        release(&mut runtime);
        cycle(&mut runtime, false);
        runtime.frame(1.0);
        runtime.pointer_down(40.0, 90.0);
        runtime.pointer_up(40.0, 90.0);
        runtime.frame(1.0);
        assert!(runtime.state.tab_switcher.is_none());
        release(&mut runtime);
        cycle(&mut runtime, false);
        runtime.update(|state, tx| crate::app::on_window_focus(state, false, tx));
        release(&mut runtime);
        assert!(runtime.state.tab_switcher.is_none());
        assert_eq!(runtime.state.browser.recent_tabs(), ids);
    }

    #[test]
    fn closing_selected_candidate_cancels_and_compacts_the_next_opening() {
        let (mut runtime, ids) = runtime();
        cycle(&mut runtime, false);
        runtime.update(|state, tx| {
            state.close(ids[1]);
            assert!(reconcile(state, tx));
        });
        release(&mut runtime);
        assert_eq!(runtime.state.browser.recent_tabs(), [ids[0], ids[2]]);
        cycle(&mut runtime, false);
        assert_eq!(
            runtime
                .elements()
                .iter()
                .find(|node| node.key.as_deref() == Some(LIST) && !node.exiting)
                .unwrap()
                .rect[3],
            2.0 * ROW
        );
        release(&mut runtime);
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), ids[2]);
    }
    #[test]
    fn omnibox_focus_is_restored_on_cancel_and_closed_on_pointer_commit() {
        let (mut runtime, ids) = runtime();
        runtime.update(|state, tx| {
            state.open_omnibox(crate::omnibox::Mode::Current);
            tx.focus("omnibox-input", false);
        });
        runtime.frame(0.0);
        cycle(&mut runtime, false);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|node| node.focused && node.key.as_deref() == Some(PANEL))
        );
        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        release(&mut runtime);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|node| node.focused && node.key.as_deref() == Some("omnibox-input"))
        );
        cycle(&mut runtime, false);
        runtime.frame(1.0);
        let [x, y, w, h] = runtime
            .elements()
            .iter()
            .find(|node| node.key.as_deref() == Some(&entry_key(ids[1])))
            .unwrap()
            .rect;
        runtime.pointer_down(x + w / 2.0, y + h / 2.0);
        runtime.pointer_up(x + w / 2.0, y + h / 2.0);
        runtime.frame(1.0);
        assert!(!runtime.state.omnibox.open);
        assert_eq!(runtime.state.browser.active_tab().unwrap().id(), ids[1]);
    }

    #[test]
    fn reduced_motion_long_list_reveals_the_wrapped_selection() {
        let (mut runtime, _) = runtime();
        runtime.set_system_reduced_motion(true);
        for i in 0..8 {
            runtime
                .state
                .browser
                .open_tab(&format!("tab{i}.example"))
                .unwrap();
        }
        cycle(&mut runtime, true);
        let selected = runtime.state.tab_switcher.as_ref().unwrap().selected;
        let nodes = runtime.elements();
        let list = nodes
            .iter()
            .find(|node| node.key.as_deref() == Some(LIST))
            .unwrap();
        let entry = nodes
            .iter()
            .find(|node| node.key.as_deref() == Some(&entry_key(selected)))
            .unwrap();
        assert_eq!(list.rect[3], VISIBLE_ROWS as f32 * ROW);
        assert!(entry.rect[1] >= list.rect[1]);
        assert!(entry.rect[1] + entry.rect[3] <= list.rect[1] + list.rect[3] + 0.01);
        release(&mut runtime);
        assert!(
            !runtime
                .elements()
                .iter()
                .any(|node| node.key.as_deref() == Some(PANEL))
        );
    }
    #[test]
    fn highlight_moves_without_moving_entries_and_panel_animates_out() {
        let (mut runtime, ids) = runtime();
        cycle(&mut runtime, false);
        runtime.frame(1.0);
        let before = runtime.elements();
        runtime.key(KeyEvent {
            key: Key::Tab,
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
        });
        runtime.frame(0.01);
        let after = runtime.elements();
        let find = |nodes: &[rune::ElementInfo], key: &str| {
            let node = nodes
                .iter()
                .find(|node| node.key.as_deref() == Some(key))
                .unwrap();
            (node.path.clone(), node.rect)
        };
        let highlight = find(&after, "quick-tab-highlight");
        let start = find(&before, "quick-tab-highlight").1[1];
        assert!(highlight.1[1] >= start && highlight.1[1] < start + ROW);
        assert_eq!(
            find(&after, &entry_key(ids[1])),
            find(&before, &entry_key(ids[1]))
        );
        runtime.physical_input(PhysicalInput::Modifiers(Mods::default()));
        runtime.frame(0.01);
        assert!(
            runtime
                .elements()
                .iter()
                .any(|node| node.key.as_deref() == Some(PANEL) && node.exiting)
        );
    }
}
