// SPDX-License-Identifier: GPL-3.0-only

//! Page accessibility trees, built only while an assistive client reads the window.

use std::collections::HashSet;

use rune::accesskit::{Action, ActionData, ActionRequest, TreeId, TreeUpdate};

use super::Browser;
use crate::engine::ViewCommand;
use crate::page_accessibility::{Applied, tree_id};
use crate::tab::TabId;

/// What the window should hand Rune for its page trees this turn.
#[derive(Debug, Default)]
pub(crate) struct AccessibilityChanges {
    pub(crate) updates: Vec<TreeUpdate>,
    pub(crate) removed: Vec<TreeId>,
}

impl Browser {
    /// Starts or stops page accessibility for every page in this window.
    pub(crate) fn set_accessibility(&mut self, active: bool) {
        if self.accessibility == active {
            return;
        }
        self.accessibility = active;
        let ids: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| !tab.is_internal())
            .map(|tab| tab.id())
            .collect();
        for id in ids {
            self.send(id, ViewCommand::SetAccessibility(active));
            if let Some(tab) = self.tab_mut(id) {
                tab.accessibility = Default::default();
                tab.accessibility_events.clear();
            }
        }
    }

    /// The tree Rune grafts under `tab`'s page, while page accessibility is on.
    pub(crate) fn accessibility_tree(&self, tab: TabId) -> Option<TreeId> {
        self.accessibility.then(|| tree_id(tab.raw()))
    }

    /// Applies the pages' reports and returns the updates and forgotten trees for Rune.
    pub(crate) fn take_accessibility_changes(&mut self) -> AccessibilityChanges {
        let mut changes = AccessibilityChanges::default();
        let mut live = HashSet::new();
        let mut reload = Vec::new();
        if self.accessibility {
            let default_zoom = self.default_zoom;
            for tab in self.tabs.iter_mut().filter(|tab| !tab.is_internal()) {
                let id = tab.id();
                let tree = tree_id(id.raw());
                let scale = f32::from(tab.zoom_override.unwrap_or(default_zoom)) / 100.0;
                live.insert(tree);
                for event in std::mem::take(&mut tab.accessibility_events) {
                    if tab.accessibility.apply(tree, event, scale) == Applied::NeedsWholeTree {
                        reload.push(id);
                    }
                }
                tab.accessibility.rescale(tree, scale);
                changes.updates.extend(tab.accessibility.take_updates());
            }
        }
        for id in reload {
            self.send(id, ViewCommand::SetAccessibility(true));
        }
        changes.removed = self
            .accessibility_trees
            .difference(&live)
            .copied()
            .collect();
        self.accessibility_trees = live;
        changes
    }

    /// Runs an assistive client's request on a page node, if it names one of this window's pages.
    pub(crate) fn accessibility_action(&self, request: &ActionRequest) -> bool {
        let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tree_id(tab.id().raw()) == request.target_tree)
        else {
            return false;
        };
        let node = request.target_node.0 as i64;
        let command = match (request.action, &request.data) {
            (Action::Click, _) => ViewCommand::AccessibilityAction {
                node,
                action: "press",
            },
            (Action::Focus, _) => ViewCommand::AccessibilityAction {
                node,
                action: "focus",
            },
            (Action::ScrollIntoView, _) => ViewCommand::AccessibilityAction {
                node,
                action: "scroll_into_view",
            },
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection)))
                if selection.anchor.node == selection.focus.node =>
            {
                let offset = |index: usize| i32::try_from(index).unwrap_or(i32::MAX);
                let (a, b) = (
                    offset(selection.anchor.character_index),
                    offset(selection.focus.character_index),
                );
                ViewCommand::AccessibilitySelect {
                    node: selection.anchor.node.0 as i64,
                    start: a.min(b),
                    end: a.max(b),
                }
            }
            _ => return false,
        };
        self.send(tab.id(), command);
        true
    }
}
