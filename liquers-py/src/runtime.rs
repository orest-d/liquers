//! The process-wide tokio runtime that runs every evaluation started from Python.
//!
//! One runtime per process, created on first use. Python callers block on it with the GIL
//! released; Python commands run on its blocking pool (`spawn_blocking`), so a blocking Python
//! function never stalls a worker thread.

use liquers_core::error::Error;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();

/// The shared runtime, created on first call.
pub fn runtime() -> Result<&'static Runtime, Error> {
    let built = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("liquers-py")
            .build()
            .map_err(|e| e.to_string())
    });
    match built {
        Ok(runtime) => Ok(runtime),
        Err(message) => Err(Error::general_error(format!(
            "could not start the liquers-py runtime: {message}"
        ))),
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
}
