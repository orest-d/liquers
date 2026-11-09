//! `lq.State` and `lq.AssetRef`: results, and the handle on an evaluation.
//!
//! A command whose first parameter is named `state` receives a `State` (`StateDelivery::State`),
//! and `Environment.evaluate_state` returns one. It is a read-only snapshot (VALUE13): the value is
//! converted once, on the first read of `value` (so a conversion error is raised there, and a value
//! read only through `data_node()` is never converted), and the metadata is a copy.
//!
//! `Environment.get_asset` returns an `AssetRef` without waiting: its status, metadata and log can
//! be read while it runs, and `cancel()` stops it. With the queued asset manager a `Submitted`
//! asset that is cancelled ends `Cancelled` and its command never runs (EVAL06).
//!
//! `get_async`, `get_state_async` and `cancel_async` are the awaitable forms; awaiting them never
//! blocks the caller's loop, and cancelling the awaitable ends only the wait (`aio.rs`).
//!
//! # Main components
//!
//! - [`State`] (`lq.State`): a value and a copy of its metadata; `value` converts lazily,
//!   `record_view()` and `data_node()` read the Liquers value in place.
//! - [`AssetRef`] (`lq.AssetRef`): a running or finished evaluation; `get`, `get_state`, `status`,
//!   `metadata`, `cancel` and their `_async` forms.
//!
//! # Where it is used
//!
//! - `environment.rs`: `evaluate_state`, `get_asset` and their `_async` forms.
//! - `command.rs`: a command whose first parameter is `state` receives a `State`.
//! - `data_node.rs`: `lq.data_node(state)` reads a state's value without converting it.
//!
//! # Related concepts
//!
//! - `liquers_core::state::State`, `liquers_core::assets::AssetRef`; `specs/reference/ASSETS.md`.
//! - `specs/reference/PYTHON_INTEGRATION.md` §5 (when a state converts) and §7 (waits).
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_EVAL_*.py` (a Rust doctest cannot start the interpreter):
//!
//! ```python
//! state = env.evaluate_state("hello")
//! assert state.value == "hello" and state.metadata.status() == "Ready"
//! asset = env.get_asset("hello")
//! assert asset.get() == "hello"
//! ```

use std::sync::Arc;

use liquers_core::assets::AssetRef as CoreAssetRef;
use liquers_core::error::Error;
use liquers_core::state::State as CoreState;
use pyo3::prelude::*;

use crate::aio::{awaitable, check_blocking_call, Shut, ToPy};
use crate::environment::{block_on, PyEnvironment};
use crate::error::{error_to_pyerr, pyerr_to_error};
use crate::metadata::{Metadata, Status};
use crate::records::Scope;
use crate::value::{value_to_py, Value};

/// A value and its metadata.
#[pyclass(frozen, name = "State", module = "liquers")]
pub struct State {
    /// The core state; the value is read from it, converted at most once.
    core: CoreState<Value>,
    /// Where a record source in the value resolves.
    scope: Scope,
    value: pyo3::sync::PyOnceLock<Py<PyAny>>,
    metadata: Py<Metadata>,
    /// The record view behind the value, kept whatever its shape (`record_view()`).
    record_view: Option<Arc<dyn liquers_records::RecordView>>,
}

impl State {
    /// A state over `state`. Nothing is converted until `value` is read; the `Result` is the
    /// `Py::new` calls'.
    pub fn from_core(py: Python<'_>, state: &CoreState<Value>, scope: &Scope) -> Result<Py<State>, Error> {
        let metadata = Py::new(
            py,
            Metadata {
                inner: state.metadata.as_ref().clone(),
            },
        )
        .map_err(|e| pyerr_to_error(py, &e))?;
        let record_view = match state.data_unchecked().as_ref() {
            Value::Extended(liquers_lib::value::ExtValue::RecordView { value: view }) => Some(view.clone()),
            Value::Extended(_) | Value::Base(_) => None,
        };
        Py::new(
            py,
            State {
                core: state.clone(),
                scope: scope.clone(),
                value: pyo3::sync::PyOnceLock::new(),
                metadata,
                record_view,
            },
        )
        .map_err(|e| pyerr_to_error(py, &e))
    }

    /// The core state, for the data node view.
    pub(crate) fn core(&self) -> &CoreState<Value> {
        &self.core
    }

    fn converted(&self, py: Python<'_>) -> PyResult<&Py<PyAny>> {
        self.value.get_or_try_init(py, || {
            value_to_py(py, self.core.data_unchecked().as_ref(), &self.scope)
                .map_err(|e| error_to_pyerr(py, &e))
        })
    }
}

#[pymethods]
impl State {
    /// The value, as a Python object. Converted on the first read, which raises the conversion's
    /// error when the value cannot be converted.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.converted(py)?.clone_ref(py))
    }

    /// The value as an `lq.DataNode`, read in place: nothing is converted.
    fn data_node(&self) -> crate::data_node::DataNodeView {
        crate::data_node::DataNodeView::of_value(self.core.data_unchecked().clone())
    }

    /// The state's `Metadata`.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<Metadata> {
        self.metadata.clone_ref(py)
    }

    /// The value as an `lq.RecordView` whatever its shape (a one-cell view included, which
    /// `value` gives as its scalar), or `None` when the value is not a record view.
    fn record_view(&self) -> Option<crate::records::view::RecordView> {
        self.record_view.clone().map(crate::records::view::RecordView::new)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let Ok(value) = self.converted(py) else {
            return "State(<unconvertible>)".to_string();
        };
        let value = value
            .bind(py)
            .repr()
            .map(|r| r.to_string())
            .unwrap_or_else(|_| "<value>".to_string());
        format!("State({value})")
    }
}

/// A handle on one evaluation.
#[pyclass(frozen, name = "AssetRef", module = "liquers")]
pub struct AssetRef {
    inner: CoreAssetRef<PyEnvironment>,
    /// The shut signal of the environment that made it, so a wait ends when that is shut down.
    shut: Arc<Shut>,
    /// Where a record source in its value resolves.
    scope: Scope,
}

impl AssetRef {
    pub fn new(inner: CoreAssetRef<PyEnvironment>, shut: Arc<Shut>, scope: Scope) -> Self {
        AssetRef { inner, shut, scope }
    }
}

#[pymethods]
impl AssetRef {
    /// Waits for the value. Raises the asset's error if it failed or was cancelled.
    fn get(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "get")?;
        let asset = self.inner.clone();
        let value = block_on(py, &self.shut, async move { asset.get().await?.value() })
            .map_err(|e| e.into_pyerr(py))?;
        value_to_py(py, &value, &self.scope).map_err(|e| error_to_pyerr(py, &e))
    }

    /// Waits for the result and returns it as a `State`, metadata included.
    fn get_state(&self, py: Python<'_>) -> PyResult<Py<State>> {
        check_blocking_call(py, "get_state")?;
        let asset = self.inner.clone();
        let state = block_on(py, &self.shut, async move { asset.get().await?.value_state() })
            .map_err(|e| e.into_pyerr(py))?;
        State::from_core(py, &state, &self.scope).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The current status, without waiting.
    #[getter]
    fn status(&self, py: Python<'_>) -> PyResult<Status> {
        let asset = self.inner.clone();
        block_on(py, &self.shut, async move { Ok(asset.status().await) })
            .map(|inner| Status { inner })
            .map_err(|e| e.into_pyerr(py))
    }

    /// The current metadata, log included, without waiting.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> PyResult<Metadata> {
        let asset = self.inner.clone();
        block_on(py, &self.shut, async move { asset.get_metadata().await })
            .map(|inner| Metadata { inner })
            .map_err(|e| e.into_pyerr(py))
    }

    /// Requests cancellation. An asset still waiting for a slot ends `Cancelled` without running.
    fn cancel(&self, py: Python<'_>) -> PyResult<()> {
        check_blocking_call(py, "cancel")?;
        let asset = self.inner.clone();
        block_on(py, &self.shut, async move { asset.cancel().await }).map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `get`.
    fn get_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let asset = self.inner.clone();
        let scope = self.scope.clone();
        awaitable(py, &self.shut, async move { Ok((asset.get().await?.value()?, scope)) }, ToPy::Value)
    }

    /// Awaitable `get_state`.
    fn get_state_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let asset = self.inner.clone();
        let scope = self.scope.clone();
        awaitable(py, &self.shut, async move { Ok((asset.get().await?.value_state()?, scope)) }, ToPy::State)
    }

    /// Awaitable `cancel`: the explicit cancel, since cancelling an awaitable only detaches it.
    fn cancel_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let asset = self.inner.clone();
        awaitable(py, &self.shut, async move { asset.cancel().await }, ToPy::Unit)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match self.status(py) {
            Ok(status) => format!("AssetRef({})", status.name()),
            Err(_) => "AssetRef(?)".to_string(),
        }
    }
}
