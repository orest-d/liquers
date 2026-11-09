//! Templates from Python: `render`, `expand` and `validate_template` on an environment and on a
//! command's context, and the classes they return.
//!
//! A call turns its Python arguments into a `RenderRequest` (`liquers_lib::template`) and runs it.
//! At the top level the render is a closure asset (`render_top_level`), so its query tags are
//! dependencies of an asset and recovered errors land in that asset's log; inside a command it runs
//! in the command's own context (`render_in_context`), so the tags are the command's dependencies.
//! `data` is read in place through the data-node adapter (`data_node.rs`), so a `dict` holding a
//! `datetime` renders its other fields.
//!
//! # Main components
//!
//! - [`build_request`]: the Python arguments as a `RenderRequest` (template source, frames, error
//!   mode, cwd, escaping). `TypeError` for an argument of the wrong kind, before any asset exists.
//! - [`RenderResult`] (`lq.RenderResult`): the text and the warnings, returned with `full=True`.
//! - [`Diagnostic`] (`lq.Diagnostic`): one problem `validate_template` found.
//! - [`rendered_to_py`]: a render's outcome as `str` or `RenderResult`; a render cancelled before
//!   it ran raises `CancelledError`.
//!
//! # Where it is used
//!
//! - `environment.rs`: `Environment.render`, `expand`, `validate_template` and the `_async` forms.
//! - `context.rs`: the same on `Context`, inside a command.
//! - `aio.rs`: `ToPy::Rendered`, an awaited render.
//!
//! # Related concepts
//!
//! - `liquers_lib::template::top_level`: `RenderRequest`, `render_in_context`, `render_top_level`.
//! - `closure.rs`: the waits for a top-level render's closure asset.
//! - `specs/reference/PYTHON_INTEGRATION.md` §Templates; `specs/guides/PYTHON_GUIDE.md`.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_TEMPLATE_top_level.py` (a Rust doctest cannot start the
//! interpreter this crate links against):
//!
//! ```python
//! env = lq.Environment()
//! assert env.expand("{{name}}!", data={"name": "Ann"}) == "Ann!"
//! r = env.expand("ok {{$ missing}}", errors="silent", full=True)
//! assert r.text == "ok " and len(r.warnings) == 1
//! env.shutdown()
//! ```

use std::sync::Arc;

use liquers_core::data_node::{AnyNode, DataNode, NodeKind};
use liquers_core::error::Error;
use liquers_core::metadata::Metadata;
use liquers_core::query::{Key, Query};
use liquers_core::state::State as CoreState;
use liquers_lib::template::{FrameData, RenderRequest, Rendered, TemplateSource};
use liquers_lib::template::{ErrorMode, FrameKind};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::error::error_to_pyerr;
use crate::value::{py_to_value, ConversionMode};

/// Which call a request is for: `render` puts `state` below `data`; `expand` has `data` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Call {
    Render,
    Expand,
}

impl Call {
    /// The method's name, for the blocking-call message.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Call::Render => "render",
            Call::Expand => "expand",
        }
    }
}

/// The Python arguments of a render call.
pub(crate) struct Arguments<'a, 'py> {
    pub template: &'a Bound<'py, PyAny>,
    pub state: Option<&'a Bound<'py, PyAny>>,
    pub data: Option<&'a Bound<'py, PyAny>>,
    pub errors: &'a str,
    pub cwd: Option<&'a Bound<'py, PyAny>>,
    pub html: bool,
}

fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_string())
}

/// A template argument: a `str` is the text, an `lq.Query` a link, an `lq.Key` the link `-R/<key>`.
pub(crate) fn template_source(object: &Bound<'_, PyAny>) -> PyResult<TemplateSource> {
    if let Ok(text) = object.cast::<PyString>() {
        return Ok(TemplateSource::Text(text.to_string()));
    }
    if let Ok(query) = object.cast::<crate::query::Query>() {
        return Ok(TemplateSource::Link(query.borrow().0.clone()));
    }
    if let Ok(key) = object.cast::<crate::query::Key>() {
        return Ok(TemplateSource::Link(Query::from(&key.borrow().0)));
    }
    Err(PyTypeError::new_err(format!(
        "a template must be a str (the text), an lq.Query or an lq.Key (a link), not {}",
        type_name(object)
    )))
}

/// The frames of a call, bottom first. `render`: `state` (`Data`), then `data` (`Names`).
/// `expand`: `data` (`Data`).
fn frames(
    py: Python<'_>,
    call: Call,
    state: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    conversion: ConversionMode,
    pickle: bool,
) -> PyResult<Vec<(FrameData, FrameKind)>> {
    let mut frames = Vec::new();
    if let Some(state) = state.filter(|s| !s.is_none()) {
        let core = match state.cast::<crate::asset::State>() {
            Ok(wrapped) => wrapped.get().core().clone(),
            Err(_) => {
                let value = py_to_value(state, conversion, pickle).map_err(|e| {
                    PyTypeError::new_err(format!("the state cannot be converted: {}", e.message))
                })?;
                CoreState::from_value_and_metadata(value, Arc::new(Metadata::new()))
            }
        };
        frames.push((FrameData::State(core), FrameKind::Data));
    }
    if let Some(data) = data.filter(|d| !d.is_none()) {
        let node = crate::data_node::node_of(py, data);
        if node.kind() != NodeKind::Map {
            return Err(PyTypeError::new_err(format!(
                "data must be a mapping (a dict, or a class reading as one), not {}",
                type_name(data)
            )));
        }
        let kind = match call {
            Call::Render => FrameKind::Names,
            Call::Expand => FrameKind::Data,
        };
        frames.push((FrameData::Node(AnyNode::Sync(Arc::new(node))), kind));
    }
    Ok(frames)
}

/// The `cwd` argument: `None` is the root.
fn cwd(object: Option<&Bound<'_, PyAny>>) -> PyResult<Key> {
    match object.filter(|c| !c.is_none()) {
        None => Ok(Key::new()),
        Some(object) => match object.cast::<crate::query::Key>() {
            Ok(key) => Ok(key.borrow().0.clone()),
            Err(_) => Err(PyTypeError::new_err(format!(
                "cwd must be an lq.Key, not {}",
                type_name(object)
            ))),
        },
    }
}

/// The request of a call. Raises `TypeError` for an argument of the wrong kind and `GeneralError`
/// for an unknown error mode, before anything runs.
pub(crate) fn build_request(
    py: Python<'_>,
    call: Call,
    arguments: Arguments<'_, '_>,
    conversion: ConversionMode,
    pickle: bool,
) -> PyResult<RenderRequest> {
    let template = template_source(arguments.template)?;
    let cwd = cwd(arguments.cwd)?;
    let errors = ErrorMode::parse(arguments.errors).map_err(|e| error_to_pyerr(py, &e))?;
    let frames = frames(py, call, arguments.state, arguments.data, conversion, pickle)?;
    Ok(RenderRequest {
        template,
        frames,
        errors,
        cwd,
        html_escape: arguments.html,
    })
}

/// The error a render cancelled before it started raises.
pub(crate) fn not_started() -> Error {
    Error::cancelled("the render was cancelled before it started".to_string())
}

/// A render's outcome as Python: the text, or with `full` an `lq.RenderResult`.
pub(crate) fn rendered_to_py(py: Python<'_>, rendered: Rendered, full: bool) -> PyResult<Py<PyAny>> {
    if full {
        Ok(Py::new(py, RenderResult::from(rendered))?.into_any())
    } else {
        Ok(PyString::new(py, &rendered.text).into_any().unbind())
    }
}

/// The text of a render and the errors it recovered from, as warnings with their positions.
///
/// Returned by `render` / `expand` with `full=True`. The same warnings are in the log of the asset
/// the render ran in (the command's, or the top-level render's own).
#[pyclass(frozen, name = "RenderResult", module = "liquers")]
pub struct RenderResult {
    /// The rendered text.
    #[pyo3(get)]
    text: String,
    warnings: Vec<liquers_core::metadata::LogEntry>,
}

impl From<Rendered> for RenderResult {
    fn from(rendered: Rendered) -> Self {
        RenderResult {
            text: rendered.text,
            warnings: rendered.warnings,
        }
    }
}

#[pymethods]
impl RenderResult {
    /// One `lq.LogEntry` per recovered error, with its template position.
    #[getter]
    fn warnings(&self) -> Vec<crate::metadata::LogEntry> {
        self.warnings
            .iter()
            .cloned()
            .map(|inner| crate::metadata::LogEntry { inner })
            .collect()
    }

    fn __str__(&self) -> String {
        self.text.clone()
    }

    fn __repr__(&self) -> String {
        let count = self.warnings.len();
        let plural = if count == 1 { "" } else { "s" };
        format!("RenderResult({:?}, {count} warning{plural})", self.text)
    }
}

/// One problem `validate_template` found: its severity (`"error"` or `"warning"`), a message and
/// its position in the template.
#[pyclass(frozen, name = "Diagnostic", module = "liquers")]
pub struct Diagnostic {
    /// `"error"` or `"warning"`.
    #[pyo3(get)]
    severity: String,
    /// What is wrong.
    #[pyo3(get)]
    message: String,
    position: liquers_core::query::Position,
}

impl From<liquers_lib::template::Diagnostic> for Diagnostic {
    fn from(diagnostic: liquers_lib::template::Diagnostic) -> Self {
        let severity = match diagnostic.severity {
            liquers_lib::template::Severity::Error => "error",
            liquers_lib::template::Severity::Warning => "warning",
        };
        Diagnostic {
            severity: severity.to_string(),
            message: diagnostic.message,
            position: diagnostic.position,
        }
    }
}

#[pymethods]
impl Diagnostic {
    /// Where the problem is, as an `lq.Position` (line and column count from 1).
    #[getter]
    fn position(&self) -> crate::query::Position {
        crate::query::Position(self.position.clone())
    }

    fn __repr__(&self) -> String {
        format!(
            "Diagnostic({}, {:?}, line {}, column {})",
            self.severity, self.message, self.position.line, self.position.column
        )
    }
}

/// Every problem in template `text`, checked against `registry`; nothing is evaluated.
pub(crate) fn validate(
    text: &str,
    registry: &liquers_core::command_metadata::CommandMetadataRegistry,
) -> Vec<Diagnostic> {
    liquers_lib::template::validate_template(text, liquers_lib::template::TemplateSyntax::Mustache, Some(registry))
        .into_iter()
        .map(Diagnostic::from)
        .collect()
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RenderResult>()?;
    m.add_class::<Diagnostic>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval<'py>(py: Python<'py>, code: &str) -> Result<Bound<'py, PyAny>, Box<dyn std::error::Error>> {
        Ok(py.eval(&std::ffi::CString::new(code)?, None, None)?)
    }

    fn request(
        py: Python<'_>,
        call: Call,
        state: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<RenderRequest> {
        let template = PyString::new(py, "{{.}}").into_any();
        build_request(
            py,
            call,
            Arguments { template: &template, state, data, errors: "strict", cwd: None, html: false },
            ConversionMode::Permissive,
            false,
        )
    }

    fn kinds(request: &RenderRequest) -> Vec<FrameKind> {
        request.frames.iter().map(|(_, kind)| *kind).collect()
    }

    #[test]
    fn frame_layouts() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let state = eval(py, "'x'")?;
            let data = eval(py, "{'a': 1}")?;
            let render = request(py, Call::Render, Some(&state), Some(&data))?;
            assert_eq!(kinds(&render), [FrameKind::Data, FrameKind::Names]);
            let expand = request(py, Call::Expand, None, Some(&data))?;
            assert_eq!(kinds(&expand), [FrameKind::Data]);
            let none = py.None().into_bound(py);
            let empty = request(py, Call::Render, Some(&none), Some(&none))?;
            assert!(empty.frames.is_empty());
            Ok(())
        })
    }

    /// A render whose asset was cancelled before it started (`Ok(None)`) raises a cancellation,
    /// in the awaited form as in the sync one (`environment.rs` maps `None` with `not_started`).
    #[test]
    fn unstarted_render_is_cancelled() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        assert!(not_started().is_cancelled());
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let awaited = crate::aio::ToPy::Rendered(Ok((None, false))).into_pyobject(py);
            assert!(awaited.is_err());
            Ok(())
        })
    }

    #[test]
    fn wrong_kinds_raise_type_error() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let list = eval(py, "[1, 2]")?;
            let error = request(py, Call::Expand, None, Some(&list)).err().ok_or("no error")?;
            assert!(error.is_instance_of::<PyTypeError>(py));
            let number = eval(py, "42")?;
            assert!(template_source(&number).is_err());
            let text = eval(py, "'x'")?;
            assert!(cwd(Some(&text)).is_err());
            Ok(())
        })
    }
}
