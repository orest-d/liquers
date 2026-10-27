//! `lq.Context`: the execution context a Python command receives.
//!
//! A command asks for it by naming a parameter `context`. It is valid only during the call that
//! received it: the command closure invalidates it when the callable returns, so a context kept
//! past that raises `GeneralError` instead of acting on an asset that has moved on.
//!
//! `evaluate` is nested evaluation. It records the query as a dependency of the current asset,
//! and it releases the GIL while it waits, so the nested command — Python or Rust — can run on
//! another thread (AC-13). An `async def` command uses `evaluate_async` instead: the sync form
//! raises there, since it would block the loop the command runs on.

use std::sync::{Arc, Mutex};

use liquers_core::context::Context as CoreContext;
use liquers_core::error::Error;
use pyo3::prelude::*;

use crate::aio::{awaitable, check_blocking_call, Shut, ToPy};
use crate::environment::{block_on, PyEnvironment};
use crate::error::{error_to_pyerr, PyFailure};
use crate::value::value_to_py;

/// The execution context of a running Python command.
#[pyclass(frozen, name = "Context", module = "liquers")]
pub struct Context {
    inner: Arc<Mutex<Option<CoreContext<PyEnvironment>>>>,
    /// The environment's shut signal, so a nested wait ends when it is shut down.
    shut: Arc<Shut>,
}

impl Context {
    pub fn new(context: CoreContext<PyEnvironment>, shut: Arc<Shut>) -> Self {
        Context {
            inner: Arc::new(Mutex::new(Some(context))),
            shut,
        }
    }

    /// Another handle on the same context; invalidating one invalidates both.
    pub fn clone_handle(&self) -> Self {
        Context {
            inner: self.inner.clone(),
            shut: self.shut.clone(),
        }
    }

    /// Ends the context's validity. Called when the command returns.
    pub fn invalidate(&self) {
        match self.inner.lock() {
            Ok(mut inner) => *inner = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
    }

    fn get(&self) -> Result<CoreContext<PyEnvironment>, Error> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::unexpected_error("context lock poisoned".to_string()))?;
        inner.clone().ok_or_else(|| {
            Error::general_error(
                "the context is used after its command returned; it is valid only during the call"
                    .to_string(),
            )
        })
    }
}

#[pymethods]
impl Context {
    /// Evaluates a query as a dependency of the current asset and returns its value.
    fn evaluate(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "evaluate")?;
        let result = (|| -> Result<_, PyFailure> {
            let query = crate::environment::to_query(query)?;
            let context = self.get()?;
            let value = block_on(py, &self.shut, async move {
                let asset = context.evaluate(&query).await?;
                let state = context.wait_for_dependency(&asset).await?;
                state.value()
            })?;
            Ok(value_to_py(py, &value)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `evaluate`, for `async def` commands.
    fn evaluate_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let prepared = (|| -> Result<_, Error> {
            crate::runtime::runtime()?;
            Ok((crate::environment::to_query(query)?, self.get()?))
        })();
        let (query, context) = prepared.map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(
            py,
            &self.shut,
            async move {
                let asset = context.evaluate(&query).await?;
                let state = context.wait_for_dependency(&asset).await?;
                state.value()
            },
            ToPy::Value,
        )
    }

    /// Adds an info entry to the asset's log.
    fn info(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.info(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Adds a warning to the asset's log.
    fn warning(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.warning(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Adds an error-level entry to the asset's log, which also fails the evaluation: the command
    /// keeps running, and once it returns the query raises `ExecutionError` with the first such
    /// message.
    fn error(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.error(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Whether cancellation of the asset has been requested. A command that sees `True` should
    /// stop, for example by raising `lq.CancelledError`.
    fn is_cancelled(&self, py: Python<'_>) -> PyResult<bool> {
        self.get()
            .map(|context| context.is_cancelled())
            .map_err(|e| error_to_pyerr(py, &e))
    }
}
