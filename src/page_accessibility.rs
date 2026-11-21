// SPDX-License-Identifier: GPL-3.0-only

//! A page's accessibility tree, kept from Ladybird's whole trees and numbered changes, and turned
//! into the AccessKit updates Rune grafts under the page's element.

use std::collections::{HashMap, HashSet};

use elaris::ladybird::{AccessibilityEvent, AccessibilityNode};
use rune::accesskit::{
    Action, Live, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate, Uuid,
};

/// High bits of every page tree's id, so page trees never collide with other grafted trees.
const TREE_ID_PREFIX: u128 = 0x454c_4152_4953_5041_u128 << 64;

/// What the page asked of the browser after an event: nothing, or its whole tree again.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    Done,
    /// A change didn't fit the tree held; ask the page for the whole tree.
    NeedsWholeTree,
}

/// One page's accessibility tree and the AccessKit updates not yet handed to Rune.
#[derive(Debug, Default)]
pub(crate) struct PageAccessibility {
    nodes: HashMap<i64, AccessibilityNode>,
    root: Option<i64>,
    seq: Option<u64>,
    focus: Option<i64>,
    /// Points per CSS pixel the bounds were last converted with.
    scale: f32,
    pending: Vec<TreeUpdate>,
}

/// The id Rune grafts a tab's page tree under.
pub(crate) fn tree_id(tab: u64) -> TreeId {
    TreeId(Uuid::from_u128(TREE_ID_PREFIX | u128::from(tab)))
}

impl PageAccessibility {
    /// Applies what the page reported. `scale` converts its CSS pixels to points (the page zoom).
    pub(crate) fn apply(&mut self, tree: TreeId, event: AccessibilityEvent, scale: f32) -> Applied {
        match event {
            AccessibilityEvent::Tree { seq, nodes } => {
                self.root = nodes.first().map(|node| node.id);
                self.nodes = nodes.into_iter().map(|node| (node.id, node)).collect();
                self.seq = Some(seq);
                self.focus = self
                    .nodes
                    .values()
                    .find(|node| node.focused)
                    .map(|node| node.id);
                self.scale = scale;
                self.push_whole(tree);
            }
            AccessibilityEvent::Change {
                base,
                seq,
                nodes,
                removed,
            } => {
                if self.seq != Some(base) {
                    return Applied::NeedsWholeTree;
                }
                for id in removed {
                    self.nodes.remove(&id);
                }
                let changed: Vec<i64> = nodes.iter().map(|node| node.id).collect();
                for node in nodes {
                    if node.focused {
                        self.focus = Some(node.id);
                    }
                    self.nodes.insert(node.id, node);
                }
                self.seq = Some(seq);
                let reachable = self.reachable();
                if changed.iter().any(|id| !reachable.contains(id)) {
                    return Applied::NeedsWholeTree;
                }
                if scale != self.scale {
                    self.scale = scale;
                    self.push_whole(tree);
                } else {
                    self.push(tree, &changed, false);
                }
            }
            AccessibilityEvent::Focus(id) => {
                self.focus = self.reachable().contains(&id).then_some(id);
                self.push(tree, &[], false);
            }
        }
        Applied::Done
    }

    /// Converts every node again after the page zoom changed.
    pub(crate) fn rescale(&mut self, tree: TreeId, scale: f32) {
        if self.root.is_some() && scale != self.scale {
            self.scale = scale;
            self.push_whole(tree);
        }
    }

    pub(crate) fn take_updates(&mut self) -> Vec<TreeUpdate> {
        std::mem::take(&mut self.pending)
    }

    /// Ids of the nodes reached from the root. AccessKit rejects a node no parent in the update
    /// holds, so detached nodes the page still reports are left out.
    fn reachable(&self) -> HashSet<i64> {
        let mut seen = HashSet::new();
        let mut stack: Vec<i64> = self.root.into_iter().collect();
        while let Some(id) = stack.pop() {
            if let Some(node) = self.nodes.get(&id)
                && seen.insert(id)
            {
                stack.extend(node.children.iter().copied());
            }
        }
        seen
    }

    fn push_whole(&mut self, tree: TreeId) {
        let ids: Vec<i64> = self.reachable().into_iter().collect();
        self.push(tree, &ids, true);
    }

    fn push(&mut self, tree: TreeId, ids: &[i64], whole: bool) {
        let Some(root) = self.root else { return };
        let nodes = ids
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .map(|node| (node_id(node.id), self.node(node)))
            .collect();
        self.pending.push(TreeUpdate {
            nodes,
            tree: whole.then(|| TreeInfo {
                root: node_id(root),
                toolkit_name: Some("Ladybird".into()),
                toolkit_version: None,
            }),
            tree_id: tree,
            focus: node_id(self.focus.unwrap_or(root)),
        });
    }

    fn node(&self, page: &AccessibilityNode) -> Node {
        let role = role(page);
        let mut node = Node::new(role);
        node.set_children(
            page.children
                .iter()
                .filter(|id| self.nodes.contains_key(id))
                .copied()
                .map(node_id)
                .collect::<Vec<_>>(),
        );
        let [x, y, w, h] = page.bounds.map(|v| f64::from(v as f32 * self.scale));
        node.set_bounds(Rect {
            x0: x,
            y0: y,
            x1: x + w.max(0.0),
            y1: y + h.max(0.0),
        });
        if !page.name.is_empty() {
            node.set_label(page.name.as_str());
        }
        if !page.description.is_empty() {
            node.set_description(page.description.as_str());
        }
        if !page.value.is_empty() && !page.password {
            node.set_value(page.value.as_str());
        }
        if !page.url.is_empty() {
            node.set_url(page.url.as_str());
        }
        if page.heading_level > 0 {
            node.set_level(page.heading_level as usize);
        } else if page.level > 0 {
            node.set_level(page.level as usize);
        }
        match page.checked {
            1 => node.set_toggled(Toggled::False),
            2 => node.set_toggled(Toggled::True),
            3 => node.set_toggled(Toggled::Mixed),
            _ if page.pressed => node.set_toggled(Toggled::True),
            _ => {}
        }
        match page.expanded {
            1 => node.set_expanded(false),
            2 => node.set_expanded(true),
            _ => {}
        }
        match page.live.as_str() {
            "assertive" => node.set_live(Live::Assertive),
            "polite" => node.set_live(Live::Polite),
            _ => {}
        }
        if page.selected {
            node.set_selected(true);
        }
        if page.disabled {
            node.set_disabled();
        }
        if page.read_only {
            node.set_read_only();
        }
        if page.required {
            node.set_required();
        }
        if page.invalid {
            node.set_invalid(rune::accesskit::Invalid::True);
        }
        if let Some(value) = page.value_numeric {
            node.set_numeric_value(value);
        }
        if let Some(value) = page.value_minimum {
            node.set_min_numeric_value(value);
        }
        if let Some(value) = page.value_maximum {
            node.set_max_numeric_value(value);
        }
        if let Some(value) = page.value_step {
            node.set_numeric_value_step(value);
        }
        if role != Role::Label && !page.disabled {
            node.add_action(Action::Focus);
            node.add_action(Action::ScrollIntoView);
            if presses(role) {
                node.add_action(Action::Click);
            }
        }
        node
    }
}

fn node_id(id: i64) -> NodeId {
    NodeId(id as u64)
}

/// Roles a click activates, as a pointer press on the element would.
fn presses(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Link
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::Tab
            | Role::ListBoxOption
            | Role::TreeItem
            | Role::ComboBox
            | Role::DisclosureTriangle
    )
}

/// AccessKit's role for a page node's ARIA role.
fn role(node: &AccessibilityNode) -> Role {
    if node.editable {
        return if node.password {
            Role::PasswordInput
        } else if node.multiline {
            Role::MultilineTextInput
        } else {
            Role::TextInput
        };
    }
    match node.role.as_str() {
        "text leaf" => Role::Label,
        "document" | "rootwebarea" => Role::Document,
        "iframe" => Role::Iframe,
        "button" => Role::Button,
        "link" => Role::Link,
        "heading" => Role::Heading,
        "paragraph" => Role::Paragraph,
        "image" | "img" => Role::Image,
        "figure" => Role::Figure,
        "checkbox" => Role::CheckBox,
        "radio" => Role::RadioButton,
        "radiogroup" => Role::RadioGroup,
        "switch" => Role::Switch,
        "slider" => Role::Slider,
        "spinbutton" => Role::SpinButton,
        "progressbar" => Role::ProgressIndicator,
        "meter" => Role::Meter,
        "combobox" => Role::ComboBox,
        "listbox" => Role::ListBox,
        "option" => Role::ListBoxOption,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "menu" => Role::Menu,
        "menubar" => Role::MenuBar,
        "menuitem" => Role::MenuItem,
        "menuitemcheckbox" => Role::MenuItemCheckBox,
        "menuitemradio" => Role::MenuItemRadio,
        "tab" => Role::Tab,
        "tablist" => Role::TabList,
        "tabpanel" => Role::TabPanel,
        "table" | "grid" => Role::Table,
        "row" => Role::Row,
        "cell" | "gridcell" => Role::Cell,
        "columnheader" => Role::ColumnHeader,
        "rowheader" => Role::RowHeader,
        "tree" => Role::Tree,
        "treeitem" => Role::TreeItem,
        "dialog" => Role::Dialog,
        "alertdialog" => Role::AlertDialog,
        "alert" => Role::Alert,
        "status" => Role::Status,
        "navigation" => Role::Navigation,
        "main" => Role::Main,
        "banner" => Role::Banner,
        "contentinfo" => Role::ContentInfo,
        "complementary" => Role::Complementary,
        "region" => Role::Region,
        "search" => Role::Search,
        "form" => Role::Form,
        "article" => Role::Article,
        "separator" => Role::Splitter,
        "toolbar" => Role::Toolbar,
        "tooltip" => Role::Tooltip,
        "group" => Role::Group,
        "textbox" | "searchbox" => Role::TextInput,
        _ => Role::GenericContainer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i64, role: &str, children: &[i64]) -> AccessibilityNode {
        AccessibilityNode {
            id,
            role: role.into(),
            name: format!("node {id}"),
            children: children.to_vec(),
            bounds: [10, 20, 100, 40],
            ..AccessibilityNode::default()
        }
    }

    #[test]
    fn detached_nodes_stay_out_of_the_graft() {
        let tree = tree_id(7);
        let mut page = PageAccessibility::default();
        let whole = AccessibilityEvent::Tree {
            seq: 1,
            nodes: vec![
                node(1, "document", &[2]),
                node(2, "button", &[]),
                node(9, "link", &[]),
            ],
        };
        assert_eq!(page.apply(tree, whole, 1.0), Applied::Done);
        let ids: Vec<NodeId> = page.take_updates()[0]
            .nodes
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert!(!ids.contains(&NodeId(9)));

        let orphan = AccessibilityEvent::Change {
            base: 1,
            seq: 2,
            nodes: vec![node(10, "link", &[])],
            removed: vec![],
        };
        assert_eq!(page.apply(tree, orphan, 1.0), Applied::NeedsWholeTree);
    }

    #[test]
    fn a_whole_tree_then_changes_become_graftable_updates() {
        let tree = tree_id(7);
        let mut page = PageAccessibility::default();
        let whole = AccessibilityEvent::Tree {
            seq: 1,
            nodes: vec![node(1, "document", &[2]), node(2, "button", &[])],
        };
        assert_eq!(page.apply(tree, whole, 2.0), Applied::Done);
        let updates = page.take_updates();
        assert_eq!(updates.len(), 1);
        assert_eq!(
            updates[0].tree.as_ref().map(|info| info.root),
            Some(NodeId(1))
        );
        let (_, button) = updates[0]
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(2))
            .unwrap();
        assert_eq!(button.role(), Role::Button);
        assert!(button.supports_action(Action::Click));
        assert_eq!(button.bounds().map(|r| (r.x0, r.y1)), Some((20.0, 120.0)));

        let stale = AccessibilityEvent::Change {
            base: 5,
            seq: 6,
            nodes: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(page.apply(tree, stale, 2.0), Applied::NeedsWholeTree);

        let mut renamed = node(2, "button", &[]);
        renamed.focused = true;
        let change = AccessibilityEvent::Change {
            base: 1,
            seq: 2,
            nodes: vec![renamed],
            removed: Vec::new(),
        };
        assert_eq!(page.apply(tree, change, 2.0), Applied::Done);
        let updates = page.take_updates();
        assert!(updates[0].tree.is_none() && updates[0].nodes.len() == 1);
        assert_eq!(updates[0].focus, NodeId(2));

        page.rescale(tree, 1.0);
        assert!(
            page.take_updates()[0].tree.is_some(),
            "a zoom change resends the whole tree"
        );
    }

    #[test]
    fn editable_and_text_nodes_get_input_and_label_roles() {
        let mut field = node(1, "textbox", &[]);
        field.editable = true;
        assert_eq!(role(&field), Role::TextInput);
        field.password = true;
        assert_eq!(role(&field), Role::PasswordInput);
        assert_eq!(role(&node(2, "text leaf", &[])), Role::Label);
        assert_eq!(role(&node(3, "made-up", &[])), Role::GenericContainer);
    }
}
