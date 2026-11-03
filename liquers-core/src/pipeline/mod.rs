//! Pipelines: the structured, serializable form of a query.
//!
//! A [`Pipeline`] is a sequence of elements in one realm. An element is an action, a value that
//! becomes the current state, a resource read, a nested pipeline (a realm change), or a piece of
//! query text inlined in place. Any argument of an action can be a literal value of any JSON
//! shape, a placeholder, a link, a nested pipeline or the input state ([`Source`]).
//!
//! Pipelines are used in three places:
//! - a **pipeline command**'s definition (`CommandDefinition::Pipeline`), whose placeholders are
//!   the command's arguments;
//! - a **recipe**'s `pipeline`, which runs before the recipe's query;
//! - code that builds or edits queries, through the editors.
//!
//! Every query converts to a pipeline; a pipeline converts back to a query when it is
//! *representable* (`specs/reference/PIPELINES.md`, design `composite-command-definition`).
//!
//! # Serialized form
//!
//! ```yaml
//! realm: ""                 # optional; the segment header name
//! elements:
//!   - "-R/data/sales.csv/-/ns-pl/from_csv"          # a bare string: query text, inlined
//!   - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
//!   - {action: head, parameters: [{placeholder: n}]}
//! filename: top.csv         # optional
//! ```
//!
//! A bare list is a pipeline with only `elements`. An element is told apart by its key:
//! `action`, `value`, `resource` (with an optional `read`), `key_value`, `cwd`, `evaluate`,
//! `query_value`, `quote`, `volatile` or `pipeline`; any element may also carry `pos`. An argument
//! is a bare scalar, array or object (a literal value), or an object with exactly one of the keys
//! `value`, `placeholder`, `link`, `pipeline`, `state` or `multiple` (plus an optional `pos`). A
//! literal object that has that shape is written inside `{value: …}`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value as Json};

use crate::error::Error;
use crate::query::Position;

mod convert;
mod editor;
pub use convert::{ArgRef, ElementPath};
pub use editor::{ActionEditor, PipelineEditor, QueryEditor};

/// A value together with where it came from.
///
/// `pos` is set when the value was converted from a parsed query, and absent when it was written
/// by hand or edited. Equality ignores it.
#[derive(Debug, Clone)]
pub struct Located<T> {
    pub item: T,
    pub pos: Option<Position>,
}

impl<T> Located<T> {
    /// `item` with no position.
    pub fn new(item: T) -> Self {
        Located { item, pos: None }
    }
    /// `item` at `pos`; an unknown position is stored as `None`.
    pub fn at(item: T, pos: &Position) -> Self {
        Located {
            item,
            pos: if pos.is_unknown() {
                None
            } else {
                Some(pos.clone())
            },
        }
    }
    /// The position, or [`Position::unknown`].
    pub fn position(&self) -> Position {
        self.pos.clone().unwrap_or_else(Position::unknown)
    }
}

impl<T: PartialEq> PartialEq for Located<T> {
    fn eq(&self, other: &Self) -> bool {
        self.item == other.item
    }
}

impl<T> From<T> for Located<T> {
    fn from(item: T) -> Self {
        Located::new(item)
    }
}

/// One element of a pipeline, with its position.
pub type PipelineElement = Located<ElementKind>;
/// One argument of an action element, with its position.
pub type Argument = Located<Source>;

/// A sequence of elements in one realm: the structured form of a query.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pipeline {
    /// The segment header name, of a resource or a transform segment; empty for the default realm.
    pub realm: String,
    /// Header parameters of the first segment, carried as they are.
    pub header_parameters: Vec<String>,
    /// A leading `/`: relative keys resolve against the root, not the working directory.
    pub rooted: bool,
    pub elements: Vec<PipelineElement>,
    /// The output filename. Only a top-level pipeline has one.
    pub filename: Option<String>,
}

/// What an element does.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    /// Query text, inlined here: its elements continue from the current state.
    Text(String),
    /// A command applied to the current state.
    Action(ActionElement),
    /// A value that becomes the current state.
    Value(Operand<Json>),
    /// A resource read: the asset, its metadata, its recipe… at a key.
    Resource { resource: Operand<String>, read: Read },
    /// The key itself as the current state (`-R-key/…`).
    KeyValue(Operand<String>),
    /// Sets the working directory (`-R-cwd/…`).
    Cwd(Operand<String>),
    /// A query or pipeline evaluated as its own asset; its result becomes the state.
    Evaluate(Operand<QueryOrPipeline>),
    /// A query as a value (`<query>/q`).
    QueryValue(Operand<QueryOrPipeline>),
    /// The query so far as a value (`q`).
    Quote,
    /// Marks the result volatile (`v`).
    Volatile,
    /// A nested pipeline: its elements run in its own realm, then the enclosing realm resumes.
    Pipeline(Pipeline),
}

/// A literal, or a placeholder bound when a pipeline command is called.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand<T> {
    Literal(T),
    Placeholder(String),
}

/// How a resource element reads its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Read {
    /// The asset's value (`-R/…`).
    #[default]
    Asset,
    /// The asset's binary (`-R-binary/…`).
    Binary,
    /// The asset's metadata (`-R-metadata/…`).
    Metadata,
    /// The asset's recipe (`-R-recipe/…`).
    Recipe,
    /// The asset directory listing (`-R-directory/…`).
    Directory,
    /// The stored bytes, bypassing the asset manager (`-R-stored_binary/…`).
    Stored,
    /// The stored metadata (`-R-stored_metadata/…`).
    StoredMetadata,
    /// The store's directory listing (`-R-store_directory/…`).
    StoredDirectory,
}

impl Read {
    /// The long spelling, written in queries and in serialized pipelines; `None` for
    /// [`Read::Asset`], which needs no header instruction.
    pub fn instruction(self) -> Option<&'static str> {
        match self {
            Read::Asset => None,
            Read::Binary => Some("binary"),
            Read::Metadata => Some("metadata"),
            Read::Recipe => Some("recipe"),
            Read::Directory => Some("directory"),
            Read::Stored => Some("stored_binary"),
            Read::StoredMetadata => Some("stored_metadata"),
            Read::StoredDirectory => Some("store_directory"),
        }
    }
    /// The serialized name: [`Read::instruction`], or `asset`.
    pub fn name(self) -> &'static str {
        self.instruction().unwrap_or("asset")
    }
    /// Reads any spelling the resource header accepts, short or long, plus `asset`.
    ///
    /// `cwd` and `key` are not reads: they are the [`ElementKind::Cwd`] and
    /// [`ElementKind::KeyValue`] elements.
    pub fn from_instruction(text: &str) -> Option<Read> {
        match text {
            "asset" | "data" | "value" => Some(Read::Asset),
            "b" | "bin" | "binary" => Some(Read::Binary),
            "meta" | "metadata" => Some(Read::Metadata),
            "r" | "recipe" => Some(Read::Recipe),
            "dir" | "directory" => Some(Read::Directory),
            "stored" | "stored_binary" | "stored_bin" | "sbin" => Some(Read::Stored),
            "stored_meta" | "stored_metadata" => Some(Read::StoredMetadata),
            "sdir" | "store_directory" => Some(Read::StoredDirectory),
            _ => None,
        }
    }
}

/// Query text, or a pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryOrPipeline {
    Query(String),
    Pipeline(Pipeline),
}

/// One or more namespaces, as written in an action element's `ns` (a string or a list).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Namespaces(pub Vec<String>);

impl From<&str> for Namespaces {
    fn from(ns: &str) -> Self {
        Namespaces(vec![ns.to_owned()])
    }
}
impl From<String> for Namespaces {
    fn from(ns: String) -> Self {
        Namespaces(vec![ns])
    }
}
impl From<Vec<String>> for Namespaces {
    fn from(ns: Vec<String>) -> Self {
        Namespaces(ns)
    }
}
impl From<Vec<&str>> for Namespaces {
    fn from(ns: Vec<&str>) -> Self {
        Namespaces(ns.into_iter().map(str::to_owned).collect())
    }
}
impl<const N: usize> From<[&str; N]> for Namespaces {
    fn from(ns: [&str; N]) -> Self {
        Namespaces(ns.iter().map(|s| (*s).to_owned()).collect())
    }
}

/// A command applied to the current state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionElement {
    /// The command name.
    pub action: String,
    /// The namespaces to look it up in; `None` for the default namespaces (never the caller's).
    pub ns: Option<Namespaces>,
    /// Positional arguments, consumed first.
    pub parameters: Vec<Argument>,
    /// Arguments by name.
    pub arguments: BTreeMap<String, Argument>,
}

impl ActionElement {
    /// An action with no arguments.
    pub fn new(action: impl Into<String>) -> Self {
        ActionElement {
            action: action.into(),
            ..Default::default()
        }
    }
}

/// Where an argument's value comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A literal value of any JSON shape.
    Value(Json),
    /// A placeholder: one of the pipeline command's arguments.
    Placeholder(String),
    /// Query text, evaluated and its result passed (a link).
    Link(String),
    /// A nested pipeline, evaluated and its result passed.
    Pipeline(Pipeline),
    /// The pipeline command's input state (`true`).
    State(bool),
    /// The elements of a variadic (`multiple`) argument.
    Multiple(Vec<Source>),
}

impl From<Json> for Source {
    fn from(v: Json) -> Self {
        Source::Value(v)
    }
}

// ---------------------------------------------------------------------------------------------
// Placeholders and binding
// ---------------------------------------------------------------------------------------------

impl Pipeline {
    /// A pipeline of `elements` in the default realm.
    pub fn new(elements: Vec<PipelineElement>) -> Self {
        Pipeline {
            elements,
            ..Default::default()
        }
    }

    /// Every placeholder name used anywhere in this pipeline, nested pipelines included.
    pub fn placeholders(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        self.collect_placeholders(&mut names);
        names
    }

    fn collect_placeholders(&self, names: &mut BTreeSet<String>) {
        fn operand<T>(op: &Operand<T>, names: &mut BTreeSet<String>) {
            match op {
                Operand::Literal(_) => {}
                Operand::Placeholder(name) => {
                    names.insert(name.clone());
                }
            }
        }
        fn query_or_pipeline(op: &Operand<QueryOrPipeline>, names: &mut BTreeSet<String>) {
            match op {
                Operand::Literal(QueryOrPipeline::Pipeline(p)) => p.collect_placeholders(names),
                Operand::Literal(QueryOrPipeline::Query(_)) => {}
                Operand::Placeholder(name) => {
                    names.insert(name.clone());
                }
            }
        }
        fn source(s: &Source, names: &mut BTreeSet<String>) {
            match s {
                Source::Placeholder(name) => {
                    names.insert(name.clone());
                }
                Source::Pipeline(p) => p.collect_placeholders(names),
                Source::Multiple(items) => items.iter().for_each(|s| source(s, names)),
                Source::Value(_) | Source::Link(_) | Source::State(_) => {}
            }
        }
        for element in self.elements.iter() {
            match &element.item {
                ElementKind::Text(_) | ElementKind::Quote | ElementKind::Volatile => {}
                ElementKind::Action(a) => {
                    a.parameters.iter().for_each(|p| source(&p.item, names));
                    a.arguments.values().for_each(|p| source(&p.item, names));
                }
                ElementKind::Value(op) => operand(op, names),
                ElementKind::Resource { resource, .. } => operand(resource, names),
                ElementKind::KeyValue(op) | ElementKind::Cwd(op) => operand(op, names),
                ElementKind::Evaluate(op) | ElementKind::QueryValue(op) => {
                    query_or_pipeline(op, names)
                }
                ElementKind::Pipeline(p) => p.collect_placeholders(names),
            }
        }
    }

    /// Whether any argument, nested pipelines included, is the input state.
    pub fn uses_input_state(&self) -> bool {
        fn source(s: &Source) -> bool {
            match s {
                Source::State(true) => true,
                Source::Pipeline(p) => p.uses_input_state(),
                Source::Multiple(items) => items.iter().any(source),
                Source::State(false) | Source::Value(_) | Source::Placeholder(_) | Source::Link(_) => {
                    false
                }
            }
        }
        self.elements.iter().any(|e| match &e.item {
            ElementKind::Action(a) => {
                a.parameters.iter().any(|p| source(&p.item))
                    || a.arguments.values().any(|p| source(&p.item))
            }
            ElementKind::Pipeline(p) => p.uses_input_state(),
            ElementKind::Evaluate(Operand::Literal(QueryOrPipeline::Pipeline(p)))
            | ElementKind::QueryValue(Operand::Literal(QueryOrPipeline::Pipeline(p))) => {
                p.uses_input_state()
            }
            ElementKind::Text(_)
            | ElementKind::Value(_)
            | ElementKind::Resource { .. }
            | ElementKind::KeyValue(_)
            | ElementKind::Cwd(_)
            | ElementKind::Evaluate(_)
            | ElementKind::QueryValue(_)
            | ElementKind::Quote
            | ElementKind::Volatile => false,
        })
    }

    /// Replaces every placeholder with its value from `values` (AC-38).
    ///
    /// A placeholder in an operand that needs text (a resource key, `cwd`, `key_value`) must be
    /// bound to a string; one in `evaluate` or `query_value` to a string (query text) or a
    /// pipeline. An unbound placeholder is an error naming it. Extra entries in `values` are
    /// ignored. The input state is not a placeholder and is left as it is.
    pub fn bind(&self, values: &BTreeMap<String, Json>) -> Result<Pipeline, Error> {
        let lookup = |name: &str| {
            values.get(name).ok_or_else(|| {
                Error::invalid_pipeline(format!("placeholder '{name}' is not bound"))
            })
        };
        let text = |op: &Operand<String>| -> Result<Operand<String>, Error> {
            match op {
                Operand::Literal(s) => Ok(Operand::Literal(s.clone())),
                Operand::Placeholder(name) => match lookup(name)? {
                    Json::String(s) => Ok(Operand::Literal(s.clone())),
                    other => Err(Error::invalid_pipeline(format!(
                        "placeholder '{name}' must be text here, not {other}"
                    ))),
                },
            }
        };
        let query_or_pipeline =
            |op: &Operand<QueryOrPipeline>| -> Result<Operand<QueryOrPipeline>, Error> {
                match op {
                    Operand::Literal(QueryOrPipeline::Query(q)) => {
                        Ok(Operand::Literal(QueryOrPipeline::Query(q.clone())))
                    }
                    Operand::Literal(QueryOrPipeline::Pipeline(p)) => {
                        Ok(Operand::Literal(QueryOrPipeline::Pipeline(p.bind(values)?)))
                    }
                    Operand::Placeholder(name) => match lookup(name)? {
                        Json::String(s) => Ok(Operand::Literal(QueryOrPipeline::Query(s.clone()))),
                        other => Ok(Operand::Literal(QueryOrPipeline::Pipeline(
                            Pipeline::from_json(other)?,
                        ))),
                    },
                }
            };
        fn source(
            s: &Source,
            values: &BTreeMap<String, Json>,
        ) -> Result<Source, Error> {
            match s {
                Source::Placeholder(name) => values
                    .get(name)
                    .map(|v| Source::Value(v.clone()))
                    .ok_or_else(|| {
                        Error::invalid_pipeline(format!("placeholder '{name}' is not bound"))
                    }),
                Source::Pipeline(p) => Ok(Source::Pipeline(p.bind(values)?)),
                Source::Multiple(items) => Ok(Source::Multiple(
                    items
                        .iter()
                        .map(|s| source(s, values))
                        .collect::<Result<_, _>>()?,
                )),
                Source::Value(_) | Source::Link(_) | Source::State(_) => Ok(s.clone()),
            }
        }
        let argument = |a: &Argument| -> Result<Argument, Error> {
            Ok(Located {
                item: source(&a.item, values)?,
                pos: a.pos.clone(),
            })
        };
        let mut elements = Vec::with_capacity(self.elements.len());
        for element in self.elements.iter() {
            let item = match &element.item {
                ElementKind::Text(_) | ElementKind::Quote | ElementKind::Volatile => {
                    element.item.clone()
                }
                ElementKind::Action(a) => ElementKind::Action(ActionElement {
                    action: a.action.clone(),
                    ns: a.ns.clone(),
                    parameters: a.parameters.iter().map(argument).collect::<Result<_, _>>()?,
                    arguments: a
                        .arguments
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), argument(v)?)))
                        .collect::<Result<_, Error>>()?,
                }),
                ElementKind::Value(Operand::Literal(v)) => {
                    ElementKind::Value(Operand::Literal(v.clone()))
                }
                ElementKind::Value(Operand::Placeholder(name)) => {
                    ElementKind::Value(Operand::Literal(lookup(name)?.clone()))
                }
                ElementKind::Resource { resource, read } => ElementKind::Resource {
                    resource: text(resource)?,
                    read: *read,
                },
                ElementKind::KeyValue(op) => ElementKind::KeyValue(text(op)?),
                ElementKind::Cwd(op) => ElementKind::Cwd(text(op)?),
                ElementKind::Evaluate(op) => ElementKind::Evaluate(query_or_pipeline(op)?),
                ElementKind::QueryValue(op) => ElementKind::QueryValue(query_or_pipeline(op)?),
                ElementKind::Pipeline(p) => ElementKind::Pipeline(p.bind(values)?),
            };
            elements.push(Located {
                item,
                pos: element.pos.clone(),
            });
        }
        Ok(Pipeline {
            elements,
            ..self.clone()
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Serialization, through `serde_json::Value`
// ---------------------------------------------------------------------------------------------

const ELEMENT_KEYS: [&str; 10] = [
    "action",
    "value",
    "resource",
    "key_value",
    "cwd",
    "evaluate",
    "query_value",
    "quote",
    "volatile",
    "pipeline",
];
const SOURCE_KEYS: [&str; 6] = ["value", "placeholder", "link", "pipeline", "state", "multiple"];

fn pos_to_json(pos: &Option<Position>, map: &mut Map<String, Json>) {
    if let Some(pos) = pos {
        if let Ok(v) = serde_json::to_value(pos) {
            map.insert("pos".to_owned(), v);
        }
    }
}

fn pos_from_json(map: &Map<String, Json>, what: &str) -> Result<Option<Position>, Error> {
    match map.get("pos") {
        None => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| Error::invalid_pipeline(format!("{what}: bad 'pos': {e}"))),
    }
}

fn operand_to_json<T, F: Fn(&T) -> Json>(op: &Operand<T>, f: F) -> Json {
    match op {
        Operand::Literal(v) => f(v),
        Operand::Placeholder(name) => placeholder_json(name),
    }
}

fn placeholder_json(name: &str) -> Json {
    let mut m = Map::new();
    m.insert("placeholder".to_owned(), Json::String(name.to_owned()));
    Json::Object(m)
}

/// `{placeholder: "name"}` and nothing else.
fn as_placeholder(v: &Json) -> Option<&str> {
    match v {
        Json::Object(m) if m.len() == 1 => m.get("placeholder").and_then(Json::as_str),
        Json::Object(_) | Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_)
        | Json::Array(_) => None,
    }
}

fn text_operand(v: &Json, what: &str) -> Result<Operand<String>, Error> {
    if let Some(name) = as_placeholder(v) {
        return Ok(Operand::Placeholder(name.to_owned()));
    }
    match v {
        Json::String(s) => Ok(Operand::Literal(s.clone())),
        other => Err(Error::invalid_pipeline(format!(
            "{what} must be text or a placeholder, not {other}"
        ))),
    }
}

fn query_or_pipeline_operand(v: &Json, what: &str) -> Result<Operand<QueryOrPipeline>, Error> {
    if let Some(name) = as_placeholder(v) {
        return Ok(Operand::Placeholder(name.to_owned()));
    }
    match v {
        Json::String(s) => Ok(Operand::Literal(QueryOrPipeline::Query(s.clone()))),
        Json::Object(_) | Json::Array(_) => Ok(Operand::Literal(QueryOrPipeline::Pipeline(
            Pipeline::from_json(v).map_err(|e| prefix_error(e, what))?,
        ))),
        Json::Null | Json::Bool(_) | Json::Number(_) => Err(Error::invalid_pipeline(format!(
            "{what} must be query text, a pipeline or a placeholder, not {v}"
        ))),
    }
}

fn query_or_pipeline_to_json(v: &QueryOrPipeline) -> Json {
    match v {
        QueryOrPipeline::Query(q) => Json::String(q.clone()),
        QueryOrPipeline::Pipeline(p) => p.to_json(),
    }
}

fn prefix_error(e: Error, what: &str) -> Error {
    let message = e
        .message
        .strip_prefix("Invalid pipeline: ")
        .unwrap_or(&e.message)
        .to_owned();
    Error::invalid_pipeline(format!("{what}: {message}"))
}

impl Source {
    /// The serialized form, without a position.
    pub fn to_json(&self) -> Json {
        self.to_json_with(&None)
    }

    fn to_json_with(&self, pos: &Option<Position>) -> Json {
        let tagged = |key: &str, v: Json| {
            let mut m = Map::new();
            m.insert(key.to_owned(), v);
            pos_to_json(pos, &mut m);
            Json::Object(m)
        };
        match self {
            Source::Value(v) => match v {
                Json::Object(_) => tagged("value", v.clone()),
                Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) | Json::Array(_) => {
                    if pos.is_none() {
                        v.clone()
                    } else {
                        tagged("value", v.clone())
                    }
                }
            },
            Source::Placeholder(name) => tagged("placeholder", Json::String(name.clone())),
            Source::Link(q) => tagged("link", Json::String(q.clone())),
            Source::Pipeline(p) => tagged("pipeline", p.to_json()),
            Source::State(b) => tagged("state", Json::Bool(*b)),
            Source::Multiple(items) => tagged(
                "multiple",
                Json::Array(items.iter().map(Source::to_json).collect()),
            ),
        }
    }

    /// Reads an argument: a tagged object, or a literal value.
    pub fn from_json(v: &Json) -> Result<Source, Error> {
        Ok(Self::located_from_json(v, "argument")?.item)
    }

    fn located_from_json(v: &Json, what: &str) -> Result<Argument, Error> {
        if let Json::Object(m) = v {
            let tags: Vec<&String> = m.keys().filter(|k| SOURCE_KEYS.contains(&k.as_str())).collect();
            let others = m.keys().filter(|k| !SOURCE_KEYS.contains(&k.as_str()) && *k != "pos");
            if tags.len() == 1 && others.count() == 0 {
                let key = tags[0].as_str();
                let body = &m[key];
                let pos = pos_from_json(m, what)?;
                let item = match key {
                    "value" => Source::Value(body.clone()),
                    "placeholder" => Source::Placeholder(
                        body.as_str()
                            .ok_or_else(|| {
                                Error::invalid_pipeline(format!("{what}: a placeholder name is text"))
                            })?
                            .to_owned(),
                    ),
                    "link" => Source::Link(
                        body.as_str()
                            .ok_or_else(|| {
                                Error::invalid_pipeline(format!("{what}: a link is query text"))
                            })?
                            .to_owned(),
                    ),
                    "pipeline" => {
                        Source::Pipeline(Pipeline::from_json(body).map_err(|e| prefix_error(e, what))?)
                    }
                    "state" => Source::State(body.as_bool().ok_or_else(|| {
                        Error::invalid_pipeline(format!("{what}: 'state' is true or false"))
                    })?),
                    "multiple" => match body {
                        Json::Array(items) => Source::Multiple(
                            items
                                .iter()
                                .enumerate()
                                .map(|(i, v)| Source::from_json(v).map_err(|e| prefix_error(e, &format!("{what}, item {i}"))))
                                .collect::<Result<_, _>>()?,
                        ),
                        Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) | Json::Object(_) => {
                            return Err(Error::invalid_pipeline(format!(
                                "{what}: 'multiple' is a list"
                            )))
                        }
                    },
                    _ => Source::Value(v.clone()),
                };
                return Ok(Located { item, pos });
            }
        }
        Ok(Located::new(Source::Value(v.clone())))
    }
}

impl ElementKind {
    fn to_json_with(&self, pos: &Option<Position>) -> Json {
        let mut m = Map::new();
        match self {
            ElementKind::Text(text) => {
                if pos.is_none() {
                    return Json::String(text.clone());
                }
                // A positioned text element keeps its position in the long form.
                m.insert("text".to_owned(), Json::String(text.clone()));
            }
            ElementKind::Action(a) => {
                m.insert("action".to_owned(), Json::String(a.action.clone()));
                if let Some(ns) = &a.ns {
                    let v = if ns.0.len() == 1 {
                        Json::String(ns.0[0].clone())
                    } else {
                        Json::Array(ns.0.iter().cloned().map(Json::String).collect())
                    };
                    m.insert("ns".to_owned(), v);
                }
                if !a.parameters.is_empty() {
                    m.insert(
                        "parameters".to_owned(),
                        Json::Array(
                            a.parameters
                                .iter()
                                .map(|p| p.item.to_json_with(&p.pos))
                                .collect(),
                        ),
                    );
                }
                if !a.arguments.is_empty() {
                    let mut args = Map::new();
                    for (k, v) in a.arguments.iter() {
                        args.insert(k.clone(), v.item.to_json_with(&v.pos));
                    }
                    m.insert("arguments".to_owned(), Json::Object(args));
                }
            }
            ElementKind::Value(op) => {
                m.insert("value".to_owned(), operand_to_json(op, |v| v.clone()));
            }
            ElementKind::Resource { resource, read } => {
                m.insert(
                    "resource".to_owned(),
                    operand_to_json(resource, |s| Json::String(s.clone())),
                );
                if let Some(read) = read.instruction() {
                    m.insert("read".to_owned(), Json::String(read.to_owned()));
                }
            }
            ElementKind::KeyValue(op) => {
                m.insert(
                    "key_value".to_owned(),
                    operand_to_json(op, |s| Json::String(s.clone())),
                );
            }
            ElementKind::Cwd(op) => {
                m.insert("cwd".to_owned(), operand_to_json(op, |s| Json::String(s.clone())));
            }
            ElementKind::Evaluate(op) => {
                m.insert("evaluate".to_owned(), operand_to_json(op, query_or_pipeline_to_json));
            }
            ElementKind::QueryValue(op) => {
                m.insert(
                    "query_value".to_owned(),
                    operand_to_json(op, query_or_pipeline_to_json),
                );
            }
            ElementKind::Quote => {
                m.insert("quote".to_owned(), Json::Bool(true));
            }
            ElementKind::Volatile => {
                m.insert("volatile".to_owned(), Json::Bool(true));
            }
            ElementKind::Pipeline(p) => {
                m.insert("pipeline".to_owned(), p.to_json());
            }
        }
        pos_to_json(pos, &mut m);
        Json::Object(m)
    }
}

fn element_from_json(v: &Json, index: usize) -> Result<PipelineElement, Error> {
    let what = format!("element {index}");
    let m = match v {
        Json::String(s) => return Ok(Located::new(ElementKind::Text(s.clone()))),
        Json::Object(m) => m,
        Json::Null | Json::Bool(_) | Json::Number(_) | Json::Array(_) => {
            return Err(Error::invalid_pipeline(format!(
                "{what} must be query text or an object, not {v}"
            )))
        }
    };
    let pos = pos_from_json(m, &what)?;
    let allowed: &[&str] = if m.contains_key("action") {
        &["action", "ns", "parameters", "arguments", "pos"]
    } else if m.contains_key("resource") {
        &["resource", "read", "pos"]
    } else if m.contains_key("text") {
        &["text", "pos"]
    } else {
        &[]
    };
    let kinds: Vec<&str> = m
        .keys()
        .map(String::as_str)
        .filter(|k| ELEMENT_KEYS.contains(k) || *k == "text")
        .collect();
    if kinds.len() != 1 {
        return Err(Error::invalid_pipeline(match kinds.len() {
            0 => format!(
                "{what} has none of the element keys ({}); keys found: {}",
                ELEMENT_KEYS.join(", "),
                m.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
            _ => format!("{what} has more than one element key: {}", kinds.join(", ")),
        }));
    }
    let kind = kinds[0];
    for key in m.keys() {
        let ok = key == kind || key == "pos" || allowed.contains(&key.as_str());
        if !ok {
            return Err(Error::invalid_pipeline(format!(
                "{what} ({kind}) has an unknown key '{key}'"
            )));
        }
    }
    let body = &m[kind];
    let flag = |name: &str| -> Result<(), Error> {
        match body {
            Json::Bool(true) => Ok(()),
            Json::Null | Json::Bool(false) | Json::Number(_) | Json::String(_) | Json::Array(_)
            | Json::Object(_) => Err(Error::invalid_pipeline(format!(
                "{what}: '{name}' must be true"
            ))),
        }
    };
    let item = match kind {
        "text" => ElementKind::Text(
            body.as_str()
                .ok_or_else(|| Error::invalid_pipeline(format!("{what}: 'text' is query text")))?
                .to_owned(),
        ),
        "action" => {
            let action = body
                .as_str()
                .ok_or_else(|| Error::invalid_pipeline(format!("{what}: 'action' is a name")))?
                .to_owned();
            let ns = match m.get("ns") {
                None => None,
                Some(Json::String(s)) => Some(Namespaces(vec![s.clone()])),
                Some(Json::Array(items)) => Some(Namespaces(
                    items
                        .iter()
                        .map(|v| {
                            v.as_str().map(str::to_owned).ok_or_else(|| {
                                Error::invalid_pipeline(format!("{what}: 'ns' lists names"))
                            })
                        })
                        .collect::<Result<_, _>>()?,
                )),
                Some(other) => {
                    return Err(Error::invalid_pipeline(format!(
                        "{what}: 'ns' is a name or a list of names, not {other}"
                    )))
                }
            };
            let parameters = match m.get("parameters") {
                None => vec![],
                Some(Json::Array(items)) => items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Source::located_from_json(v, &format!("{what}, parameter {i}")))
                    .collect::<Result<_, _>>()?,
                Some(other) => {
                    return Err(Error::invalid_pipeline(format!(
                        "{what}: 'parameters' is a list, not {other}"
                    )))
                }
            };
            let arguments = match m.get("arguments") {
                None => BTreeMap::new(),
                Some(Json::Object(args)) => args
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            k.clone(),
                            Source::located_from_json(v, &format!("{what}, argument '{k}'"))?,
                        ))
                    })
                    .collect::<Result<_, Error>>()?,
                Some(other) => {
                    return Err(Error::invalid_pipeline(format!(
                        "{what}: 'arguments' maps names to arguments, not {other}"
                    )))
                }
            };
            ElementKind::Action(ActionElement {
                action,
                ns,
                parameters,
                arguments,
            })
        }
        "value" => match as_placeholder(body) {
            Some(name) => ElementKind::Value(Operand::Placeholder(name.to_owned())),
            None => ElementKind::Value(Operand::Literal(body.clone())),
        },
        "resource" => {
            let read = match m.get("read") {
                None => Read::Asset,
                Some(Json::String(s)) => Read::from_instruction(s).ok_or_else(|| {
                    Error::invalid_pipeline(format!("{what}: unknown read '{s}'"))
                })?,
                Some(other) => {
                    return Err(Error::invalid_pipeline(format!(
                        "{what}: 'read' is a name, not {other}"
                    )))
                }
            };
            ElementKind::Resource {
                resource: text_operand(body, &what)?,
                read,
            }
        }
        "key_value" => ElementKind::KeyValue(text_operand(body, &what)?),
        "cwd" => ElementKind::Cwd(text_operand(body, &what)?),
        "evaluate" => ElementKind::Evaluate(query_or_pipeline_operand(body, &what)?),
        "query_value" => ElementKind::QueryValue(query_or_pipeline_operand(body, &what)?),
        "quote" => {
            flag("quote")?;
            ElementKind::Quote
        }
        "volatile" => {
            flag("volatile")?;
            ElementKind::Volatile
        }
        "pipeline" => {
            ElementKind::Pipeline(Pipeline::from_json(body).map_err(|e| prefix_error(e, &what))?)
        }
        other => {
            return Err(Error::invalid_pipeline(format!(
                "{what} has an unknown key '{other}'"
            )))
        }
    };
    Ok(Located { item, pos })
}

impl Pipeline {
    /// The serialized form, always a map with `elements`.
    pub fn to_json(&self) -> Json {
        let mut m = Map::new();
        if !self.realm.is_empty() {
            m.insert("realm".to_owned(), Json::String(self.realm.clone()));
        }
        if !self.header_parameters.is_empty() {
            m.insert(
                "header_parameters".to_owned(),
                Json::Array(self.header_parameters.iter().cloned().map(Json::String).collect()),
            );
        }
        if self.rooted {
            m.insert("rooted".to_owned(), Json::Bool(true));
        }
        m.insert(
            "elements".to_owned(),
            Json::Array(
                self.elements
                    .iter()
                    .map(|e| e.item.to_json_with(&e.pos))
                    .collect(),
            ),
        );
        if let Some(filename) = &self.filename {
            m.insert("filename".to_owned(), Json::String(filename.clone()));
        }
        Json::Object(m)
    }

    /// Reads a pipeline: a map, or a bare list of elements.
    pub fn from_json(v: &Json) -> Result<Pipeline, Error> {
        let elements_from = |items: &Vec<Json>| -> Result<Vec<PipelineElement>, Error> {
            items
                .iter()
                .enumerate()
                .map(|(i, v)| element_from_json(v, i))
                .collect()
        };
        match v {
            Json::Array(items) => Ok(Pipeline::new(elements_from(items)?)),
            Json::Object(m) => {
                for key in m.keys() {
                    match key.as_str() {
                        "realm" | "header_parameters" | "rooted" | "elements" | "filename" => {}
                        other => {
                            return Err(Error::invalid_pipeline(format!(
                                "unknown pipeline key '{other}' (expected realm, \
                                 header_parameters, rooted, elements, filename)"
                            )))
                        }
                    }
                }
                let text = |key: &str| -> Result<Option<String>, Error> {
                    match m.get(key) {
                        None | Some(Json::Null) => Ok(None),
                        Some(Json::String(s)) => Ok(Some(s.clone())),
                        Some(other) => Err(Error::invalid_pipeline(format!(
                            "'{key}' is text, not {other}"
                        ))),
                    }
                };
                let header_parameters = match m.get("header_parameters") {
                    None => vec![],
                    Some(Json::Array(items)) => items
                        .iter()
                        .map(|v| {
                            v.as_str().map(str::to_owned).ok_or_else(|| {
                                Error::invalid_pipeline("'header_parameters' lists text")
                            })
                        })
                        .collect::<Result<_, _>>()?,
                    Some(other) => {
                        return Err(Error::invalid_pipeline(format!(
                            "'header_parameters' is a list, not {other}"
                        )))
                    }
                };
                let rooted = match m.get("rooted") {
                    None => false,
                    Some(Json::Bool(b)) => *b,
                    Some(other) => {
                        return Err(Error::invalid_pipeline(format!(
                            "'rooted' is true or false, not {other}"
                        )))
                    }
                };
                let elements = match m.get("elements") {
                    None => vec![],
                    Some(Json::Array(items)) => elements_from(items)?,
                    Some(other) => {
                        return Err(Error::invalid_pipeline(format!(
                            "'elements' is a list, not {other}"
                        )))
                    }
                };
                Ok(Pipeline {
                    realm: text("realm")?.unwrap_or_default(),
                    header_parameters,
                    rooted,
                    elements,
                    filename: text("filename")?,
                })
            }
            Json::Null | Json::Bool(_) | Json::Number(_) | Json::String(_) => Err(
                Error::invalid_pipeline(format!("a pipeline is a list or a map, not {v}")),
            ),
        }
    }
}

impl Serialize for Pipeline {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Pipeline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Json::deserialize(deserializer)?;
        Pipeline::from_json(&v).map_err(|e| serde::de::Error::custom(e.message.clone()))
    }
}

impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Source {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Json::deserialize(deserializer)?;
        Source::from_json(&v).map_err(|e| serde::de::Error::custom(e.message.clone()))
    }
}

#[cfg(test)]
mod tests;
