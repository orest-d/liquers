//! What the renderers draw (AC-1, AC-2, AC-4): the tree projected for a mode. Simple and
//! Advanced are two layouts of one tree, so switching keeps every edit (AC-3).

use liquers_core::command_metadata::CommandKey;
use serde::{Deserialize, Serialize};

use super::tree::{EditorNode, EditorTree, FieldValue, NodePath};

/// The editor's mode (AC-1, AC-2). Every editor starts in Simple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode {
    #[default]
    Simple,
    Advanced,
}

impl EditorMode {
    /// The other mode.
    pub fn toggled(self) -> EditorMode {
        match self {
            EditorMode::Simple => EditorMode::Advanced,
            EditorMode::Advanced => EditorMode::Simple,
        }
    }
}

/// A read-only step of Simple mode, named by its label.
#[derive(Debug, Clone, PartialEq)]
pub struct StepChip {
    pub path: NodePath,
    pub label: String,
    pub command: Option<CommandKey>,
}

/// Which part of the query a group of cards is (AC-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardGroupKind {
    KeyPart,
    QueryPart,
}

/// The state marks of a card header (AC-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CardMarks {
    /// An argument differs from its default.
    pub modified: bool,
    /// An argument is a link.
    pub link: bool,
    /// The last refusal or message concerns this card, or it cannot be edited.
    pub error: bool,
}

/// One card.
#[derive(Debug, Clone)]
pub struct Card<'t> {
    pub node: &'t EditorNode,
    pub marks: CardMarks,
}

/// Cards of one part of the query.
#[derive(Debug, Clone)]
pub struct CardGroup<'t> {
    pub kind: CardGroupKind,
    pub cards: Vec<Card<'t>>,
}

/// The layout of an editor in one mode.
#[derive(Debug, Clone)]
pub struct EditorLayout<'t> {
    /// Read-only steps before the card (Simple mode).
    pub before: Vec<StepChip>,
    pub groups: Vec<CardGroup<'t>>,
    /// Read-only steps after the card (Simple mode): `q`/`v` directives, values.
    pub after: Vec<StepChip>,
    /// Draw the outline (Advanced mode).
    pub outline: bool,
}

/// The label a node reads as, for chips and the outline.
pub fn node_label(node: &EditorNode) -> String {
    match node {
        EditorNode::Resource { key, header, .. } => {
            let name = key.rsplit('/').find(|s| !s.is_empty()).unwrap_or(key.as_str());
            match header.as_deref() {
                Some(h) if !h.is_empty() && h != "key" => format!("{name} ({h})"),
                Some(_) | None => name.to_string(),
            }
        }
        EditorNode::Namespaces { list, .. } => format!("ns {}", list.join(", ")),
        EditorNode::Action { label, .. } => label.clone(),
        EditorNode::Directive { name, .. } => name.clone(),
        EditorNode::Value { json, .. } => json.clone(),
        EditorNode::Nested { label, .. } => label.clone(),
        EditorNode::Opaque { text, .. } => text.clone(),
    }
}

/// The state marks of `node` in `tree`.
pub fn card_marks(tree: &EditorTree, node: &EditorNode) -> CardMarks {
    let mut marks = CardMarks::default();
    for field in node.fields() {
        match &field.value {
            FieldValue::Text(t) => {
                if field.default_text.as_deref() != Some(t.as_str()) {
                    marks.modified = true;
                }
            }
            FieldValue::Link { .. } => {
                marks.link = true;
                marks.modified = true;
            }
            FieldValue::Items(items) => {
                if !items.is_empty() {
                    marks.modified = true;
                }
                if items.iter().any(|i| matches!(i, FieldValue::Link { .. })) {
                    marks.link = true;
                }
            }
            FieldValue::Unset | FieldValue::Other(_) => {}
        }
    }
    let concerns_node = tree.message.as_ref().is_some_and(|(p, _)| p == node.path());
    marks.error = concerns_node || matches!(node, EditorNode::Opaque { .. });
    marks
}

fn chip(node: &EditorNode) -> StepChip {
    StepChip {
        path: node.path().clone(),
        label: node_label(node),
        command: match node {
            EditorNode::Action { command, .. } => Some(command.clone()),
            EditorNode::Resource { .. }
            | EditorNode::Namespaces { .. }
            | EditorNode::Directive { .. }
            | EditorNode::Value { .. }
            | EditorNode::Nested { .. }
            | EditorNode::Opaque { .. } => None,
        },
    }
}

fn card<'t>(tree: &EditorTree, node: &'t EditorNode) -> Card<'t> {
    Card { node, marks: card_marks(tree, node) }
}

/// The top-level node Simple mode shows as its card: the last action, else the first resource,
/// else the last node.
pub fn simple_card(tree: &EditorTree) -> Option<usize> {
    tree.last_action()
        .or_else(|| tree.nodes.iter().position(|n| matches!(n, EditorNode::Resource { .. })))
        .or(if tree.nodes.is_empty() { None } else { Some(tree.nodes.len() - 1) })
}

/// The layout of `tree` in `mode`.
pub fn editor_layout(tree: &EditorTree, mode: EditorMode) -> EditorLayout<'_> {
    let visible: Vec<&EditorNode> = tree.nodes.iter().collect();
    match mode {
        EditorMode::Simple => {
            let Some(at) = simple_card(tree) else {
                return EditorLayout { before: vec![], groups: vec![], after: vec![], outline: false };
            };
            let is_chip = |n: &&EditorNode| !matches!(*n, EditorNode::Namespaces { .. });
            let before = visible[..at].iter().copied().filter(is_chip).map(chip).collect();
            let after = visible[at + 1..].iter().copied().filter(is_chip).map(chip).collect();
            let kind = match visible[at] {
                EditorNode::Resource { .. } => CardGroupKind::KeyPart,
                EditorNode::Namespaces { .. }
                | EditorNode::Action { .. }
                | EditorNode::Directive { .. }
                | EditorNode::Value { .. }
                | EditorNode::Nested { .. }
                | EditorNode::Opaque { .. } => CardGroupKind::QueryPart,
            };
            EditorLayout {
                before,
                groups: vec![CardGroup { kind, cards: vec![card(tree, visible[at])] }],
                after,
                outline: false,
            }
        }
        EditorMode::Advanced => {
            let split = visible.iter().position(|n| !matches!(n, EditorNode::Resource { .. })).unwrap_or(visible.len());
            let mut groups = Vec::new();
            if split > 0 {
                groups.push(CardGroup {
                    kind: CardGroupKind::KeyPart,
                    cards: visible[..split].iter().map(|n| card(tree, n)).collect(),
                });
            }
            if split < visible.len() {
                groups.push(CardGroup {
                    kind: CardGroupKind::QueryPart,
                    cards: visible[split..].iter().map(|n| card(tree, n)).collect(),
                });
            }
            EditorLayout { before: vec![], groups, after: vec![], outline: visible.len() > 1 }
        }
    }
}
