//! Filling a query template's placeholders from the context stack.
//!
//! A value is filled into the AST, never into text, so it cannot change the query's structure. In
//! a position without an escape form (an action name, a key segment, a filename) the filled token
//! must be valid there, or it is a positioned error. A non-scalar value is a parameter override,
//! and only as a whole parameter of the last action. See
//! `specs/design/liquers-mustache/query-placeholders.md`.

use std::collections::HashMap;

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::data_node::{AnyNode, NodeKind, Scalar};
use liquers_core::error::Error;
use liquers_core::escape::is_resource_name_char;
use liquers_core::query::{ActionParameter, Query, QuerySegment};

use crate::name::{kind_name, resolve, Resolved};
use crate::query::ast::{
    whole_slot, ActionParameterTemplate, Piece, QuerySegmentTemplate, QueryTemplate, Slot,
};
use crate::stack::ContextStack;

/// An instantiated query, with the values that fill its overridden parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Instantiated {
    pub query: Query,
    /// Argument name and value, for `Recipe::arguments`.
    pub overrides: Vec<(String, serde_json::Value)>,
}

/// A slot's value, resolved before the query is built.
enum Filled {
    Scalar(Scalar<'static>),
    /// A list or map, with its JSON, which an override passes on.
    Node(AnyNode, serde_json::Value),
}

fn slot_id(slot: &Slot) -> usize {
    std::ptr::from_ref(slot) as usize
}

/// Where a token stands, which decides what a filled value must look like.
#[derive(Clone, Copy)]
enum Place {
    Parameter,
    Namespace,
    ActionName,
    KeySegment,
    Filename,
}

impl Place {
    fn describe(self) -> &'static str {
        match self {
            Place::Parameter => "an action parameter",
            Place::Namespace => "a namespace (an identifier)",
            Place::ActionName => "an action name (an identifier: letters, digits and '_', not starting with a digit)",
            Place::KeySegment => "a key segment (letters, digits, '_', '.' and an inner '-'; not '.' or '..')",
            Place::Filename => "a filename (a stem, '.' and an extension)",
        }
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        Some(_) | None => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_key_segment(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if is_resource_name_char(c) => {}
        Some(_) | None => return false,
    }
    text != "." && text != ".." && chars.all(|c| is_resource_name_char(c) || c == '-')
}

fn is_filename(text: &str) -> bool {
    let Some((stem, extension)) = text.split_once('.') else {
        return false;
    };
    stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !extension.is_empty()
        && extension
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

fn valid(place: Place, text: &str) -> bool {
    match place {
        Place::Parameter => true,
        Place::Namespace | Place::ActionName => is_identifier(text),
        Place::KeySegment => is_key_segment(text),
        Place::Filename => is_filename(text),
    }
}

struct Builder<'v> {
    values: &'v HashMap<usize, Filled>,
}

impl Builder<'_> {
    fn scalar_text(&self, slot: &Slot) -> Result<String, Error> {
        match self.values.get(&slot_id(slot)) {
            Some(Filled::Scalar(Scalar::Null)) | None => Err(Error::general_error(format!(
                "{{{{{}}}}} is missing or null, which a query cannot take",
                slot.name
            ))
            .with_position(&slot.position)),
            Some(Filled::Scalar(s)) => Ok(s.to_text().into_owned()),
            Some(Filled::Node(n, _)) => Err(Error::not_supported(format!(
                "{{{{{}}}}} is a {}; a list or a map can only fill a whole parameter of the last action",
                slot.name,
                kind_name(n.kind())
            ))
            .with_position(&slot.position)),
        }
    }

    fn token(&self, pieces: &[Piece], place: Place) -> Result<String, Error> {
        let mut out = String::new();
        for piece in pieces {
            match piece {
                Piece::Literal(t) => out.push_str(t),
                Piece::Slot(slot) => out.push_str(&self.scalar_text(slot)?),
            }
        }
        if !valid(place, &out) {
            if let Some(slot) = pieces.iter().find_map(|p| match p {
                Piece::Slot(s) => Some(s),
                Piece::Literal(_) => None,
            }) {
                return Err(Error::conversion_error_at_position(
                    format!("{out:?}"),
                    place.describe(),
                    &slot.position,
                ));
            }
        }
        Ok(out)
    }

    /// Builds the query. `overridable` marks the last action, whose whole-slot parameters may take
    /// a non-scalar; each such parameter is reported as (parameter index, slot, value).
    fn query(
        &self,
        template: &QueryTemplate,
        last: Option<(usize, usize)>,
        overrides: &mut Vec<(usize, Slot, serde_json::Value)>,
    ) -> Result<Query, Error> {
        let mut query = template.build(&mut NoSlots)?; // shape only; every token is replaced below
        for (si, (segment, built)) in template.segments.iter().zip(query.segments.iter_mut()).enumerate() {
            match (segment, built) {
                (QuerySegmentTemplate::Resource(r), QuerySegment::Resource(b)) => {
                    for (token, name) in r.key.0.iter().zip(b.key.0.iter_mut()) {
                        name.name = self.token(&token.pieces, Place::KeySegment)?;
                    }
                }
                (QuerySegmentTemplate::Transform(t), QuerySegment::Transform(b)) => {
                    for (ai, (action, built)) in t.query.iter().zip(b.query.iter_mut()).enumerate() {
                        built.name = self.token(&action.name.pieces, Place::ActionName)?;
                        let is_ns = built.name == "ns";
                        let is_last = last == Some((si, ai));
                        for (pi, (parameter, out)) in
                            action.parameters.iter().zip(built.parameters.iter_mut()).enumerate()
                        {
                            match (parameter, out) {
                                (ActionParameterTemplate::String(pieces, _), ActionParameter::String(text, _)) => {
                                    if let Some(slot) = whole_slot(pieces) {
                                        if let Some(Filled::Node(node, json)) = self.values.get(&slot_id(slot)) {
                                            if !is_last {
                                                return Err(Error::not_supported(format!(
                                                    "{{{{{}}}}} is a {}, which overrides a parameter; \
                                                     overrides reach only the last action of a query",
                                                    slot.name,
                                                    kind_name(node.kind())
                                                ))
                                                .with_position(&slot.position));
                                            }
                                            overrides.push((pi, slot.clone(), json.clone()));
                                            *text = String::new();
                                            continue;
                                        }
                                    }
                                    let place = if is_ns { Place::Namespace } else { Place::Parameter };
                                    *text = self.token(pieces, place)?;
                                }
                                (ActionParameterTemplate::Link(link, _), ActionParameter::Link(q, _)) => {
                                    *q = self.query(link, None, overrides)?;
                                }
                                (ActionParameterTemplate::String(..), ActionParameter::Link(..))
                                | (ActionParameterTemplate::Link(..), ActionParameter::String(..)) => {
                                    return Err(Error::unexpected_error(
                                        "query template and query differ in shape".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    if let (Some(f), Some(name)) = (&t.filename, b.filename.as_mut()) {
                        name.name = self.token(&f.pieces, Place::Filename)?;
                    }
                }
                (QuerySegmentTemplate::Resource(_), QuerySegment::Transform(_))
                | (QuerySegmentTemplate::Transform(_), QuerySegment::Resource(_)) => {
                    return Err(Error::unexpected_error(
                        "query template and query differ in shape".to_string(),
                    ));
                }
            }
        }
        Ok(query)
    }
}

struct NoSlots;
impl crate::query::ast::SlotFill for NoSlots {
    fn fill(&mut self, _slot: &Slot) -> Result<String, Error> {
        Ok(String::new())
    }
}

/// The last action of a query: (segment index, action index), when the last segment is a transform.
fn last_action(template: &QueryTemplate) -> Option<(usize, usize)> {
    let si = template.segments.len().checked_sub(1)?;
    match &template.segments[si] {
        QuerySegmentTemplate::Transform(t) => t.query.len().checked_sub(1).map(|ai| (si, ai)),
        QuerySegmentTemplate::Resource(_) => None,
    }
}

/// The name of the argument that parameter `index` of the last action stands for.
fn argument_name(
    query: &Query,
    cmr: &CommandMetadataRegistry,
    index: usize,
    slot: &Slot,
) -> Result<String, Error> {
    let not_found = |what: String| Error::general_error(what).with_position(&slot.position);
    let Some(QuerySegment::Transform(segment)) = query.segments.last() else {
        return Err(not_found("an override needs a final action".to_string()));
    };
    let Some(action) = segment.query.last() else {
        return Err(not_found("an override needs a final action".to_string()));
    };
    let realm = segment.name();
    let mut explicit = Vec::new();
    if let Some(ns) = query.last_ns() {
        for parameter in ns {
            match parameter {
                ActionParameter::String(s, _) => explicit.push(s),
                ActionParameter::Link(..) => {}
            }
        }
    }
    let namespaces = cmr.lookup_namespaces(&explicit);
    let command = cmr
        .find_command_in_namespaces(&realm, &namespaces, &action.name)
        .ok_or_else(|| {
            not_found(format!(
                "{{{{{}}}}} overrides a parameter of '{}', which is not a registered command, so the \
                 argument cannot be named",
                slot.name, action.name
            ))
        })?;
    let mut position = 0;
    for argument in command.arguments.iter().filter(|a| !a.injected) {
        if argument.multiple || position == index {
            return Ok(argument.name.clone());
        }
        position += 1;
    }
    Err(not_found(format!(
        "{{{{{}}}}} is parameter {} of '{}', which declares {} arguments",
        slot.name,
        index + 1,
        action.name,
        position
    )))
}

impl QueryTemplate {
    /// Fills every placeholder from `data`. `cmr` names the arguments that non-scalar values
    /// override.
    pub async fn instantiate(
        &self,
        data: &ContextStack<'_>,
        cmr: &CommandMetadataRegistry,
    ) -> Result<Instantiated, Error> {
        let mut values = HashMap::new();
        for slot in self.slots() {
            let resolved = resolve(&slot.name, data).await.map_err(|e| {
                if e.position.is_unknown() {
                    e.with_position(&slot.position)
                } else {
                    e
                }
            })?;
            let scalar = match &resolved {
                Resolved::Missing => Some(Scalar::Null),
                Resolved::Node(n) => n.node().scalar().map(|s| s.into_owned()),
                Resolved::Any(AnyNode::Sync(n)) => n.scalar().map(|s| s.into_owned()),
                Resolved::Any(AnyNode::Async(n)) => {
                    if n.kind().is_scalar() {
                        Some(n.scalar().await?.unwrap_or(Scalar::Null))
                    } else {
                        None
                    }
                }
            };
            let filled = match scalar {
                Some(scalar) => Filled::Scalar(scalar),
                None => {
                    let node = resolved.to_any();
                    match node.kind() {
                    NodeKind::Opaque => {
                        return Err(Error::not_supported(format!(
                            "{{{{{}}}}} has no data view and cannot fill a query",
                            slot.name
                        ))
                        .with_position(&slot.position))
                    }
                    NodeKind::Null
                    | NodeKind::Bool
                    | NodeKind::Integer
                    | NodeKind::Float
                    | NodeKind::Text
                    | NodeKind::List
                    | NodeKind::Map => {
                        let json = node.to_json().await?;
                        Filled::Node(node, json)
                    }
                    }
                }
            };
            values.insert(slot_id(slot), filled);
        }
        let builder = Builder { values: &values };
        let mut found = Vec::new();
        let mut query = builder.query(self, last_action(self), &mut found)?;
        let mut overrides = Vec::with_capacity(found.len());
        for (index, slot, json) in &found {
            overrides.push((argument_name(&query, cmr, *index, slot)?, json.clone()));
        }
        // An overridden parameter at the end is left out; one before a written parameter keeps
        // its place as an empty value, which the override replaces.
        if !found.is_empty() {
            if let Some(QuerySegment::Transform(segment)) = query.segments.last_mut() {
                if let Some(action) = segment.query.last_mut() {
                    let overridden: Vec<usize> = found.iter().map(|(i, _, _)| *i).collect();
                    while action
                        .parameters
                        .len()
                        .checked_sub(1)
                        .is_some_and(|last| overridden.contains(&last))
                    {
                        action.parameters.pop();
                    }
                }
            }
        }
        Ok(Instantiated { query, overrides })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse::parse_query_template;
    use crate::stack::FrameKind;
    use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata};
    use serde_json::json;

    fn stack(data: serde_json::Value) -> ContextStack<'static> {
        let mut stack = ContextStack::new();
        stack.push(AnyNode::from_json(data), FrameKind::Data);
        stack
    }

    fn cmr() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("cmd");
        cmd.arguments = vec![ArgumentInfo::any_argument("arg")];
        cmr.add_command(&cmd);
        cmr
    }

    fn fill(template: &str, data: serde_json::Value) -> Result<Instantiated, Error> {
        let t = parse_query_template(template)?;
        futures::executor::block_on(t.instantiate(&stack(data), &cmr()))
    }

    #[test]
    fn hygienic_parameter_is_one_escaped_token() {
        let i = fill("cmd-{{x}}/q", json!({"x": "a-b"})).expect("query");
        assert_eq!(i.query.encode(), "cmd-a~_b/q");
        assert!(i.overrides.is_empty());
        let i = fill("cmd-{{n}}-{{b}}", json!({"n": 42, "b": true})).expect("query");
        assert_eq!(i.query.encode(), "cmd-42-true");
    }

    #[test]
    fn hygienic_fragment_in_parameter() {
        let i = fill(
            "-R-key/specs/issues/-/search-area~ncolon~{{area}}~.~_status",
            json!({"area": "core/store"}),
        )
        .expect("query");
        assert_eq!(i.query.encode(), "-R-key/specs/issues/-/search-area~ncolon~core~/store~.~_status");
    }

    #[test]
    fn hygienic_slot_inside_link() {
        let i = fill("cmd-~X~-R/data/{{key}}~E", json!({"key": "x.csv"})).expect("query");
        assert_eq!(i.query.encode(), "cmd-~X~-R/data/x.csv~E");
    }

    #[test]
    fn hygienic_value_must_fit_its_position() {
        let err = fill("{{op}}-1", json!({"op": "a.b"})).expect_err("bad action name");
        assert!(err.message.contains("action name"), "{}", err.message);
        assert_eq!(err.position.column, 1);
        let err = fill("-R/data/{{name}}", json!({"name": "a/b"})).expect_err("slash in key");
        assert!(err.message.contains("key segment"), "{}", err.message);
        assert!(fill("-R/data/{{name}}", json!({"name": ".."})).is_err());
        let err = fill("cmd/out.{{ext}}", json!({"ext": "a b"})).expect_err("filename");
        assert!(err.message.contains("filename"), "{}", err.message);
        let err = fill("ns-{{lib}}/cmd", json!({"lib": "a-b"})).expect_err("namespace");
        assert!(err.message.contains("namespace"), "{}", err.message);
        let err = fill("cmd-{{missing}}", json!({})).expect_err("missing");
        assert!(err.message.contains("missing or null"), "{}", err.message);
        assert_eq!(fill("to_{{fmt}}", json!({"fmt": "html"})).expect("q").query.encode(), "to_html");
        assert_eq!(
            fill("-R/data/{{stem}}.csv", json!({"stem": "a_1"})).expect("q").query.encode(),
            "-R/data/a_1.csv"
        );
    }

    #[test]
    fn non_scalar_last_action_becomes_override() {
        let i = fill("cmd-{{tags}}", json!({"tags": ["p", "q"]})).expect("override");
        assert_eq!(i.query.encode(), "cmd");
        assert_eq!(i.overrides, vec![("arg".to_string(), json!(["p", "q"]))]);
    }

    #[test]
    fn non_scalar_earlier_action_is_error() {
        let err = fill("cmd-{{tags}}/q", json!({"tags": ["p", "q"]})).expect_err("not last");
        assert!(err.message.contains("only the last action"), "{}", err.message);
        assert_eq!(err.position.column, 5);
        let err = fill("cmd-a{{tags}}", json!({"tags": ["p"]})).expect_err("fragment");
        assert!(err.message.contains("whole parameter"), "{}", err.message);
    }

    #[test]
    fn bracket_in_query_slot() {
        let data = json!({"items": [{"title": "a"}, {"title": "b"}], "i": 1});
        assert_eq!(fill("cmd-{{items[i].title}}", data.clone()).expect("q").query.encode(), "cmd-b");
        let err = fill("cmd-{{items[k].title}}", json!({"items": [], "k": 3})).expect_err("range");
        assert!(err.position.column > 5, "{:?}", err.position);
    }
}
