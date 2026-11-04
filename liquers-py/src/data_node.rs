//! Python data read in place as Liquers data nodes, so a template or any other node reader reads
//! a retained Python object field by field without converting it.
//!
//! A `dict`, a `list` or a scalar returned by a Python command is retained as `py.Object` when
//! conversion refuses it (a `datetime` inside, an integer key); its node still reads, member by
//! member, with only the unreadable members opaque. Other classes opt in with
//! `__liquers_data__` (`True`: read through `keys` / `__getitem__` / `__len__` / `__index__` /
//! `__float__`; a method: the data it returns).
//!
//! # Main components
//!
//! - [`node_of`]: classifies a Python object as a [`PyNode`] (the table of
//!   `specs/reference/PYTHON_INTEGRATION.md` §Data nodes).
//! - [`PyNode`]: the `DataNode` over a live Python object; every lookup attaches to the
//!   interpreter, on a runtime worker through `block_in_place`.
//! - The hook: `PyObjectValue`'s `ForeignValue::data_node` (`value.rs`) returns a `PyNode`.
//! - [`DataNodeView`] (`lq.DataNode`): the outbound view, a root and a path. The root is a plain
//!   node (`lq.data_node(x)`), a state read through a running command's context
//!   (`context.data_node(state)`), or a state read at the top level of an environment
//!   (`env.data_node(state)`, each async read one closure asset). A node that needs a context
//!   (a folder, a record source) is read with the `_async` members; the sync ones raise
//!   `NotSupportedError` there.
//!
//! # Where it is used
//!
//! - `ns-template/render` and `expand` over a retained Python value (through `Value::data_node`).
//! - `lq.data_node(x)`, `Context.data_node`, `Environment.data_node`, `State.data_node`.
//! - `liquers-py/tests/test_DATANODE_*.py`.
//!
//! # Related concepts
//!
//! - Conversion (`value.rs` `py_to_value`): copies a structure into a Liquers value once; a node
//!   reads the live object, so a change between two lookups is seen.
//! - `liquers_core::data_node`: the traits implemented here, and the JSON node a converted value
//!   reads through.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_DATANODE_mapping.py` (a Rust doctest cannot start the
//! interpreter this crate links against):
//!
//! ```python
//! node = lq.data_node({"name": "Ann", "when": datetime.datetime.now()})
//! assert node["name"].scalar == "Ann" and node["when"].kind == lq.NodeKind.Opaque
//! folder = env.data_node(env.evaluate_state("-R-dir/data"))
//! entry = await folder.get_async("sales.csv")     # one closure asset
//! ```
//!
//! More: `test_DATANODE_protocols.py` (opt-in), `test_DATANODE_render.py` (templates),
//! `test_DATANODE_views.py` (async views).

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use liquers_core::data_node::{AnyNode, DataNode, KeyIter, NodeIter, NodeKind, NodeRef, Scalar};
use liquers_core::error::Error;
use liquers_core::state::State as CoreState;
use liquers_core::value::ValueInterface;
use liquers_lib::value::ExtValueInterface;
use pyo3::exceptions::{PyIndexError, PyKeyError};
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyMemoryView, PyString, PyTuple,
};

use crate::value::{attach_off_worker, int_out_of_range_error, Value};

/// A Python object as a data node.
pub struct PyNode {
    object: Py<PyAny>,
    shape: Shape,
}

/// How a node reads its object, decided once by [`node_of`].
enum Shape {
    /// An exact `dict`, read with `PyDict` access (no Python code runs).
    Dict,
    /// An exact `list` or `tuple`.
    Sequence,
    /// `__liquers_data__ = True` with `keys` and `__getitem__`.
    ProtocolMap,
    /// `__liquers_data__ = True` with `__len__` and `__getitem__`.
    ProtocolList,
    /// Read once: `None`, `bool`, `int`, `float`, `str`, or an opted-in `__index__` / `__float__`.
    Scalar(Scalar<'static>),
    /// `lq.Query`, `lq.Key`, `lq.RecordView`: their value's own node.
    Delegate(Arc<dyn DataNode>),
    /// No data view; the error `to_json` raises when there is a specific one.
    Opaque(Option<Error>),
}

/// Builds the node of `object`.
pub(crate) fn node_of(py: Python<'_>, object: &Bound<'_, PyAny>) -> PyNode {
    classify(py, object, true)
}

/// `allow_method`: whether a `__liquers_data__` method is called. The result of one is classified
/// with it off, so a method returning another object with a method never loops.
fn classify(py: Python<'_>, object: &Bound<'_, PyAny>, allow_method: bool) -> PyNode {
    let shape = shape_of(py, object, allow_method);
    match shape {
        Classified::Own(shape) => PyNode {
            object: object.clone().unbind(),
            shape,
        },
        Classified::Replaced(node) => node,
    }
}

enum Classified {
    Own(Shape),
    /// The data a `__liquers_data__` method returned.
    Replaced(PyNode),
}

fn shape_of(py: Python<'_>, object: &Bound<'_, PyAny>, allow_method: bool) -> Classified {
    if object.is_none() {
        return Classified::Own(Shape::Scalar(Scalar::Null));
    }
    // `bool` is a subclass of `int`, so it is tested first.
    if let Ok(b) = object.cast::<PyBool>() {
        return Classified::Own(Shape::Scalar(Scalar::Bool(b.is_true())));
    }
    if object.cast::<PyInt>().is_ok() {
        return Classified::Own(int_shape(object));
    }
    if let Ok(f) = object.cast::<PyFloat>() {
        return Classified::Own(Shape::Scalar(Scalar::Float(f.value())));
    }
    if let Ok(s) = object.cast::<PyString>() {
        return Classified::Own(match s.to_str() {
            Ok(text) => Shape::Scalar(Scalar::Text(Cow::Owned(text.to_string()))),
            Err(e) => Shape::Opaque(Some(crate::error::pyerr_to_error(py, &e))),
        });
    }
    if object.cast::<PyBytes>().is_ok()
        || object.cast::<PyByteArray>().is_ok()
        || object.cast::<PyMemoryView>().is_ok()
    {
        return Classified::Own(Shape::Opaque(None));
    }
    if let Some(node) = delegate(object) {
        return Classified::Own(node);
    }
    if object.is_exact_instance_of::<PyDict>() {
        return Classified::Own(Shape::Dict);
    }
    if object.is_exact_instance_of::<PyList>() || object.is_exact_instance_of::<PyTuple>() {
        return Classified::Own(Shape::Sequence);
    }
    opted_in(py, object, allow_method)
}

fn int_shape(object: &Bound<'_, PyAny>) -> Shape {
    match object.extract::<i64>() {
        Ok(i) => Shape::Scalar(Scalar::Integer(i)),
        Err(_) => Shape::Opaque(Some(int_out_of_range_error(object))),
    }
}

/// `lq.Query`, `lq.Key` and `lq.RecordView` read as the value they wrap.
fn delegate(object: &Bound<'_, PyAny>) -> Option<Shape> {
    let value = if let Ok(query) = object.cast::<crate::query::Query>() {
        Value::from_query(&query.borrow().0)
    } else if let Ok(key) = object.cast::<crate::query::Key>() {
        Value::from_key(&key.borrow().0)
    } else if let Ok(view) = object.cast::<crate::records::view::RecordView>() {
        Value::from_record_view(view.get().inner.clone())
    } else {
        return None;
    };
    Some(match Value::data_node(Arc::new(value)) {
        AnyNode::Sync(node) => Shape::Delegate(node),
        AnyNode::Async(_) => Shape::Opaque(None),
    })
}

/// The opt-in, read from the type so an instance `__getattr__` never runs.
fn opted_in(py: Python<'_>, object: &Bound<'_, PyAny>, allow_method: bool) -> Classified {
    let ty = object.get_type();
    let marker = match ty.getattr("__liquers_data__") {
        Ok(marker) => marker,
        Err(_) => return Classified::Own(Shape::Opaque(None)),
    };
    if let Ok(flag) = marker.cast::<PyBool>() {
        if flag.is_true() {
            return Classified::Own(protocol_shape(py, object));
        }
        return Classified::Own(Shape::Opaque(None));
    }
    if allow_method && marker.is_callable() {
        return match object.call_method0("__liquers_data__") {
            Ok(data) => Classified::Replaced(classify(py, &data, false)),
            Err(e) => Classified::Own(Shape::Opaque(Some(crate::error::pyerr_to_error(py, &e)))),
        };
    }
    Classified::Own(Shape::Opaque(None))
}

/// The protocol shape of an object with `__liquers_data__ = True`: a map, a list, then a number.
fn protocol_shape(py: Python<'_>, object: &Bound<'_, PyAny>) -> Shape {
    let ty = object.get_type();
    let has = |name: &str| ty.hasattr(name).unwrap_or(false);
    if has("keys") && has("__getitem__") {
        return Shape::ProtocolMap;
    }
    if has("__len__") && has("__getitem__") {
        return Shape::ProtocolList;
    }
    if has("__index__") {
        return match py
            .import("operator")
            .and_then(|operator| operator.call_method1("index", (object,)))
        {
            Ok(index) => int_shape(&index),
            Err(e) => Shape::Opaque(Some(crate::error::pyerr_to_error(py, &e))),
        };
    }
    if has("__float__") {
        return match object
            .call_method0("__float__")
            .and_then(|f| f.extract::<f64>())
        {
            Ok(f) => Shape::Scalar(Scalar::Float(f)),
            Err(e) => Shape::Opaque(Some(crate::error::pyerr_to_error(py, &e))),
        };
    }
    Shape::Opaque(None)
}

/// A child as a node reference: scalars by value, delegates shared, containers owned.
fn child_ref<'a>(node: PyNode) -> NodeRef<'a> {
    match node.shape {
        Shape::Scalar(scalar) => NodeRef::Scalar(scalar),
        Shape::Delegate(inner) => NodeRef::Shared(inner),
        Shape::Dict
        | Shape::Sequence
        | Shape::ProtocolMap
        | Shape::ProtocolList
        | Shape::Opaque(_) => NodeRef::Owned(Box::new(node)),
    }
}

/// An opaque child carrying the error of a failed lookup.
fn failed_child(py: Python<'_>, object: &Bound<'_, PyAny>, error: PyErr) -> PyNode {
    PyNode {
        object: object.clone().unbind(),
        shape: Shape::Opaque(Some(crate::error::pyerr_to_error(py, &error))),
    }
}

impl PyNode {
    /// The child named `name`, attached.
    fn child(&self, py: Python<'_>, name: &str) -> Option<PyNode> {
        let object = self.object.bind(py);
        match &self.shape {
            Shape::Dict => {
                let dict = object.cast::<PyDict>().ok()?;
                match dict.get_item(name) {
                    Ok(Some(value)) => Some(node_of(py, &value)),
                    Ok(None) => None,
                    Err(e) => Some(failed_child(py, object, e)),
                }
            }
            Shape::ProtocolMap => match object.get_item(name) {
                Ok(value) => Some(node_of(py, &value)),
                Err(e) if e.is_instance_of::<PyKeyError>(py) => None,
                Err(e) => Some(failed_child(py, object, e)),
            },
            Shape::Sequence
            | Shape::ProtocolList
            | Shape::Scalar(_)
            | Shape::Delegate(_)
            | Shape::Opaque(_) => None,
        }
    }

    /// Item `i`, attached.
    fn item(&self, py: Python<'_>, i: usize) -> Option<PyNode> {
        let object = self.object.bind(py);
        match &self.shape {
            Shape::Sequence => {
                if let Ok(list) = object.cast::<PyList>() {
                    return list.get_item(i).ok().map(|value| node_of(py, &value));
                }
                let tuple = object.cast::<PyTuple>().ok()?;
                tuple.get_item(i).ok().map(|value| node_of(py, &value))
            }
            Shape::ProtocolList => match object.get_item(i) {
                Ok(value) => Some(node_of(py, &value)),
                Err(e) if e.is_instance_of::<PyIndexError>(py) => None,
                Err(e) => Some(failed_child(py, object, e)),
            },
            Shape::Dict
            | Shape::ProtocolMap
            | Shape::Scalar(_)
            | Shape::Delegate(_)
            | Shape::Opaque(_) => None,
        }
    }

    /// The `str` keys, in order (others skipped).
    fn key_list(&self, py: Python<'_>) -> Vec<String> {
        let object = self.object.bind(py);
        let keys = match &self.shape {
            Shape::Dict => match object.cast::<PyDict>() {
                Ok(dict) => dict.keys().into_any(),
                Err(_) => return Vec::new(),
            },
            Shape::ProtocolMap => match object.call_method0("keys") {
                Ok(keys) => keys,
                Err(_) => return Vec::new(),
            },
            Shape::Sequence
            | Shape::ProtocolList
            | Shape::Scalar(_)
            | Shape::Delegate(_)
            | Shape::Opaque(_) => return Vec::new(),
        };
        let Ok(iter) = keys.try_iter() else {
            return Vec::new();
        };
        iter.filter_map(|key| key.ok())
            .filter_map(|key| key.cast::<PyString>().ok().map(|s| s.to_string()))
            .collect()
    }

    /// JSON of this node, in one hold. `path` holds the identities of the containers being
    /// written, so a cycle is an error rather than a recursion.
    fn json(&self, py: Python<'_>, path: &mut HashSet<usize>) -> Result<serde_json::Value, Error> {
        let object = self.object.bind(py);
        match &self.shape {
            Shape::Scalar(scalar) => scalar.to_json(),
            Shape::Delegate(inner) => inner.to_json(),
            Shape::Opaque(Some(error)) => Err(error.clone()),
            Shape::Opaque(None) => Err(Error::conversion_error_with_message(
                &type_name(object),
                "JSON",
                "the object has no data view",
            )),
            Shape::Dict | Shape::Sequence | Shape::ProtocolMap | Shape::ProtocolList => {
                let id = object.as_ptr() as usize;
                if !path.insert(id) {
                    return Err(Error::conversion_error_with_message(
                        &type_name(object),
                        "JSON",
                        "the structure contains itself",
                    ));
                }
                let result = self.container_json(py, object, path);
                path.remove(&id);
                result
            }
        }
    }

    fn container_json(
        &self,
        py: Python<'_>,
        object: &Bound<'_, PyAny>,
        path: &mut HashSet<usize>,
    ) -> Result<serde_json::Value, Error> {
        match &self.shape {
            Shape::Dict => {
                let dict = object
                    .cast::<PyDict>()
                    .map_err(|e| crate::error::pyerr_to_error(py, &e.into()))?;
                let mut map = serde_json::Map::new();
                for (key, value) in dict.iter() {
                    let key = key.cast::<PyString>().map(|s| s.to_string()).map_err(|_| {
                        Error::conversion_error_with_message(
                            "dict",
                            "JSON",
                            &format!("key {} is not a str", repr(&key)),
                        )
                    })?;
                    map.insert(key, node_of(py, &value).json(py, path)?);
                }
                Ok(serde_json::Value::Object(map))
            }
            Shape::ProtocolMap => {
                let mut map = serde_json::Map::new();
                for key in self.key_list(py) {
                    if let Some(child) = self.child(py, &key) {
                        map.insert(key, child.json(py, path)?);
                    }
                }
                Ok(serde_json::Value::Object(map))
            }
            Shape::Sequence | Shape::ProtocolList => {
                let mut items = Vec::new();
                let mut i = 0;
                while let Some(item) = self.item(py, i) {
                    items.push(item.json(py, path)?);
                    i += 1;
                }
                Ok(serde_json::Value::Array(items))
            }
            Shape::Scalar(_) | Shape::Delegate(_) | Shape::Opaque(_) => self.json(py, path),
        }
    }
}

fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "object".to_string())
}

fn repr(object: &Bound<'_, PyAny>) -> String {
    object
        .repr()
        .map(|r| r.to_string())
        .unwrap_or_else(|_| "<unprintable>".to_string())
}

impl DataNode for PyNode {
    fn kind(&self) -> NodeKind {
        match &self.shape {
            Shape::Dict | Shape::ProtocolMap => NodeKind::Map,
            Shape::Sequence | Shape::ProtocolList => NodeKind::List,
            Shape::Scalar(scalar) => scalar.kind(),
            Shape::Delegate(inner) => inner.kind(),
            Shape::Opaque(_) => NodeKind::Opaque,
        }
    }

    fn scalar(&self) -> Option<Scalar<'_>> {
        match &self.shape {
            Shape::Scalar(scalar) => Some(scalar.clone()),
            Shape::Delegate(inner) => inner.scalar(),
            Shape::Dict
            | Shape::Sequence
            | Shape::ProtocolMap
            | Shape::ProtocolList
            | Shape::Opaque(_) => None,
        }
    }

    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        match &self.shape {
            Shape::Delegate(inner) => inner.get(name),
            Shape::Dict | Shape::ProtocolMap => {
                attach_off_worker(|py| self.child(py, name)).map(child_ref)
            }
            Shape::Sequence | Shape::ProtocolList | Shape::Scalar(_) | Shape::Opaque(_) => None,
        }
    }

    fn len(&self) -> Option<usize> {
        match &self.shape {
            Shape::Delegate(inner) => inner.len(),
            Shape::Sequence | Shape::ProtocolList => {
                attach_off_worker(|py| self.object.bind(py).len().ok())
            }
            Shape::Dict | Shape::ProtocolMap | Shape::Scalar(_) | Shape::Opaque(_) => None,
        }
    }

    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        match &self.shape {
            Shape::Delegate(inner) => inner.items(),
            // One hold per item, so a list shortened while it is read ends early.
            Shape::Sequence | Shape::ProtocolList => {
                Box::new((0..).map_while(move |i| self.index(i)))
            }
            Shape::Dict | Shape::ProtocolMap | Shape::Scalar(_) | Shape::Opaque(_) => {
                Box::new(std::iter::empty())
            }
        }
    }

    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        match &self.shape {
            Shape::Delegate(inner) => inner.keys(),
            Shape::Dict | Shape::ProtocolMap => {
                let keys = attach_off_worker(|py| self.key_list(py));
                Box::new(keys.into_iter().map(Cow::Owned))
            }
            Shape::Sequence | Shape::ProtocolList | Shape::Scalar(_) | Shape::Opaque(_) => {
                Box::new(std::iter::empty())
            }
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, Error> {
        attach_off_worker(|py| self.json(py, &mut HashSet::new()))
    }

    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        match &self.shape {
            Shape::Delegate(inner) => inner.index(i),
            Shape::Sequence | Shape::ProtocolList => {
                attach_off_worker(|py| self.item(py, i)).map(child_ref)
            }
            Shape::Dict | Shape::ProtocolMap | Shape::Scalar(_) | Shape::Opaque(_) => None,
        }
    }

    fn is_truthy(&self) -> bool {
        match &self.shape {
            Shape::Scalar(scalar) => scalar.is_truthy(),
            Shape::Delegate(inner) => inner.is_truthy(),
            Shape::Sequence | Shape::ProtocolList => self.len().is_some_and(|n| n > 0),
            Shape::Dict | Shape::ProtocolMap => true,
            Shape::Opaque(_) => true,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// The Python view: `lq.NodeKind`, `lq.DataNode`, `lq.data_node`
// ------------------------------------------------------------------------------------------------

/// The kind of a data node.
#[pyclass(frozen, eq, eq_int, name = "NodeKind", module = "liquers")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PyNodeKind {
    Null,
    Bool,
    Integer,
    Float,
    Text,
    List,
    Map,
    Opaque,
}

impl From<NodeKind> for PyNodeKind {
    fn from(kind: NodeKind) -> Self {
        match kind {
            NodeKind::Null => PyNodeKind::Null,
            NodeKind::Bool => PyNodeKind::Bool,
            NodeKind::Integer => PyNodeKind::Integer,
            NodeKind::Float => PyNodeKind::Float,
            NodeKind::Text => PyNodeKind::Text,
            NodeKind::List => PyNodeKind::List,
            NodeKind::Map => PyNodeKind::Map,
            NodeKind::Opaque => PyNodeKind::Opaque,
        }
    }
}

/// One step from a view's root to its node.
#[derive(Clone, Debug)]
enum Step {
    Key(String),
    Index(usize),
}

/// What a view reads from.
enum Root {
    /// A node: `lq.data_node(x)`, `State.data_node()`, and every view whose node holds no context.
    Node(AnyNode),
    /// A value that needs a command's context (a folder, a record source): `context.data_node`.
    /// Valid while the command runs.
    Command {
        state: CoreState<Value>,
        context: crate::context::Context,
    },
    /// The same at the top level: `env.data_node`. Each read runs in a closure asset.
    Env {
        state: CoreState<Value>,
        env: std::sync::Weak<crate::environment::PyEnvironment>,
        shut: Arc<crate::aio::Shut>,
    },
}

/// A view of data, read field by field without converting it: `lq.data_node(x)`.
///
/// The view is live: each read looks the data up again, so a `dict` changed between two reads
/// shows the change. Children are views too. Data that is read on demand (a store folder, a record
/// source) is read with the `_async` members; the sync ones raise `NotSupportedError` for it.
#[pyclass(frozen, name = "DataNode", module = "liquers")]
pub struct DataNodeView {
    root: Arc<Root>,
    path: Arc<[Step]>,
    /// The kind, known without a read: a view reached through an await, or a context root.
    kind: Option<NodeKind>,
}

/// Walks `path` from `node` and calls `f` on the node reached; `None` when a step is missing.
fn with_node<R>(
    node: &dyn DataNode,
    path: &[Step],
    f: &mut dyn FnMut(&dyn DataNode) -> R,
) -> Option<R> {
    match path.split_first() {
        None => Some(f(node)),
        Some((step, rest)) => {
            let child = match step {
                Step::Key(name) => node.get(name)?,
                Step::Index(i) => node.index(*i)?,
            };
            with_node(child.node(), rest, f)
        }
    }
}

fn scalar_to_py(py: Python<'_>, scalar: Scalar<'_>) -> PyResult<Py<PyAny>> {
    use pyo3::IntoPyObjectExt;
    match scalar {
        Scalar::Null => Ok(py.None()),
        Scalar::Bool(b) => b.into_py_any(py),
        Scalar::Integer(i) => i.into_py_any(py),
        Scalar::UInt(u) => u.into_py_any(py),
        Scalar::Float(f) => f.into_py_any(py),
        Scalar::Text(t) => t.as_ref().into_py_any(py),
    }
}

fn async_only() -> Error {
    Error::not_supported(
        "this data is read asynchronously: use the `_async` form (`get_async`, `scalar_async`, \
         `len_async`, `to_json_async`)"
            .to_string(),
    )
}

fn gone(path: &[Step]) -> Error {
    Error::general_error(format!("the data at {path:?} is no longer there"))
}

/// An async read.
enum Op {
    Get(Step),
    Scalar,
    Len,
    ToJson,
}

/// What an async read gives.
enum Outcome {
    /// `Get`: the child, owned, or `None` when missing; `SameRoot` when it is reached through a
    /// sync node and is read from the same root.
    Child(Option<AnyNode>),
    SameRoot(bool, Option<NodeKind>),
    Scalar(Option<Scalar<'static>>),
    Len(Option<usize>),
    Json(serde_json::Value),
}

fn sync_op(node: &dyn DataNode, path: &[Step], op: &Op) -> Result<Outcome, Error> {
    let answer = with_node(node, path, &mut |node| -> Result<Outcome, Error> {
        Ok(match op {
            Op::Get(step) => {
                let child = match step {
                    Step::Key(name) => node.get(name),
                    Step::Index(i) => node.index(*i),
                };
                let kind = child.as_ref().map(|c| c.node().kind());
                Outcome::SameRoot(child.is_some(), kind)
            }
            Op::Scalar => Outcome::Scalar(node.scalar().map(Scalar::into_owned)),
            Op::Len => Outcome::Len(node.len()),
            Op::ToJson => Outcome::Json(node.to_json()?),
        })
    });
    answer.unwrap_or_else(|| Err(gone(path)))
}

/// Applies `op` to the node `path` names under `node`, awaiting where the data is async.
async fn async_op(node: AnyNode, path: &[Step], op: &Op) -> Result<Outcome, Error> {
    let mut node = node;
    for (i, step) in path.iter().enumerate() {
        node = match node {
            AnyNode::Sync(sync) => return sync_op(sync.as_ref(), &path[i..], op),
            AnyNode::Async(a) => match step {
                Step::Key(name) => a.get(name).await?,
                Step::Index(j) => a.index(*j).await?,
            }
            .ok_or_else(|| gone(path))?,
        };
    }
    match node {
        AnyNode::Sync(sync) => sync_op(sync.as_ref(), &[], op),
        AnyNode::Async(a) => Ok(match op {
            Op::Get(Step::Key(name)) => Outcome::Child(a.get(name).await?),
            Op::Get(Step::Index(j)) => Outcome::Child(a.index(*j).await?),
            Op::Scalar => Outcome::Scalar(a.scalar().await?),
            Op::Len => Outcome::Len(a.len().await?),
            Op::ToJson => Outcome::Json(a.to_json().await?),
        }),
    }
}

/// An awaited read, converted to Python on the caller's loop.
pub(crate) enum NodeAnswer {
    View(Option<DataNodeView>),
    Scalar(Option<Scalar<'static>>),
    Len(Option<usize>),
    Json(serde_json::Value),
}

impl NodeAnswer {
    pub(crate) fn into_py(self, py: Python<'_>) -> Result<Py<PyAny>, Error> {
        use pyo3::IntoPyObjectExt;
        let converted = match self {
            NodeAnswer::View(Some(view)) => view.into_py_any(py),
            NodeAnswer::View(None) | NodeAnswer::Scalar(None) => Ok(py.None()),
            NodeAnswer::Scalar(Some(scalar)) => scalar_to_py(py, scalar),
            NodeAnswer::Len(Some(n)) => n.into_py_any(py),
            NodeAnswer::Len(None) => {
                return Err(Error::general_error(
                    "only a list data node has a length".to_string(),
                ))
            }
            NodeAnswer::Json(json) => crate::store::json_to_py(py, &json),
        };
        converted.map_err(|e| crate::error::pyerr_to_error(py, &e))
    }
}

impl DataNodeView {
    /// A view of a Liquers value's own node (a retained Python object reads through the hook).
    pub(crate) fn of_value(value: Arc<Value>) -> Self {
        Self::of_node(Value::data_node(value))
    }

    pub(crate) fn of_node(node: AnyNode) -> Self {
        let kind = match &node {
            AnyNode::Async(a) => Some(a.kind()),
            AnyNode::Sync(_) => None,
        };
        DataNodeView {
            root: Arc::new(Root::Node(node)),
            path: Arc::from(Vec::new()),
            kind,
        }
    }

    /// A view of `state` read in a command's context: a value that needs none is a plain node.
    pub(crate) fn of_command(state: CoreState<Value>, context: crate::context::Context) -> Self {
        match liquers_lib::template::host::context_node_kind(state.data_unchecked()) {
            None => Self::of_value(state.data_unchecked().clone()),
            Some(kind) => DataNodeView {
                root: Arc::new(Root::Command { state, context }),
                path: Arc::from(Vec::new()),
                kind: Some(kind),
            },
        }
    }

    /// A view of `state` read at the top level of `envref`.
    pub(crate) fn of_env(
        state: CoreState<Value>,
        envref: &liquers_core::context::EnvRef<crate::environment::PyEnvironment>,
        shut: Arc<crate::aio::Shut>,
    ) -> Self {
        match liquers_lib::template::host::context_node_kind(state.data_unchecked()) {
            None => Self::of_value(state.data_unchecked().clone()),
            Some(kind) => DataNodeView {
                root: Arc::new(Root::Env {
                    state,
                    env: Arc::downgrade(&envref.0),
                    shut,
                }),
                path: Arc::from(Vec::new()),
                kind: Some(kind),
            },
        }
    }

    fn child(&self, step: Step, kind: Option<NodeKind>) -> Self {
        let mut path = self.path.to_vec();
        path.push(step);
        DataNodeView {
            root: self.root.clone(),
            path: Arc::from(path),
            kind,
        }
    }

    /// Runs `f` on the node this view names. `NotSupportedError` for data read asynchronously,
    /// `KeyError` when the node is gone.
    fn read<R>(&self, py: Python<'_>, mut f: impl FnMut(&dyn DataNode) -> R) -> PyResult<R> {
        let node = match self.root.as_ref() {
            Root::Node(AnyNode::Sync(node)) => node,
            Root::Node(AnyNode::Async(_)) | Root::Command { .. } | Root::Env { .. } => {
                return Err(crate::error::error_to_pyerr(py, &async_only()))
            }
        };
        // The caller is attached already; each lookup attaches again, which is reentrant.
        with_node(node.as_ref(), &self.path, &mut f)
            .ok_or_else(|| PyKeyError::new_err(format!("{:?} is no longer there", self.path)))
    }

    fn step_of(key: &Bound<'_, PyAny>) -> PyResult<Step> {
        if let Ok(name) = key.cast::<PyString>() {
            return Ok(Step::Key(name.to_string()));
        }
        if key.cast::<PyBool>().is_err() {
            if let Ok(i) = key.extract::<usize>() {
                return Ok(Step::Index(i));
            }
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "a data node is indexed by a str (a field) or a non-negative int (an item)",
        ))
    }

    /// The child at `step`, when it exists.
    fn existing_child(&self, py: Python<'_>, step: Step) -> PyResult<Option<Self>> {
        let found = self.read(py, |node| {
            let child = match &step {
                Step::Key(name) => node.get(name),
                Step::Index(i) => node.index(*i),
            };
            child.is_some()
        })?;
        Ok(found.then(|| self.child(step, None)))
    }

    /// The view an async `get` gives for `outcome`.
    fn child_view(root: &Arc<Root>, path: &[Step], step: Step, outcome: Outcome) -> Option<Self> {
        let extend = |kind| {
            let mut longer = path.to_vec();
            longer.push(step.clone());
            DataNodeView {
                root: root.clone(),
                path: Arc::from(longer),
                kind,
            }
        };
        match outcome {
            Outcome::Child(None) | Outcome::SameRoot(false, _) => None,
            Outcome::SameRoot(true, kind) => Some(extend(kind)),
            // A sync node holds no context, so it reads on its own from now on.
            Outcome::Child(Some(node @ AnyNode::Sync(_))) => Some(Self::of_node(node)),
            Outcome::Child(Some(AnyNode::Async(a))) => match root.as_ref() {
                Root::Node(_) => Some(Self::of_node(AnyNode::Async(a))),
                // Read again from the root, so the command's validity and the run's asset apply.
                Root::Command { .. } | Root::Env { .. } => Some(extend(Some(a.kind()))),
            },
            Outcome::Scalar(_) | Outcome::Len(_) | Outcome::Json(_) => None,
        }
    }

    /// An awaitable of `op` on this view's node.
    fn awaitable<'py>(&self, py: Python<'py>, op: Op) -> PyResult<Bound<'py, PyAny>> {
        let root = self.root.clone();
        let path = self.path.clone();
        let finish = move |outcome: Outcome, root: &Arc<Root>, path: &[Step], op: Op| -> NodeAnswer {
            match op {
                Op::Get(step) => NodeAnswer::View(Self::child_view(root, path, step, outcome)),
                Op::Scalar | Op::Len | Op::ToJson => match outcome {
                    Outcome::Scalar(s) => NodeAnswer::Scalar(s),
                    Outcome::Len(n) => NodeAnswer::Len(n),
                    Outcome::Json(j) => NodeAnswer::Json(j),
                    Outcome::Child(_) | Outcome::SameRoot(..) => NodeAnswer::View(None),
                },
            }
        };
        let matched = self.root.clone();
        match matched.as_ref() {
            Root::Node(node) => {
                let node = node.clone();
                let shut = Arc::new(crate::aio::Shut::default());
                crate::aio::awaitable(
                    py,
                    &shut,
                    async move {
                        let outcome = async_op(node, &path, &op).await?;
                        Ok(finish(outcome, &root, &path, op))
                    },
                    ToPyNode::wrap,
                )
            }
            Root::Command { state, context } => {
                let core = context.core().ok_or_else(|| {
                    crate::error::error_to_pyerr(
                        py,
                        &Error::general_error(
                            "this data node belongs to a command that has returned".to_string(),
                        ),
                    )
                })?;
                let state = state.clone();
                crate::aio::awaitable(
                    py,
                    &context.shut(),
                    async move {
                        let host = liquers_lib::template::ContextHost::new(
                            core,
                            liquers_core::query::Key::new(),
                        );
                        let node = host.node_of(&state);
                        drop(host);
                        let outcome = async_op(node, &path, &op).await?;
                        Ok(finish(outcome, &root, &path, op))
                    },
                    ToPyNode::wrap,
                )
            }
            Root::Env { state, env, shut } => {
                let envref = env
                    .upgrade()
                    .map(liquers_core::context::EnvRef)
                    .ok_or_else(|| {
                        crate::error::error_to_pyerr(py, &crate::environment::shut_down_error())
                    })?;
                if shut.is_set() {
                    return Err(crate::error::error_to_pyerr(
                        py,
                        &crate::environment::shut_down_error(),
                    ));
                }
                use liquers_core::assets::AssetManager;
                let closure = envref
                    .get_asset_manager()
                    .closure_asset("data node read")
                    .map_err(|e| crate::error::error_to_pyerr(py, &e))?;
                let asset = closure.asset().clone();
                let state = state.clone();
                let run = closure.run(
                    move |context| async move {
                        let host = liquers_lib::template::ContextHost::new(
                            context,
                            liquers_core::query::Key::new(),
                        );
                        let node = host.node_of(&state);
                        drop(host);
                        let outcome = async_op(node, &path, &op).await?;
                        Ok((Value::none(), finish(outcome, &root, &path, op)))
                    },
                    None,
                );
                crate::closure::awaitable_run(py, shut, asset, run, |result| {
                    crate::aio::ToPy::Node(match result {
                        Ok(Some(answer)) => Ok(answer),
                        Ok(None) => Err(Error::cancelled("the data node read was cancelled".to_string())),
                        Err(e) => Err(e),
                    })
                })
            }
        }
    }
}

/// `ToPy::Node` as the `fn` `aio::awaitable` takes.
struct ToPyNode;
impl ToPyNode {
    fn wrap(result: Result<NodeAnswer, Error>) -> crate::aio::ToPy {
        crate::aio::ToPy::Node(result)
    }
}

/// `x` as a data node: an `lq.State`'s value read in place, or any Python object read by the
/// mapping (standard types) and the `__liquers_data__` opt-in.
#[pyfunction]
pub fn data_node(py: Python<'_>, x: &Bound<'_, PyAny>) -> DataNodeView {
    if let Ok(state) = x.cast::<crate::asset::State>() {
        return DataNodeView::of_value(state.get().core().data_unchecked().clone());
    }
    DataNodeView::of_node(AnyNode::Sync(Arc::new(node_of(py, x))))
}

#[pymethods]
impl DataNodeView {
    /// The node's kind, an `lq.NodeKind`.
    #[getter]
    fn kind(&self, py: Python<'_>) -> PyResult<PyNodeKind> {
        if let Some(kind) = self.kind {
            return Ok(PyNodeKind::from(kind));
        }
        self.read(py, |node| PyNodeKind::from(node.kind()))
    }

    /// The scalar (`None`, `bool`, `int`, `float` or `str`), or `None` for a list, a map or an
    /// opaque node.
    #[getter]
    fn scalar(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let scalar = self.read(py, |node| node.scalar().map(Scalar::into_owned))?;
        match scalar {
            Some(scalar) => scalar_to_py(py, scalar),
            None => Ok(py.None()),
        }
    }

    /// The field (a `str`) or item (an `int`): a view. `KeyError` / `IndexError` when missing.
    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Self> {
        let step = Self::step_of(key)?;
        let missing = match &step {
            Step::Key(name) => PyKeyError::new_err(name.clone()),
            Step::Index(i) => PyIndexError::new_err(*i),
        };
        self.existing_child(py, step)?.ok_or(missing)
    }

    /// The field or item, or `default` when it is missing.
    #[pyo3(signature = (key, default=None))]
    fn get(
        &self,
        py: Python<'_>,
        key: &Bound<'_, PyAny>,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        use pyo3::IntoPyObjectExt;
        let step = Self::step_of(key)?;
        match self.existing_child(py, step)? {
            Some(child) => child.into_py_any(py),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    /// The field names of a map (`str` keys only), in order; empty for anything else.
    fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        self.read(py, |node| node.keys().map(|k| k.into_owned()).collect())
    }

    /// The number of items of a list. `TypeError` for anything else.
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        self.read(py, |node| node.len())?.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err("only a list data node has a length")
        })
    }

    /// The items of a list (views), or the keys of a map.
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        use pyo3::IntoPyObjectExt;
        let (kind, len) = self.read(py, |node| (node.kind(), node.len()))?;
        let list = match kind {
            NodeKind::Map => self.keys(py)?.into_py_any(py)?,
            NodeKind::List => {
                let count = match len {
                    Some(n) => n,
                    None => self.read(py, |node| node.items().count())?,
                };
                (0..count)
                    .map(|i| self.child(Step::Index(i), None))
                    .collect::<Vec<_>>()
                    .into_py_any(py)?
            }
            NodeKind::Null
            | NodeKind::Bool
            | NodeKind::Integer
            | NodeKind::Float
            | NodeKind::Text
            | NodeKind::Opaque => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "only a list or a map data node can be iterated",
                ))
            }
        };
        Ok(list.bind(py).try_iter()?.into_any().unbind())
    }

    /// Mustache truthiness: `None`, `False` and an empty list are false.
    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        self.read(py, |node| node.is_truthy())
    }

    /// The node as JSON-shaped Python data. `lq.ConversionError` when it has none (an opaque
    /// member, a cycle, a non-`str` key, an integer outside 64 bits, a non-finite float).
    fn to_json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json = self
            .read(py, |node| node.to_json())?
            .map_err(|e| crate::error::error_to_pyerr(py, &e))?;
        crate::store::json_to_py(py, &json)
    }

    /// Awaitable `get`: the field or item as a view, or `None` when missing. Reads data that is
    /// read on demand (a store folder, a record source); at the top level each read runs in an
    /// asset of its own.
    fn get_async<'py>(&self, py: Python<'py>, key: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let step = Self::step_of(key)?;
        self.awaitable(py, Op::Get(step))
    }

    /// Awaitable `scalar`.
    fn scalar_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.awaitable(py, Op::Scalar)
    }

    /// Awaitable `len()`.
    fn len_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.awaitable(py, Op::Len)
    }

    /// Awaitable `to_json()`.
    fn to_json_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.awaitable(py, Op::ToJson)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match self.kind(py) {
            Ok(kind) => format!("DataNode({kind:?})"),
            Err(_) => "DataNode(?)".to_string(),
        }
    }
}

/// Registers the classes and `data_node`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNodeKind>()?;
    m.add_class::<DataNodeView>()?;
    m.add_function(wrap_pyfunction!(data_node, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(py: Python<'_>, code: &str) -> Result<PyNode, Box<dyn std::error::Error>> {
        let object = py.eval(&std::ffi::CString::new(code)?, None, None)?;
        Ok(node_of(py, &object))
    }

    #[test]
    fn kinds_and_missing_keys() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let root = node(py, "{'a': [1, {'b': 'x'}], 'c': None, 'd': b'raw'}")?;
            assert_eq!(root.kind(), NodeKind::Map);
            let a = root.get("a").ok_or("a missing")?;
            let a = a.node();
            assert_eq!(a.kind(), NodeKind::List);
            assert_eq!(a.len(), Some(2));
            assert_eq!(root.get("c").ok_or("c missing")?.node().kind(), NodeKind::Null);
            assert_eq!(root.get("d").ok_or("d missing")?.node().kind(), NodeKind::Opaque);
            assert!(root.get("missing").is_none());
            let keys: Vec<String> = root.keys().map(|k| k.to_string()).collect();
            assert_eq!(keys, ["a", "c", "d"]);
            Ok(())
        })
    }

    #[test]
    fn integer_and_nan_edges() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            assert_eq!(node(py, "2**63 - 1")?.kind(), NodeKind::Integer);
            for code in ["2**63", "2**64"] {
                let big = node(py, code)?;
                assert_eq!(big.kind(), NodeKind::Opaque);
                assert!(big.to_json().is_err());
            }
            let nan = node(py, "float('nan')")?;
            assert_eq!(nan.kind(), NodeKind::Float);
            assert!(nan.to_json().is_err());
            Ok(())
        })
    }

    #[test]
    fn a_cycle_navigates_and_refuses_json() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let cyclic = py.eval(c"(lambda d: (d.__setitem__('self', d), d)[1])({})", None, None)?;
            let root = node_of(py, &cyclic);
            let inner = root.get("self").ok_or("self missing")?;
            assert_eq!(inner.node().kind(), NodeKind::Map);
            assert!(root.to_json().is_err());
            Ok(())
        })
    }
}
