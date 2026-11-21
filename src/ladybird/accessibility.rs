// SPDX-License-Identifier: GPL-3.0-only

//! A page's accessibility tree as Ladybird reports it: whole trees, numbered changes and focus.

use serde::Deserialize;

/// One node of a page's accessibility tree.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AccessibilityNode {
    pub id: i64,
    pub children: Vec<i64>,
    /// An ARIA role name, `"text leaf"` for text, or empty.
    pub role: String,
    pub name: String,
    pub description: String,
    pub value: String,
    pub url: String,
    /// The node's box in CSS pixels from the viewport's top-left: x, y, width, height.
    pub bounds: [i32; 4],
    pub focused: bool,
    pub disabled: bool,
    pub heading_level: i32,
    pub level: i32,
    pub live: String,
    pub selected: bool,
    /// 0 not applicable, 1 unchecked, 2 checked, 3 mixed.
    pub checked: u8,
    /// 0 not applicable, 1 collapsed, 2 expanded.
    pub expanded: u8,
    pub pressed: bool,
    pub editable: bool,
    pub multiline: bool,
    pub password: bool,
    pub read_only: bool,
    pub required: bool,
    pub invalid: bool,
    pub value_numeric: Option<f64>,
    pub value_minimum: Option<f64>,
    pub value_maximum: Option<f64>,
    pub value_step: Option<f64>,
    /// Unicode character offsets, -1 when there is no selection or caret in the node.
    pub selection_start: i32,
    pub selection_end: i32,
    pub caret: i32,
}

/// What the page reported about its accessibility tree.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessibilityEvent {
    /// The whole tree, replacing any held.
    Tree {
        seq: u64,
        nodes: Vec<AccessibilityNode>,
    },
    /// A change to the tree numbered `base`, making it `seq`.
    Change {
        base: u64,
        seq: u64,
        nodes: Vec<AccessibilityNode>,
        removed: Vec<i64>,
    },
    Focus(i64),
}

#[cfg(any(feature = "ladybird-bridge", test))]
#[derive(Deserialize)]
struct Payload {
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    base: u64,
    #[serde(default)]
    nodes: Vec<AccessibilityNode>,
    #[serde(default)]
    removed: Vec<i64>,
    #[serde(default)]
    focus: i64,
}

#[cfg(any(feature = "ladybird-bridge", test))]
impl AccessibilityEvent {
    /// Reads the bridge's JSON for event value `kind`: 1 a tree, 2 a change, 3 a focus change.
    pub(crate) fn parse(kind: i32, json: &str) -> Option<Self> {
        let payload: Payload = serde_json::from_str(json).ok()?;
        Some(match kind {
            1 => Self::Tree {
                seq: payload.seq,
                nodes: payload.nodes,
            },
            2 => Self::Change {
                base: payload.base,
                seq: payload.seq,
                nodes: payload.nodes,
                removed: payload.removed,
            },
            3 => Self::Focus(payload.focus),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_payloads_parse_into_trees_changes_and_focus() {
        let tree = AccessibilityEvent::parse(
            1,
            r#"{"seq":3,"nodes":[{"id":1,"children":[2],"role":"document","bounds":[0,0,800,600]}]}"#,
        );
        let Some(AccessibilityEvent::Tree { seq: 3, nodes }) = tree else {
            panic!("expected a tree, got {tree:?}");
        };
        assert_eq!(
            (nodes[0].children.as_slice(), nodes[0].bounds),
            (&[2][..], [0, 0, 800, 600])
        );
        assert_eq!(
            AccessibilityEvent::parse(2, r#"{"base":3,"seq":4,"nodes":[],"removed":[2]}"#),
            Some(AccessibilityEvent::Change {
                base: 3,
                seq: 4,
                nodes: Vec::new(),
                removed: vec![2]
            })
        );
        assert_eq!(
            AccessibilityEvent::parse(3, r#"{"focus":7}"#),
            Some(AccessibilityEvent::Focus(7))
        );
        assert_eq!(AccessibilityEvent::parse(9, "{}"), None);
    }
}
