//! `env.serve()`: an `lq.Environment` served over the Liquers HTTP API.
//!
//! The server is liquers-axum's [`LiquersServer`], bound on the process runtime from the `server`
//! section of the environment's configuration plus `serve`'s keyword overrides. It answers each
//! request from the environment's current build ([`CellSource`]), so a command registered after
//! `serve` is reachable at once, and never takes the GIL unless a request needs a rebuild or runs a
//! Python command.
//!
//! # Main components
//!
//! - [`Server`] (`lq.Server`): the handle `serve` returns; `address`, `port`, `url`, `running`,
//!   `shutdown(timeout=5.0)`, `shutdown_async`, a context manager. Dropping it does not stop the
//!   server.
//! - [`start`]: binds a server for an environment and records it in the environment's list and in
//!   the process list.
//! - [`stop_all`]: stops every server of the process; run at interpreter exit before commands are
//!   cancelled and before the async-command loops stop.
//! - [`CellSource`]: the environment per request: the current build without the GIL, or one
//!   rebuild at a time on a blocking thread after a registration.
//!
//! # Where it is used
//!
//! - `Environment.serve`, `Environment.serve_async` and `Environment.servers` (`environment.rs`).
//! - `EnvCell::shutdown` stops the environment's servers first; `exit::at_exit` and the loop pool's
//!   exit hook call [`stop_all`].
//! - `tests/test_WEBSERV_server.py`.
//!
//! # Related concepts
//!
//! - [`liquers_axum::server`] is the server itself, shared with Rust hosts; this module only
//!   connects it to a Python environment's lifetime.
//! - [`crate::config::PyConfig`] holds the `server` section the overrides start from.
//! - `evaluate` runs a query in-process; the server runs the same queries for HTTP clients.
//!
//! # Examples
//!
//! ```python
//! import liquers as lq, urllib.request
//! env = lq.Environment()
//! @env.first_command
//! def greet(name: str = "world") -> str:
//!     return f"Hello, {name}!"
//! with env.serve(port=0) as server:
//!     assert urllib.request.urlopen(f"{server.url}/liquer/q/greet-Ann").read() == b"Hello, Ann!"
//! ```
//!
//! More: `liquers-py/tests/test_WEBSERV_server.py` and `docs/guides/python/12-serving.md`.

use std::sync::{Arc, Mutex, Once, Weak};
use std::time::Duration;

use async_trait::async_trait;
use liquers_axum::{EnvSource, LiquersServer, LiquersServerBuilder, ServerConfig};
use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use pyo3::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::environment::{EnvCell, PyEnvironment};
use crate::error::error_to_pyerr;

/// How long `env.shutdown()` lets each of its servers' requests finish.
pub(crate) const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

/// How long interpreter exit lets the servers' requests finish.
pub(crate) const EXIT_DEADLINE: Duration = Duration::from_secs(1);

/// The environment a request is answered with.
pub(crate) struct CellSource {
    cell: Arc<EnvCell>,
    rebuild: tokio::sync::Mutex<()>,
}

impl CellSource {
    pub(crate) fn new(cell: Arc<EnvCell>) -> Self {
        CellSource { cell, rebuild: tokio::sync::Mutex::new(()) }
    }
}

#[async_trait]
impl EnvSource<PyEnvironment> for CellSource {
    async fn current(&self) -> Result<EnvRef<PyEnvironment>, Error> {
        if let Some(envref) = self.cell.current_without_gil()? {
            return Ok(envref);
        }
        // A registration retired the build: one request rebuilds, the others wait for it.
        let _one_at_a_time = self.rebuild.lock().await;
        if let Some(envref) = self.cell.current_without_gil()? {
            return Ok(envref);
        }
        if crate::exit::exiting() {
            return Err(Error::cancelled("the Python interpreter is exiting"));
        }
        let cell = self.cell.clone();
        tokio::task::spawn_blocking(move || crate::exit::attach_tracked(|py| cell.envref(py)))
            .await
            .map_err(|e| Error::unexpected_error(format!("the environment rebuild ended abnormally: {e}")))?
    }
}

/// A running server, shared by its handles, its environment and the process list.
pub(crate) struct ServerSlot {
    pub(crate) server: LiquersServer,
}

impl ServerSlot {
    /// Stops the server and waits, without needing a tokio context (only tokens are awaited).
    pub(crate) fn stop_blocking(&self, deadline: Duration) {
        futures::executor::block_on(self.server.shutdown(deadline));
    }
}

/// Every server started in this process, for the exit hook.
static SERVERS: Mutex<Vec<Weak<ServerSlot>>> = Mutex::new(Vec::new());

fn servers() -> std::sync::MutexGuard<'static, Vec<Weak<ServerSlot>>> {
    match SERVERS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Binds a server for `cell` and records it. Async: runs on the process runtime.
///
/// If the environment was shut down while the server was binding, the server is stopped again
/// and the call fails with the shut-down error, so no listener outlives its environment.
pub(crate) async fn start(cell: Arc<EnvCell>, config: ServerConfig) -> Result<Arc<ServerSlot>, Error> {
    let server = LiquersServerBuilder::<PyEnvironment>::from_config(config)
        .bind(CellSource::new(cell.clone()))
        .await?;
    let slot = Arc::new(ServerSlot { server });
    if let Err(e) = cell.add_server(slot.clone()) {
        slot.server.shutdown(Duration::ZERO).await;
        return Err(e);
    }
    let mut all = servers();
    all.retain(|weak| weak.upgrade().is_some_and(|s| s.server.is_running()));
    all.push(Arc::downgrade(&slot));
    Ok(slot)
}

/// Stops every server of the process, with the GIL released. Idempotent.
pub(crate) fn stop_all(py: Python<'_>, deadline: Duration) {
    let slots: Vec<Arc<ServerSlot>> = servers().drain(..).filter_map(|weak| weak.upgrade()).collect();
    if slots.is_empty() {
        return;
    }
    py.detach(|| {
        for slot in &slots {
            slot.stop_blocking(deadline);
        }
    });
}

/// The `atexit` hook `serve` registers: it runs before the module's own exit hook (`atexit` is
/// last in, first out), so servers stop before commands are cancelled.
#[pyfunction]
fn stop_servers_at_exit(py: Python<'_>) {
    stop_all(py, EXIT_DEADLINE);
}

/// Registers [`stop_servers_at_exit`] once per process.
pub(crate) fn register_exit_hook(py: Python<'_>) -> PyResult<()> {
    static REGISTERED: Once = Once::new();
    let mut result = Ok(());
    REGISTERED.call_once(|| {
        result = (|| {
            let hook = wrap_pyfunction!(stop_servers_at_exit, py)?;
            py.import("atexit")?.call_method1("register", (hook,))?;
            Ok(())
        })();
    });
    result
}

/// Cancels a server's stop token when dropped: `serve_async`'s task was cancelled.
pub(crate) struct StopOnDrop(pub(crate) CancellationToken);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// A running Liquers HTTP server, as `env.serve()` returns it.
///
/// The server keeps running when this handle is dropped; `shutdown()`, `env.shutdown()` or the
/// interpreter's exit stops it. As a context manager it stops the server on leaving the block.
#[pyclass(name = "Server", module = "liquers", frozen)]
pub struct Server {
    pub(crate) slot: Arc<ServerSlot>,
}

#[pymethods]
impl Server {
    /// The IP address the server listens on (`"127.0.0.1"` by default).
    #[getter]
    fn address(&self) -> String {
        self.slot.server.local_addr().ip().to_string()
    }

    /// The port the server listens on: the real one when it was started with `port=0`.
    #[getter]
    fn port(&self) -> u16 {
        self.slot.server.local_addr().port()
    }

    /// The server's base URL, `http://address:port`; the APIs are under its prefix
    /// (`{url}/liquer/q/…` by default).
    #[getter]
    fn url(&self) -> String {
        format!("http://{}", self.slot.server.local_addr())
    }

    /// Whether the server still runs.
    #[getter]
    fn running(&self) -> bool {
        self.slot.server.is_running()
    }

    /// Stops the server: new connections are refused at once, requests in flight get `timeout`
    /// seconds to finish, then are cut off; WebSockets receive a Close frame. The port is free
    /// when this returns. A second call does nothing.
    #[pyo3(signature = (timeout = 5.0))]
    fn shutdown(&self, py: Python<'_>, timeout: f64) -> PyResult<()> {
        crate::aio::check_blocking_call(py, "shutdown")?;
        let deadline = deadline(py, timeout)?;
        let slot = self.slot.clone();
        py.detach(move || slot.stop_blocking(deadline));
        Ok(())
    }

    /// Awaitable `shutdown`: `await server.shutdown_async()`.
    #[pyo3(signature = (timeout = 5.0))]
    fn shutdown_async<'py>(&self, py: Python<'py>, timeout: f64) -> PyResult<Bound<'py, PyAny>> {
        let deadline = deadline(py, timeout)?;
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
        let slot = self.slot.clone();
        pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            slot.server.shutdown(deadline).await;
            Ok(())
        })
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, py: Python<'_>, _exc: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<bool> {
        self.shutdown(py, SHUTDOWN_DEADLINE.as_secs_f64())?;
        Ok(false)
    }

    fn __repr__(&self) -> String {
        let state = if self.slot.server.is_running() { "running" } else { "stopped" };
        format!("<liquers.Server {} {state}>", self.url())
    }
}

fn deadline(py: Python<'_>, timeout: f64) -> PyResult<Duration> {
    Duration::try_from_secs_f64(timeout).map_err(|_| {
        error_to_pyerr(
            py,
            &Error::configuration_error(format!("timeout must be a number of seconds, not {timeout}")),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PyConfig;
    use crate::value::ConversionMode;
    use std::num::NonZeroUsize;

    fn attached<T>(f: impl for<'py> FnOnce(Python<'py>) -> Result<T, Error>) -> Result<T, Error> {
        Python::initialize();
        Python::attach(f)
    }

    /// After a registration retires the build, a burst of requests rebuilds once.
    #[test]
    fn cell_source_rebuilds_once_after_invalidate() -> Result<(), Error> {
        let cell = Arc::new(EnvCell::new(PyConfig::default(), ConversionMode::Permissive, false, NonZeroUsize::MIN)?);
        let first = attached(|py| cell.envref(py))?;
        let source = Arc::new(CellSource::new(cell.clone()));
        let runtime = crate::runtime::runtime()?;
        let same = runtime.block_on(source.current())?;
        assert!(Arc::ptr_eq(&first.0, &same.0), "no rebuild while the build is current");

        attached(|py| cell.invalidate(py))?;
        assert!(cell.current_without_gil()?.is_none());
        let builds: Vec<EnvRef<PyEnvironment>> = Python::attach(|py| {
            py.detach(|| {
                runtime.block_on(async {
                    let calls = (0..8).map(|_| {
                        let source = source.clone();
                        tokio::spawn(async move { source.current().await })
                    });
                    let mut builds = Vec::new();
                    for call in calls {
                        builds.push(call.await.map_err(|e| Error::general_error(e.to_string()))??);
                    }
                    Ok::<_, Error>(builds)
                })
            })
        })?;
        let rebuilt = &builds[0];
        assert!(!Arc::ptr_eq(&rebuilt.0, &first.0), "the retired build is not served");
        for build in &builds {
            assert!(Arc::ptr_eq(&build.0, &rebuilt.0), "every request got the one rebuild");
        }
        attached(|py| cell.shutdown(py))?;
        assert!(cell.current_without_gil()?.is_none());
        Ok(())
    }
}
