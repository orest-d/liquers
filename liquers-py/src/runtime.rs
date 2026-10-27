//! The process-wide tokio runtime that runs every evaluation started from Python.
//!
//! One runtime per process, created on first use. Python callers block on it with the GIL
//! released; Python commands run on its blocking pool (`spawn_blocking`), so a blocking Python
//! function never stalls a worker thread.
//!
//! The same runtime drives `pyo3-async-runtimes`, which the awaitable `_async` methods use: it is
//! bound on first use, so the bridge never builds a runtime of its own.

use liquers_core::error::Error;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

static BRIDGE: OnceLock<Result<(), String>> = OnceLock::new();

/// The shared runtime, created on first call and bound to `pyo3-async-runtimes`.
pub fn runtime() -> Result<&'static Runtime, Error> {
    let built = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("liquers-py")
            .build()
            .map_err(|e| e.to_string())
    });
    let runtime = match built {
        Ok(runtime) => runtime,
        Err(message) => {
            return Err(Error::general_error(format!(
                "could not start the liquers-py runtime: {message}"
            )))
        }
    };
    let bound = BRIDGE.get_or_init(|| match pyo3_async_runtimes::tokio::init_with_runtime(runtime) {
        Ok(()) => Ok(()),
        // Already bound: harmless only if it is bound to this runtime.
        Err(()) if std::ptr::eq(pyo3_async_runtimes::tokio::get_runtime(), runtime) => Ok(()),
        Err(()) => Err("the async bridge is bound to another tokio runtime".to_string()),
    });
    match bound {
        Ok(()) => Ok(runtime),
        Err(message) => Err(Error::unexpected_error(message.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_is_created_once_and_runs_futures() -> Result<(), Box<dyn std::error::Error>> {
        let first = runtime()? as *const Runtime;
        let second = runtime()? as *const Runtime;
        assert_eq!(first, second);
        assert_eq!(runtime()?.block_on(async { 41 + 1 }), 42);
        Ok(())
    }

    #[test]
    fn bridge_uses_the_liquers_runtime() -> Result<(), Box<dyn std::error::Error>> {
        let ours = runtime()? as *const Runtime;
        let bridge = pyo3_async_runtimes::tokio::get_runtime() as *const Runtime;
        assert_eq!(ours, bridge);
        Ok(())
    }
}
