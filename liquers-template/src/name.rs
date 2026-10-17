//! Names: `a.b`, `items.0`, `x[y]`, `.`; their parser, encoder and resolution.
//!
//! Dotted parts follow Mustache: a missing part makes the whole name missing. A bracket is a strict
//! lookup: its key is resolved first, as a whole name from the top of the stack, and an absent
//! element is an error. See `specs/design/liquers-mustache/names-and-errors.md`.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use liquers_core::data_node::{AnyNode, AsyncDataNode, DataNode, NodeKind, NodeRef, Scalar};
use liquers_core::error::Error;
use liquers_core::maybe_send::{BoxFuture, MaybeBoxed};
use liquers_core::query::Position;
use serde::{Deserialize, Serialize};

use crate::advance;
use crate::stack::{ContextStack, FrameView};

/// A name; no parts is `.`, the current context.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Name {
    pub parts: Vec<NamePart>,
    pub position: Position,
}

/// One part of a name.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum NamePart {
    /// `a` in `a.b`.
    Field(String, Position),
    /// `0` in `items.0`.
    Index(usize, Position),
    /// `[y]`.
    Lookup(LookupKey, Position),
}

/// The key inside `[…]`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LookupKey {
    /// `[y]`: a name, resolved from the top of the context stack.
    Name(Name),
    /// `[0]`, `[-1]`; a negative index counts from the end.
    Integer(i64),
    /// `["a b"]`.
    Text(String),
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.parts == other.parts
    }
}
impl Eq for Name {}
impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.parts.hash(state);
    }
}

impl PartialEq for NamePart {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (NamePart::Field(a, _), NamePart::Field(b, _)) => a == b,
            (NamePart::Index(a, _), NamePart::Index(b, _)) => a == b,
            (NamePart::Lookup(a, _), NamePart::Lookup(b, _)) => a == b,
            (NamePart::Field(..), _) | (NamePart::Index(..), _) | (NamePart::Lookup(..), _) => false,
        }
    }
}
impl Eq for NamePart {}
impl Hash for NamePart {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            NamePart::Field(f, _) => {
                0u8.hash(state);
                f.hash(state);
            }
            NamePart::Index(i, _) => {
                1u8.hash(state);
                i.hash(state);
            }
            NamePart::Lookup(k, _) => {
                2u8.hash(state);
                k.hash(state);
            }
        }
    }
}

impl PartialEq for LookupKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LookupKey::Name(a), LookupKey::Name(b)) => a == b,
            (LookupKey::Integer(a), LookupKey::Integer(b)) => a == b,
            (LookupKey::Text(a), LookupKey::Text(b)) => a == b,
            (LookupKey::Name(_), _) | (LookupKey::Integer(_), _) | (LookupKey::Text(_), _) => false,
        }
    }
}
impl Eq for LookupKey {}
impl Hash for LookupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LookupKey::Name(n) => {
                0u8.hash(state);
                n.hash(state);
            }
            LookupKey::Integer(i) => {
                1u8.hash(state);
                i.hash(state);
            }
            LookupKey::Text(t) => {
                2u8.hash(state);
                t.hash(state);
            }
        }
    }
}

impl Name {
    /// `.`, the current context.
    pub fn current() -> Self {
        Name {
            parts: Vec::new(),
            position: Position::unknown(),
        }
    }

    /// A one-field name, as built by hand.
    pub fn field(name: &str) -> Self {
        Name {
            parts: vec![NamePart::Field(name.to_string(), Position::unknown())],
            position: Position::unknown(),
        }
    }

    pub fn with_position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }

    /// Whether this is `.`.
    pub fn is_current(&self) -> bool {
        self.parts.is_empty()
    }

    /// The canonical text.
    pub fn encode(&self) -> String {
        if self.parts.is_empty() {
            return ".".to_string();
        }
        let mut out = String::new();
        for (i, part) in self.parts.iter().enumerate() {
            match part {
                NamePart::Field(f, _) => {
                    if i > 0 {
                        out.push('.');
                    }
                    out.push_str(f);
                }
                NamePart::Index(n, _) => {
                    out.push('.');
                    out.push_str(&n.to_string());
                }
                NamePart::Lookup(key, _) => {
                    out.push('[');
                    out.push_str(&key.encode());
                    out.push(']');
                }
            }
        }
        out
    }
}

impl LookupKey {
    pub fn encode(&self) -> String {
        match self {
            LookupKey::Name(n) => n.encode(),
            LookupKey::Integer(i) => i.to_string(),
            LookupKey::Text(t) => {
                let mut out = String::from("\"");
                for c in t.chars() {
                    match c {
                        '"' | '\\' => {
                            out.push('\\');
                            out.push(c);
                        }
                        other => out.push(other),
                    }
                }
                out.push('"');
                out
            }
        }
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.encode())
    }
}

fn is_field_char(c: char) -> bool {
    !(c.is_whitespace() || c == '.' || c == '[' || c == ']')
}

struct Cursor<'t> {
    text: &'t str,
    at: usize,
    position: Position,
}

impl<'t> Cursor<'t> {
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        let len = c.len_utf8();
        self.position = advance(&self.position, &self.text[self.at..self.at + len]);
        self.at += len;
        Some(c)
    }
    fn error(&self, message: &str) -> Error {
        Error::parse_error(message.to_string()).with_position(&self.position)
    }
    fn take_field(&mut self) -> String {
        let start = self.at;
        while let Some(c) = self.peek() {
            if !is_field_char(c) {
                break;
            }
            self.bump();
        }
        self.text[start..self.at].to_string()
    }
}

/// Parses a whole name. `start` is the position of its first character in the template.
pub fn parse_name(text: &str, start: &Position) -> Result<Name, Error> {
    let mut cursor = Cursor {
        text,
        at: 0,
        position: start.clone(),
    };
    if text == "." {
        return Ok(Name::current().with_position(start.clone()));
    }
    let name = parse_name_at(&mut cursor)?;
    if cursor.at < text.len() {
        return Err(cursor.error(&format!("unexpected text in name '{text}'")));
    }
    Ok(name)
}

fn parse_name_at(cursor: &mut Cursor<'_>) -> Result<Name, Error> {
    let position = cursor.position.clone();
    if cursor.peek() == Some('.') {
        // `.` alone, inside brackets
        let mut probe = Cursor {
            text: cursor.text,
            at: cursor.at,
            position: cursor.position.clone(),
        };
        probe.bump();
        if probe.peek().is_none() || probe.peek() == Some(']') {
            cursor.bump();
            return Ok(Name::current().with_position(position));
        }
    }
    let head = cursor.take_field();
    if head.is_empty() {
        return Err(cursor.error("a name is expected"));
    }
    let mut parts = vec![NamePart::Field(head, position.clone())];
    loop {
        match cursor.peek() {
            Some('.') => {
                cursor.bump();
                let part_position = cursor.position.clone();
                let field = cursor.take_field();
                if field.is_empty() {
                    return Err(cursor.error("a name part is expected after '.'"));
                }
                if field.chars().all(|c| c.is_ascii_digit()) {
                    let index = field.parse::<usize>().map_err(|_| {
                        Error::parse_error(format!("index '{field}' is too large"))
                            .with_position(&part_position)
                    })?;
                    parts.push(NamePart::Index(index, part_position));
                } else {
                    parts.push(NamePart::Field(field, part_position));
                }
            }
            Some('[') => {
                let part_position = cursor.position.clone();
                cursor.bump();
                let key = parse_key(cursor)?;
                if cursor.bump() != Some(']') {
                    return Err(cursor.error("']' is expected"));
                }
                parts.push(NamePart::Lookup(key, part_position));
            }
            Some(_) | None => break,
        }
    }
    Ok(Name { parts, position })
}

fn parse_key(cursor: &mut Cursor<'_>) -> Result<LookupKey, Error> {
    match cursor.peek() {
        Some('"') => {
            cursor.bump();
            let mut text = String::new();
            loop {
                match cursor.bump() {
                    Some('"') => return Ok(LookupKey::Text(text)),
                    Some('\\') => match cursor.bump() {
                        Some(c @ ('"' | '\\')) => text.push(c),
                        Some(_) | None => {
                            return Err(cursor.error("only '\\\"' and '\\\\' are escapes in a key"))
                        }
                    },
                    Some(c) => text.push(c),
                    None => return Err(cursor.error("unterminated string key")),
                }
            }
        }
        Some(c) if c == '-' || c.is_ascii_digit() => {
            let start = cursor.at;
            let position = cursor.position.clone();
            cursor.bump();
            while cursor.peek().is_some_and(|c| c.is_ascii_digit()) {
                cursor.bump();
            }
            let digits = &cursor.text[start..cursor.at];
            if cursor.peek().is_some_and(|c| c != ']') {
                return Err(cursor.error("']' is expected after an integer key"));
            }
            digits.parse::<i64>().map(LookupKey::Integer).map_err(|_| {
                Error::parse_error(format!("'{digits}' is not an integer")).with_position(&position)
            })
        }
        Some(_) => parse_name_at(cursor).map(LookupKey::Name),
        None => Err(cursor.error("a key is expected after '['")),
    }
}

/// What a name resolved to.
pub enum Resolved<'a> {
    Missing,
    Node(NodeRef<'a>),
    Any(AnyNode),
}

impl<'a> Resolved<'a> {
    pub fn is_missing(&self) -> bool {
        match self {
            Resolved::Missing => true,
            Resolved::Node(_) | Resolved::Any(_) => false,
        }
    }

    /// The kind, `Null` when missing.
    pub fn kind(&self) -> NodeKind {
        match self {
            Resolved::Missing => NodeKind::Null,
            Resolved::Node(n) => n.node().kind(),
            Resolved::Any(a) => a.kind(),
        }
    }

    /// The value as an owned node, missing as null.
    pub fn to_any(&self) -> AnyNode {
        match self {
            Resolved::Missing => AnyNode::null(),
            Resolved::Node(n) => own(n),
            Resolved::Any(a) => a.clone(),
        }
    }
}

/// An owned copy of a node: shared nodes are shared, scalars copied, anything else snapshotted.
pub(crate) fn own(node: &NodeRef<'_>) -> AnyNode {
    match node {
        NodeRef::Shared(a) => AnyNode::Sync(a.clone()),
        NodeRef::Scalar(s) => AnyNode::Sync(Arc::new(s.clone().into_owned())),
        NodeRef::Borrowed(_) | NodeRef::Owned(_) => match node.node().to_json() {
            Ok(json) => AnyNode::from_json(json),
            Err(_) => AnyNode::opaque(),
        },
    }
}

/// The name of a kind, in messages.
pub fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Null => "null",
        NodeKind::Bool => "boolean",
        NodeKind::Integer => "integer",
        NodeKind::Float => "number",
        NodeKind::Text => "text",
        NodeKind::List => "list",
        NodeKind::Map => "map",
        NodeKind::Opaque => "value with no data view",
    }
}

enum Key {
    Integer(i64),
    Text(String),
}

impl Key {
    fn text(&self) -> String {
        match self {
            Key::Integer(i) => i.to_string(),
            Key::Text(t) => t.clone(),
        }
    }
}

/// Resolves `name` against `stack`.
pub fn resolve<'s>(
    name: &'s Name,
    stack: &'s ContextStack<'_>,
) -> BoxFuture<'s, Result<Resolved<'s>, Error>> {
    async move {
        let Some((first, rest)) = name.parts.split_first() else {
            return Ok(match stack.top_data().map(|f| f.view()) {
                Some(FrameView::Sync(n)) => Resolved::Node(NodeRef::Borrowed(n)),
                Some(FrameView::Async(a)) => Resolved::Any(AnyNode::Async(a.clone())),
                None => Resolved::Missing,
            });
        };
        let field = match first {
            NamePart::Field(f, _) => f.clone(),
            NamePart::Index(i, _) => i.to_string(),
            NamePart::Lookup(_, position) => {
                return Err(Error::parse_error("a name cannot start with '['".to_string())
                    .with_position(position))
            }
        };
        let mut current = Resolved::Missing;
        for frame in stack.top_down() {
            match frame.view() {
                FrameView::Sync(n) => {
                    if n.kind() == NodeKind::Map {
                        if let Some(child) = n.get(&field) {
                            current = Resolved::Node(child);
                            break;
                        }
                    }
                }
                FrameView::Async(a) => {
                    if a.kind() == NodeKind::Map {
                        if let Some(child) = a.get(&field).await? {
                            current = Resolved::Any(child);
                            break;
                        }
                    }
                }
            }
        }
        for part in rest {
            if current.is_missing() {
                break;
            }
            let key = match part {
                NamePart::Lookup(key, position) => Some(lookup_key(key, stack, position).await?),
                NamePart::Field(..) | NamePart::Index(..) => None,
            };
            current = step(current, part, key).await?;
        }
        Ok(current)
    }
    .maybe_boxed()
}

async fn lookup_key<'s>(
    key: &'s LookupKey,
    stack: &'s ContextStack<'_>,
    position: &Position,
) -> Result<Key, Error> {
    match key {
        LookupKey::Integer(i) => Ok(Key::Integer(*i)),
        LookupKey::Text(t) => Ok(Key::Text(t.clone())),
        LookupKey::Name(name) => {
            let resolved = resolve(name, stack).await?;
            let scalar = match &resolved {
                Resolved::Missing => None,
                Resolved::Node(n) => n.node().scalar().map(|s| s.into_owned()),
                Resolved::Any(AnyNode::Sync(n)) => n.scalar().map(|s| s.into_owned()),
                Resolved::Any(AnyNode::Async(n)) => n.scalar().await?,
            };
            match scalar {
                Some(Scalar::Integer(i)) => Ok(Key::Integer(i)),
                Some(Scalar::Text(t)) => Ok(Key::Text(t.into_owned())),
                Some(other @ (Scalar::Bool(_) | Scalar::Float(_))) => {
                    Ok(Key::Text(other.to_text().into_owned()))
                }
                Some(Scalar::Null) | None if resolved.is_missing() => Err(Error::general_error(
                    format!("the key [{}] is missing", name.encode()),
                )
                .with_position(position)),
                Some(Scalar::Null) | None => Err(Error::general_error(format!(
                    "the key [{}] is a {}, not a scalar",
                    name.encode(),
                    kind_name(resolved.kind())
                ))
                .with_position(position)),
            }
        }
    }
}

fn not_indexable(kind: NodeKind, key: &Key, position: &Position) -> Error {
    Error::general_error(format!(
        "a {} is not indexable by [{}]",
        kind_name(kind),
        key.text()
    ))
    .with_position(position)
}

/// The list index a bracket key selects.
fn list_index(key: &Key, len: Option<usize>, position: &Position) -> Result<usize, Error> {
    let i = match key {
        Key::Integer(i) => *i,
        Key::Text(t) => {
            return Err(Error::general_error(format!(
                "a list is indexed by an integer, not \"{t}\""
            ))
            .with_position(position))
        }
    };
    let out_of_range = |len: usize| {
        Error::general_error(format!("index {i} out of range for a list of {len}"))
            .with_position(position)
    };
    if i >= 0 {
        let index = i as usize;
        match len {
            Some(len) if index >= len => Err(out_of_range(len)),
            Some(_) | None => Ok(index),
        }
    } else {
        let len = len.ok_or_else(|| {
            Error::general_error(
                "a negative index needs the length of the list, which is not known".to_string(),
            )
            .with_position(position)
        })?;
        let back = i.unsigned_abs() as usize;
        if back > len {
            Err(out_of_range(len))
        } else {
            Ok(len - back)
        }
    }
}

fn sync_step<'n>(
    node: &'n dyn DataNode,
    part: &NamePart,
    key: Option<&Key>,
) -> Result<Option<NodeRef<'n>>, Error> {
    let kind = node.kind();
    match (part, key) {
        (NamePart::Field(f, _), _) => Ok(match kind {
            NodeKind::Map => node.get(f),
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::List
            | NodeKind::Opaque => None,
        }),
        (NamePart::Index(i, _), _) => Ok(match kind {
            NodeKind::List => node.index(*i),
            NodeKind::Map => node.get(&i.to_string()),
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::Opaque => None,
        }),
        (NamePart::Lookup(_, position), Some(key)) => match kind {
            NodeKind::List => {
                let i = list_index(key, node.len(), position)?;
                node.index(i).map(Some).ok_or_else(|| {
                    Error::general_error(format!("index {i} out of range")).with_position(position)
                })
            }
            NodeKind::Map => {
                let field = key.text();
                node.get(&field).map(Some).ok_or_else(|| {
                    Error::general_error(format!("no field \"{field}\" in map")).with_position(position)
                })
            }
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::Opaque => Err(not_indexable(kind, key, position)),
        },
        (NamePart::Lookup(_, position), None) => Err(Error::unexpected_error(
            "a lookup without a key".to_string(),
        )
        .with_position(position)),
    }
}

async fn async_step(
    node: &Arc<dyn AsyncDataNode>,
    part: &NamePart,
    key: Option<&Key>,
) -> Result<Option<AnyNode>, Error> {
    let kind = node.kind();
    match (part, key) {
        (NamePart::Field(f, _), _) => match kind {
            NodeKind::Map => node.get(f).await,
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::List
            | NodeKind::Opaque => Ok(None),
        },
        (NamePart::Index(i, _), _) => match kind {
            NodeKind::List => node.index(*i).await,
            NodeKind::Map => node.get(&i.to_string()).await,
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::Opaque => Ok(None),
        },
        (NamePart::Lookup(_, position), Some(key)) => match kind {
            NodeKind::List => {
                let len = node.len().await?;
                let i = list_index(key, len, position)?;
                match node.index(i).await? {
                    Some(item) => Ok(Some(item)),
                    None => Err(Error::general_error(format!("index {i} out of range"))
                        .with_position(position)),
                }
            }
            NodeKind::Map => {
                let field = key.text();
                match node.get(&field).await? {
                    Some(child) => Ok(Some(child)),
                    None => Err(Error::general_error(format!("no field \"{field}\" in map"))
                        .with_position(position)),
                }
            }
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::Opaque => Err(not_indexable(kind, key, position)),
        },
        (NamePart::Lookup(_, position), None) => Err(Error::unexpected_error(
            "a lookup without a key".to_string(),
        )
        .with_position(position)),
    }
}

async fn step<'s>(
    current: Resolved<'s>,
    part: &NamePart,
    key: Option<Key>,
) -> Result<Resolved<'s>, Error> {
    let key = key.as_ref();
    match current {
        Resolved::Missing => Ok(Resolved::Missing),
        Resolved::Node(NodeRef::Borrowed(n)) => Ok(match sync_step(n, part, key)? {
            Some(child) => Resolved::Node(child),
            None => Resolved::Missing,
        }),
        Resolved::Node(other) => Ok(match sync_step(other.node(), part, key)? {
            Some(child) => Resolved::Any(own(&child)),
            None => Resolved::Missing,
        }),
        Resolved::Any(AnyNode::Sync(n)) => Ok(match sync_step(n.as_ref(), part, key)? {
            Some(child) => Resolved::Any(own(&child)),
            None => Resolved::Missing,
        }),
        Resolved::Any(AnyNode::Async(n)) => Ok(match async_step(&n, part, key).await? {
            Some(child) => Resolved::Any(child),
            None => Resolved::Missing,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::FrameKind;
    use serde_json::json;

    fn start() -> Position {
        Position::new(0, 1, 1)
    }

    fn stack(data: serde_json::Value) -> ContextStack<'static> {
        let mut stack = ContextStack::new();
        stack.push(AnyNode::from_json(data), FrameKind::Data);
        stack
    }

    fn text_of(name: &str, data: serde_json::Value) -> Result<Option<String>, Error> {
        let name = parse_name(name, &start())?;
        let stack = stack(data);
        let resolved = futures::executor::block_on(resolve(&name, &stack))?;
        Ok(match resolved {
            Resolved::Missing => None,
            other => futures::executor::block_on(other.to_any().to_json())
                .ok()
                .map(|j| match j {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                }),
        })
    }

    fn data() -> serde_json::Value {
        json!({"items": [{"title": "first"}, {"title": "second"}], "i": 1, "m": {"a b": 7, "1": "one"}, "k": "a b"})
    }

    #[test]
    fn parse_and_encode() {
        for text in [".", "a", "a.b", "items.0.title", "x[y]", "x[-1]", "x[\"a \\\"b\\\"\"]", "x[y[0]].z", "x[.]"] {
            let name = parse_name(text, &start()).expect(text);
            assert_eq!(name.encode(), text);
            assert_eq!(parse_name(&name.encode(), &start()).expect("again"), name);
        }
        for bad in ["", "a..b", "a.", "a b", "x[", "x[1", "x[]", "[a]", "x[\"a]"] {
            assert!(parse_name(bad, &start()).is_err(), "{bad} must fail");
        }
        let name = parse_name("items[i]", &Position::new(10, 2, 5)).expect("name");
        match &name.parts[1] {
            NamePart::Lookup(_, position) => assert_eq!((position.line, position.column), (2, 10)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dotted_index_selects_item() {
        assert_eq!(text_of("items.0.title", data()).ok().flatten().as_deref(), Some("first"));
        assert_eq!(text_of("m.1", data()).ok().flatten().as_deref(), Some("one"));
    }

    #[test]
    fn dotted_missing_is_empty() {
        assert_eq!(text_of("items.5.title", data()).ok().flatten(), None);
        assert_eq!(text_of("nothing.at.all", data()).ok().flatten(), None);
        assert_eq!(text_of("i.x", data()).ok().flatten(), None);
    }

    #[test]
    fn bracket_lookup_by_kind() {
        assert_eq!(text_of("items[i].title", data()).ok().flatten().as_deref(), Some("second"));
        assert_eq!(text_of("m[\"a b\"]", data()).ok().flatten().as_deref(), Some("7"));
        assert_eq!(text_of("m[k]", data()).ok().flatten().as_deref(), Some("7"));
        assert_eq!(text_of("m[1]", data()).ok().flatten().as_deref(), Some("one"));
    }

    #[test]
    fn bracket_missing_is_error() {
        let err = text_of("items[5].title", data()).expect_err("out of range");
        assert!(err.message.contains("index 5 out of range for a list of 2"), "{}", err.message);
        assert_eq!(err.position.column, 6);
        let err = text_of("m[\"titel\"]", data()).expect_err("no field");
        assert!(err.message.contains("no field \"titel\" in map"), "{}", err.message);
        let err = text_of("i[0]", data()).expect_err("not indexable");
        assert!(err.message.contains("integer is not indexable"), "{}", err.message);
        let err = text_of("items[k]", data()).expect_err("text key on list");
        assert!(err.message.contains("indexed by an integer"), "{}", err.message);
        let err = text_of("items[nothing]", data()).expect_err("missing key");
        assert!(err.message.contains("missing"), "{}", err.message);
    }

    #[test]
    fn bracket_negative_index() {
        assert_eq!(text_of("items[-1].title", data()).ok().flatten().as_deref(), Some("second"));
        assert_eq!(text_of("items[-2].title", data()).ok().flatten().as_deref(), Some("first"));
        assert!(text_of("items[-3]", data()).is_err());
    }
}
