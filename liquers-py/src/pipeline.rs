//! Pipelines from Python: `lq.Pipeline`, the editors, and the conversions every pipeline entry
//! point shares.
//!
//! A pipeline is the structured form of a query (`specs/reference/PIPELINES.md`): a list of
//! elements, each a resource read, an action with named or positional arguments, a value, and so
//! on. Python writes one as plain lists and dicts (its JSON form) or as an `lq.Pipeline`; the JSON
//! form is the bridge, so no rule of the core type is reimplemented here.
//!
//! # Main components
//!
//! - [`Pipeline`] (`lq.Pipeline`): the core pipeline, with `to_query(env)`, `from_query`,
//!   `to_json` / `from_json` and `to_list`.
//! - [`pipeline_from_py`]: any Python form of a pipeline as the core type; [`json_slot`]: a Python
//!   value as JSON for a slot that holds one, naming the path of what it refuses.
//! - [`PipelineEditor`], [`QueryEditor`], [`ActionEditor`]: the core editors, each edit checked
//!   against the environment's commands; a refused edit leaves the editor unchanged.
//! - [`pipeline_has_relative_key`]: the relative-key check a pipeline evaluated in a command needs.
//!
//! # Where it is used
//!
//! - `environment.rs` / `context.rs`: `evaluate` and its forms accept a pipeline;
//!   `Environment.pipeline_editor`, `query_editor`, `pipeline_command`.
//! - `value.rs`: a pipeline value crosses as `lq.Pipeline`; `recipes.rs`, `command_metadata.rs`:
//!   `Recipe.pipeline`, `ArgumentInfo.default`, `CommandDefinition.pipeline`.
//!
//! # Related concepts
//!
//! - `liquers_core::pipeline`: the type, its conversions and editors.
//! - `declared.rs`: pipeline commands are declarations, as template commands are.
//! - `specs/reference/PYTHON_INTEGRATION.md` §Pipelines; `specs/guides/PYTHON_GUIDE.md`.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_PIPELINE_*.py` (a Rust doctest cannot start the
//! interpreter this crate links against):
//!
//! ```python
//! top = ["-R/data/sales.csv/-/ns-pl/from_csv",
//!        {"action": "head", "ns": "pl", "parameters": [5]}]
//! assert lq.Pipeline(top).to_query(env).encode() == "-R/data/sales.csv/-/ns-pl/from_csv/head-5"
//! frame = env.evaluate(top)                        # evaluated as its query would be
//! ```

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::error::Error;
use liquers_core::pipeline::{
    ArgRef, ElementKind, Namespaces, Operand, Pipeline as CorePipeline, PipelineEditor as CorePipelineEditor,
    PipelineElement, QueryEditor as CoreQueryEditor, QueryOrPipeline, Read, Source,
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::sync::MutexExt;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use crate::error::error_to_pyerr;

type Json = serde_json::Value;

/// Where a JSON slot's walk is, for error messages: `$[1].arguments.values[0]`.
#[derive(Debug, Default, Clone)]
pub(crate) struct JsonPath(String);

impl JsonPath {
    fn root() -> Self {
        JsonPath("$".to_string())
    }
    fn index(&self, i: usize) -> Self {
        JsonPath(format!("{}[{i}]", self.0))
    }
    fn key(&self, k: &str) -> Self {
        JsonPath(format!("{}.{k}", self.0))
    }
}

fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn refuse(path: &JsonPath, what: &str) -> Error {
    Error::conversion_error_with_message(&path.0, "JSON", what)
}

/// A Python value as JSON for a slot that holds one. `None`, `bool`, `int` (in `i64`), `float`
/// (NaN and ±inf as `"NaN"`, `"inf"`, `"-inf"`, as the reference serializer writes them), `str`,
/// `list` / `tuple`, `dict` with `str` keys, and an `lq.Pipeline` (its serialized form). Anything
/// else, and a container that contains itself, is refused with a `ConversionError` naming `path`.
pub(crate) fn json_slot(object: &Bound<'_, PyAny>, path: &JsonPath) -> Result<Json, Error> {
    let mut on_path = HashSet::new();
    walk(object, path, &mut on_path)
}

fn walk(object: &Bound<'_, PyAny>, path: &JsonPath, on_path: &mut HashSet<usize>) -> Result<Json, Error> {
    if object.is_none() {
        return Ok(Json::Null);
    }
    if let Ok(b) = object.cast::<PyBool>() {
        return Ok(Json::Bool(b.is_true()));
    }
    if let Ok(i) = object.cast::<PyInt>() {
        return i
            .extract::<i64>()
            .map(Json::from)
            .map_err(|_| refuse(path, "an integer outside the 64-bit range"));
    }
    if let Ok(f) = object.cast::<PyFloat>() {
        let v = f.value();
        return Ok(if v.is_nan() {
            Json::String("NaN".to_string())
        } else if v.is_infinite() {
            Json::String(if v > 0.0 { "inf" } else { "-inf" }.to_string())
        } else {
            serde_json::Number::from_f64(v).map(Json::Number).unwrap_or(Json::Null)
        });
    }
    if let Ok(s) = object.cast::<PyString>() {
        return Ok(Json::String(s.to_string()));
    }
    if let Ok(p) = object.cast::<Pipeline>() {
        return Ok(p.get().0.to_json());
    }
    let id = object.as_ptr() as usize;
    if let Ok(list) = object.cast::<PyList>() {
        if !on_path.insert(id) {
            return Err(refuse(path, "a list that contains itself"));
        }
        let items = list
            .iter()
            .enumerate()
            .map(|(i, item)| walk(&item, &path.index(i), on_path))
            .collect::<Result<Vec<_>, _>>();
        on_path.remove(&id);
        return items.map(Json::Array);
    }
    if let Ok(tuple) = object.cast::<PyTuple>() {
        let items = tuple
            .iter()
            .enumerate()
            .map(|(i, item)| walk(&item, &path.index(i), on_path))
            .collect::<Result<Vec<_>, _>>();
        return items.map(Json::Array);
    }
    if let Ok(dict) = object.cast::<PyDict>() {
        if !on_path.insert(id) {
            return Err(refuse(path, "a dict that contains itself"));
        }
        let mut map = serde_json::Map::new();
        let mut result = Ok(());
        for (key, value) in dict.iter() {
            let Ok(key) = key.cast::<PyString>().map(|k| k.to_string()) else {
                result = Err(refuse(path, &format!("a key of type {}; keys must be str", type_name(&key))));
                break;
            };
            match walk(&value, &path.key(&key), on_path) {
                Ok(json) => {
                    map.insert(key, json);
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        on_path.remove(&id);
        return result.map(|()| Json::Object(map));
    }
    Err(refuse(path, &format!("a {}, which has no JSON form", type_name(object))))
}

/// Any Python form of a pipeline as the core type: an `lq.Pipeline`, or its JSON form (a list of
/// elements, or a dict with `elements`). A `str` is a query, not a pipeline: `TypeError`.
pub(crate) fn pipeline_from_py(object: &Bound<'_, PyAny>) -> PyResult<CorePipeline> {
    if let Ok(p) = object.cast::<Pipeline>() {
        return Ok(p.get().0.clone());
    }
    if !(object.is_instance_of::<PyList>() || object.is_instance_of::<PyTuple>() || object.is_instance_of::<PyDict>()) {
        return Err(PyTypeError::new_err(format!(
            "a pipeline is a list of elements, a dict with `elements`, or an lq.Pipeline, not {} \
             (a query string is lq.Pipeline.from_query)",
            type_name(object)
        )));
    }
    let py = object.py();
    json_slot(object, &JsonPath::root())
        .and_then(|json| CorePipeline::from_json(&json))
        .map_err(|e| error_to_pyerr(py, &e))
}

/// One element, from any Python form `pipeline_from_py` reads for an element (a query text, or a
/// dict).
fn element_from_py(object: &Bound<'_, PyAny>) -> PyResult<PipelineElement> {
    let py = object.py();
    let json = json_slot(object, &JsonPath::root()).map_err(|e| error_to_pyerr(py, &e))?;
    let pipeline = CorePipeline::from_json(&Json::Array(vec![json])).map_err(|e| error_to_pyerr(py, &e))?;
    pipeline
        .elements
        .into_iter()
        .next()
        .ok_or_else(|| PyTypeError::new_err("an element is one query text or one dict"))
}

fn is_relative(key: &str) -> bool {
    liquers_core::parse::parse_key(key)
        .ok()
        .and_then(|key| key.0.first().map(|name| name.is_cwd() || name.is_parent()))
        .unwrap_or(false)
}

/// The first key of `pipeline` that is relative to the working directory (`./x`, `../x`), in a
/// resource, key or cwd element, or in a nested pipeline or argument, if any.
pub(crate) fn pipeline_has_relative_key(pipeline: &CorePipeline) -> Option<String> {
    fn operand(o: &Operand<String>) -> Option<String> {
        match o {
            Operand::Literal(key) if is_relative(key) => Some(key.clone()),
            Operand::Literal(_) | Operand::Placeholder(_) => None,
        }
    }
    fn query_or_pipeline(q: &Operand<QueryOrPipeline>) -> Option<String> {
        match q {
            Operand::Literal(QueryOrPipeline::Pipeline(p)) => pipeline_has_relative_key(p),
            Operand::Literal(QueryOrPipeline::Query(q)) => liquers_core::parse::parse_query(q)
                .ok()
                .filter(|q| q.has_relative_operand())
                .map(|q| q.encode()),
            Operand::Placeholder(_) => None,
        }
    }
    fn source(s: &Source) -> Option<String> {
        match s {
            Source::Pipeline(p) => pipeline_has_relative_key(p),
            Source::Link(text) => liquers_core::parse::parse_query(text)
                .ok()
                .filter(|q| q.has_relative_operand())
                .map(|q| q.encode()),
            Source::Multiple(items) => items.iter().find_map(source),
            Source::Value(_) | Source::Placeholder(_) | Source::State(_) => None,
        }
    }
    pipeline.elements.iter().find_map(|element| match &element.item {
        ElementKind::Resource { resource, .. } => operand(resource),
        ElementKind::KeyValue(key) | ElementKind::Cwd(key) => operand(key),
        ElementKind::Pipeline(p) => pipeline_has_relative_key(p),
        ElementKind::Evaluate(q) | ElementKind::QueryValue(q) => query_or_pipeline(q),
        ElementKind::Action(action) => action
            .parameters
            .iter()
            .chain(action.arguments.values())
            .find_map(|argument| source(&argument.item)),
        ElementKind::Text(text) => liquers_core::parse::parse_query(text)
            .ok()
            .filter(|q| q.has_relative_operand())
            .map(|q| q.encode()),
        ElementKind::Value(_) | ElementKind::Quote | ElementKind::Volatile => None,
    })
}

/// A pipeline: the structured form of a query (`PIPELINES.md`).
///
/// `lq.Pipeline(data)` reads its JSON form: a list of elements (query text, or dicts such as
/// `{"action": "head", "ns": "pl", "parameters": [5]}`), a dict with `elements`, or another
/// `lq.Pipeline`. `Pipeline.from_query(q)` reads a query. Pipelines compare by value.
#[pyclass(frozen, eq, name = "Pipeline", module = "liquers")]
#[derive(Clone, PartialEq)]
pub struct Pipeline(pub CorePipeline);

#[pymethods]
impl Pipeline {
    #[new]
    fn new(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        pipeline_from_py(data).map(Pipeline)
    }

    /// The pipeline of a query (a `str` or an `lq.Query`).
    #[staticmethod]
    fn from_query(py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::environment::to_query(query)
            .and_then(|q| CorePipeline::try_from(&q))
            .map(Pipeline)
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Reads a pipeline from JSON text, as `to_json()` writes it.
    #[staticmethod]
    fn from_json(py: Python<'_>, text: &str) -> PyResult<Self> {
        serde_json::from_str::<Json>(text)
            .map_err(|e| Error::conversion_error_with_message("text", "pipeline", &e.to_string()))
            .and_then(|json| CorePipeline::from_json(&json))
            .map(Pipeline)
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The serialized form as JSON text.
    fn to_json(&self) -> String {
        self.0.to_json().to_string()
    }

    /// The serialized form as Python data: a dict with `elements`.
    fn to_list(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::store::json_to_py(py, &self.0.to_json())
    }

    /// The query this pipeline is, checked against `env`'s commands; raises `ParameterError`
    /// naming the representability rule when it has none.
    fn to_query(&self, py: Python<'_>, env: &Bound<'_, crate::environment::Environment>) -> PyResult<crate::query::Query> {
        let envref = env.get().cell.envref(py).map_err(|e| error_to_pyerr(py, &e))?;
        self.0
            .to_query_with(envref.get_command_metadata_registry())
            .map(crate::query::Query)
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The number of elements.
    #[getter]
    fn elements_count(&self) -> usize {
        self.0.elements.len()
    }

    fn __repr__(&self) -> String {
        format!("Pipeline({})", self.0.to_json())
    }
}

/// A namespace argument: a `str` or a list of them.
fn namespaces(object: &Bound<'_, PyAny>) -> PyResult<Namespaces> {
    if let Ok(text) = object.extract::<String>() {
        return Ok(Namespaces::from(text));
    }
    object
        .extract::<Vec<String>>()
        .map(Namespaces::from)
        .map_err(|_| PyTypeError::new_err("a namespace is a str or a list of str"))
}

/// An argument reference: a name or a position.
fn arg_ref(object: &Bound<'_, PyAny>) -> PyResult<ArgRef> {
    if let Ok(name) = object.extract::<String>() {
        return Ok(ArgRef::from(name));
    }
    object
        .extract::<usize>()
        .map(ArgRef::from)
        .map_err(|_| PyTypeError::new_err("an argument is named by a str or a position (int)"))
}

fn read(name: &str) -> Result<Read, Error> {
    Read::from_instruction(name)
        .ok_or_else(|| Error::general_error(format!("'{name}' is not a resource read")))
}

/// The registry an editor checks against: the environment's commands now.
pub(crate) fn editor_registry(env: &crate::environment::Environment, py: Python<'_>) -> Result<Arc<CommandMetadataRegistry>, Error> {
    let envref = env.cell.envref(py)?;
    Ok(Arc::new(envref.get_command_metadata_registry().clone()))
}

/// The operations both core editors share, so one macro writes both Python classes.
trait CoreEditor: Clone + Send {
    fn push(&mut self, e: PipelineElement) -> Result<(), Error>;
    fn insert(&mut self, at: usize, e: PipelineElement) -> Result<(), Error>;
    fn replace(&mut self, at: usize, e: PipelineElement) -> Result<(), Error>;
    fn remove(&mut self, at: usize) -> Result<(), Error>;
    fn move_to(&mut self, from: usize, to: usize) -> Result<(), Error>;
    fn set_realm(&mut self, realm: &str) -> Result<(), Error>;
    fn set_filename(&mut self, name: Option<&str>) -> Result<(), Error>;
    fn resource(&mut self, key: &str, read: Read) -> Result<(), Error>;
    fn default_ns(&mut self, ns: Option<Namespaces>);
    fn action(&mut self, name: &str, ns: Option<Namespaces>) -> Result<usize, Error>;
    fn with_action(&mut self, at: usize, f: &mut dyn FnMut(&mut liquers_core::pipeline::ActionEditor<'_>) -> Result<(), Error>) -> Result<(), Error>;
    fn base(&self) -> &CorePipelineEditor;
}

macro_rules! core_editor {
    ($t:ty, $base:expr) => {
        impl CoreEditor for $t {
            fn push(&mut self, e: PipelineElement) -> Result<(), Error> {
                <$t>::push(self, e).map(|_| ())
            }
            fn insert(&mut self, at: usize, e: PipelineElement) -> Result<(), Error> {
                <$t>::insert(self, at, e).map(|_| ())
            }
            fn replace(&mut self, at: usize, e: PipelineElement) -> Result<(), Error> {
                <$t>::replace(self, at, e).map(|_| ())
            }
            fn remove(&mut self, at: usize) -> Result<(), Error> {
                <$t>::remove(self, at).map(|_| ())
            }
            fn move_to(&mut self, from: usize, to: usize) -> Result<(), Error> {
                <$t>::move_to(self, from, to).map(|_| ())
            }
            fn set_realm(&mut self, realm: &str) -> Result<(), Error> {
                <$t>::set_realm(self, realm).map(|_| ())
            }
            fn set_filename(&mut self, name: Option<&str>) -> Result<(), Error> {
                <$t>::set_filename(self, name).map(|_| ())
            }
            fn resource(&mut self, key: &str, read: Read) -> Result<(), Error> {
                <$t>::resource(self, key, read).map(|_| ())
            }
            fn default_ns(&mut self, ns: Option<Namespaces>) {
                <$t>::default_ns(self, ns);
            }
            fn action(&mut self, name: &str, ns: Option<Namespaces>) -> Result<usize, Error> {
                let mut action = <$t>::action(self, name)?;
                if let Some(ns) = ns {
                    action.ns(ns)?;
                }
                Ok(action.index())
            }
            fn with_action(
                &mut self,
                at: usize,
                f: &mut dyn FnMut(&mut liquers_core::pipeline::ActionEditor<'_>) -> Result<(), Error>,
            ) -> Result<(), Error> {
                let mut action = <$t>::edit_action(self, at)?;
                f(&mut action)
            }
            fn base(&self) -> &CorePipelineEditor {
                let base: fn(&$t) -> &CorePipelineEditor = $base;
                base(self)
            }
        }
    };
}

core_editor!(CorePipelineEditor, |e| e);
core_editor!(CoreQueryEditor, |e| e.editor());

/// Locks `inner`, edits a clone with `f`, and keeps the clone only when every step succeeded.
fn edit<E: CoreEditor, R>(py: Python<'_>, inner: &Mutex<E>, f: impl FnOnce(&mut E) -> Result<R, Error>) -> PyResult<R> {
    let mut guard = inner
        .lock_py_attached(py)
        .map_err(|_| error_to_pyerr(py, &Error::unexpected_error("editor lock poisoned".to_string())))?;
    let mut copy = guard.clone();
    let result = f(&mut copy).map_err(|e| error_to_pyerr(py, &e))?;
    *guard = copy;
    Ok(result)
}

/// Reads `inner` under its lock.
fn read_editor<E: CoreEditor, R>(py: Python<'_>, inner: &Mutex<E>, f: impl FnOnce(&E) -> Result<R, Error>) -> PyResult<R> {
    let guard = inner
        .lock_py_attached(py)
        .map_err(|_| error_to_pyerr(py, &Error::unexpected_error("editor lock poisoned".to_string())))?;
    f(&guard).map_err(|e| error_to_pyerr(py, &e))
}

/// Whose step an `ActionEditor` edits.
enum Owner {
    Pipeline(Py<PipelineEditor>),
    Query(Py<QueryEditor>),
}

macro_rules! editor_methods {
    ($class:ident, $owner:ident { $($extra:tt)* }) => {
        #[pymethods]
        impl $class {
            /// Appends an element (query text, or a dict in the pipeline's JSON form).
            fn push<'py>(slf: Bound<'py, Self>, element: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
                let element = element_from_py(element)?;
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::push(e, element))?;
                Ok(slf)
            }
            /// Inserts an element at `at`.
            fn insert<'py>(slf: Bound<'py, Self>, at: usize, element: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
                let element = element_from_py(element)?;
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::insert(e, at, element))?;
                Ok(slf)
            }
            /// Replaces the element at `at`.
            fn replace<'py>(slf: Bound<'py, Self>, at: usize, element: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
                let element = element_from_py(element)?;
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::replace(e, at, element))?;
                Ok(slf)
            }
            /// Removes the element at `at`.
            fn remove(slf: Bound<'_, Self>, at: usize) -> PyResult<Bound<'_, Self>> {
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::remove(e, at))?;
                Ok(slf)
            }
            /// Moves the element at `from_` to `to`.
            fn move_to(slf: Bound<'_, Self>, from_: usize, to: usize) -> PyResult<Bound<'_, Self>> {
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::move_to(e, from_, to))?;
                Ok(slf)
            }
            /// Sets the realm.
            fn set_realm<'py>(slf: Bound<'py, Self>, realm: &str) -> PyResult<Bound<'py, Self>> {
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::set_realm(e, realm))?;
                Ok(slf)
            }
            /// Sets the output filename, or removes it with `None`.
            #[pyo3(signature = (name))]
            fn set_filename<'py>(slf: Bound<'py, Self>, name: Option<&str>) -> PyResult<Bound<'py, Self>> {
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::set_filename(e, name))?;
                Ok(slf)
            }
            /// Appends a resource read of `key`; `read` is `asset` (default), `binary`,
            /// `metadata`, `recipe`, `directory`, …
            #[pyo3(signature = (key, read="asset"))]
            fn resource<'py>(slf: Bound<'py, Self>, key: &str, read: &str) -> PyResult<Bound<'py, Self>> {
                let how = self::read(read).map_err(|e| error_to_pyerr(slf.py(), &e))?;
                edit(slf.py(), &slf.get().inner, |e| CoreEditor::resource(e, key, how))?;
                Ok(slf)
            }
            /// Sets the namespaces later actions are looked up in (`None`: the default ones).
            #[pyo3(signature = (ns))]
            fn default_ns<'py>(slf: Bound<'py, Self>, ns: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, Self>> {
                let ns = ns.map(namespaces).transpose()?;
                edit(slf.py(), &slf.get().inner, |e| {
                    CoreEditor::default_ns(e, ns);
                    Ok(())
                })?;
                Ok(slf)
            }
            /// Appends an action and returns an `lq.ActionEditor` on it. `ns` is a namespace or a
            /// list of them; the two steps succeed or fail together.
            #[pyo3(signature = (name, ns=None))]
            fn action(slf: Bound<'_, Self>, name: &str, ns: Option<&Bound<'_, PyAny>>) -> PyResult<ActionEditor> {
                let ns = ns.map(namespaces).transpose()?;
                let index = edit(slf.py(), &slf.get().inner, |e| CoreEditor::action(e, name, ns))?;
                Ok(ActionEditor { owner: Owner::$owner(slf.unbind()), index })
            }
            /// An `lq.ActionEditor` on the action at `at`.
            fn edit_action(slf: Bound<'_, Self>, at: usize) -> PyResult<ActionEditor> {
                read_editor(slf.py(), &slf.get().inner, |e| e.base().command_metadata(at).map(|_| ()))?;
                Ok(ActionEditor { owner: Owner::$owner(slf.unbind()), index: at })
            }
            /// The metadata of the command the action at `at` calls.
            fn command_metadata(&self, py: Python<'_>, at: usize) -> PyResult<crate::command_metadata::CommandMetadata> {
                read_editor(py, &self.inner, |e| {
                    e.base().command_metadata(at).cloned().map(crate::command_metadata::CommandMetadata)
                })
            }
            /// The argument `arg` (a name or a position) of the action at `at`.
            fn argument_info(&self, py: Python<'_>, at: usize, arg: &Bound<'_, PyAny>) -> PyResult<crate::command_metadata::ArgumentInfo> {
                let arg = arg_ref(arg)?;
                read_editor(py, &self.inner, |e| {
                    e.base().argument_info(at, arg).cloned().map(crate::command_metadata::ArgumentInfo)
                })
            }
            /// The arguments of the action at `at` that are not set yet.
            fn unset_arguments(&self, py: Python<'_>, at: usize) -> PyResult<Vec<crate::command_metadata::ArgumentInfo>> {
                read_editor(py, &self.inner, |e| {
                    e.base()
                        .unset_arguments(at)
                        .map(|args| args.into_iter().cloned().map(crate::command_metadata::ArgumentInfo).collect())
                })
            }
            /// Raises the first problem of the pipeline as it stands, if any.
            fn validate(&self, py: Python<'_>) -> PyResult<()> {
                read_editor(py, &self.inner, |e| e.base().validate())
            }
            /// The pipeline as it stands, an `lq.Pipeline`.
            #[getter]
            fn pipeline(&self, py: Python<'_>) -> PyResult<Pipeline> {
                read_editor(py, &self.inner, |e| Ok(Pipeline(e.base().pipeline().clone())))
            }
            $($extra)*
        }
    };
}

/// Edits a pipeline step by step, each edit checked against the environment's commands as they
/// were when the editor was made. Every method returns the editor, so edits chain; a refused edit
/// raises and leaves the editor as it was. Made by `env.pipeline_editor(pipeline=None)`.
#[pyclass(frozen, name = "PipelineEditor", module = "liquers")]
pub struct PipelineEditor {
    inner: Mutex<CorePipelineEditor>,
}

impl PipelineEditor {
    pub(crate) fn new(editor: CorePipelineEditor) -> Self {
        PipelineEditor { inner: Mutex::new(editor) }
    }
}

/// A `PipelineEditor` that keeps the pipeline a query at every step; `query()` gives it. Made by
/// `env.query_editor(query=None)`.
#[pyclass(frozen, name = "QueryEditor", module = "liquers")]
pub struct QueryEditor {
    inner: Mutex<CoreQueryEditor>,
}

impl QueryEditor {
    pub(crate) fn new(editor: CoreQueryEditor) -> Self {
        QueryEditor { inner: Mutex::new(editor) }
    }
}

editor_methods!(PipelineEditor, Pipeline {});
editor_methods!(QueryEditor, Query {
    /// The query as it stands, an `lq.Query`.
    fn query(&self, py: Python<'_>) -> PyResult<crate::query::Query> {
        read_editor(py, &self.inner, |e| Ok(crate::query::Query(e.query())))
    }
});

/// Edits one action of a `PipelineEditor` or `QueryEditor`. Every method is one edit of the owner,
/// checked against the action's command, and returns this editor; a refused edit raises and leaves
/// the owner as it was. An action editor whose step was removed raises `ParameterError`.
#[pyclass(frozen, name = "ActionEditor", module = "liquers")]
pub struct ActionEditor {
    owner: Owner,
    index: usize,
}

impl ActionEditor {
    fn apply(&self, py: Python<'_>, mut f: impl FnMut(&mut liquers_core::pipeline::ActionEditor<'_>) -> Result<(), Error>) -> PyResult<()> {
        let index = self.index;
        match &self.owner {
            Owner::Pipeline(owner) => edit(py, &owner.get().inner, |e| e.with_action(index, &mut f)),
            Owner::Query(owner) => edit(py, &owner.get().inner, |e| e.with_action(index, &mut f)),
        }
    }

    fn inspect<R>(&self, py: Python<'_>, f: impl FnOnce(&CorePipelineEditor, usize) -> Result<R, Error>) -> PyResult<R> {
        let index = self.index;
        match &self.owner {
            Owner::Pipeline(owner) => read_editor(py, &owner.get().inner, |e| f(e.base(), index)),
            Owner::Query(owner) => read_editor(py, &owner.get().inner, |e| f(e.base(), index)),
        }
    }
}

#[pymethods]
impl ActionEditor {
    /// The action's position in the pipeline.
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Sets the namespaces the action is looked up in (a str or a list).
    fn ns<'py>(slf: Bound<'py, Self>, ns: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let ns = namespaces(ns)?;
        slf.get().apply(slf.py(), |a| a.ns(ns.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Sets argument `arg` (a name or a position) to `value`, any JSON-shaped Python value.
    fn set<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        let value = json_slot(value, &JsonPath::root()).map_err(|e| error_to_pyerr(slf.py(), &e))?;
        slf.get().apply(slf.py(), |a| a.set(arg.clone(), value.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Binds argument `arg` to the pipeline command's placeholder `name`.
    fn placeholder<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        slf.get().apply(slf.py(), |a| a.placeholder(arg.clone(), name).map(|_| ()))?;
        Ok(slf)
    }

    /// Sets argument `arg` to a link: the value of `query` (a str or an `lq.Query`).
    fn link<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        let query = crate::environment::to_query(query)
            .map(|q| q.encode())
            .map_err(|e| error_to_pyerr(slf.py(), &e))?;
        slf.get().apply(slf.py(), |a| a.link(arg.clone(), &query).map(|_| ()))?;
        Ok(slf)
    }

    /// Sets argument `arg` to the result of a nested pipeline.
    fn pipeline<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>, p: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        let p = pipeline_from_py(p)?;
        slf.get().apply(slf.py(), |a| a.pipeline(arg.clone(), p.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Sets argument `arg` to the pipeline command's input state.
    fn state<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        slf.get().apply(slf.py(), |a| a.state(arg.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Appends `value` as the next positional parameter.
    fn push<'py>(slf: Bound<'py, Self>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let value = json_slot(value, &JsonPath::root()).map_err(|e| error_to_pyerr(slf.py(), &e))?;
        slf.get().apply(slf.py(), |a| a.push(value.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Removes argument `arg`.
    fn unset<'py>(slf: Bound<'py, Self>, arg: &Bound<'py, PyAny>) -> PyResult<Bound<'py, Self>> {
        let arg = arg_ref(arg)?;
        slf.get().apply(slf.py(), |a| a.unset(arg.clone()).map(|_| ()))?;
        Ok(slf)
    }

    /// Changes the action's command, keeping the arguments the new command also has.
    fn rename<'py>(slf: Bound<'py, Self>, action: &str) -> PyResult<Bound<'py, Self>> {
        slf.get().apply(slf.py(), |a| a.rename(action).map(|_| ()))?;
        Ok(slf)
    }

    /// The metadata of the action's command.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> PyResult<crate::command_metadata::CommandMetadata> {
        self.inspect(py, |e, at| e.command_metadata(at).cloned().map(crate::command_metadata::CommandMetadata))
    }

    /// The argument `arg` (a name or a position) of the action's command.
    fn argument_info(&self, py: Python<'_>, arg: &Bound<'_, PyAny>) -> PyResult<crate::command_metadata::ArgumentInfo> {
        let arg = arg_ref(arg)?;
        self.inspect(py, |e, at| e.argument_info(at, arg).cloned().map(crate::command_metadata::ArgumentInfo))
    }

    fn __repr__(&self) -> String {
        format!("ActionEditor(index={})", self.index)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Pipeline>()?;
    m.add_class::<PipelineEditor>()?;
    m.add_class::<QueryEditor>()?;
    m.add_class::<ActionEditor>()?;
    Ok(())
}
