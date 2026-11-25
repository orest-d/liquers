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
}

/// The label a node reads as, for step chips and jump hints.
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

/// Whether the editor offers "Add key…": a query editor whose query has no resource (key) yet,
/// such as an empty query. The command it sends is a resource with an empty key inserted first.
pub fn offers_key(tree: &EditorTree) -> bool {
    tree.has_filename && !tree.nodes.iter().any(|n| matches!(n, EditorNode::Resource { .. }))
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
                return EditorLayout { before: vec![], groups: vec![], after: vec![] };
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
            EditorLayout { before: vec![], groups, after: vec![] }
        }
    }
}

/// A button of an expanded card's toolbar (AC-5), in drawing order: Delete stays far right,
/// away from Reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolItem {
    Reset,
    Presets,
    Next,
    Filename,
    MoveUp,
    MoveDown,
    Delete,
}

/// The toolbar of an expanded card of `node` in `mode`: Presets and Next only when the command
/// has some, Move only in Advanced.
pub fn toolbar_items(node: &EditorNode, mode: EditorMode) -> Vec<ToolItem> {
    let mut items = Vec::new();
    if let EditorNode::Action { presets, next, filename, fields, .. } = node {
        if !fields.is_empty() {
            items.push(ToolItem::Reset);
        }
        if !presets.is_empty() {
            items.push(ToolItem::Presets);
        }
        if !next.is_empty() {
            items.push(ToolItem::Next);
        }
        if filename.is_some() {
            items.push(ToolItem::Filename);
        }
    }
    let movable = node.path().last_element().is_some_and(|at| at > 0);
    if mode == EditorMode::Advanced && movable {
        items.push(ToolItem::MoveUp);
        items.push(ToolItem::MoveDown);
    }
    if !node.path().0.is_empty() {
        items.push(ToolItem::Delete);
    }
    items
}

/// One part of a card header (AC-4), in drawing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderPart {
    Chevron,
    Icon,
    /// The command's realm, when the registry has several.
    Realm(String),
    /// The label: the human step ("Greater than").
    Label(String),
    /// The dimmed `namespace/name`.
    CommandId(String),
    Modified,
    Error,
    /// The card's 1-based position in the editor.
    Position(usize),
    Menu,
}

/// The `namespace/name` of a command.
pub fn command_id(command: &CommandKey) -> String {
    if command.namespace.is_empty() {
        command.name.clone()
    } else {
        format!("{}/{}", command.namespace, command.name)
    }
}

/// The header of the card of `node`: collapsed it reads icon, label, then the dimmed
/// `namespace/name`; expanded the `[realm] namespace/name` line comes above the label.
pub fn header_parts(tree: &EditorTree, node: &EditorNode, collapsed: bool, position: usize) -> Vec<HeaderPart> {
    let marks = card_marks(tree, node);
    let mut parts = vec![HeaderPart::Chevron, HeaderPart::Icon];
    let label = HeaderPart::Label(node_label(node));
    match node {
        EditorNode::Action { command, .. } => {
            let id = HeaderPart::CommandId(command_id(command));
            let realm = (tree.show_realm && !command.realm.is_empty()).then(|| HeaderPart::Realm(command.realm.clone()));
            if collapsed {
                parts.push(label);
                parts.push(id);
            } else {
                parts.extend(realm);
                parts.push(id);
                parts.push(label);
            }
        }
        EditorNode::Resource { .. }
        | EditorNode::Namespaces { .. }
        | EditorNode::Directive { .. }
        | EditorNode::Value { .. }
        | EditorNode::Nested { .. }
        | EditorNode::Opaque { .. } => parts.push(label),
    }
    if marks.modified {
        parts.push(HeaderPart::Modified);
    }
    if marks.error {
        parts.push(HeaderPart::Error);
    }
    parts.push(HeaderPart::Position(position));
    parts.push(HeaderPart::Menu);
    parts
}

/// One row of a card's form grid (AC-6): the label column, the editor, the help line and a
/// refused edit's message under the field.
#[derive(Debug, Clone, PartialEq)]
pub struct FormRow<'t> {
    pub field: &'t super::tree::EditorField,
    pub label: String,
    /// No default: the query must give it.
    pub required: bool,
    /// The argument description, under the editor.
    pub help: String,
    /// The last refused edit's message, when it was this field's.
    pub message: Option<String>,
}

/// The form rows of `node`. `last_field` is the field id ([`super::view::field_id`]) last
/// committed: a message concerning the node shows under that field, else under the first.
pub fn form_rows<'t>(tree: &EditorTree, node: &'t EditorNode, last_field: Option<&str>) -> Vec<FormRow<'t>> {
    let fields = node.fields();
    let message = tree.message.as_ref().filter(|(p, _)| p == node.path()).map(|(_, m)| m.clone());
    let owner = message.as_ref().map(|_| {
        fields
            .iter()
            .position(|f| {
                let base = super::view::field_id(node.path(), &f.name, None);
                last_field.is_some_and(|id| id == base || id.strip_prefix(base.as_str()).is_some_and(|r| r.starts_with(':')))
            })
            .unwrap_or(0)
    });
    fields
        .iter()
        .enumerate()
        .map(|(i, field)| FormRow {
            field,
            label: if field.label.is_empty() { field.name.clone() } else { field.label.clone() },
            required: field.default_text.is_none() && !field.multiple,
            help: field.description.clone(),
            message: if owner == Some(i) { message.clone() } else { None },
        })
        .collect()
}
