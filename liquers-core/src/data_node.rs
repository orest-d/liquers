//! Values seen as template data: a read-only tree of scalars, lists and maps.
//!
//! A consumer asks a value five questions — its kind, its scalar text, a named field, its items in
//! order, and itself as JSON — without converting the whole value first. Two traits answer them:
//!
//! - [`DataNode`] is synchronous, for values held in memory (`Value`, [`MetadataRecord`],
//!   `serde_json::Value`, a record view, a polars frame). A lookup is a function call.
//! - [`AsyncDataNode`] is asynchronous, for data read on demand (a record source, a store folder).
//!   Only the fields a consumer touches are read.
//!
//! [`AnyNode`] holds either. A consumer that is async anyway (the template renderer) takes both and
//! awaits only the async arm; a sync consumer (a store handler) calls [`AnyNode::as_sync`] and
//! refuses an async node with an error instead of blocking.
//!
//! Design: `specs/design/liquers-mustache/data-node-interface.md`.

use std::any::Any;
use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::error::Error;
use crate::maybe_send::{MaybeSend, MaybeSync};
use crate::metadata::MetadataRecord;
use crate::value::{Value, ValueInterface};

/// What a node is. Known without reading any data, also for an async node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Null,
    Bool,
    Integer,
    Float,
    Text,
    List,
    Map,
    /// A value with no data view (bytes, a handle): it has no fields, items or text.
    Opaque,
}

impl NodeKind {
    /// Whether the kind is a scalar (`Null` included).
    pub fn is_scalar(self) -> bool {
        match self {
            NodeKind::Null | NodeKind::Bool | NodeKind::Integer | NodeKind::Float | NodeKind::Text => {
                true
            }
            NodeKind::List | NodeKind::Map | NodeKind::Opaque => false,
        }
    }
}

/// A scalar value, borrowed from its node where possible.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar<'a> {
    Null,
    Bool(bool),
    Integer(i64),
    /// An integer above `i64::MAX`, kept exact rather than rounded to a float.
    UInt(u64),
    Float(f64),
    Text(Cow<'a, str>),
}

impl<'a> Scalar<'a> {
    /// The kind of this scalar.
    pub fn kind(&self) -> NodeKind {
        match self {
            Scalar::Null => NodeKind::Null,
            Scalar::Bool(_) => NodeKind::Bool,
            Scalar::Integer(_) | Scalar::UInt(_) => NodeKind::Integer,
            Scalar::Float(_) => NodeKind::Float,
            Scalar::Text(_) => NodeKind::Text,
        }
    }

    /// The text a template writes for this scalar: `""` for null, `true`/`false`, the number, the
    /// text itself.
    pub fn to_text(&self) -> Cow<'_, str> {
        match self {
            Scalar::Null => Cow::Borrowed(""),
            Scalar::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            Scalar::Integer(i) => Cow::Owned(i.to_string()),
            Scalar::UInt(u) => Cow::Owned(u.to_string()),
            Scalar::Float(f) => Cow::Owned(f.to_string()),
            Scalar::Text(t) => Cow::Borrowed(t.as_ref()),
        }
    }

    /// An owned copy, independent of the node it came from.
    pub fn into_owned(self) -> Scalar<'static> {
        match self {
            Scalar::Null => Scalar::Null,
            Scalar::Bool(b) => Scalar::Bool(b),
            Scalar::Integer(i) => Scalar::Integer(i),
            Scalar::UInt(u) => Scalar::UInt(u),
            Scalar::Float(f) => Scalar::Float(f),
            Scalar::Text(t) => Scalar::Text(Cow::Owned(t.into_owned())),
        }
    }

    /// The scalar as JSON. A non-finite float is a conversion error, as in `try_into_json_value`.
    pub fn to_json(&self) -> Result<serde_json::Value, Error> {
        match self {
            Scalar::Null => Ok(serde_json::Value::Null),
            Scalar::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            Scalar::Integer(i) => Ok(serde_json::Value::from(*i)),
            Scalar::UInt(u) => Ok(serde_json::Value::from(*u)),
            Scalar::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| Error::conversion_error(f, "JSON number")),
            Scalar::Text(t) => Ok(serde_json::Value::String(t.to_string())),
        }
    }

    /// Mustache truthiness: null and `false` are false, every other scalar is true.
    pub fn is_truthy(&self) -> bool {
        match self {
            Scalar::Null => false,
            Scalar::Bool(b) => *b,
            Scalar::Integer(_) | Scalar::UInt(_) | Scalar::Float(_) | Scalar::Text(_) => true,
        }
    }
}

/// Items of a list node, as a `MaybeSend` iterator.
pub trait NodeIter<'a>: Iterator<Item = NodeRef<'a>> + MaybeSend {}
impl<'a, T: Iterator<Item = NodeRef<'a>> + MaybeSend> NodeIter<'a> for T {}

/// Field names of a map node, as a `MaybeSend` iterator.
pub trait KeyIter<'a>: Iterator<Item = Cow<'a, str>> + MaybeSend {}
impl<'a, T: Iterator<Item = Cow<'a, str>> + MaybeSend> KeyIter<'a> for T {}

/// Items of an async list node, as a `MaybeSend` stream.
pub trait NodeStream<'a>: Stream<Item = Result<AnyNode, Error>> + MaybeSend {}
impl<'a, T: Stream<Item = Result<AnyNode, Error>> + MaybeSend> NodeStream<'a> for T {}

/// Synchronous access to a value held in memory.
///
/// Every method has the answer for the kind it does not apply to: `scalar` is `None` for a list, a
/// map or an opaque node, `get` is `None` for anything but a map, `items` is empty for anything but
/// a list.
#[allow(clippy::len_without_is_empty)] // `len` is `None` for a node that is not a list
pub trait DataNode: MaybeSend + MaybeSync {
    /// What the node is.
    fn kind(&self) -> NodeKind;
    /// The scalar, for a scalar node.
    fn scalar(&self) -> Option<Scalar<'_>>;
    /// The field `name` of a map node.
    fn get(&self, name: &str) -> Option<NodeRef<'_>>;
    /// The number of items of a list, when known.
    fn len(&self) -> Option<usize>;
    /// The items of a list, in order.
    fn items(&self) -> Box<dyn NodeIter<'_> + '_>;
    /// The field names of a map, in the node's own order.
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_>;
    /// The node as JSON (a parameter override, JSON output).
    fn to_json(&self) -> Result<serde_json::Value, Error>;

    /// Item `i` of a list. The default walks [`items`](Self::items); a node with random access
    /// overrides it.
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        self.items().nth(i)
    }

    /// Mustache truthiness: null, `false` and an empty list are false.
    fn is_truthy(&self) -> bool {
        match self.kind() {
            NodeKind::Null => false,
            NodeKind::Bool | NodeKind::Integer | NodeKind::Float | NodeKind::Text => {
                self.scalar().map(|s| s.is_truthy()).unwrap_or(false)
            }
            NodeKind::List => match self.len() {
                Some(n) => n > 0,
                None => self.items().next().is_some(),
            },
            NodeKind::Map | NodeKind::Opaque => true,
        }
    }

    /// The node as `Any`, so a host that knows the concrete type can recover the value without a
    /// conversion (an apply input keeps its metadata that way).
    fn as_any(&self) -> Option<&dyn Any> {
        None
    }
}

/// A child node: borrowed when the parent holds it, owned when the parent builds it (a frame row),
/// shared when it came from an [`AnyNode`], or a bare scalar.
pub enum NodeRef<'a> {
    Borrowed(&'a dyn DataNode),
    Owned(Box<dyn DataNode + 'a>),
    Shared(Arc<dyn DataNode>),
    Scalar(Scalar<'a>),
}

impl<'a> NodeRef<'a> {
    /// The node, whatever holds it.
    pub fn node(&self) -> &(dyn DataNode + '_) {
        match self {
            NodeRef::Borrowed(n) => *n,
            NodeRef::Owned(n) => n.as_ref(),
            NodeRef::Shared(n) => n.as_ref(),
            NodeRef::Scalar(s) => s,
        }
    }
}

impl std::fmt::Debug for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeRef({:?})", self.node().kind())
    }
}

/// Asynchronous access to data read on demand.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[allow(clippy::len_without_is_empty)]
pub trait AsyncDataNode: MaybeSend + MaybeSync {
    /// What the node is; known without I/O.
    fn kind(&self) -> NodeKind;
    /// The scalar, for a scalar node.
    async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error>;
    /// The field `name` of a map node.
    async fn get(&self, name: &str) -> Result<Option<AnyNode>, Error>;
    /// The items of a list, in order.
    fn items(&self) -> Pin<Box<dyn NodeStream<'_> + '_>>;
    /// The node as JSON.
    async fn to_json(&self) -> Result<serde_json::Value, Error>;

    /// The number of items of a list, when known without reading them.
    async fn len(&self) -> Result<Option<usize>, Error> {
        Ok(None)
    }

    /// Item `i` of a list. The default reads the stream up to it.
    async fn index(&self, i: usize) -> Result<Option<AnyNode>, Error> {
        let mut items = self.items();
        let mut n = 0;
        while let Some(item) = items.next().await {
            let item = item?;
            if n == i {
                return Ok(Some(item));
            }
            n += 1;
        }
        Ok(None)
    }
}

/// A node of either kind. A sync child of an async node stays sync, so a folder of files costs one
/// await per file, not one per field.
#[derive(Clone)]
pub enum AnyNode {
    Sync(Arc<dyn DataNode>),
    Async(Arc<dyn AsyncDataNode>),
}

impl AnyNode {
    /// A sync node over a JSON value.
    pub fn from_json(value: serde_json::Value) -> Self {
        AnyNode::Sync(Arc::new(value))
    }

    /// The null node.
    pub fn null() -> Self {
        AnyNode::Sync(Arc::new(serde_json::Value::Null))
    }

    /// An opaque node, for a value with no data view.
    pub fn opaque() -> Self {
        AnyNode::Sync(Arc::new(OpaqueNode))
    }

    /// The node's kind.
    pub fn kind(&self) -> NodeKind {
        match self {
            AnyNode::Sync(n) => n.kind(),
            AnyNode::Async(n) => n.kind(),
        }
    }

    /// The sync node, for a consumer that cannot await. An async node is refused rather than
    /// blocked on, since blocking is not possible in async code or on wasm32.
    pub fn as_sync(&self) -> Result<&dyn DataNode, Error> {
        match self {
            AnyNode::Sync(n) => Ok(n.as_ref()),
            AnyNode::Async(_) => Err(Error::not_supported(
                "This data is read asynchronously and cannot be used where synchronous access is required"
                    .to_string(),
            )),
        }
    }

    /// The node as JSON, awaiting an async node.
    pub async fn to_json(&self) -> Result<serde_json::Value, Error> {
        match self {
            AnyNode::Sync(n) => n.to_json(),
            AnyNode::Async(n) => n.to_json().await,
        }
    }
}

impl std::fmt::Debug for AnyNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyNode::Sync(n) => write!(f, "AnyNode::Sync({:?})", n.kind()),
            AnyNode::Async(n) => write!(f, "AnyNode::Async({:?})", n.kind()),
        }
    }
}

/// The node of a value with no data view.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpaqueNode;

impl DataNode for OpaqueNode {
    fn kind(&self) -> NodeKind {
        NodeKind::Opaque
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        None
    }
    fn get(&self, _name: &str) -> Option<NodeRef<'_>> {
        None
    }
    fn len(&self) -> Option<usize> {
        None
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        Err(Error::not_supported(
            "This value has no data view and cannot be written as JSON".to_string(),
        ))
    }
}

impl DataNode for Scalar<'_> {
    fn kind(&self) -> NodeKind {
        Scalar::kind(self)
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        Some(match self {
            Scalar::Text(t) => Scalar::Text(Cow::Borrowed(t.as_ref())),
            Scalar::Null => Scalar::Null,
            Scalar::Bool(b) => Scalar::Bool(*b),
            Scalar::Integer(i) => Scalar::Integer(*i),
            Scalar::UInt(u) => Scalar::UInt(*u),
            Scalar::Float(f) => Scalar::Float(*f),
        })
    }
    fn get(&self, _name: &str) -> Option<NodeRef<'_>> {
        None
    }
    fn len(&self) -> Option<usize> {
        None
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        Scalar::to_json(self)
    }
}

impl DataNode for serde_json::Value {
    fn kind(&self) -> NodeKind {
        match self {
            serde_json::Value::Null => NodeKind::Null,
            serde_json::Value::Bool(_) => NodeKind::Bool,
            serde_json::Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    NodeKind::Integer
                } else {
                    NodeKind::Float
                }
            }
            serde_json::Value::String(_) => NodeKind::Text,
            serde_json::Value::Array(_) => NodeKind::List,
            serde_json::Value::Object(_) => NodeKind::Map,
        }
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        match self {
            serde_json::Value::Null => Some(Scalar::Null),
            serde_json::Value::Bool(b) => Some(Scalar::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(Scalar::Integer(i))
                } else if let Some(u) = n.as_u64() {
                    Some(Scalar::UInt(u))
                } else {
                    n.as_f64().map(Scalar::Float)
                }
            }
            serde_json::Value::String(s) => Some(Scalar::Text(Cow::Borrowed(s))),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => None,
        }
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        match self {
            serde_json::Value::Object(map) => {
                map.get(name).map(|v| NodeRef::Borrowed(v as &dyn DataNode))
            }
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Array(_) => None,
        }
    }
    fn len(&self) -> Option<usize> {
        match self {
            serde_json::Value::Array(items) => Some(items.len()),
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Object(_) => None,
        }
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        match self {
            serde_json::Value::Array(items) => {
                Box::new(items.iter().map(|v| NodeRef::Borrowed(v as &dyn DataNode)))
            }
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Object(_) => Box::new(std::iter::empty()),
        }
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        match self {
            serde_json::Value::Object(map) => Box::new(map.keys().map(|k| Cow::Borrowed(k.as_str()))),
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Array(_) => Box::new(std::iter::empty()),
        }
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        match self {
            serde_json::Value::Array(items) => items.get(i).map(|v| NodeRef::Borrowed(v as &dyn DataNode)),
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
            | serde_json::Value::Object(_) => None,
        }
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        Ok(self.clone())
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

impl DataNode for Value {
    fn kind(&self) -> NodeKind {
        match self {
            Value::None => NodeKind::Null,
            Value::Bool(_) => NodeKind::Bool,
            Value::I32(_) | Value::I64(_) => NodeKind::Integer,
            Value::F64(_) => NodeKind::Float,
            Value::Text(_) | Value::Query(_) | Value::Key(_) => NodeKind::Text,
            Value::Array(_) | Value::AssetInfo(_) => NodeKind::List,
            Value::Object(_) | Value::Metadata(_) | Value::Recipe(_) | Value::CommandMetadata(_) => {
                NodeKind::Map
            }
            Value::Bytes(_) => NodeKind::Opaque,
        }
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        match self {
            Value::None => Some(Scalar::Null),
            Value::Bool(b) => Some(Scalar::Bool(*b)),
            Value::I32(i) => Some(Scalar::Integer(i64::from(*i))),
            Value::I64(i) => Some(Scalar::Integer(*i)),
            Value::F64(f) => Some(Scalar::Float(*f)),
            Value::Text(t) => Some(Scalar::Text(Cow::Borrowed(t))),
            Value::Query(q) => Some(Scalar::Text(Cow::Owned(q.encode()))),
            Value::Key(k) => Some(Scalar::Text(Cow::Owned(k.encode()))),
            Value::Array(_)
            | Value::Object(_)
            | Value::Bytes(_)
            | Value::Metadata(_)
            | Value::AssetInfo(_)
            | Value::Recipe(_)
            | Value::CommandMetadata(_) => None,
        }
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        match self {
            Value::Object(map) => map.get(name).map(|v| NodeRef::Borrowed(v as &dyn DataNode)),
            Value::Metadata(m) => m.get(name),
            Value::Recipe(_) | Value::CommandMetadata(_) => snapshot_field(self, name),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Array(_)
            | Value::Bytes(_)
            | Value::AssetInfo(_)
            | Value::Query(_)
            | Value::Key(_) => None,
        }
    }
    fn len(&self) -> Option<usize> {
        match self {
            Value::Array(items) => Some(items.len()),
            Value::AssetInfo(items) => Some(items.len()),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Object(_)
            | Value::Bytes(_)
            | Value::Metadata(_)
            | Value::Recipe(_)
            | Value::CommandMetadata(_)
            | Value::Query(_)
            | Value::Key(_) => None,
        }
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        match self {
            Value::Array(items) => Box::new(items.iter().map(|v| NodeRef::Borrowed(v as &dyn DataNode))),
            Value::AssetInfo(items) => Box::new(items.iter().map(|info| {
                match serde_json::to_value(info) {
                    Ok(json) => NodeRef::Owned(Box::new(json)),
                    Err(_) => NodeRef::Owned(Box::new(OpaqueNode)),
                }
            })),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Object(_)
            | Value::Bytes(_)
            | Value::Metadata(_)
            | Value::Recipe(_)
            | Value::CommandMetadata(_)
            | Value::Query(_)
            | Value::Key(_) => Box::new(std::iter::empty()),
        }
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        match self {
            Value::Object(map) => Box::new(map.keys().map(|k| Cow::Borrowed(k.as_str()))),
            Value::Metadata(m) => m.keys(),
            Value::Recipe(_) | Value::CommandMetadata(_) => snapshot_keys(self),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Array(_)
            | Value::Bytes(_)
            | Value::AssetInfo(_)
            | Value::Query(_)
            | Value::Key(_) => Box::new(std::iter::empty()),
        }
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        match self {
            Value::Array(items) => items.get(i).map(|v| NodeRef::Borrowed(v as &dyn DataNode)),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Object(_)
            | Value::Bytes(_)
            | Value::Metadata(_)
            | Value::AssetInfo(_)
            | Value::Recipe(_)
            | Value::CommandMetadata(_)
            | Value::Query(_)
            | Value::Key(_) => self.items().nth(i),
        }
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        match self {
            Value::Query(q) => Ok(serde_json::Value::String(q.encode())),
            Value::Key(k) => Ok(serde_json::Value::String(k.encode())),
            Value::None
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::F64(_)
            | Value::Text(_)
            | Value::Array(_)
            | Value::Object(_)
            | Value::Bytes(_)
            | Value::Metadata(_)
            | Value::AssetInfo(_)
            | Value::Recipe(_)
            | Value::CommandMetadata(_) => self.try_into_json_value(),
        }
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

/// A field of a value without a field-level view, taken from its JSON snapshot.
pub fn snapshot_field<'a, T: serde::Serialize>(value: &T, name: &str) -> Option<NodeRef<'a>> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(mut map)) => {
            map.remove(name).map(|v| NodeRef::Owned(Box::new(v)))
        }
        Ok(_) | Err(_) => None,
    }
}

/// The field names of a value without a field-level view, from its JSON snapshot.
pub fn snapshot_keys<'a, T: serde::Serialize>(value: &T) -> Box<dyn KeyIter<'a> + 'a> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(map)) => {
            Box::new(map.into_iter().map(|(k, _)| Cow::Owned(k)).collect::<Vec<_>>().into_iter())
        }
        Ok(_) | Err(_) => Box::new(std::iter::empty()),
    }
}

fn text_ref(text: &str) -> Option<NodeRef<'_>> {
    Some(NodeRef::Scalar(Scalar::Text(Cow::Borrowed(text))))
}

fn optional_text_ref(text: &Option<String>) -> Option<NodeRef<'_>> {
    Some(NodeRef::Scalar(match text {
        Some(t) => Scalar::Text(Cow::Borrowed(t)),
        None => Scalar::Null,
    }))
}

/// A map of the record's fields. The text fields a template or a store handler reads most are
/// borrowed; the rest come from the record's JSON form.
impl DataNode for MetadataRecord {
    fn kind(&self) -> NodeKind {
        NodeKind::Map
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        None
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        match name {
            "title" => text_ref(&self.title),
            "description" => text_ref(&self.description),
            "message" => text_ref(&self.message),
            "type_identifier" => text_ref(&self.type_identifier),
            "type_name" => text_ref(&self.type_name),
            "unicode_icon" => text_ref(&self.unicode_icon),
            "updated" => text_ref(&self.updated),
            "media_type" => optional_text_ref(&self.media_type),
            "filename" => optional_text_ref(&self.filename),
            "data_format" => optional_text_ref(&self.data_format),
            "key" => Some(NodeRef::Scalar(match &self.key {
                Some(k) => Scalar::Text(Cow::Owned(k.encode())),
                None => Scalar::Null,
            })),
            "query" => Some(NodeRef::Scalar(Scalar::Text(Cow::Owned(self.query.encode())))),
            "is_error" => Some(NodeRef::Scalar(Scalar::Bool(self.is_error))),
            "is_dir" => Some(NodeRef::Scalar(Scalar::Bool(self.is_dir))),
            "file_size" => Some(NodeRef::Scalar(match self.file_size {
                Some(n) => i64::try_from(n).map(Scalar::Integer).unwrap_or(Scalar::UInt(n)),
                None => Scalar::Null,
            })),
            _ => snapshot_field(self, name),
        }
    }
    fn len(&self) -> Option<usize> {
        None
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        snapshot_keys(self)
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        serde_json::to_value(self).map_err(|e| Error::conversion_error(e, "JSON"))
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn texts(node: &dyn DataNode) -> Vec<String> {
        node.items()
            .map(|item| item.node().scalar().map(|s| s.to_text().into_owned()).unwrap_or_default())
            .collect()
    }

    #[test]
    fn value_as_data_node() {
        let mut map = BTreeMap::new();
        map.insert("title".to_string(), Value::from("Store listing"));
        map.insert(
            "tags".to_string(),
            Value::Array(vec![Value::from("a"), Value::from("b")]),
        );
        map.insert("n".to_string(), Value::I32(3));
        let value = Value::Object(map);

        assert_eq!(value.kind(), NodeKind::Map);
        let title = value.get("title").expect("title");
        assert_eq!(title.node().scalar(), Some(Scalar::Text(Cow::Borrowed("Store listing"))));
        let tags = value.get("tags").expect("tags");
        assert_eq!(tags.node().len(), Some(2));
        assert_eq!(texts(tags.node()), vec!["a", "b"]);
        assert_eq!(
            tags.node().index(1).map(|n| n.node().scalar().map(|s| s.into_owned())),
            Some(Some(Scalar::Text(Cow::Owned("b".to_string()))))
        );
        assert_eq!(
            value.get("n").and_then(|n| n.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Integer(3))
        );
        assert!(value.get("missing").is_none());
        let keys: Vec<String> = value.keys().map(|k| k.into_owned()).collect();
        assert_eq!(keys, vec!["n", "tags", "title"]);
        assert!(!Value::None.is_truthy());
        assert!(!Value::Array(vec![]).is_truthy());
        assert_eq!(Value::Bytes(vec![1]).kind(), NodeKind::Opaque);
        assert_eq!(value.to_json().ok(), value.try_into_json_value().ok());
    }

    #[test]
    fn metadata_record_as_data_node() {
        let mut record = MetadataRecord::new();
        record.title = "Report".to_string();
        record.media_type = Some("text/markdown".to_string());
        assert_eq!(record.kind(), NodeKind::Map);
        assert_eq!(
            record.get("title").and_then(|n| n.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Text(Cow::Owned("Report".to_string())))
        );
        assert_eq!(
            record.get("media_type").and_then(|n| n.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Text(Cow::Owned("text/markdown".to_string())))
        );
        assert_eq!(
            record.get("filename").and_then(|n| n.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Null)
        );
        // A field without a borrowed view comes from the JSON form.
        assert_eq!(record.get("status").map(|n| n.node().kind()), Some(NodeKind::Text));
        assert!(record.keys().any(|k| k == "title"));
        assert!(record.get("no_such_field").is_none());
    }

    #[test]
    fn json_as_data_node() {
        let json = serde_json::json!({"items": [{"title": "first"}, {"title": "second"}], "x": 1.5, "b": false});
        let items = DataNode::get(&json, "items").expect("items");
        assert_eq!(items.node().kind(), NodeKind::List);
        let second = items.node().index(1).expect("item 1");
        assert_eq!(
            second.node().get("title").and_then(|t| t.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Text(Cow::Owned("second".to_string())))
        );
        assert!(items.node().index(2).is_none());
        assert_eq!(
            DataNode::get(&json, "x").and_then(|x| x.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Float(1.5))
        );
        assert!(!DataNode::get(&json, "b").map(|b| b.node().is_truthy()).unwrap_or(true));
        assert_eq!(DataNode::to_json(&json).ok(), Some(json.clone()));
        assert_eq!(Scalar::Float(f64::NAN).to_json().is_err(), true);
    }

    #[test]
    fn large_unsigned_integer_stays_exact() {
        let json = serde_json::json!({"id": u64::MAX});
        let id = DataNode::get(&json, "id").expect("id");
        let scalar = id.node().scalar().map(|s| s.into_owned());
        assert_eq!(scalar, Some(Scalar::UInt(u64::MAX)));
        assert_eq!(id.node().kind(), NodeKind::Integer);
        assert_eq!(Scalar::UInt(u64::MAX).to_text(), "18446744073709551615");
        assert_eq!(Scalar::UInt(u64::MAX).to_json().ok(), Some(serde_json::json!(u64::MAX)));
    }

    struct Lazy;

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl AsyncDataNode for Lazy {
        fn kind(&self) -> NodeKind {
            NodeKind::List
        }
        async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error> {
            Ok(None)
        }
        async fn get(&self, _name: &str) -> Result<Option<AnyNode>, Error> {
            Ok(None)
        }
        fn items(&self) -> Pin<Box<dyn NodeStream<'_> + '_>> {
            Box::pin(futures::stream::iter(vec![
                Ok(AnyNode::from_json(serde_json::json!(1))),
                Ok(AnyNode::from_json(serde_json::json!(2))),
            ]))
        }
        async fn to_json(&self) -> Result<serde_json::Value, Error> {
            Ok(serde_json::json!([1, 2]))
        }
    }

    #[test]
    fn sync_consumer_refuses_async_node() {
        let node = AnyNode::Async(Arc::new(Lazy));
        let err = node.as_sync().err().expect("refused");
        assert_eq!(err.error_type, crate::error::ErrorType::NotSupported);
        assert!(AnyNode::null().as_sync().is_ok());
        let second = futures::executor::block_on(async {
            match &node {
                AnyNode::Async(n) => n.index(1).await,
                AnyNode::Sync(_) => Ok(None),
            }
        });
        let second = second.ok().flatten().expect("item 1");
        assert_eq!(
            futures::executor::block_on(second.to_json()).ok(),
            Some(serde_json::json!(2))
        );
    }
}
