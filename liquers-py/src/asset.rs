//! `lq.State` and `lq.AssetRef`: results, and the handle on an evaluation.
//!
//! A command whose first parameter is named `state` receives a `State` (`StateDelivery::State`),
//! and `Environment.evaluate_state` returns one. It is a read-only snapshot (VALUE13): the value is
//! converted once, when the state is made, and the metadata is a copy.
//!
//! `Environment.get_asset` returns an `AssetRef` without waiting: its status, metadata and log can
//! be read while it runs, and `cancel()` stops it. With the queued asset manager a `Submitted`
//! asset that is cancelled ends `Cancelled` and its command never runs (EVAL06).
//!
//! `get_async`, `get_state_async` and `cancel_async` are the awaitable forms; awaiting them never
//! blocks the caller's loop, and cancelling the awaitable ends only the wait (`aio.rs`).

use std::sync::Arc;

use liquers_core::assets::AssetRef as CoreAssetRef;
use liquers_core::error::Error;
use liquers_core::state::State as CoreState;
use pyo3::prelude::*;

use crate::aio::{awaitable, check_blocking_call, Shut, ToPy};
use crate::environment::{block_on, PyEnvironment};
use crate::error::{error_to_pyerr, pyerr_to_error};
use crate::metadata::{Metadata, Status};
use crate::value::{value_to_py, Value};

/// A value and its metadata.
#[pyclass(frozen, name = "State", module = "liquers")]
pub struct State {
    value: Py<PyAny>,
    metadata: Py<Metadata>,
}

impl State {
    pub fn from_core(py: Python<'_>, state: &CoreState<Value>) -> Result<Py<State>, Error> {
        let value = value_to_py(py, state.data_unchecked().as_ref())?;
        let metadata = Py::new(
            py,
            Metadata {
                inner: state.metadata.as_ref().clone(),
            },
        )
        .map_err(|e| pyerr_to_error(py, &e))?;
        Py::new(py, State { value, metadata }).map_err(|e| pyerr_to_error(py, &e))
    }
}

#[pymethods]
impl State {
    /// The value, as a Python object.
    #[getter]
    fn value(&self, py: Python<'_>) -> Py<PyAny> {
        self.value.clone_ref(py)
    }

    /// The state's `Metadata`.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<Metadata> {
        self.metadata.clone_ref(py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let value = self
            .value
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
}

impl AssetRef {
    pub fn new(inner: CoreAssetRef<PyEnvironment>, shut: Arc<Shut>) -> Self {
        AssetRef { inner, shut }
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
        value_to_py(py, &value).map_err(|e| error_to_pyerr(py, &e))
    }

    /// Waits for the result and returns it as a `State`, metadata included.
    fn get_state(&self, py: Python<'_>) -> PyResult<Py<State>> {
        check_blocking_call(py, "get_state")?;
        let asset = self.inner.clone();
        let state = block_on(py, &self.shut, async move { asset.get().await?.value_state() })
            .map_err(|e| e.into_pyerr(py))?;
        State::from_core(py, &state).map_err(|e| error_to_pyerr(py, &e))
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
        awaitable(py, &self.shut, async move { asset.get().await?.value() }, ToPy::Value)
    }

    /// Awaitable `get_state`.
    fn get_state_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let asset = self.inner.clone();
        awaitable(py, &self.shut, async move { asset.get().await?.value_state() }, ToPy::State)
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
