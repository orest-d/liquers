//! Running a closure asset from Python: the waits for `AssetManager::closure_asset` runs.
//!
//! A run must reach its end: dropping it is not cancelling it (`ad-hoc-closure-asset`). So the run
//! is spawned as its own task on the liquers-py runtime, and what Python holds is a wait for that
//! task. On shutdown the wait cancels the asset, then waits for the run to settle. Cancelling the
//! awaitable cancels the asset too: the asset is private to the call, so nobody else waits for it.
//! What the run was waiting for (a shared asset, such as a file it reads) is not cancelled, which
//! is the M3 rule.
//!
//! # Main components
//!
//! - [`awaitable_run`]: an `asyncio` awaitable of a run (`DataNode.get_async` on an
//!   environment's view; the async template renders).
//!
//! # Where it is used
//!
//! - `data_node.rs`: each async read of an `env.data_node(state)` view is one run.
//! - `liquers-py-templates`: `Environment.render_async` / `expand_async`.
//!
//! # Related concepts
//!
//! - `aio::awaitable`: the awaitable of a plain future; on shutdown it drops the future, which is
//!   right for a wait and wrong for a run.
//!
//! # Examples
//!
//! Exercised by `liquers-py/tests/test_DATANODE_views.py` (`test_async_nodes_at_top_level`,
//! `test_env_read_cancelled_at_shutdown`); a Rust doctest cannot start the interpreter.

use std::future::Future;
use std::sync::Arc;

use liquers_core::assets::AssetRef as CoreAssetRef;
use liquers_core::error::Error;
use pyo3::prelude::*;

use crate::aio::{Shut, ToPy};
use crate::environment::PyEnvironment;
use crate::error::error_to_pyerr;
use crate::runtime::runtime;

/// An awaitable of a closure asset's run. The run is spawned on the liquers-py runtime and always
/// runs to its end; the awaitable waits for it. On shutdown it cancels `asset`, then waits for the
/// run. Cancelling the awaitable cancels `asset` (private to the call) and ends the wait.
pub(crate) fn awaitable_run<'py, T: Send + 'static>(
    py: Python<'py>,
    shut: &Arc<Shut>,
    asset: CoreAssetRef<PyEnvironment>,
    run: impl Future<Output = Result<Option<T>, Error>> + Send + 'static,
    wrap: impl FnOnce(Result<Option<T>, Error>) -> ToPy + Send + 'static,
) -> PyResult<Bound<'py, PyAny>> {
    let runtime = runtime().map_err(|e| error_to_pyerr(py, &e))?;
    // Taken before anything is spawned, so a call with no running loop starts no work.
    let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
    let shut = shut.clone();
    let mut task = runtime.spawn(run);
    pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
        let mut guard = CancelRunOnDrop(Some(asset.clone()));
        let joined = tokio::select! {
            joined = &mut task => joined,
            () = shut.wait() => {
                let _ = asset.cancel().await;
                task.await
            }
        };
        guard.0 = None;
        let result = match joined {
            Ok(result) => result,
            Err(e) => Err(Error::unexpected_error(format!(
                "a liquers closure run ended abnormally: {e}"
            ))),
        };
        Ok(wrap(result))
    })
}

/// Cancels a run's asset when the awaitable is cancelled (its future dropped before the run
/// ended). The cancel is spawned, since `Drop` cannot await.
struct CancelRunOnDrop(Option<CoreAssetRef<PyEnvironment>>);

impl Drop for CancelRunOnDrop {
    fn drop(&mut self) {
        if let Some(asset) = self.0.take() {
            if let Ok(runtime) = runtime() {
                runtime.spawn(async move {
                    let _ = asset.cancel().await;
                });
            }
        }
    }
}
