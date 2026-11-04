//! Conversions between pipelines, queries and recipes.
//!
//! - [`TryFrom<&Query>`](Pipeline#impl-TryFrom<%26Query>-for-Pipeline) is syntactic: it needs no
//!   registry, keeps every position, and gives text parameters.
//! - [`Pipeline::to_query_with`] needs the registry (rule N1): named arguments become positional,
//!   and namespaces are placed so every action resolves to the command its element names. It
//!   checks the representability rules in element order and reports the first that fails.
//! - [`Recipe::to_pipeline`] concatenates a recipe's parts.
//! - [`Pipeline::typed_with`] gives each text parameter its argument's declared type.

use std::fmt;

use serde_json::Value as Json;

use crate::command_metadata::{
    ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandMetadataRegistry,
    CommandParameterValue,
};
use crate::error::Error;
use crate::escape::{is_filename, is_header_value, is_identifier, is_realm_name, is_resource_name};
use crate::parse::{parse_key, parse_query};
use crate::query::{
    ActionParameter, ActionRequest, HeaderParameter, Key, Position, Query, QuerySegment,
    ResourceName, ResourceQuerySegment, SegmentHeader, TransformQuerySegment, NS_INSTRUCTION,
    Q_INSTRUCTION, V_INSTRUCTION,
};
use crate::recipes::Recipe;

use super::{
    ActionElement, ElementKind, Located, Namespaces, Operand, Pipeline, PipelineElement,
    QueryOrPipeline, Read, Source,
};

/// An argument of an action element: by position (0-based) or by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgRef {
    Position(usize),
    Name(String),
}

impl From<usize> for ArgRef {
    fn from(i: usize) -> Self {
        ArgRef::Position(i)
    }
}
impl From<&str> for ArgRef {
    fn from(name: &str) -> Self {
        ArgRef::Name(name.to_owned())
    }
}
impl From<String> for ArgRef {
    fn from(name: String) -> Self {
        ArgRef::Name(name)
    }
}

impl fmt::Display for ArgRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgRef::Position(i) => write!(f, "parameter {i}"),
            ArgRef::Name(name) => write!(f, "argument '{name}'"),
        }
    }
}

/// Where in a pipeline something is: the indices of the nested pipelines leading to it, the
/// element, and optionally an argument.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementPath {
    pub nested: Vec<usize>,
    pub element: Option<usize>,
    pub argument: Option<ArgRef>,
}

impl ElementPath {
    fn element(&self, index: usize) -> ElementPath {
        ElementPath {
            nested: self.nested.clone(),
            element: Some(index),
            argument: None,
        }
    }
    fn nest(&self, index: usize) -> ElementPath {
        let mut nested = self.nested.clone();
        nested.push(index);
        ElementPath {
            nested,
            element: None,
            argument: None,
        }
    }
    fn argument(&self, argument: ArgRef) -> ElementPath {
        ElementPath {
            argument: Some(argument),
            ..self.clone()
        }
    }
}

impl fmt::Display for ElementPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        for i in self.nested.iter() {
            parts.push(format!("nested pipeline {i}"));
        }
        match self.element {
            Some(i) => parts.push(format!("element {i}")),
            None => {
                if parts.is_empty() {
                    parts.push("the pipeline".to_owned());
                }
            }
        }
        if let Some(argument) = &self.argument {
            parts.push(argument.to_string());
        }
        write!(f, "{}", parts.join(", "))
    }
}

fn not_representable(
    rule: &str,
    path: &ElementPath,
    pos: &Option<Position>,
    detail: &str,
) -> Error {
    let e = Error::not_representable(rule, path, detail);
    match pos {
        Some(pos) => e.with_position(pos),
        None => e,
    }
}

// ---------------------------------------------------------------------------------------------
// Query → pipeline
// ---------------------------------------------------------------------------------------------

impl TryFrom<&Query> for Pipeline {
    type Error = Error;

    /// Converts a query to a pipeline, syntactically: parameters are text, positions are kept.
    ///
    /// Consecutive segments of one realm form one pipeline; a segment in another realm opens a
    /// nested pipeline (a realm change), and a later segment back in the first realm resumes it
    /// (AC-40). Fails for a query [`Query::check_encodable`] refuses, a header level other than
    /// `-`, a resource header with more than one instruction or an unknown one, and a filename
    /// before the last segment.
    fn try_from(query: &Query) -> Result<Self, Self::Error> {
        query.check_encodable()?;
        let mut top = Pipeline {
            rooted: query.rooted,
            ..Default::default()
        };
        // Index into `top.elements` of the nested pipeline currently receiving segments.
        let mut current: Option<usize> = None;
        let count = query.segments.len();
        for (index, segment) in query.segments.iter().enumerate() {
            let header = match segment {
                QuerySegment::Resource(r) => r.header.as_ref(),
                QuerySegment::Transform(t) => t.header.as_ref(),
            };
            let realm = header.map(|h| h.name.clone()).unwrap_or_default();
            if let Some(h) = header {
                if h.level != 0 {
                    return Err(Error::invalid_pipeline(format!(
                        "segment {index} ('{}') has header level {}; levels other than '-' are \
                         reserved for nesting queries and are not supported",
                        h.encode(),
                        h.level
                    ))
                    .with_position(&h.position));
                }
            }
            let transform_header_parameters: Vec<String> = match (segment, header) {
                (QuerySegment::Transform(_), Some(h)) => {
                    h.parameters.iter().map(|p| p.value.clone()).collect()
                }
                (QuerySegment::Transform(_), None) | (QuerySegment::Resource(_), _) => vec![],
            };
            if index == 0 {
                top.realm = realm.clone();
            }
            // Choose the pipeline that receives this segment.
            if realm == top.realm {
                current = None;
                if !transform_header_parameters.is_empty()
                    && !(top.elements.is_empty() && top.header_parameters.is_empty())
                {
                    // Header parameters that cannot be the pipeline's own: a nested pipeline.
                    top.elements
                        .push(Located::new(ElementKind::Pipeline(Pipeline {
                            realm: realm.clone(),
                            header_parameters: transform_header_parameters.clone(),
                            ..Default::default()
                        })));
                    current = Some(top.elements.len() - 1);
                } else if !transform_header_parameters.is_empty() {
                    top.header_parameters = transform_header_parameters.clone();
                }
            } else {
                let reuse = match current {
                    Some(i) => match &top.elements[i].item {
                        ElementKind::Pipeline(p) => {
                            i + 1 == top.elements.len()
                                && p.realm == realm
                                && (transform_header_parameters.is_empty()
                                    || p.header_parameters == transform_header_parameters)
                        }
                        ElementKind::Text(_)
                        | ElementKind::Action(_)
                        | ElementKind::Value(_)
                        | ElementKind::Resource { .. }
                        | ElementKind::KeyValue(_)
                        | ElementKind::Cwd(_)
                        | ElementKind::Evaluate(_)
                        | ElementKind::QueryValue(_)
                        | ElementKind::Quote
                        | ElementKind::Volatile => false,
                    },
                    None => false,
                };
                if !reuse {
                    top.elements.push(Located::at(
                        ElementKind::Pipeline(Pipeline {
                            realm: realm.clone(),
                            header_parameters: transform_header_parameters.clone(),
                            ..Default::default()
                        }),
                        &header
                            .map(|h| h.position.clone())
                            .unwrap_or_else(Position::unknown),
                    ));
                    current = Some(top.elements.len() - 1);
                }
            }
            let mut buf: Vec<PipelineElement> = Vec::new();
            match segment {
                QuerySegment::Resource(r) => {
                    let (kind_of, pos) = match header {
                        Some(h) => {
                            if let Some(excess) = h.parameters.get(1) {
                                return Err(Error::too_many_parameters(
                                    "resource header",
                                    1,
                                    2,
                                    &excess.value,
                                    &excess.position,
                                ));
                            }
                            (
                                h.parameters.first().map(|p| p.value.clone()),
                                h.position.clone(),
                            )
                        }
                        None => (None, r.position()),
                    };
                    let key = Operand::Literal(r.key.encode());
                    let item = match kind_of.as_deref() {
                        None => ElementKind::Resource {
                            resource: key,
                            read: Read::Asset,
                        },
                        Some("cwd") => ElementKind::Cwd(key),
                        Some("key") => ElementKind::KeyValue(key),
                        Some(instruction) => match Read::from_instruction(instruction) {
                            Some(read) => ElementKind::Resource {
                                resource: key,
                                read,
                            },
                            None => {
                                return Err(Error::invalid_pipeline(format!(
                                    "unknown resource header instruction '{instruction}'"
                                ))
                                .with_position(&pos))
                            }
                        },
                    };
                    buf.push(Located::at(item, &pos));
                }
                QuerySegment::Transform(t) => {
                    let mut ns: Option<Namespaces> = None;
                    for action in t.query.iter() {
                        if action.name == NS_INSTRUCTION {
                            let mut names = Vec::new();
                            for p in action.parameters.iter() {
                                match p {
                                    ActionParameter::String(s, _) => names.push(s.clone()),
                                    ActionParameter::Link(_, pos) => {
                                        return Err(Error::not_supported(
                                            "Only string parameters are supported in ns".into(),
                                        )
                                        .with_position(pos))
                                    }
                                }
                            }
                            ns = Some(Namespaces(names));
                            continue;
                        }
                        if action.name == Q_INSTRUCTION || action.name == V_INSTRUCTION {
                            if let Some(excess) = action.parameters.first() {
                                return Err(Error::too_many_parameters(
                                    &format!("instruction '{}'", action.name),
                                    0,
                                    1,
                                    &excess.encode(),
                                    &excess.position(),
                                ));
                            }
                            let item = if action.name == Q_INSTRUCTION {
                                ElementKind::Quote
                            } else {
                                ElementKind::Volatile
                            };
                            buf.push(Located::at(item, &action.position));
                            continue;
                        }
                        let parameters = action
                            .parameters
                            .iter()
                            .map(|p| match p {
                                ActionParameter::String(s, pos) => {
                                    Located::at(Source::Value(Json::String(s.clone())), pos)
                                }
                                ActionParameter::Link(q, pos) => {
                                    Located::at(Source::Link(q.encode()), pos)
                                }
                            })
                            .collect();
                        buf.push(Located::at(
                            ElementKind::Action(ActionElement {
                                action: action.name.clone(),
                                ns: ns.clone(),
                                parameters,
                                arguments: Default::default(),
                            }),
                            &action.position,
                        ));
                    }
                    if let Some(filename) = &t.filename {
                        if index + 1 != count {
                            return Err(Error::invalid_pipeline(format!(
                                "segment {index} has a filename '{}' but is not the last segment",
                                filename.name
                            ))
                            .with_position(&filename.position));
                        }
                        top.filename = Some(filename.name.clone());
                    }
                }
            }
            match current {
                Some(i) => {
                    if let ElementKind::Pipeline(p) = &mut top.elements[i].item {
                        p.elements.append(&mut buf);
                    }
                }
                None => top.elements.append(&mut buf),
            }
        }
        Ok(top)
    }
}

impl TryFrom<Query> for Pipeline {
    type Error = Error;
    fn try_from(query: Query) -> Result<Self, Self::Error> {
        Pipeline::try_from(&query)
    }
}

/// The pipeline of a link-like parameter (AC-18): a link converts syntactically, a pipeline
/// variant is cloned. A value, a placeholder, an injected argument, the input state or a list is
/// refused, since none of them names something to evaluate.
impl TryFrom<&crate::plan::ParameterValue> for Pipeline {
    type Error = Error;
    fn try_from(parameter: &crate::plan::ParameterValue) -> Result<Self, Self::Error> {
        use crate::plan::ParameterValue as P;
        match parameter {
            P::DefaultLink(_, q)
            | P::ParameterLink(_, q, _)
            | P::OverrideLink(_, q)
            | P::EnumLink(_, q, _) => Pipeline::try_from(q),
            P::ParameterPipeline(_, p, _)
            | P::OverridePipeline(_, p)
            | P::DefaultPipeline(_, p) => Ok(p.clone()),
            P::DefaultValue(_, _)
            | P::ParameterValue(_, _, _)
            | P::OverrideValue(_, _)
            | P::Placeholder(_)
            | P::MultipleParameters(_, _)
            | P::Injected(_)
            | P::InputState(_)
            | P::FrameArgument(_, _)
            | P::None => Err(Error::conversion_error_with_message(
                parameter.to_string(),
                "Pipeline",
                "only a link or a pipeline parameter has a pipeline",
            )),
        }
    }
}

impl Pipeline {
    /// Parses `text` as a query and converts it.
    pub fn from_query_text(text: &str) -> Result<Pipeline, Error> {
        Pipeline::try_from(&parse_query(text)?)
    }

    /// The pipeline's text form as a value writes it (format `txt`): the query it converts to
    /// with no registry, each action written as it stands. A pipeline with no query form (a
    /// placeholder, a structured value) is a serialization error.
    pub fn to_query_text(&self) -> Result<String, Error> {
        self.to_query_lenient(&CommandMetadataRegistry::new())
            .map(|q| q.encode())
            .map_err(|e| {
                Error::from_error(
                    crate::error::ErrorType::SerializationError,
                    format!("This pipeline has no query text: {}", e.message),
                )
            })
    }
}

// ---------------------------------------------------------------------------------------------
// Typing text parameters through the registry
// ---------------------------------------------------------------------------------------------

/// The typed form of a query text parameter for an argument of `argument_type`, or the text
/// itself when the type is text-like or the text does not parse as the type. Empty text stays
/// text: in a query it means "the default".
pub(crate) fn typed_scalar(text: &str, argument_type: &ArgumentType) -> Json {
    if text.is_empty() {
        return Json::String(String::new());
    }
    let text_value = || Json::String(text.to_owned());
    match argument_type {
        ArgumentType::Integer | ArgumentType::IntegerOption => text
            .parse::<i64>()
            .map(Json::from)
            .unwrap_or_else(|_| text_value()),
        ArgumentType::Float | ArgumentType::FloatOption => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or_else(text_value),
        ArgumentType::Boolean | ArgumentType::BooleanOption => match text.to_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" => Json::Bool(true),
            "false" | "f" | "no" | "n" | "0" => Json::Bool(false),
            _ => text_value(),
        },
        ArgumentType::String
        | ArgumentType::Enum(_)
        | ArgumentType::GlobalEnum(_)
        | ArgumentType::Any
        | ArgumentType::None => text_value(),
    }
}

/// The non-injected arguments, in the order query parameters fill them.
pub(crate) fn positional_arguments(md: &CommandMetadata) -> Vec<&ArgumentInfo> {
    md.arguments.iter().filter(|a| !a.injected).collect()
}

/// The command an action element names, looked up in `realm`.
pub(crate) fn resolve_action(
    cmr: &CommandMetadataRegistry,
    realm: &str,
    action: &ActionElement,
) -> Result<CommandMetadata, Error> {
    let explicit = action.ns.as_ref().map(|n| n.0.clone()).unwrap_or_default();
    let namespaces = cmr.lookup_namespaces(&explicit);
    cmr.find_command_in_namespaces(realm, &namespaces, &action.action)
        .ok_or_else(|| {
            Error::action_not_registered(
                &ActionRequest::new(action.action.clone()),
                realm,
                &namespaces,
            )
        })
}

impl Pipeline {
    /// Gives each text parameter its argument's declared type (rule S1), so a parameter of an
    /// `int` argument becomes a number. Text that does not parse as the type, and parameters of
    /// actions the registry does not know, stay text.
    pub fn typed_with(&self, cmr: &CommandMetadataRegistry) -> Pipeline {
        self.typed_in(cmr, &self.realm)
    }

    fn typed_in(&self, cmr: &CommandMetadataRegistry, realm: &str) -> Pipeline {
        let mut out = self.clone();
        for element in out.elements.iter_mut() {
            match &mut element.item {
                ElementKind::Action(action) => {
                    let Ok(md) = resolve_action(cmr, realm, action) else {
                        continue;
                    };
                    let slots = positional_arguments(&md);
                    let mut slot_index = 0usize;
                    for parameter in action.parameters.iter_mut() {
                        let Some(slot) = slots.get(slot_index) else {
                            break;
                        };
                        if let Source::Value(Json::String(text)) = &parameter.item {
                            parameter.item = Source::Value(typed_scalar(text, &slot.argument_type));
                        }
                        if !slot.multiple {
                            slot_index += 1;
                        }
                    }
                    for (name, argument) in action.arguments.iter_mut() {
                        if let Some(slot) = slots.iter().find(|s| &s.name == name) {
                            if let Source::Value(Json::String(text)) = &argument.item {
                                argument.item =
                                    Source::Value(typed_scalar(text, &slot.argument_type));
                            }
                        }
                    }
                }
                ElementKind::Pipeline(p) => {
                    let realm = p.realm.clone();
                    *p = p.typed_in(cmr, &realm);
                }
                ElementKind::Text(_)
                | ElementKind::Value(_)
                | ElementKind::Resource { .. }
                | ElementKind::KeyValue(_)
                | ElementKind::Cwd(_)
                | ElementKind::Evaluate(_)
                | ElementKind::QueryValue(_)
                | ElementKind::Quote
                | ElementKind::Volatile => {}
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Pipeline → query
// ---------------------------------------------------------------------------------------------

/// One flattened element, tagged with the realm it runs in.
enum Flat {
    Action {
        realm: String,
        header: Option<Vec<String>>,
        element: ActionElement,
        pos: Option<Position>,
        path: ElementPath,
    },
    Instruction {
        realm: String,
        header: Option<Vec<String>>,
        name: &'static str,
        pos: Option<Position>,
    },
    Resource {
        realm: String,
        instruction: Option<&'static str>,
        key: Key,
        pos: Option<Position>,
    },
}

/// A transform segment before its namespaces are placed and its arguments written.
struct RawTransform {
    realm: String,
    header_parameters: Vec<String>,
    items: Vec<Flat>,
}

enum Raw {
    Resource(ResourceQuerySegment),
    Transform(RawTransform),
}

#[derive(Default)]
struct Flattener {
    items: Vec<Flat>,
    filename: Option<String>,
    rooted: bool,
}

impl Flattener {
    /// Flattens `p`. `header` holds `p`'s header parameters until its first transform element
    /// takes them.
    fn pipeline(
        &mut self,
        p: &Pipeline,
        is_top: bool,
        path: &ElementPath,
        header: &mut Option<Vec<String>>,
    ) -> Result<(), Error> {
        if !is_realm_name(&p.realm) {
            return Err(not_representable(
                "P4",
                path,
                &None,
                &format!("'{}' is not a realm name", p.realm),
            ));
        }
        for hp in p.header_parameters.iter() {
            if !is_header_value(hp) {
                return Err(not_representable(
                    "P3",
                    path,
                    &None,
                    &format!("'{hp}' is not a header parameter"),
                ));
            }
        }
        if !p.header_parameters.is_empty() && p.realm.is_empty() {
            return Err(not_representable(
                "P3",
                path,
                &None,
                "header parameters need a realm name to be written",
            ));
        }
        if p.filename.is_some() && !is_top {
            return Err(not_representable(
                "P2",
                path,
                &None,
                "only the top-level pipeline has a filename",
            ));
        }
        if p.rooted && !is_top {
            return Err(not_representable(
                "P3",
                path,
                &None,
                "only the top-level pipeline can be rooted",
            ));
        }
        let realm = p.realm.as_str();
        let count = p.elements.len();
        let at_start =
            |index: usize, this: &Flattener| is_top && index == 0 && this.items.is_empty();
        for (index, element) in p.elements.iter().enumerate() {
            let epath = path.element(index);
            let pos = element.pos.clone();
            match &element.item {
                ElementKind::Action(a) => self.items.push(Flat::Action {
                    realm: realm.to_owned(),
                    header: header.take(),
                    element: a.clone(),
                    pos,
                    path: epath,
                }),
                ElementKind::Quote => self.items.push(Flat::Instruction {
                    realm: realm.to_owned(),
                    header: header.take(),
                    name: Q_INSTRUCTION,
                    pos,
                }),
                ElementKind::Volatile => self.items.push(Flat::Instruction {
                    realm: realm.to_owned(),
                    header: header.take(),
                    name: V_INSTRUCTION,
                    pos,
                }),
                ElementKind::Resource { resource, read } => {
                    let key = key_of(resource, &epath, &pos, "E4")?;
                    self.items.push(Flat::Resource {
                        realm: realm.to_owned(),
                        instruction: read.instruction(),
                        key,
                        pos,
                    });
                }
                ElementKind::KeyValue(op) | ElementKind::Cwd(op) => {
                    let key = key_of(op, &epath, &pos, "E5")?;
                    let instruction = match &element.item {
                        ElementKind::Cwd(_) => "cwd",
                        ElementKind::KeyValue(_)
                        | ElementKind::Text(_)
                        | ElementKind::Action(_)
                        | ElementKind::Value(_)
                        | ElementKind::Resource { .. }
                        | ElementKind::Evaluate(_)
                        | ElementKind::QueryValue(_)
                        | ElementKind::Quote
                        | ElementKind::Volatile
                        | ElementKind::Pipeline(_) => "key",
                    };
                    self.items.push(Flat::Resource {
                        realm: realm.to_owned(),
                        instruction: Some(instruction),
                        key,
                        pos,
                    });
                }
                ElementKind::Value(_) => {
                    return Err(not_representable(
                        "P1",
                        &epath,
                        &pos,
                        "a value as the current state has no query spelling",
                    ))
                }
                ElementKind::Evaluate(_) => {
                    return Err(not_representable(
                        "P1",
                        &epath,
                        &pos,
                        "an evaluated query or pipeline as the current state has no query spelling",
                    ))
                }
                ElementKind::QueryValue(op) => {
                    if !at_start(index, self) {
                        return Err(not_representable(
                            "E8",
                            &epath,
                            &pos,
                            "a query value must be the first element",
                        ));
                    }
                    let inner = match op {
                        Operand::Placeholder(name) => {
                            return Err(not_representable(
                                "E8",
                                &epath,
                                &pos,
                                &format!("placeholder '{name}'"),
                            ))
                        }
                        Operand::Literal(QueryOrPipeline::Query(text)) => {
                            Pipeline::from_query_text(text)
                                .map_err(|e| not_representable("E8", &epath, &pos, &e.message))?
                        }
                        Operand::Literal(QueryOrPipeline::Pipeline(p)) => p.clone(),
                    };
                    if inner.filename.is_some() || inner.rooted {
                        return Err(not_representable(
                            "E8",
                            &epath,
                            &pos,
                            "the quoted query has a filename or a leading '/'",
                        ));
                    }
                    let mut inner_header = Some(inner.header_parameters.clone());
                    self.pipeline(&inner, false, &path.nest(index), &mut inner_header)?;
                    let (last_realm, last_header) = match self.items.last() {
                        Some(Flat::Action { realm, .. })
                        | Some(Flat::Instruction { realm, .. })
                        | Some(Flat::Resource { realm, .. }) => (realm.clone(), None),
                        None => (realm.to_owned(), header.take()),
                    };
                    self.items.push(Flat::Instruction {
                        realm: last_realm,
                        header: last_header,
                        name: Q_INSTRUCTION,
                        pos,
                    });
                }
                ElementKind::Text(text) => {
                    let inner = Pipeline::from_query_text(text).map_err(|e| {
                        not_representable(
                            "E6",
                            &epath,
                            &pos,
                            &format!("query text does not convert: {}", e.message),
                        )
                    })?;
                    if inner.rooted {
                        if !at_start(index, self) {
                            return Err(not_representable(
                                "E6",
                                &epath,
                                &pos,
                                "query text with a leading '/' can only start the pipeline",
                            ));
                        }
                        self.rooted = true;
                    }
                    if let Some(filename) = &inner.filename {
                        if !(is_top && index + 1 == count && p.filename.is_none()) {
                            return Err(not_representable(
                                "P2",
                                &epath,
                                &pos,
                                "a filename in query text must end the pipeline",
                            ));
                        }
                        self.filename = Some(filename.clone());
                    }
                    let inner = Pipeline {
                        filename: None,
                        rooted: false,
                        ..inner
                    };
                    if inner.realm == p.realm && inner.header_parameters.is_empty() {
                        // Inlined: its elements continue this pipeline.
                        let inline = Pipeline {
                            header_parameters: vec![],
                            ..inner
                        };
                        self.pipeline(&inline, false, &path.nest(index), header)?;
                    } else {
                        let mut inner_header = Some(inner.header_parameters.clone());
                        self.pipeline(&inner, false, &path.nest(index), &mut inner_header)?;
                    }
                }
                ElementKind::Pipeline(np) => {
                    if np.realm == p.realm
                        && np.header_parameters.is_empty()
                        && np.filename.is_none()
                        && !np.rooted
                    {
                        // A nested pipeline in its parent's realm adds nothing: inlined.
                        self.pipeline(np, false, &path.nest(index), header)?;
                    } else {
                        let mut inner_header = Some(np.header_parameters.clone());
                        self.pipeline(np, false, &path.nest(index), &mut inner_header)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn key_of(
    op: &Operand<String>,
    path: &ElementPath,
    pos: &Option<Position>,
    rule: &str,
) -> Result<Key, Error> {
    match op {
        Operand::Placeholder(name) => Err(not_representable(
            rule,
            path,
            pos,
            &format!("placeholder '{name}' as a key"),
        )),
        Operand::Literal(text) => {
            for segment in text.split('/').filter(|s| !s.is_empty()) {
                if !is_resource_name(segment) {
                    return Err(not_representable(
                        rule,
                        path,
                        pos,
                        &format!("'{segment}' is not a resource name"),
                    ));
                }
            }
            parse_key(text).map_err(|e| not_representable(rule, path, pos, &e.message))
        }
    }
}

/// The text of a scalar for a query parameter (S1), `None` for anything else.
fn scalar_text(v: &Json) -> Option<String> {
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Bool(b) => Some(b.to_string()),
        Json::Number(n) => Some(n.to_string()),
        Json::Null | Json::Array(_) | Json::Object(_) => None,
    }
}

/// The parameter for one non-variadic source.
fn single_parameter(
    source: &Source,
    pos: &Position,
    cmr: &CommandMetadataRegistry,
    path: &ElementPath,
    epos: &Option<Position>,
) -> Result<ActionParameter, Error> {
    match source {
        Source::Value(v) => match scalar_text(v) {
            Some(text) => Ok(ActionParameter::String(text, pos.clone())),
            None => Err(not_representable(
                "S4",
                path,
                epos,
                &format!("{v} has no query spelling for a single argument"),
            )),
        },
        Source::Link(text) => {
            let q =
                parse_query(text).map_err(|e| not_representable("S5", path, epos, &e.message))?;
            q.check_encodable()
                .map_err(|e| not_representable("S5", path, epos, &e.message))?;
            Ok(ActionParameter::Link(q, pos.clone()))
        }
        Source::Pipeline(p) => {
            let q = p.to_query_with(cmr).map_err(|e| {
                not_representable("S6", path, epos, &format!("nested pipeline: {}", e.message))
            })?;
            Ok(ActionParameter::Link(q, pos.clone()))
        }
        Source::Placeholder(name) => Err(not_representable(
            "S7",
            path,
            epos,
            &format!("placeholder '{name}' exists only in a command definition"),
        )),
        Source::State(_) => Err(not_representable(
            "S7",
            path,
            epos,
            "the input state exists only in a command definition",
        )),
        Source::Multiple(_) => Err(not_representable(
            "S3",
            path,
            epos,
            "a multiple source is only for a variadic argument",
        )),
    }
}

/// The parameters for a variadic argument's source (S2, S3).
fn multiple_parameters(
    source: &Source,
    pos: &Position,
    cmr: &CommandMetadataRegistry,
    path: &ElementPath,
    epos: &Option<Position>,
) -> Result<Vec<ActionParameter>, Error> {
    match source {
        Source::Value(Json::Array(items)) => items
            .iter()
            .map(|v| match scalar_text(v) {
                Some(text) => Ok(ActionParameter::String(text, Position::unknown())),
                None => Err(not_representable(
                    "S2",
                    path,
                    epos,
                    &format!("{v} in a list has no query spelling"),
                )),
            })
            .collect(),
        Source::Multiple(items) => items
            .iter()
            .map(|s| match s {
                Source::Multiple(_) => Err(not_representable(
                    "S3",
                    path,
                    epos,
                    "a multiple source cannot nest",
                )),
                Source::Value(Json::Array(_)) => Err(not_representable(
                    "S3",
                    path,
                    epos,
                    "a list inside a multiple source has no query spelling",
                )),
                Source::Value(_)
                | Source::Link(_)
                | Source::Pipeline(_)
                | Source::Placeholder(_)
                | Source::State(_) => single_parameter(s, &Position::unknown(), cmr, path, epos),
            })
            .collect(),
        Source::Value(_)
        | Source::Link(_)
        | Source::Pipeline(_)
        | Source::Placeholder(_)
        | Source::State(_) => Ok(vec![single_parameter(source, pos, cmr, path, epos)?]),
    }
}

/// Writes an action element's arguments as positional query parameters (N2–N4).
pub(crate) fn encode_arguments(
    action: &ActionElement,
    md: &CommandMetadata,
    cmr: &CommandMetadataRegistry,
    path: &ElementPath,
    epos: &Option<Position>,
) -> Result<Vec<ActionParameter>, Error> {
    let slots = positional_arguments(md);
    check_argument_names(action, md)?;
    let mut out = Vec::new();
    // Text written for skipped slots, flushed only when a later slot is written (N2).
    let mut gap: Vec<ActionParameter> = Vec::new();
    let mut positional = action.parameters.iter().enumerate().peekable();
    for slot in slots.iter() {
        let named = action.arguments.get(&slot.name);
        if slot.multiple {
            let mut written = Vec::new();
            for (i, p) in positional.by_ref() {
                written.extend(multiple_parameters(
                    &p.item,
                    &p.position(),
                    cmr,
                    &path.argument(ArgRef::Position(i)),
                    epos,
                )?);
            }
            if let Some(a) = named {
                written.extend(multiple_parameters(
                    &a.item,
                    &a.position(),
                    cmr,
                    &path.argument(ArgRef::Name(slot.name.clone())),
                    epos,
                )?);
            }
            if !written.is_empty() {
                out.append(&mut gap);
                out.extend(written);
            }
            break;
        }
        if let Some((i, p)) = positional.next() {
            out.append(&mut gap);
            out.push(single_parameter(
                &p.item,
                &p.position(),
                cmr,
                &path.argument(ArgRef::Position(i)),
                epos,
            )?);
            continue;
        }
        if let Some(a) = named {
            out.append(&mut gap);
            out.push(single_parameter(
                &a.item,
                &a.position(),
                cmr,
                &path.argument(ArgRef::Name(slot.name.clone())),
                epos,
            )?);
            continue;
        }
        // A gap: it matters only if something follows it.
        let later_named = slots
            .iter()
            .skip_while(|s| s.name != slot.name)
            .skip(1)
            .any(|s| action.arguments.contains_key(&s.name));
        if !later_named {
            break;
        }
        match &slot.default {
            CommandParameterValue::Value(v) => match scalar_text(v) {
                Some(text) => gap.push(ActionParameter::String(text, Position::unknown())),
                None => {
                    return Err(not_representable(
                        "N2",
                        &path.argument(ArgRef::Name(slot.name.clone())),
                        epos,
                        "the skipped argument's default has no query spelling",
                    ))
                }
            },
            CommandParameterValue::Query(q) => {
                gap.push(ActionParameter::Link(q.clone(), Position::unknown()))
            }
            CommandParameterValue::Pipeline(p) => match p.to_query_with(cmr) {
                Ok(q) => gap.push(ActionParameter::Link(q, Position::unknown())),
                Err(_) => {
                    return Err(not_representable(
                        "N2",
                        &path.argument(ArgRef::Name(slot.name.clone())),
                        epos,
                        "the skipped argument's default is a pipeline with no query form",
                    ))
                }
            },
            CommandParameterValue::None => {
                return Err(not_representable(
                    "N3",
                    &path.argument(ArgRef::Name(slot.name.clone())),
                    epos,
                    "a later argument is named, but this one has no default to write",
                ))
            }
        }
    }
    if let Some((i, p)) = positional.next() {
        return Err(Error::too_many_parameters(
            &format!("command '{}'", md.name),
            slots.len(),
            i + 1,
            &p.item.to_json().to_string(),
            &p.position(),
        ));
    }
    Ok(out)
}

/// Rejects named arguments that are unknown, injected, or also given by position (N4).
pub(crate) fn check_argument_names(
    action: &ActionElement,
    md: &CommandMetadata,
) -> Result<(), Error> {
    let slots = positional_arguments(md);
    let variadic_at = slots.iter().position(|s| s.multiple);
    for name in action.arguments.keys() {
        let Some(info) = md.arguments.iter().find(|a| &a.name == name) else {
            return Err(Error::invalid_argument(&md.name, name, "no such argument"));
        };
        if info.injected {
            return Err(Error::invalid_argument(
                &md.name,
                name,
                "injected arguments are supplied by the environment",
            ));
        }
        if let Some(index) = slots.iter().position(|s| &s.name == name) {
            let by_position = match variadic_at {
                Some(v) if index >= v => action.parameters.len() > v,
                Some(_) | None => action.parameters.len() > index,
            };
            if by_position {
                return Err(Error::invalid_argument(
                    &md.name,
                    name,
                    "given both by position and by name",
                ));
            }
        }
    }
    Ok(())
}

impl Pipeline {
    /// Converts the pipeline to a query in canonical form, or reports the first
    /// representability rule that fails (AC-29).
    ///
    /// The registry is required (N1): named arguments become positional in the command's
    /// order, gaps before a named argument take the skipped argument's default, and namespaces
    /// are placed so every action resolves to the command its element names (Q21). An action
    /// the registry cannot resolve is an error.
    pub fn to_query_with(&self, cmr: &CommandMetadataRegistry) -> Result<Query, Error> {
        self.to_query_impl(cmr, false)
    }

    /// As [`Pipeline::to_query_with`], but an action the registry cannot resolve is written as it
    /// stands (its own `ns`, positional parameters only) instead of failing. The query editor
    /// uses it so an action can be named before its namespace is set.
    pub(crate) fn to_query_lenient(&self, cmr: &CommandMetadataRegistry) -> Result<Query, Error> {
        self.to_query_impl(cmr, true)
    }

    fn to_query_impl(&self, cmr: &CommandMetadataRegistry, lenient: bool) -> Result<Query, Error> {
        let mut flat = Flattener::default();
        let mut header = Some(self.header_parameters.clone());
        flat.pipeline(self, true, &ElementPath::default(), &mut header)?;
        if let Some(filename) = &self.filename {
            if !is_filename(filename) {
                return Err(not_representable(
                    "P2",
                    &ElementPath::default(),
                    &None,
                    &format!("'{filename}' is not a filename"),
                ));
            }
        }
        let filename = self.filename.clone().or(flat.filename.take());

        // Group the flattened elements into segments.
        let mut raw: Vec<Raw> = Vec::new();
        for item in flat.items.into_iter() {
            match item {
                Flat::Resource {
                    realm,
                    instruction,
                    key,
                    pos,
                } => raw.push(Raw::Resource(ResourceQuerySegment {
                    header: Some(SegmentHeader {
                        name: realm,
                        level: 0,
                        parameters: instruction
                            .map(|i| vec![HeaderParameter::new(i.to_owned())])
                            .unwrap_or_default(),
                        resource: true,
                        position: pos.unwrap_or_else(Position::unknown),
                    }),
                    key,
                })),
                Flat::Action {
                    ref realm,
                    ref header,
                    ..
                }
                | Flat::Instruction {
                    ref realm,
                    ref header,
                    ..
                } => {
                    let joins = match raw.last() {
                        Some(Raw::Transform(t)) => {
                            &t.realm == realm
                                && match header {
                                    None => true,
                                    Some(h) => h.is_empty() || *h == t.header_parameters,
                                }
                        }
                        Some(Raw::Resource(_)) | None => false,
                    };
                    if !joins {
                        raw.push(Raw::Transform(RawTransform {
                            realm: realm.clone(),
                            header_parameters: header.clone().unwrap_or_default(),
                            items: Vec::new(),
                        }));
                    }
                    if let Some(Raw::Transform(t)) = raw.last_mut() {
                        t.items.push(item);
                    }
                }
            }
        }

        // A headerless first segment followed by others can read back as a resource shorthand
        // (`a/b/-/c`), so the header is omitted only when the query has one segment.
        let single_segment = raw.len() == 1
            && (filename.is_none() || matches!(raw.first(), Some(Raw::Transform(_))));
        let mut segments = Vec::with_capacity(raw.len() + 1);
        for (index, segment) in raw.into_iter().enumerate() {
            match segment {
                Raw::Resource(r) => segments.push(QuerySegment::Resource(r)),
                Raw::Transform(t) => {
                    let header = if index == 0
                        && single_segment
                        && t.realm.is_empty()
                        && t.header_parameters.is_empty()
                    {
                        None
                    } else {
                        Some(SegmentHeader {
                            name: t.realm.clone(),
                            level: 0,
                            parameters: t
                                .header_parameters
                                .iter()
                                .map(|p| HeaderParameter::new(p.clone()))
                                .collect(),
                            resource: false,
                            position: Position::unknown(),
                        })
                    };
                    let query = write_transform(&t, cmr, lenient)?;
                    segments.push(QuerySegment::Transform(TransformQuerySegment {
                        header,
                        query,
                        filename: None,
                    }));
                }
            }
        }
        if let Some(filename) = filename {
            let name = ResourceName::new(filename);
            match segments.last_mut() {
                Some(QuerySegment::Transform(t)) => t.filename = Some(name),
                Some(QuerySegment::Resource(_)) | None => {
                    let header = if segments.is_empty() {
                        None
                    } else {
                        Some(SegmentHeader {
                            name: self.realm.clone(),
                            ..SegmentHeader::new()
                        })
                    };
                    segments.push(QuerySegment::Transform(TransformQuerySegment {
                        header,
                        query: vec![],
                        filename: Some(name),
                    }))
                }
            }
        }
        let query = Query {
            segments,
            rooted: self.rooted || flat.rooted,
            source: Default::default(),
        };
        query.check_encodable()?;
        Ok(query)
    }
}

/// Places namespaces in one transform segment (Q21) and writes its actions.
fn write_transform(
    t: &RawTransform,
    cmr: &CommandMetadataRegistry,
    lenient: bool,
) -> Result<Vec<ActionRequest>, Error> {
    // Resolve every action to its target command first.
    let mut targets: Vec<Option<(CommandKey, CommandMetadata)>> = Vec::with_capacity(t.items.len());
    for item in t.items.iter() {
        match item {
            Flat::Action {
                element, pos, path, ..
            } => {
                if !is_identifier(&element.action)
                    || [NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION]
                        .contains(&element.action.as_str())
                {
                    return Err(not_representable(
                        "E1",
                        path,
                        pos,
                        &format!("'{}' is not an action name", element.action),
                    ));
                }
                if let Some(ns) = &element.ns {
                    for n in ns.0.iter() {
                        if !(n.is_empty() || is_identifier(n)) {
                            return Err(not_representable(
                                "E2",
                                path,
                                pos,
                                &format!("'{n}' is not a namespace"),
                            ));
                        }
                    }
                }
                match resolve_action(cmr, &t.realm, element) {
                    Ok(md) => targets.push(Some((md.key(), md))),
                    Err(_) if lenient => {
                        if !element.arguments.is_empty() {
                            return Err(not_representable(
                                "N1",
                                path,
                                pos,
                                &format!(
                                    "'{}' is not resolved, so its named arguments cannot be placed",
                                    element.action
                                ),
                            ));
                        }
                        targets.push(None)
                    }
                    Err(e) => {
                        return Err(match pos {
                            Some(p) => e.with_position(p),
                            None => e,
                        })
                    }
                }
            }
            Flat::Instruction { .. } | Flat::Resource { .. } => targets.push(None),
        }
    }
    let resolves = |explicit: &[String], index: usize| -> bool {
        match (&t.items[index], &targets[index]) {
            (Flat::Action { element, .. }, Some((key, _))) => cmr
                .find_command_in_namespaces(
                    &t.realm,
                    &cmr.lookup_namespaces(explicit),
                    &element.action,
                )
                .map(|md| &md.key() == key)
                .unwrap_or(false),
            // An unresolved action (lenient mode) needs exactly its own `ns`, if it names one.
            (Flat::Action { element, .. }, None) => match &element.ns {
                Some(ns) => explicit == ns.0.as_slice(),
                None => true,
            },
            (Flat::Instruction { .. }, _) | (Flat::Resource { .. }, _) => true,
        }
    };
    // A `q` quotes everything before it, so no `ns` may move across one: namespaces are placed
    // in each run of items between `q`s, starting from the `ns` in effect after the previous run.
    let mut placements: Vec<(usize, Vec<String>)> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < t.items.len() {
        let end = (start..t.items.len())
            .find(|&i| matches!(&t.items[i], Flat::Instruction { name, .. } if *name == Q_INSTRUCTION))
            .map(|i| i + 1)
            .unwrap_or(t.items.len());
        let all = |explicit: &[String]| (start..end).all(|i| resolves(explicit, i));
        if !all(&current) {
            let mut union: Vec<String> = Vec::new();
            for item in t.items[start..end].iter() {
                if let Flat::Action {
                    element: ActionElement { ns: Some(ns), .. },
                    ..
                } = item
                {
                    for n in ns.0.iter() {
                        if !union.contains(n) {
                            union.push(n.clone());
                        }
                    }
                }
            }
            if !union.is_empty() && all(&union) {
                placements.push((start, union.clone()));
                current = union;
            } else {
                for index in start..end {
                    if resolves(&current, index) {
                        continue;
                    }
                    let names = match (&t.items[index], &targets[index]) {
                        (
                            Flat::Action {
                                element: ActionElement { ns: Some(ns), .. },
                                ..
                            },
                            _,
                        ) => ns.0.clone(),
                        (Flat::Action { .. }, Some((key, _))) => {
                            vec![if key.namespace.is_empty() {
                                "root".to_owned()
                            } else {
                                key.namespace.clone()
                            }]
                        }
                        (Flat::Action { .. }, None)
                        | (Flat::Instruction { .. }, _)
                        | (Flat::Resource { .. }, _) => vec![],
                    };
                    placements.push((index, names.clone()));
                    current = names;
                }
            }
        }
        start = end;
    }

    let mut out = Vec::with_capacity(t.items.len() + placements.len());
    let mut next_placement = placements.into_iter().peekable();
    for (index, item) in t.items.iter().enumerate() {
        while let Some((at, _)) = next_placement.peek() {
            if *at != index {
                break;
            }
            if let Some((_, names)) = next_placement.next() {
                out.push(
                    ActionRequest::new(NS_INSTRUCTION.to_owned()).with_parameters(
                        names.into_iter().map(ActionParameter::new_string).collect(),
                    ),
                );
            }
        }
        match (item, &targets[index]) {
            (
                Flat::Action {
                    element, pos, path, ..
                },
                Some((_, md)),
            ) => {
                let parameters = encode_arguments(element, md, cmr, path, pos)?;
                out.push(ActionRequest {
                    name: element.action.clone(),
                    parameters,
                    position: pos.clone().unwrap_or_else(Position::unknown),
                });
            }
            (
                Flat::Action {
                    element, pos, path, ..
                },
                None,
            ) => {
                let parameters = element
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        single_parameter(
                            &p.item,
                            &p.position(),
                            cmr,
                            &path.argument(ArgRef::Position(i)),
                            pos,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                out.push(ActionRequest {
                    name: element.action.clone(),
                    parameters,
                    position: pos.clone().unwrap_or_else(Position::unknown),
                });
            }
            (Flat::Instruction { name, pos, .. }, _) => out.push(ActionRequest {
                name: (*name).to_owned(),
                parameters: vec![],
                position: pos.clone().unwrap_or_else(Position::unknown),
            }),
            (Flat::Resource { .. }, _) => {}
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Recipe → pipeline
// ---------------------------------------------------------------------------------------------

impl Recipe {
    /// The recipe as one pipeline (AC-30): `cwd` as a leading element, then `volatile`, then the
    /// query's elements. `arguments` and `links` become arguments of the query's last action:
    /// a positional parameter for the same argument is replaced, as a recipe override replaces
    /// it, which is why the registry is needed. The filename comes from the query. `title`,
    /// `description` and the asset declarations stay on the recipe.
    pub fn to_pipeline(&self, cmr: &CommandMetadataRegistry) -> Result<Pipeline, Error> {
        let mut pipeline = match self.combined_pipeline()? {
            Some(combined) => combined,
            None => Pipeline::try_from(&self.get_query()?)?,
        };
        if !self.arguments.is_empty() || !self.links.is_empty() {
            let (realm, action) = last_action_mut(&mut pipeline).ok_or_else(|| {
                Error::invalid_pipeline(
                    "recipe arguments or links need a query that ends with an action",
                )
            })?;
            let md = resolve_action(cmr, &realm, action)?;
            let slots: Vec<String> = positional_arguments(&md)
                .iter()
                .map(|a| a.name.clone())
                .collect();
            let mut overrides: Vec<(String, Source)> = self
                .arguments
                .iter()
                .map(|(k, v)| (k.clone(), Source::Value(v.clone())))
                .collect();
            overrides.extend(
                self.links
                    .iter()
                    .map(|(k, v)| (k.clone(), Source::Link(v.clone()))),
            );
            overrides.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, source) in overrides {
                match slots.iter().position(|s| s == &name) {
                    Some(i) if i < action.parameters.len() => {
                        let pos = action.parameters[i].pos.clone();
                        action.parameters[i] = Located { item: source, pos };
                    }
                    Some(_) | None => {
                        action.arguments.insert(name, Located::new(source));
                    }
                }
            }
        }
        let mut prefix: Vec<PipelineElement> = Vec::new();
        if let Some(cwd) = &self.cwd {
            prefix.push(Located::new(ElementKind::Cwd(Operand::Literal(
                cwd.clone(),
            ))));
        }
        if self.volatile {
            prefix.push(Located::new(ElementKind::Volatile));
        }
        if !prefix.is_empty() {
            prefix.append(&mut pipeline.elements);
            pipeline.elements = prefix;
        }
        Ok(pipeline)
    }
}

/// The last action element and the realm it runs in, nested pipelines included.
fn last_action_mut(p: &mut Pipeline) -> Option<(String, &mut ActionElement)> {
    let own_realm = p.realm.clone();
    for element in p.elements.iter_mut().rev() {
        match &mut element.item {
            ElementKind::Action(a) => return Some((own_realm, a)),
            ElementKind::Pipeline(np) => return last_action_mut(np),
            ElementKind::Text(_)
            | ElementKind::Value(_)
            | ElementKind::Resource { .. }
            | ElementKind::KeyValue(_)
            | ElementKind::Cwd(_)
            | ElementKind::Evaluate(_)
            | ElementKind::QueryValue(_)
            | ElementKind::Quote
            | ElementKind::Volatile => return None,
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests;
