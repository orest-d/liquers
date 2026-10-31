//! Interpreter exit: no liquers-owned thread may be inside Python once the interpreter finalizes.
//!
//! Python commands run on threads Python did not start (the runtime's blocking pool). When the
//! interpreter finalizes while such a thread is attached but has released the GIL (a `time.sleep`,
//! any blocking call), CPython before 3.14 ends the thread as it takes the GIL back
//! (`PyThread_exit_thread`). That unwinds through the Rust frames above it, which release a thread
//! state that is no longer current: "Fatal Python error: PyGILState_Release", or a segfault.
//! CPython 3.14 parks such a thread instead, so the crash is version-dependent, but a command cut
//! off mid-call is wrong on every version.
//!
//! So every attach from a liquers-owned thread is counted ([`attach_tracked`]), and a command call
//! also records its asset ([`command_call`]). An `atexit` hook registered at module import
//! ([`at_exit`]) runs after every environment's own hooks (they are registered later, and `atexit`
//! runs last-in first-out). It refuses new command calls, requests cancellation of the running
//! ones, and waits up to [`EXIT_GRACE`] for every tracked attach to end. A command that ignores
//! cancellation for longer is left running with a `RuntimeWarning`, as the loop pool does.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use liquers_core::assets::AssetRef as CoreAssetRef;
use liquers_core::error::Error;
use pyo3::prelude::*;

use crate::aio::STOP_GRACE;
use crate::environment::PyEnvironment;
use crate::runtime::runtime;

/// How long the exit hook waits for running Python commands after requesting their cancellation.
pub(crate) const EXIT_GRACE: Duration = STOP_GRACE;

/// Set by the exit hook; from then on [`command_call`] refuses.
static EXITING: AtomicBool = AtomicBool::new(false);

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

/// The attaches in flight: a token each, with the asset when the attach runs a command.
static IN_FLIGHT: Mutex<Vec<(u64, Option<CoreAssetRef<PyEnvironment>>)>> = Mutex::new(Vec::new());

/// Notified whenever an attach in flight ends.
static ENDED: Condvar = Condvar::new();

fn in_flight() -> MutexGuard<'static, Vec<(u64, Option<CoreAssetRef<PyEnvironment>>)>> {
    match IN_FLIGHT.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Removes its entry when the attach has ended, the thread state released.
struct Tracked(u64);

impl Tracked {
    fn new(asset: Option<CoreAssetRef<PyEnvironment>>) -> Tracked {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        in_flight().push((token, asset));
        Tracked(token)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        // The asset is dropped after the lock: its last reference may run arbitrary drops.
        let removed = {
            let mut entries = in_flight();
            let index = entries.iter().position(|(token, _)| *token == self.0);
            index.map(|index| entries.swap_remove(index))
        };
        ENDED.notify_all();
        drop(removed);
    }
}

/// `Python::attach` from a thread Python did not start, counted so the exit hook waits for it.
pub(crate) fn attach_tracked<T>(f: impl for<'py> FnOnce(Python<'py>) -> T) -> T {
    let _tracked = Tracked::new(None);
    Python::attach(f)
}

/// Runs a Python command call attached, unless the interpreter is exiting: then it is refused
/// with `Cancelled`, since the interpreter it would run in is going away.
pub(crate) fn command_call<T>(
    asset: CoreAssetRef<PyEnvironment>,
    f: impl for<'py> FnOnce(Python<'py>) -> Result<T, Error>,
) -> Result<T, Error> {
    let _tracked = Tracked::new(Some(asset));
    // Checked after registering, so the hook either sees this call or this call sees the flag.
    if EXITING.load(Ordering::SeqCst) {
        return Err(Error::cancelled(
            "the Python interpreter is exiting; the command was not run".to_string(),
        ));
    }
    Python::attach(f)
}

/// The module's `atexit` hook: cancels the running Python commands and waits for them.
#[pyfunction]
pub(crate) fn at_exit(py: Python<'_>) -> PyResult<()> {
    EXITING.store(true, Ordering::SeqCst);
    let assets: Vec<CoreAssetRef<PyEnvironment>> =
        in_flight().iter().filter_map(|(_, asset)| asset.clone()).collect();
    if !assets.is_empty() {
        if let Ok(runtime) = runtime() {
            for asset in assets {
                // Not awaited here: `cancel` waits for its run, and the runs are waited for below.
                runtime.spawn(async move {
                    let _ = asset.cancel().await;
                });
            }
        }
    }
    let deadline = Instant::now() + EXIT_GRACE;
    let left = py.detach(|| {
        let mut entries = in_flight();
        loop {
            let now = Instant::now();
            if entries.is_empty() || now >= deadline {
                return entries.len();
            }
            entries = match ENDED.wait_timeout(entries, deadline - now) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    });
    if left > 0 {
        let warning = py.get_type::<pyo3::exceptions::PyRuntimeWarning>();
        py.import("warnings")?.call_method1(
            "warn",
            (
                format!(
                    "liquers: {left} Python call(s) on liquers threads were still running {} s after the interpreter began to exit; they were left behind, and on CPython before 3.14 the exit may crash",
                    EXIT_GRACE.as_secs()
                ),
                warning,
            ),
        )?;
    }
    Ok(())
}

/// Registers [`at_exit`]; called once, from module init.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let hook = wrap_pyfunction!(at_exit, m)?;
    m.py().import("atexit")?.call_method1("register", (hook,))?;
    Ok(())
}
