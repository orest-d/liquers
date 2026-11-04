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
//!
//! # Where it is used
//!
//! - `ns-template/render` and `expand` over a retained Python value (through `Value::data_node`).
//! - `lq.data_node(x)` and the view classes of this module's Python half.
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
//! ```
//!
//! More: `test_DATANODE_protocols.py` (opt-in), `test_DATANODE_render.py` (templates).

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use liquers_core::data_node::{AnyNode, DataNode, KeyIter, NodeIter, NodeKind, NodeRef, Scalar};
use liquers_core::error::Error;
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
    /// A node: `lq.data_node(x)`, `State.data_node()`.
    Node(AnyNode),
}

/// A view of data, read field by field without converting it: `lq.data_node(x)`.
///
/// The view is live: each read looks the data up again, so a `dict` changed between two reads
/// shows the change. Children are views too.
#[pyclass(frozen, name = "DataNode", module = "liquers")]
pub struct DataNodeView {
    root: Arc<Root>,
    path: Arc<[Step]>,
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

impl DataNodeView {
    /// A view of a Liquers value's own node (a retained Python object reads through the hook).
    pub(crate) fn of_value(value: Arc<Value>) -> Self {
        Self::of_node(Value::data_node(value))
    }

    pub(crate) fn of_node(node: AnyNode) -> Self {
        DataNodeView {
            root: Arc::new(Root::Node(node)),
            path: Arc::from(Vec::new()),
        }
    }

    fn child(&self, step: Step) -> Self {
        let mut path = self.path.to_vec();
        path.push(step);
        DataNodeView {
            root: self.root.clone(),
            path: Arc::from(path),
        }
    }

    /// Runs `f` on the node this view names. `NotSupportedError` for data read asynchronously,
    /// `KeyError` when the node is gone.
    fn read<R>(&self, py: Python<'_>, mut f: impl FnMut(&dyn DataNode) -> R) -> PyResult<R> {
        let root = match self.root.as_ref() {
            Root::Node(node) => node,
        };
        let node = root.as_sync().map_err(|_| {
            crate::error::error_to_pyerr(
                py,
                &Error::not_supported(
                    "this data is read asynchronously: use the `_async` form (`get_async`, \
                     `scalar_async`, `len_async`, `to_json_async`)"
                        .to_string(),
                ),
            )
        })?;
        // The caller is attached already; each lookup attaches again, which is reentrant.
        with_node(node, &self.path, &mut f)
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
        let exists = self.read(py, |node| match &step {
            Step::Key(name) => node.get(name).is_some(),
            Step::Index(i) => node.index(*i).is_some(),
        })?;
        Ok(exists.then(|| self.child(step)))
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
                    .map(|i| self.child(Step::Index(i)))
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

    fn __repr__(&self, py: Python<'_>) -> String {
        match self.read(py, |node| node.kind()) {
            Ok(kind) => format!("DataNode({:?})", PyNodeKind::from(kind)),
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
