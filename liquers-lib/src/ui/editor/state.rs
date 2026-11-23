//! The backend-free core of the editors: a working copy of a query, pipeline or key in a
//! [`QueryEditor`] / [`PipelineEditor`], every [`EditorCommand`] applied to it as a validated
//! edit, and the [`EditorTree`] both renderers draw.

use std::sync::Arc;

use liquers_core::accessor::Fingerprint;
use liquers_core::command_metadata::{
    ArgumentGUIInfo, ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry,
};
use liquers_core::error::Error;
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::pipeline::{
    ActionEditor, ActionElement, ElementKind, Located, NamespaceImpact, Namespaces, Operand,
    Pipeline, PipelineEditor, QueryEditor, QueryOrPipeline, Read, Source,
};
use liquers_core::query::Query;
use liquers_core::value::ValueInterface;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::catalog::command_catalog;
use super::command::{EditorCommand, EditorEffect, NewElement};
use super::tree::{
    default_text, effective_gui, enum_alternatives, is_numeric, json_text, scale_numeric,
    Browser, Confirmation, DirectoryListing, EditorField, EditorNode, EditorTree, FieldValue,
    NodePath, PathStep, Picker, PresetEntry,
};
use crate::value::Value;

/// What an editor element edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EditorKind {
    /// A query or pipeline, as a tree beside its text.
    #[default]
    Query,
    /// One action: the one-element pipeline an action accessor gives (AC-8).
    Action,
    /// The first resource of a query or pipeline (AC-7).
    Resource,
    /// A key, as text (AC-15).
    Key,
}

/// The working copy.
#[derive(Debug, Clone)]
enum Edited {
    Query(QueryEditor),
    Pipeline(PipelineEditor),
    Key(String),
}

impl Edited {
    fn editor(&self) -> Result<&PipelineEditor, Error> {
        match self {
            Edited::Query(q) => Ok(q.editor()),
            Edited::Pipeline(p) => Ok(p),
            Edited::Key(_) => Err(Error::general_error("a key has no elements".to_string())),
        }
    }
    fn edit_action(&mut self, at: usize) -> Result<ActionEditor<'_>, Error> {
        match self {
            Edited::Query(q) => q.edit_action(at),
            Edited::Pipeline(p) => p.edit_action(at),
            Edited::Key(_) => Err(Error::general_error("a key has no actions".to_string())),
        }
    }
    fn insert(&mut self, at: usize, element: ElementKind) -> Result<(), Error> {
        let element = Located::new(element);
        match self {
            Edited::Query(q) => q.insert(at, element).map(|_| ()),
            Edited::Pipeline(p) => p.insert(at, element).map(|_| ()),
            Edited::Key(_) => Err(Error::general_error("a key has no elements".to_string())),
        }
    }
    fn replace(&mut self, at: usize, element: ElementKind) -> Result<(), Error> {
        let element = Located::new(element);
        match self {
            Edited::Query(q) => q.replace(at, element).map(|_| ()),
            Edited::Pipeline(p) => p.replace(at, element).map(|_| ()),
            Edited::Key(_) => Err(Error::general_error("a key has no elements".to_string())),
        }
    }
    fn remove(&mut self, at: usize) -> Result<(), Error> {
        match self {
            Edited::Query(q) => q.remove(at).map(|_| ()),
            Edited::Pipeline(p) => p.remove(at).map(|_| ()),
            Edited::Key(_) => Err(Error::general_error("a key has no elements".to_string())),
        }
    }
    fn move_to(&mut self, from: usize, to: usize) -> Result<(), Error> {
        match self {
            Edited::Query(q) => q.move_to(from, to).map(|_| ()),
            Edited::Pipeline(p) => p.move_to(from, to).map(|_| ()),
            Edited::Key(_) => Err(Error::general_error("a key has no elements".to_string())),
        }
    }
    fn set_filename(&mut self, name: Option<&str>) -> Result<(), Error> {
        match self {
            Edited::Query(q) => q.set_filename(name).map(|_| ()),
            Edited::Pipeline(p) => p.set_filename(name).map(|_| ()),
            Edited::Key(_) => Err(Error::general_error("a key has no filename".to_string())),
        }
    }
    fn set_namespaces(&mut self, at: usize, ns: Vec<String>, accept: bool) -> Result<NamespaceImpact, Error> {
        match self {
            Edited::Query(q) => q.set_namespaces(at, ns, accept),
            Edited::Pipeline(p) => p.set_namespaces(at, ns, accept),
            Edited::Key(_) => Err(Error::general_error("a key has no namespaces".to_string())),
        }
    }
    fn len(&self) -> usize {
        self.editor().map(|e| e.pipeline().elements.len()).unwrap_or(0)
    }
    /// The text of this level: a query's encoding, a key, or a representable pipeline's query.
    fn text(&self) -> Result<String, Error> {
        match self {
            Edited::Query(q) => Ok(q.query().encode()),
            Edited::Pipeline(p) => Query::try_from(p).map(|q| q.encode()),
            Edited::Key(k) => Ok(k.clone()),
        }
    }
    fn value(&self) -> Value {
        match self {
            Edited::Query(q) => Value::from_query(&q.query()),
            Edited::Pipeline(p) => Value::from_pipeline(p.pipeline().clone()),
            Edited::Key(k) => Value::from(k.clone()),
        }
    }
}

/// Opens the level of a link argument.
fn open_link(text: &str, registry: &Arc<CommandMetadataRegistry>) -> Result<Edited, Error> {
    Ok(Edited::Query(QueryEditor::from_query(&parse_query(text)?, registry.clone())?))
}

fn no_node(path: &[PathStep]) -> Error {
    Error::invalid_pipeline(format!("no node at {}", NodePath(path.to_vec()).id()))
}

/// Applies `f` to the level `path` ends in, then writes each nested level back into its link
/// argument. `f` receives the level and the last element index.
fn with_level<R>(
    level: &mut Edited,
    path: &[PathStep],
    registry: &Arc<CommandMetadataRegistry>,
    f: impl FnOnce(&mut Edited, usize) -> Result<R, Error>,
) -> Result<R, Error> {
    match path {
        [PathStep::Element(i)] => f(level, *i),
        [PathStep::Element(i), PathStep::Argument(arg), rest @ ..] => {
            let (item, rest) = match rest {
                [PathStep::Item(k), rest @ ..] => (Some(*k), rest),
                rest => (None, rest),
            };
            if rest.is_empty() {
                return Err(no_node(path));
            }
            let source = level
                .editor()?
                .argument_source(*i, arg.as_str())?
                .ok_or_else(|| no_node(path))?;
            let text = match (item, source) {
                (None, Source::Link(text)) => text,
                (Some(k), Source::Multiple(items)) => match items.get(k) {
                    Some(Source::Link(text)) => text.clone(),
                    Some(_) | None => return Err(no_node(path)),
                },
                (Some(_), _) | (None, _) => return Err(no_node(path)),
            };
            let mut nested = open_link(&text, registry)?;
            let result = with_level(&mut nested, rest, registry, f)?;
            let new_text = nested.text()?;
            let mut action = level.edit_action(*i)?;
            match item {
                None => {
                    action.set_source(arg.as_str(), Source::Link(new_text))?;
                }
                Some(k) => {
                    action.remove_item(arg.as_str(), k)?;
                    action.insert_item(arg.as_str(), k, Source::Link(new_text))?;
                }
            }
            Ok(result)
        }
        _ => Err(no_node(path)),
    }
}

/// A literal typed into a field, as the argument's type reads it.
fn literal(info: &ArgumentInfo, text: &str) -> Json {
    match &info.argument_type {
        ArgumentType::Integer | ArgumentType::IntegerOption => match text.trim().parse::<i64>() {
            Ok(n) => Json::from(n),
            Err(_) => Json::String(text.to_string()),
        },
        ArgumentType::Float | ArgumentType::FloatOption => match text.trim().parse::<f64>() {
            Ok(x) => serde_json::Number::from_f64(x)
                .map(Json::Number)
                .unwrap_or_else(|| Json::String(text.to_string())),
            Err(_) => Json::String(text.to_string()),
        },
        ArgumentType::Boolean | ArgumentType::BooleanOption => match text.trim() {
            "true" | "t" | "yes" | "y" | "1" => Json::Bool(true),
            "false" | "f" | "no" | "n" | "0" => Json::Bool(false),
            _ => Json::String(text.to_string()),
        },
        ArgumentType::String
        | ArgumentType::Enum(_)
        | ArgumentType::GlobalEnum(_)
        | ArgumentType::Any
        | ArgumentType::None
        | ArgumentType::Node(_) => Json::String(text.to_string()),
    }
}

/// The header word of a resource element, as the dropdown shows it.
fn header_of(kind: &ElementKind) -> Option<&'static str> {
    match kind {
        ElementKind::Resource { read, .. } => Some(match read {
            Read::Asset => "",
            Read::Directory => "dir",
            Read::StoredDirectory => "sdir",
            Read::Binary | Read::Metadata | Read::Recipe | Read::Stored | Read::StoredMetadata => {
                read.instruction().unwrap_or("")
            }
        }),
        ElementKind::KeyValue(_) => Some("key"),
        ElementKind::Cwd(_) => Some("cwd"),
        ElementKind::Text(_)
        | ElementKind::Action(_)
        | ElementKind::Value(_)
        | ElementKind::Evaluate(_)
        | ElementKind::QueryValue(_)
        | ElementKind::Quote
        | ElementKind::Volatile
        | ElementKind::Pipeline(_) => None,
    }
}

fn operand_text(op: &Operand<String>) -> String {
    match op {
        Operand::Literal(text) => text.clone(),
        Operand::Placeholder(name) => format!("${{{name}}}"),
    }
}

/// The element a header word and a key make (AC-7).
fn resource_element(header: &str, key: &str) -> Result<ElementKind, Error> {
    let key = Operand::Literal(key.to_string());
    match header {
        "" => Ok(ElementKind::Resource { resource: key, read: Read::Asset }),
        "key" => Ok(ElementKind::KeyValue(key)),
        "cwd" => Ok(ElementKind::Cwd(key)),
        other => match Read::from_instruction(other) {
            Some(read) => Ok(ElementKind::Resource { resource: key, read }),
            None => Err(Error::invalid_token(other, "a resource header")),
        },
    }
}

fn action_text(a: &ActionElement) -> String {
    let mut out = a.action.clone();
    for p in a.parameters.iter() {
        out.push('-');
        out.push_str(&match &p.item {
            Source::Value(v) => json_text(v),
            Source::Link(q) => format!("~X~{q}~E"),
            Source::Placeholder(_) | Source::Pipeline(_) | Source::State(_) | Source::Multiple(_) => {
                "…".to_string()
            }
        });
    }
    out
}

fn preset_entries(presets: &[liquers_core::command_metadata::CommandPreset], current: Option<&str>) -> Vec<PresetEntry> {
    presets
        .iter()
        .map(|p| PresetEntry {
            label: if p.label.is_empty() { p.action.encode() } else { p.label.clone() },
            description: p.description.clone(),
            active: current.is_some_and(|c| c == p.action.encode()),
        })
        .collect()
}

/// The backend-free core of an editor element. Not serialized: it is rebuilt from the binding.
#[derive(Debug, Clone)]
pub struct EditorState {
    kind: EditorKind,
    registry: Arc<CommandMetadataRegistry>,
    root: Option<Edited>,
    /// The fingerprint of the accessor value the copy was loaded from or last written as.
    fingerprint: Option<Fingerprint>,
    continuous: bool,
    pending: bool,
    write_in_flight: bool,
    /// The value changed elsewhere while edits were pending: the next write is a conflict.
    remote_changed: bool,
    read_only: bool,
    typed_text: Option<(String, String)>,
    picker: Option<(Option<NodePath>, String)>,
    browser: Option<Browser>,
    confirmation: Option<Confirmation>,
    message: Option<(NodePath, String)>,
    tree: EditorTree,
}

impl EditorState {
    /// An empty editor of `kind`; its value arrives with [`Self::on_snapshot`].
    pub fn new(kind: EditorKind, registry: Arc<CommandMetadataRegistry>) -> Self {
        let mut state = EditorState {
            kind,
            registry,
            root: None,
            fingerprint: None,
            continuous: true,
            pending: false,
            write_in_flight: false,
            remote_changed: false,
            read_only: false,
            typed_text: None,
            picker: None,
            browser: None,
            confirmation: None,
            message: None,
            tree: EditorTree::default(),
        };
        state.rebuild();
        state
    }

    /// Sets "Update immediately" (AC-26) without writing.
    pub fn with_continuous(mut self, continuous: bool) -> Self {
        self.continuous = continuous;
        self.rebuild();
        self
    }

    pub fn kind(&self) -> EditorKind {
        self.kind
    }
    pub fn continuous(&self) -> bool {
        self.continuous
    }
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    /// The tree to draw.
    pub fn tree(&self) -> &EditorTree {
        &self.tree
    }
    /// The working copy as a value (`None` before the first snapshot).
    pub fn value(&self) -> Option<Value> {
        self.root.as_ref().map(Edited::value)
    }

    /// Makes the editor read-only with a reason (a binding that failed, or cannot be written).
    pub fn set_read_only(&mut self, reason: Option<String>) {
        self.read_only = true;
        if let Some(reason) = reason {
            self.message = Some((NodePath::default(), reason));
        }
        self.rebuild();
    }

    fn load(&self, value: &Value) -> Result<Edited, Error> {
        match self.kind {
            EditorKind::Key => match value.try_into_key() {
                Ok(key) => Ok(Edited::Key(key.encode())),
                Err(_) => Ok(Edited::Key(value.try_into_string()?)),
            },
            EditorKind::Action => Ok(Edited::Pipeline(PipelineEditor::from_pipeline(
                value.try_into_pipeline()?,
                self.registry.clone(),
            )?)),
            EditorKind::Query | EditorKind::Resource => match value.try_into_query() {
                Ok(query) => Ok(Edited::Query(QueryEditor::from_query(&query, self.registry.clone())?)),
                Err(_) => Ok(Edited::Pipeline(PipelineEditor::from_pipeline(
                    value.try_into_pipeline()?,
                    self.registry.clone(),
                )?)),
            },
        }
    }

    /// The accessor's value, read on start, after a change elsewhere, or on Revert. While edits
    /// are pending, a change elsewhere is not loaded: the next write reports the conflict (AC-9).
    pub fn on_snapshot(&mut self, value: &Value, fingerprint: Option<Fingerprint>) -> Result<(), Error> {
        if self.root.is_some() && fingerprint.is_some() && fingerprint == self.fingerprint {
            return Ok(());
        }
        if self.pending || self.write_in_flight {
            self.remote_changed = true;
            self.message = Some((
                NodePath::default(),
                "The value was changed elsewhere; Update reports a conflict, Revert loads it."
                    .to_string(),
            ));
            self.rebuild();
            return Ok(());
        }
        let loaded = self.load(value);
        match loaded {
            Ok(root) => {
                self.root = Some(root);
                self.fingerprint = fingerprint;
                self.typed_text = None;
                self.remote_changed = false;
                self.rebuild();
                Ok(())
            }
            Err(e) => {
                self.root = None;
                self.read_only = true;
                self.message = Some((NodePath::default(), e.to_string()));
                self.rebuild();
                Err(e)
            }
        }
    }

    /// The result of a write: the new fingerprint. Edits made meanwhile are written next.
    pub fn on_written(&mut self, fingerprint: Fingerprint) -> EditorEffect {
        self.write_in_flight = false;
        self.fingerprint = Some(fingerprint);
        let effect = if self.pending && self.continuous { self.write() } else { EditorEffect::None };
        self.rebuild();
        effect
    }

    /// A write was refused because the value changed meanwhile (AC-9): the live value is loaded
    /// and the user is told; nothing is lost silently, since the refused copy is reported.
    pub fn on_conflict(&mut self, live: &Value, fingerprint: Option<Fingerprint>) {
        self.write_in_flight = false;
        self.pending = false;
        self.remote_changed = false;
        let lost = self.root.as_ref().and_then(|r| r.text().ok()).unwrap_or_default();
        let _ = self.on_snapshot(live, fingerprint);
        self.message = Some((
            NodePath::default(),
            format!("Conflict: the value was changed elsewhere and has been reloaded. Your version was: {lost}"),
        ));
        self.rebuild();
    }

    /// A write failed for another reason: the edits stay pending.
    pub fn on_write_failed(&mut self, error: &Error) {
        self.write_in_flight = false;
        self.pending = true;
        self.message = Some((NodePath::default(), error.to_string()));
        self.rebuild();
    }

    /// A directory listing for the open file selector (AC-14).
    pub fn on_directory(&mut self, listing: DirectoryListing) {
        if let Some(browser) = &mut self.browser {
            browser.listing = Some(listing);
        }
        self.rebuild();
    }

    fn write(&mut self) -> EditorEffect {
        match &self.root {
            Some(root) if !self.read_only && !self.write_in_flight => {
                self.write_in_flight = true;
                self.pending = false;
                EditorEffect::Write { value: root.value(), expected: self.fingerprint }
            }
            Some(_) | None => EditorEffect::None,
        }
    }

    /// An edit of the copy was made: write it now (continuous) or mark it pending.
    fn commit(&mut self) -> EditorEffect {
        self.pending = true;
        if self.continuous {
            self.write()
        } else {
            EditorEffect::None
        }
    }

    /// Applies one command. A refused edit returns the error, shown at its node, and changes
    /// nothing (AC-5).
    pub fn apply(&mut self, command: EditorCommand) -> Result<EditorEffect, Error> {
        let backup = self.root.clone();
        let path = command_path(&command);
        let result = self.apply_inner(command);
        match &result {
            Ok(_) => {
                if !matches!(self.message, Some((_, ref m)) if m.starts_with("Conflict") || m.contains("changed elsewhere")) {
                    self.message = None;
                }
            }
            Err(e) => {
                self.root = backup;
                self.message = Some((path, e.to_string()));
            }
        }
        self.rebuild();
        result
    }

    fn root_mut(&mut self) -> Result<&mut Edited, Error> {
        if self.read_only {
            return Err(Error::general_error("the editor is read-only".to_string()));
        }
        self.root
            .as_mut()
            .ok_or_else(|| Error::general_error("nothing to edit yet".to_string()))
    }

    fn at_level<R>(&mut self, path: &NodePath, f: impl FnOnce(&mut Edited, usize) -> Result<R, Error>) -> Result<R, Error> {
        let registry = self.registry.clone();
        let root = self.root_mut()?;
        with_level(root, &path.0, &registry, f)
    }

    fn edit_argument(
        &mut self,
        path: &NodePath,
        f: impl FnOnce(&mut ActionEditor<'_>, &ArgumentInfo) -> Result<(), Error>,
        arg: &str,
    ) -> Result<EditorEffect, Error> {
        let arg = arg.to_string();
        self.at_level(path, |level, at| {
            let info = level.editor()?.argument_info(at, arg.as_str())?.clone();
            let mut action = level.edit_action(at)?;
            f(&mut action, &info)
        })?;
        Ok(self.commit())
    }

    fn metadata_at(&self, path: &NodePath) -> Result<CommandMetadata, Error> {
        let root = self.root.as_ref().ok_or_else(|| no_node(&path.0))?;
        let mut level = root.clone();
        let registry = self.registry.clone();
        with_level(&mut level, &path.0, &registry, |l, at| l.editor()?.command_metadata(at).cloned())
    }

    fn apply_inner(&mut self, command: EditorCommand) -> Result<EditorEffect, Error> {
        match command {
            EditorCommand::SetText(text) => self.set_text(text),
            EditorCommand::SetField { path, arg, text } => {
                self.edit_argument(&path, |a, info| {
                    a.set_source(info.name.as_str(), Source::Value(literal(info, &text)))?;
                    Ok(())
                }, &arg)
            }
            EditorCommand::SetItem { path, arg, at, text } => self.edit_argument(&path, |a, info| {
                a.remove_item(info.name.as_str(), at)?;
                a.insert_item(info.name.as_str(), at, Source::Value(literal(info, &text)))?;
                Ok(())
            }, &arg),
            EditorCommand::SetLink { path, arg, query } => {
                parse_query(&query)?;
                self.edit_argument(&path, |a, info| {
                    a.set_source(info.name.as_str(), Source::Link(query))?;
                    Ok(())
                }, &arg)
            }
            EditorCommand::ToLink { path, arg } => self.edit_argument(&path, |a, info| {
                a.set_source(info.name.as_str(), Source::Link("-R/new".to_string()))?;
                Ok(())
            }, &arg),
            EditorCommand::ToValue { path, arg } => self.edit_argument(&path, |a, info| {
                a.reset(info.name.as_str())?;
                if let Some(default) = Source::from_parameter_value(&info.default) {
                    match default {
                        Source::Link(_) | Source::Pipeline(_) => {}
                        Source::Value(_) | Source::Placeholder(_) | Source::State(_) | Source::Multiple(_) => {
                            a.set_source(info.name.as_str(), default)?;
                        }
                    }
                }
                Ok(())
            }, &arg),
            EditorCommand::Reset { path, arg } => self.edit_argument(&path, |a, info| {
                a.reset(info.name.as_str())?;
                Ok(())
            }, &arg),
            EditorCommand::InsertItem { path, arg, at, text } => self.edit_argument(&path, |a, info| {
                a.insert_item(info.name.as_str(), at, Source::Value(literal(info, &text)))?;
                Ok(())
            }, &arg),
            EditorCommand::RemoveItem { path, arg, at } => self.edit_argument(&path, |a, info| {
                a.remove_item(info.name.as_str(), at)?;
                Ok(())
            }, &arg),
            EditorCommand::MoveItem { path, arg, from, to } => self.edit_argument(&path, |a, info| {
                a.move_item(info.name.as_str(), from, to)?;
                Ok(())
            }, &arg),
            EditorCommand::Insert { path, element } => {
                let element = self.new_element(element)?;
                self.at_level(&path, |level, at| level.insert(at, element))?;
                Ok(self.commit())
            }
            EditorCommand::Append(element) => {
                let element = self.new_element(element)?;
                let root = self.root_mut()?;
                let at = root.len();
                root.insert(at, element)?;
                Ok(self.commit())
            }
            EditorCommand::Remove { path } => {
                self.at_level(&path, |level, at| level.remove(at))?;
                Ok(self.commit())
            }
            EditorCommand::Move { path, to } => {
                self.at_level(&path, |level, at| level.move_to(at, to))?;
                Ok(self.commit())
            }
            EditorCommand::SetNamespaces { path, namespaces } => {
                let ns = namespaces.clone();
                let impact = self.at_level(&path, |level, at| level.set_namespaces(at, ns, false))?;
                match impact {
                    NamespaceImpact::Same => Ok(self.commit()),
                    NamespaceImpact::Changed(changes) => {
                        self.confirmation = Some(Confirmation {
                            path,
                            namespaces,
                            changes: changes
                                .iter()
                                .map(|c| {
                                    format!(
                                        "{} resolves to {} instead of {}",
                                        c.before.name, c.after, c.before
                                    )
                                })
                                .collect(),
                        });
                        Ok(EditorEffect::None)
                    }
                    NamespaceImpact::Invalid(e) => Err(e),
                }
            }
            EditorCommand::ConfirmNamespaces => match self.confirmation.take() {
                Some(c) => {
                    self.at_level(&c.path, |level, at| level.set_namespaces(at, c.namespaces, true))?;
                    Ok(self.commit())
                }
                None => Ok(EditorEffect::None),
            },
            EditorCommand::CancelNamespaces => {
                self.confirmation = None;
                Ok(EditorEffect::None)
            }
            EditorCommand::ApplyPreset { path, index } => {
                let md = self.metadata_at(&path)?;
                let preset = md.presets.get(index).cloned().ok_or_else(|| no_node(&path.0))?;
                self.at_level(&path, |level, at| {
                    let ns = preset_ns(&preset.ns, level, at);
                    let element = ActionElement::from_action_request(&preset.action, ns);
                    level.replace(at, ElementKind::Action(element))
                })?;
                Ok(self.commit())
            }
            EditorCommand::ApplyNext { path, index } => {
                let md = self.metadata_at(&path)?;
                let preset = md.next.get(index).cloned().ok_or_else(|| no_node(&path.0))?;
                self.at_level(&path, |level, at| {
                    let ns = preset_ns(&preset.ns, level, at);
                    let element = ActionElement::from_action_request(&preset.action, ns);
                    level.insert(at + 1, ElementKind::Action(element))
                })?;
                Ok(self.commit())
            }
            EditorCommand::ApplyArgumentPreset { path, arg, index } => self.edit_argument(&path, |a, info| {
                let preset = info.presets.get(index).ok_or_else(|| {
                    Error::invalid_argument("preset", &info.name, &format!("has no preset {index}"))
                })?;
                let source = Source::from_parameter_value(preset.value()).ok_or_else(|| {
                    Error::invalid_argument("preset", &info.name, "the preset has no value")
                })?;
                a.set_source(info.name.as_str(), source)?;
                Ok(())
            }, &arg),
            EditorCommand::SetHeader { path, header } => {
                if self.kind == EditorKind::Key {
                    return Err(Error::general_error("a key editor has no header".to_string()));
                }
                self.at_level(&path, |level, at| {
                    let current = &level.editor()?.pipeline().elements.get(at).ok_or_else(|| no_node(&[]))?.item;
                    let key = match current {
                        ElementKind::Resource { resource, .. } => operand_text(resource),
                        ElementKind::KeyValue(op) | ElementKind::Cwd(op) => operand_text(op),
                        ElementKind::Text(_)
                        | ElementKind::Action(_)
                        | ElementKind::Value(_)
                        | ElementKind::Evaluate(_)
                        | ElementKind::QueryValue(_)
                        | ElementKind::Quote
                        | ElementKind::Volatile
                        | ElementKind::Pipeline(_) => {
                            return Err(Error::invalid_pipeline("the element is not a resource"))
                        }
                    };
                    level.replace(at, resource_element(&header, &key)?)
                })?;
                Ok(self.commit())
            }
            EditorCommand::SetKey { path, key } => {
                let key = key.trim().trim_start_matches('/').to_string();
                parse_key(&key)?;
                self.browser = None;
                if self.kind == EditorKind::Key {
                    *self.root_mut()? = Edited::Key(key);
                    return Ok(self.commit());
                }
                self.at_level(&path, |level, at| {
                    let current = level.editor()?.pipeline().elements.get(at).ok_or_else(|| no_node(&[]))?.item.clone();
                    let header = header_of(&current)
                        .ok_or_else(|| Error::invalid_pipeline("the element is not a resource"))?;
                    level.replace(at, resource_element(header, &key)?)
                })?;
                Ok(self.commit())
            }
            EditorCommand::Browse { path, folder } => {
                let folder = folder.trim().trim_matches('/').to_string();
                parse_key(&folder)?;
                self.browser = Some(Browser { path, listing: None });
                Ok(EditorEffect::ListDirectory(folder))
            }
            EditorCommand::CloseBrowser => {
                self.browser = None;
                Ok(EditorEffect::None)
            }
            EditorCommand::SearchCommands { insert_at, filter } => {
                self.picker = Some((insert_at, filter));
                Ok(EditorEffect::None)
            }
            EditorCommand::PickCommand(key) => {
                let (insert_at, _) = self.picker.take().unwrap_or((None, String::new()));
                let command = EditorCommand::Insert {
                    path: insert_at.clone().unwrap_or_default(),
                    element: NewElement::Action(key.clone()),
                };
                match insert_at {
                    Some(_) => self.apply_inner(command),
                    None => self.apply_inner(EditorCommand::Append(NewElement::Action(key))),
                }
            }
            EditorCommand::ClosePicker => {
                self.picker = None;
                Ok(EditorEffect::None)
            }
            EditorCommand::SetFilename(name) => {
                let name = name.trim().to_string();
                self.root_mut()?
                    .set_filename(if name.is_empty() { None } else { Some(name.as_str()) })?;
                Ok(self.commit())
            }
            EditorCommand::FilenameFrom { path } => {
                let md = self.metadata_at(&path)?;
                if md.filename.is_empty() {
                    return Err(Error::general_error(format!("{} declares no filename", md.name)));
                }
                self.root_mut()?.set_filename(Some(md.filename.as_str()))?;
                Ok(self.commit())
            }
            EditorCommand::DefaultFilename => {
                let name = self
                    .last_action_metadata()
                    .map(|md| md.filename)
                    .filter(|f| !f.is_empty())
                    .ok_or_else(|| Error::general_error("the last command declares no filename".to_string()))?;
                self.root_mut()?.set_filename(Some(name.as_str()))?;
                Ok(self.commit())
            }
            EditorCommand::SetNumeric { index, value, max } => {
                let at = self
                    .numeric_target()
                    .ok_or_else(|| Error::general_error("no action to control".to_string()))?;
                let md = self.metadata_at(&NodePath::element(at))?;
                let info = md
                    .arguments
                    .iter()
                    .filter(|a| visible(a) && is_numeric(a))
                    .nth(index)
                    .cloned()
                    .ok_or_else(|| Error::general_error(format!("the action has no numeric argument {index}")))?;
                let json = scale_numeric(&info, value, max)?;
                self.edit_argument(&NodePath::element(at), |a, info| {
                    a.set_source(info.name.as_str(), Source::Value(json))?;
                    Ok(())
                }, &info.name)
            }
            EditorCommand::Update => {
                if self.pending {
                    Ok(self.write())
                } else {
                    Ok(EditorEffect::None)
                }
            }
            EditorCommand::Revert => {
                self.pending = false;
                self.remote_changed = false;
                self.typed_text = None;
                self.fingerprint = None;
                self.message = None;
                Ok(EditorEffect::Reload)
            }
            EditorCommand::SetContinuous(on) => {
                self.continuous = on;
                if on && self.pending {
                    Ok(self.write())
                } else {
                    Ok(EditorEffect::None)
                }
            }
        }
    }

    fn set_text(&mut self, text: String) -> Result<EditorEffect, Error> {
        match self.kind {
            EditorKind::Query => {}
            EditorKind::Action | EditorKind::Resource | EditorKind::Key => {
                return Err(Error::general_error("the text is read-only here".to_string()))
            }
        }
        let parsed = parse_query(&text).and_then(|q| match self.root {
            Some(Edited::Pipeline(_)) => {
                PipelineEditor::from_pipeline(Pipeline::try_from(&q)?, self.registry.clone()).map(Edited::Pipeline)
            }
            Some(Edited::Query(_)) | Some(Edited::Key(_)) | None => {
                QueryEditor::from_query(&q, self.registry.clone()).map(Edited::Query)
            }
        });
        match parsed {
            Ok(root) => {
                self.root_mut()?;
                self.root = Some(root);
                self.typed_text = None;
                Ok(self.commit())
            }
            Err(e) => {
                self.typed_text = Some((text, e.to_string()));
                Ok(EditorEffect::None)
            }
        }
    }

    fn new_element(&self, element: NewElement) -> Result<ElementKind, Error> {
        Ok(match element {
            NewElement::Action(key) => {
                let ns = match key.namespace.as_str() {
                    "" | "root" => None,
                    n => Some(Namespaces::from(n)),
                };
                ElementKind::Action(ActionElement {
                    action: key.name.clone(),
                    ns,
                    parameters: vec![],
                    arguments: Default::default(),
                })
            }
            NewElement::Resource(key) => {
                parse_key(&key)?;
                resource_element("", &key)?
            }
            NewElement::Quote => ElementKind::Quote,
            NewElement::Volatile => ElementKind::Volatile,
        })
    }

    fn last_action_index(&self) -> Option<usize> {
        let editor = self.root.as_ref()?.editor().ok()?;
        editor
            .pipeline()
            .elements
            .iter()
            .rposition(|e| matches!(e.item, ElementKind::Action(_)))
    }

    fn last_action_metadata(&self) -> Option<CommandMetadata> {
        let at = self.last_action_index()?;
        self.root.as_ref()?.editor().ok()?.command_metadata(at).ok().cloned()
    }

    /// The action a controller drives (AC-13): the one edited by an action editor, the last one
    /// otherwise.
    fn numeric_target(&self) -> Option<usize> {
        self.last_action_index()
    }

    // -----------------------------------------------------------------------------------------
    // The tree
    // -----------------------------------------------------------------------------------------

    fn rebuild(&mut self) {
        let mut tree = EditorTree {
            continuous: self.continuous,
            pending: self.pending,
            read_only: self.read_only,
            message: self.message.clone(),
            browser: self.browser.clone(),
            confirmation: self.confirmation.clone(),
            has_filename: self.kind == EditorKind::Query,
            ..EditorTree::default()
        };
        if let Some(root) = &self.root {
            match root.text() {
                Ok(text) => {
                    tree.text = text;
                    tree.text_editable = self.kind == EditorKind::Query && !self.read_only;
                }
                Err(e) => {
                    tree.text = String::new();
                    tree.text_note = Some(format!("The pipeline has no query form: {}", e.message));
                }
            }
            match root {
                Edited::Key(key) => {
                    tree.nodes = vec![EditorNode::Resource {
                        path: NodePath::default(),
                        header: None,
                        key: key.clone(),
                        note: None,
                    }]
                }
                Edited::Query(_) | Edited::Pipeline(_) => {
                    if let Ok(editor) = root.editor() {
                        let nodes = build_nodes(editor, &NodePath::default(), &self.registry);
                        tree.nodes = match self.kind {
                            EditorKind::Resource => nodes
                                .into_iter()
                                .filter(|n| matches!(n, EditorNode::Resource { .. }))
                                .take(1)
                                .collect(),
                            EditorKind::Query | EditorKind::Action | EditorKind::Key => nodes,
                        };
                        tree.filename = editor.pipeline().filename.clone().unwrap_or_default();
                        if self.kind == EditorKind::Query {
                            tree.spans = super::spans::spans_of(&tree.text, editor, &NodePath::default(), 0, &self.registry);
                        }
                    }
                    tree.filename_default = self
                        .last_action_metadata()
                        .map(|md| md.filename)
                        .filter(|f| !f.is_empty());
                }
            }
        } else if self.registry.commands.is_empty() {
            tree.text_note = Some("no command registry".to_string());
        }
        if let Some((text, error)) = &self.typed_text {
            tree.text = text.clone();
            tree.text_note = Some(error.clone());
            tree.stale = true;
            tree.spans.clear();
        }
        tree.show_realm = {
            let mut realms = self.registry.commands.iter().map(|c| c.realm.as_str());
            match realms.next() {
                Some(first) => realms.any(|r| r != first),
                None => false,
            }
        };
        if let Some((insert_at, filter)) = &self.picker {
            let realm = self
                .root
                .as_ref()
                .and_then(|r| r.editor().ok())
                .map(|e| e.pipeline().realm.clone())
                .unwrap_or_default();
            tree.picker = Some(Picker {
                insert_at: insert_at.clone(),
                filter: filter.clone(),
                catalog: command_catalog(&self.registry, &realm, filter),
            });
        }
        self.tree = tree;
    }
}

fn preset_ns(preset_ns: &str, level: &Edited, at: usize) -> Option<Namespaces> {
    if !preset_ns.is_empty() {
        return Some(Namespaces::from(preset_ns));
    }
    match level.editor().ok()?.pipeline().elements.get(at).map(|e| &e.item) {
        Some(ElementKind::Action(a)) => a.ns.clone(),
        Some(_) | None => None,
    }
}

fn visible(info: &ArgumentInfo) -> bool {
    !info.injected && effective_gui(info) != ArgumentGUIInfo::Hide
}

fn command_path(command: &EditorCommand) -> NodePath {
    match command {
        EditorCommand::SetField { path, .. }
        | EditorCommand::SetItem { path, .. }
        | EditorCommand::SetLink { path, .. }
        | EditorCommand::ToLink { path, .. }
        | EditorCommand::ToValue { path, .. }
        | EditorCommand::Reset { path, .. }
        | EditorCommand::InsertItem { path, .. }
        | EditorCommand::RemoveItem { path, .. }
        | EditorCommand::MoveItem { path, .. }
        | EditorCommand::Insert { path, .. }
        | EditorCommand::Remove { path }
        | EditorCommand::Move { path, .. }
        | EditorCommand::SetNamespaces { path, .. }
        | EditorCommand::ApplyPreset { path, .. }
        | EditorCommand::ApplyNext { path, .. }
        | EditorCommand::ApplyArgumentPreset { path, .. }
        | EditorCommand::SetHeader { path, .. }
        | EditorCommand::SetKey { path, .. }
        | EditorCommand::Browse { path, .. }
        | EditorCommand::FilenameFrom { path } => path.clone(),
        EditorCommand::SetText(_)
        | EditorCommand::Append(_)
        | EditorCommand::ConfirmNamespaces
        | EditorCommand::CancelNamespaces
        | EditorCommand::CloseBrowser
        | EditorCommand::SearchCommands { .. }
        | EditorCommand::PickCommand(_)
        | EditorCommand::ClosePicker
        | EditorCommand::SetFilename(_)
        | EditorCommand::DefaultFilename
        | EditorCommand::SetNumeric { .. }
        | EditorCommand::Update
        | EditorCommand::Revert
        | EditorCommand::SetContinuous(_) => NodePath::default(),
    }
}

/// The nodes of one level.
fn build_nodes(editor: &PipelineEditor, prefix: &NodePath, registry: &Arc<CommandMetadataRegistry>) -> Vec<EditorNode> {
    let mut nodes = Vec::new();
    for (i, element) in editor.pipeline().elements.iter().enumerate() {
        let path = prefix.then(PathStep::Element(i));
        match &element.item {
            ElementKind::Resource { resource, .. } => nodes.push(EditorNode::Resource {
                path,
                header: header_of(&element.item).map(str::to_string),
                key: operand_text(resource),
                note: None,
            }),
            ElementKind::KeyValue(op) => nodes.push(EditorNode::Resource {
                path,
                header: Some("key".to_string()),
                key: operand_text(op),
                note: Some("passes the key itself; nothing is read".to_string()),
            }),
            ElementKind::Cwd(op) => nodes.push(EditorNode::Resource {
                path,
                header: Some("cwd".to_string()),
                key: operand_text(op),
                note: Some("sets the working directory; nothing is read".to_string()),
            }),
            ElementKind::Action(a) => {
                if let Some(ns) = &a.ns {
                    let first = editor.namespace_run(i).map(|r| r.start == i).unwrap_or(true);
                    if first {
                        nodes.push(EditorNode::Namespaces {
                            path: path.clone(),
                            list: ns.0.clone(),
                            unused: editor.unused_namespaces(i).unwrap_or_default(),
                        });
                    }
                }
                match editor.command_metadata(i) {
                    Ok(md) => nodes.push(action_node(editor, i, a, md, path, registry)),
                    Err(e) => nodes.push(EditorNode::Opaque {
                        path,
                        text: action_text(a),
                        reason: e.message.clone(),
                    }),
                }
            }
            ElementKind::Quote => nodes.push(EditorNode::Directive { path, name: "q".to_string() }),
            ElementKind::Volatile => nodes.push(EditorNode::Directive { path, name: "v".to_string() }),
            ElementKind::Value(Operand::Literal(v)) => nodes.push(EditorNode::Value { path, json: v.to_string() }),
            ElementKind::Value(Operand::Placeholder(name)) => nodes.push(EditorNode::Opaque {
                path,
                text: format!("${{{name}}}"),
                reason: "a placeholder of a command definition".to_string(),
            }),
            ElementKind::Text(text) => nodes.push(EditorNode::Opaque {
                path,
                text: text.clone(),
                reason: "query text inside a pipeline".to_string(),
            }),
            ElementKind::Evaluate(op) | ElementKind::QueryValue(op) => nodes.push(EditorNode::Opaque {
                path,
                text: match op {
                    Operand::Literal(QueryOrPipeline::Query(q)) => q.clone(),
                    Operand::Literal(QueryOrPipeline::Pipeline(_)) => "a pipeline".to_string(),
                    Operand::Placeholder(name) => format!("${{{name}}}"),
                },
                reason: "evaluated at run time".to_string(),
            }),
            ElementKind::Pipeline(p) => {
                let nested = PipelineEditor::from_pipeline(p.clone(), registry.clone())
                    .map(|e| build_nodes(&e, &path, registry))
                    .unwrap_or_default();
                nodes.push(EditorNode::Nested {
                    path,
                    label: if p.realm.is_empty() { "pipeline".to_string() } else { format!("realm {}", p.realm) },
                    nodes: nested,
                })
            }
        }
    }
    nodes
}

fn action_node(
    editor: &PipelineEditor,
    at: usize,
    action: &ActionElement,
    md: &CommandMetadata,
    path: NodePath,
    registry: &Arc<CommandMetadataRegistry>,
) -> EditorNode {
    let mut fields = Vec::new();
    let mut numeric = 0usize;
    for info in md.arguments.iter() {
        if !visible(info) {
            continue;
        }
        let info = info.resolve_global_enums(registry).unwrap_or_else(|_| info.clone());
        let arg_path = path.then(PathStep::Argument(info.name.clone()));
        let value = match editor.argument_source(at, info.name.as_str()) {
            Ok(Some(source)) => field_value(&source, &arg_path, info.multiple, registry),
            Ok(None) | Err(_) => FieldValue::Unset,
        };
        let (alternatives, others_allowed) = match &info.argument_type {
            ArgumentType::Enum(e) => (enum_alternatives(e), e.others_allowed),
            ArgumentType::String
            | ArgumentType::Integer
            | ArgumentType::IntegerOption
            | ArgumentType::Float
            | ArgumentType::FloatOption
            | ArgumentType::Boolean
            | ArgumentType::BooleanOption
            | ArgumentType::GlobalEnum(_)
            | ArgumentType::Any
            | ArgumentType::None
            | ArgumentType::Node(_) => (Vec::new(), true),
        };
        let numeric_index = if is_numeric(&info) {
            numeric += 1;
            Some(numeric - 1)
        } else {
            None
        };
        fields.push(EditorField {
            name: info.name.clone(),
            label: if info.label.is_empty() { info.name.clone() } else { info.label.clone() },
            description: info.description.clone(),
            type_name: crate::utils::argument_type_name(&info.argument_type),
            default_text: default_text(&info.default),
            widget: effective_gui(&info),
            alternatives,
            others_allowed,
            multiple: info.multiple,
            value,
            presets: info
                .presets
                .iter()
                .map(|p| PresetEntry {
                    label: p.name().to_string(),
                    description: p.description().to_string(),
                    active: false,
                })
                .collect(),
            numeric_index,
        });
    }
    let slots = md.arguments.iter().filter(|a| !a.injected).count();
    let variadic = md.arguments.iter().any(|a| !a.injected && a.multiple);
    let excess = if variadic {
        Vec::new()
    } else {
        action
            .parameters
            .iter()
            .skip(slots)
            .map(|p| match &p.item {
                Source::Value(v) => json_text(v),
                Source::Link(q) => format!("~X~{q}~E"),
                Source::Placeholder(_) | Source::Pipeline(_) | Source::State(_) | Source::Multiple(_) => "…".to_string(),
            })
            .collect()
    };
    EditorNode::Action {
        path,
        command: md.key(),
        label: if md.label.is_empty() { md.name.clone() } else { md.label.clone() },
        doc: md.doc.clone(),
        presets: preset_entries(&md.presets, Some(&action_text(action))),
        next: preset_entries(&md.next, None),
        filename: if md.filename.is_empty() { None } else { Some(md.filename.clone()) },
        fields,
        excess,
        hint_icon: md.hints.get("icon").and_then(|v| crate::ui::theme::Icon::from_hint(v).ok()),
    }
}

fn field_value(source: &Source, path: &NodePath, multiple: bool, registry: &Arc<CommandMetadataRegistry>) -> FieldValue {
    match source {
        Source::Multiple(items) => FieldValue::Items(
            items
                .iter()
                .enumerate()
                .map(|(k, s)| field_value(s, &path.then(PathStep::Item(k)), false, registry))
                .collect(),
        ),
        Source::Value(Json::Array(items)) if multiple => FieldValue::Items(
            items.iter().map(|v| FieldValue::Text(json_text(v))).collect(),
        ),
        _ if multiple => FieldValue::Items(vec![field_value(source, &path.then(PathStep::Item(0)), false, registry)]),
        Source::Value(v) => FieldValue::Text(json_text(v)),
        Source::Link(query) => {
            let nodes = QueryEditor::from_query(&match parse_query(query) {
                Ok(q) => q,
                Err(_) => return FieldValue::Link { query: query.clone(), nodes: vec![], is_key: false },
            }, registry.clone())
            .map(|e| build_nodes(e.editor(), path, registry))
            .unwrap_or_default();
            FieldValue::Link { query: query.clone(), nodes, is_key: query.starts_with("-R-key/") }
        }
        Source::Placeholder(name) => FieldValue::Other(format!("${{{name}}}")),
        Source::Pipeline(_) => FieldValue::Other("a pipeline".to_string()),
        Source::State(_) => FieldValue::Other("the input state".to_string()),
    }
}
