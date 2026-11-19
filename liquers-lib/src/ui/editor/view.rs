//! UI-local editor state shared by the renderers, and the markup names the browser renderer and
//! [`EditorView::from_ui_event`] agree on.

use std::collections::HashMap;

use liquers_core::command_metadata::CommandKey;
use liquers_core::value::ValueInterface;

use super::command::{EditorCommand, NewElement};
use super::tree::{EditorNode, EditorTree, FieldValue, NodePath};
use crate::ui::event::{UIEvent, UIEventKind};

/// What the user is typing, between frames: text buffers keyed by field id, so a field keeps
/// what is typed until it is committed (Enter or focus loss). Not serialized.
#[derive(Debug, Clone, Default)]
pub struct EditorView {
    /// The query text being typed, and the tree text it started from.
    pub text: Option<(String, String)>,
    /// Field buffers by id ([`field_id`]), with the value text they started from.
    pub buffers: HashMap<String, (String, String)>,
    /// The namespace to append, per `ns` node path id.
    pub new_namespace: HashMap<String, String>,
}

impl EditorView {
    /// The buffer of field `id`, reset to `current` when the tree's value changed under it.
    pub fn buffer(&mut self, id: &str, current: &str) -> &mut String {
        let entry = self
            .buffers
            .entry(id.to_string())
            .or_insert_with(|| (current.to_string(), current.to_string()));
        if entry.1 != current {
            *entry = (current.to_string(), current.to_string());
        }
        &mut entry.0
    }

    /// The query text buffer, reset when the tree's text changed under it.
    pub fn text_buffer(&mut self, current: &str) -> &mut String {
        let reset = !matches!(&self.text, Some((_, from)) if from == current);
        if reset {
            self.text = Some((current.to_string(), current.to_string()));
        }
        &mut self
            .text
            .get_or_insert_with(|| (current.to_string(), current.to_string()))
            .0
    }

    /// Decodes a browser event of an editor's markup ([`super::super::web`] `editor_html`) into
    /// commands: a named event is one command, a submit commits every field whose value differs
    /// from the tree, a change commits the one field it names.
    pub fn from_ui_event(event: &UIEvent, tree: &EditorTree) -> Vec<EditorCommand> {
        let field_text = |name: &str| event.field(name).and_then(|v| v.try_into_string().ok());
        match &event.kind {
            UIEventKind::Named(name) => decode_named(name, &field_text).into_iter().collect(),
            UIEventKind::Change(field) => match field_text(field) {
                Some(value) => decode_field(field, &value, tree).into_iter().collect(),
                None => vec![],
            },
            UIEventKind::Submit => {
                let mut out = Vec::new();
                for (name, value) in event.fields.iter() {
                    if let Ok(text) = value.try_into_string() {
                        if let Some(command) = decode_field(name, &text, tree) {
                            out.push(command);
                        }
                    }
                }
                out
            }
        }
    }
}

/// The id of an argument field: `f:<path>:<arg>`, or `f:<path>:<arg>:<item>` for a variadic item.
pub fn field_id(path: &NodePath, arg: &str, item: Option<usize>) -> String {
    match item {
        Some(k) => format!("f:{}:{arg}:{k}", path.id()),
        None => format!("f:{}:{arg}", path.id()),
    }
}

/// The name of a named event: `ed:<verb>:<path>:<rest>`.
pub fn event_name(verb: &str, path: &NodePath, rest: &str) -> String {
    format!("ed:{verb}:{}:{rest}", path.id())
}

fn decode_named(name: &str, field_text: &dyn Fn(&str) -> Option<String>) -> Option<EditorCommand> {
    let rest = name.strip_prefix("ed:")?;
    let mut parts = rest.splitn(3, ':');
    let verb = parts.next()?;
    let path = NodePath::from_id(parts.next().unwrap_or("")).ok()?;
    let extra = parts.next().unwrap_or("");
    let two = || {
        let mut it = extra.splitn(2, ':');
        (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string())
    };
    let index = |s: &str| s.parse::<usize>().ok();
    Some(match verb {
        "reset" => EditorCommand::Reset { path, arg: extra.to_string() },
        "tolink" => EditorCommand::ToLink { path, arg: extra.to_string() },
        "tovalue" => EditorCommand::ToValue { path, arg: extra.to_string() },
        "additem" => {
            let (arg, at) = two();
            EditorCommand::InsertItem { path, arg, at: index(&at)?, text: String::new() }
        }
        "delitem" => {
            let (arg, at) = two();
            EditorCommand::RemoveItem { path, arg, at: index(&at)? }
        }
        "itemup" | "itemdown" => {
            let (arg, at) = two();
            let at = index(&at)?;
            let to = if verb == "itemup" { at.checked_sub(1)? } else { at + 1 };
            EditorCommand::MoveItem { path, arg, from: at, to }
        }
        "remove" => EditorCommand::Remove { path },
        "up" => {
            let at = path.last_element()?;
            EditorCommand::Move { path, to: at.checked_sub(1)? }
        }
        "down" => {
            let at = path.last_element()?;
            EditorCommand::Move { path, to: at + 1 }
        }
        "preset" => EditorCommand::ApplyPreset { path, index: index(extra)? },
        "next" => EditorCommand::ApplyNext { path, index: index(extra)? },
        "argpreset" => {
            let (arg, i) = two();
            EditorCommand::ApplyArgumentPreset { path, arg, index: index(&i)? }
        }
        "filenamefrom" => EditorCommand::FilenameFrom { path },
        "defaultfilename" => EditorCommand::DefaultFilename,
        "browse" => EditorCommand::Browse { path, folder: extra.to_string() },
        "closebrowser" => EditorCommand::CloseBrowser,
        "pick" => EditorCommand::SetKey { path, key: extra.to_string() },
        "picker" => EditorCommand::SearchCommands {
            insert_at: if path.0.is_empty() { None } else { Some(path) },
            filter: field_text("picker").unwrap_or_default(),
        },
        "closepicker" => EditorCommand::ClosePicker,
        "command" => {
            let mut it = extra.splitn(3, '/');
            let realm = it.next()?.to_string();
            let namespace = it.next()?.to_string();
            let name = it.next()?.to_string();
            EditorCommand::PickCommand(CommandKey::new(&realm, &namespace, &name))
        }
        "nsdel" | "nsup" | "nsdown" | "nsadd" => {
            let list: Vec<String> = field_text(&format!("ns:{}", path.id()))
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            let mut list = list;
            match verb {
                "nsadd" => list.push(field_text(&format!("nsnew:{}", path.id()))?),
                _ => {
                    let at = index(extra)?;
                    match verb {
                        "nsdel" if at < list.len() => {
                            list.remove(at);
                        }
                        "nsup" if at > 0 && at < list.len() => list.swap(at, at - 1),
                        "nsdown" if at + 1 < list.len() => list.swap(at, at + 1),
                        _ => return None,
                    }
                }
            }
            EditorCommand::SetNamespaces { path, namespaces: list }
        }
        "confirm" => EditorCommand::ConfirmNamespaces,
        "cancel" => EditorCommand::CancelNamespaces,
        "append" => match extra {
            "q" => EditorCommand::Append(NewElement::Quote),
            "v" => EditorCommand::Append(NewElement::Volatile),
            "resource" => EditorCommand::Append(NewElement::Resource("data".to_string())),
            _ => return None,
        },
        "update" => EditorCommand::Update,
        "revert" => EditorCommand::Revert,
        _ => return None,
    })
}

/// The command for a field whose value is now `value`, or `None` when it did not change.
fn decode_field(name: &str, value: &str, tree: &EditorTree) -> Option<EditorCommand> {
    if name == "text" {
        return (value != tree.text).then(|| EditorCommand::SetText(value.to_string()));
    }
    if name == "filename" {
        return (value != tree.filename).then(|| EditorCommand::SetFilename(value.to_string()));
    }
    if name == "continuous" {
        let on = value == "true";
        return (on != tree.continuous).then_some(EditorCommand::SetContinuous(on));
    }
    if let Some(rest) = name.strip_prefix("key:") {
        let path = NodePath::from_id(rest).ok()?;
        let current = find_node(&tree.nodes, &path).and_then(|n| match n {
            EditorNode::Resource { key, .. } => Some(key.clone()),
            EditorNode::Namespaces { .. }
            | EditorNode::Action { .. }
            | EditorNode::Directive { .. }
            | EditorNode::Value { .. }
            | EditorNode::Nested { .. }
            | EditorNode::Opaque { .. } => None,
        });
        return (current.as_deref() != Some(value))
            .then(|| EditorCommand::SetKey { path, key: value.to_string() });
    }
    if let Some(rest) = name.strip_prefix("header:") {
        let path = NodePath::from_id(rest).ok()?;
        let current = find_node(&tree.nodes, &path).and_then(|n| match n {
            EditorNode::Resource { header, .. } => header.clone(),
            EditorNode::Namespaces { .. }
            | EditorNode::Action { .. }
            | EditorNode::Directive { .. }
            | EditorNode::Value { .. }
            | EditorNode::Nested { .. }
            | EditorNode::Opaque { .. } => None,
        });
        return (current.as_deref() != Some(value))
            .then(|| EditorCommand::SetHeader { path, header: value.to_string() });
    }
    let rest = name.strip_prefix("f:")?;
    let parts: Vec<&str> = rest.split(':').collect();
    let (path, arg, item) = match parts.as_slice() {
        [path, arg] => (NodePath::from_id(path).ok()?, arg.to_string(), None),
        [path, arg, item] => (NodePath::from_id(path).ok()?, arg.to_string(), item.parse::<usize>().ok()),
        _ => return None,
    };
    let node = find_node(&tree.nodes, &path)?;
    let field = node.fields().iter().find(|f| f.name == arg)?;
    match (item, &field.value) {
        (Some(k), FieldValue::Items(items)) => match items.get(k) {
            Some(FieldValue::Text(t)) if t == value => None,
            Some(_) | None => Some(EditorCommand::SetItem { path, arg, at: k, text: value.to_string() }),
        },
        (Some(_), _) => None,
        (None, FieldValue::Text(t)) if t == value => None,
        (None, FieldValue::Unset) if value.is_empty() => None,
        (None, FieldValue::Link { query, .. }) => {
            (query != value).then(|| EditorCommand::SetLink { path, arg, query: value.to_string() })
        }
        (None, _) => Some(EditorCommand::SetField { path, arg, text: value.to_string() }),
    }
}

/// The node at `path`, searching link subtrees too.
pub fn find_node<'a>(nodes: &'a [EditorNode], path: &NodePath) -> Option<&'a EditorNode> {
    for node in nodes {
        if node.path() == path && !matches!(node, EditorNode::Namespaces { .. }) {
            return Some(node);
        }
        for field in node.fields() {
            if let Some(found) = find_in_value(&field.value, path) {
                return Some(found);
            }
        }
        if let EditorNode::Nested { nodes, .. } = node {
            if let Some(found) = find_node(nodes, path) {
                return Some(found);
            }
        }
    }
    None
}

fn find_in_value<'a>(value: &'a FieldValue, path: &NodePath) -> Option<&'a EditorNode> {
    match value {
        FieldValue::Link { nodes, .. } => find_node(nodes, path),
        FieldValue::Items(items) => items.iter().find_map(|v| find_in_value(v, path)),
        FieldValue::Unset | FieldValue::Text(_) | FieldValue::Other(_) => None,
    }
}
