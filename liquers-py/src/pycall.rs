//! Calling Python from the runtime: the path Python commands, stores and recipe providers share.
//!
//! A `def` callable runs on the runtime's blocking pool, attached through [`crate::exit`]; an
//! `async def` callable makes its coroutine there and runs it on the environment's loop pool,
//! cancelled when the future waiting for it is dropped. Either way a runtime worker never takes
//! the GIL, and the result is converted, or the exception mapped, while the GIL is still held
//! ([`FromPyReturn`]), so the async side never attaches a second time.
//!
//! Commands keep their own entry point (`command.rs`), because a command's call also carries its
//! `Context` and asset; they use [`joined`], [`schedule_coroutine`] and [`LiveHandle`] from here.

use std::sync::{Arc, Mutex};

use liquers_core::error::Error;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::aio::{lock_pending, shut_down_while_waiting, store_future, CancelOnDrop, LazyLoopPool, Pending, Shut};

/// A Python callable resolved once: the bound method, and whether it is a coroutine function.
pub(crate) struct PyMethod {
    pub(crate) object: Py<PyAny>,
    pub(crate) is_async: bool,
    _handle: LiveHandle,
}

impl PyMethod {
    /// Wraps a callable, detecting `async def` once.
    pub(crate) fn new(py: Python<'_>, callable: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(PyMethod {
            object: callable.clone().unbind(),
            is_async: is_coroutine_function(py, callable)?,
            _handle: LiveHandle::new(),
        })
    }

    /// The attribute `name` of `object` as a method; `None` when it is absent or `None`.
    ///
    /// Fails when the attribute is present but not callable.
    pub(crate) fn resolve(py: Python<'_>, object: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<Self>> {
        if !object.hasattr(name)? {
            return Ok(None);
        }
        let attribute = object.getattr(name)?;
        if attribute.is_none() {
            return Ok(None);
        }
        if !attribute.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "'{name}' is not callable (it is {})",
                attribute.get_type().name()?
            )));
        }
        PyMethod::new(py, &attribute).map(Some)
    }
}

/// `inspect.iscoroutinefunction(callable)`.
pub(crate) fn is_coroutine_function(py: Python<'_>, callable: &Bound<'_, PyAny>) -> PyResult<bool> {
    py.import("inspect")?
        .call_method1("iscoroutinefunction", (callable,))?
        .is_truthy()
}

/// Turns what a Python call returned, or raised, into the caller's result, with the GIL held.
pub(crate) trait FromPyReturn: Send + 'static {
    type Output: Send + 'static;
    fn convert(self, py: Python<'_>, returned: Bound<'_, PyAny>) -> Result<Self::Output, Error>;
    fn failure(&self, py: Python<'_>, error: PyErr) -> Error;
}

/// Calls `method` with the arguments `args` builds, and converts the result with `convert`.
///
/// `subject` names the store or provider in cancellation and exit messages ("store 'notes'").
/// Refused with `Cancelled` once the interpreter is exiting; an `async def` method's wait also
/// ends with `Cancelled` when `shut` fires.
pub(crate) async fn call<A, R>(
    method: Arc<PyMethod>,
    loops: Arc<LazyLoopPool>,
    shut: Arc<Shut>,
    subject: String,
    args: A,
    convert: R,
) -> Result<R::Output, Error>
where
    A: for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>> + Send + 'static,
    R: FromPyReturn,
{
    if method.is_async {
        return call_async(method, loops, shut, subject, args, convert).await;
    }
    let what = subject.clone();
    joined(
        tokio::task::spawn_blocking(move || {
            crate::exit::py_call(&subject, |py| {
                let returned = args(py)
                    .and_then(|args| method.object.bind(py).call1(args))
                    .map_err(|e| convert.failure(py, e))?;
                if is_coroutine(py, &returned).map_err(|e| convert.failure(py, e))? {
                    // Closed so Python does not also warn that it was never awaited.
                    let _ = returned.call_method0("close");
                    return Err(Error::execution_error(format!(
                        "{subject} returned a coroutine from a method that is not `async def`"
                    )));
                }
                convert.convert(py, returned)
            })
        })
        .await,
        &what,
    )
}

async fn call_async<A, R>(
    method: Arc<PyMethod>,
    loops: Arc<LazyLoopPool>,
    shut: Arc<Shut>,
    subject: String,
    args: A,
    convert: R,
) -> Result<R::Output, Error>
where
    A: for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>> + Send + 'static,
    R: FromPyReturn,
{
    let pending = Arc::new(Mutex::new(Pending::default()));
    // Made first: from here on, dropping this future cancels whatever has been started.
    let _guard = CancelOnDrop(pending.clone());
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    let started = {
        let pending = pending.clone();
        let subject = subject.clone();
        tokio::task::spawn_blocking(move || {
            crate::exit::py_call(&subject, |py| {
                let pool = loops.get(py)?;
                let scheduled = args(py)
                    .and_then(|args| method.object.bind(py).call1(args))
                    .and_then(|coroutine| {
                        schedule_coroutine(py, coroutine, pool.pick().event_loop(py), &pending, done_tx)
                    });
                match scheduled {
                    Ok(()) => Ok(convert),
                    Err(e) => Err(convert.failure(py, e)),
                }
            })
        })
        .await
    };
    let convert = joined(started, &subject)?;
    tokio::select! {
        done = done_rx => done.map_err(|_| {
            Error::cancelled(format!("{subject}: the coroutine was dropped before it finished"))
        })?,
        // A coroutine that ignores cancellation never completes; the shutdown does not wait for it.
        () = shut.wait() => return Err(shut_down_while_waiting()),
    }
    let what = subject.clone();
    joined(
        tokio::task::spawn_blocking(move || {
            crate::exit::attach_tracked(|py| {
                let future = lock_pending(&pending)
                    .finish()
                    .ok_or_else(|| Error::cancelled(format!("{subject}: the coroutine was cancelled")))?;
                let future = future.bind(py);
                let cancelled = future
                    .call_method0("cancelled")
                    .and_then(|cancelled| cancelled.is_truthy())
                    .map_err(|e| convert.failure(py, e))?;
                if cancelled {
                    return Err(Error::cancelled(format!("{subject}: the coroutine was cancelled")));
                }
                match future.call_method0("result") {
                    Ok(returned) => convert.convert(py, returned),
                    Err(e) => Err(convert.failure(py, e)),
                }
            })
        })
        .await,
        &what,
    )
}

/// Checks that `coroutine` is one, schedules it on `event_loop`, and wakes `done_tx` when its
/// future completes. The future is kept in `pending`, so the guard can cancel it.
pub(crate) fn schedule_coroutine(
    py: Python<'_>,
    coroutine: Bound<'_, PyAny>,
    event_loop: &Bound<'_, PyAny>,
    pending: &Mutex<Pending>,
    done_tx: tokio::sync::oneshot::Sender<()>,
) -> PyResult<()> {
    let asyncio = py.import("asyncio")?;
    if !asyncio.call_method1("iscoroutine", (&coroutine,))?.is_truthy()? {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "an async def callable returned {} instead of a coroutine",
            coroutine.get_type().name()?
        )));
    }
    let future = match asyncio.call_method1("run_coroutine_threadsafe", (&coroutine, event_loop)) {
        Ok(future) => future,
        Err(e) => {
            // Never scheduled (the loop is closed): closed so Python does not warn that it was
            // never awaited.
            let _ = coroutine.call_method0("close");
            return Err(e);
        }
    };
    let completer = Completer {
        done: Mutex::new(Some(done_tx)),
    };
    future.call_method1("add_done_callback", (completer,))?;
    store_future(py, pending, future.unbind())
}

/// `asyncio.iscoroutine(object)`.
pub(crate) fn is_coroutine(py: Python<'_>, object: &Bound<'_, PyAny>) -> PyResult<bool> {
    py.import("asyncio")?
        .call_method1("iscoroutine", (object,))?
        .is_truthy()
}

/// The done callback of a coroutine's future: wakes the call that waits for it.
#[pyclass(frozen)]
struct Completer {
    done: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[pymethods]
impl Completer {
    fn __call__(&self, _future: &Bound<'_, PyAny>) {
        let sender = match self.done.lock() {
            Ok(mut done) => done.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(sender) = sender {
            let _ = sender.send(());
        }
    }
}

/// The result of a blocking-thread task; `subject` names what ran in the panic or cancel message.
pub(crate) fn joined<T>(joined: Result<Result<T, Error>, tokio::task::JoinError>, subject: &str) -> Result<T, Error> {
    match joined {
        Ok(result) => result,
        Err(e) if e.is_panic() => Err(Error::unexpected_error(format!("{subject} panicked: {e}"))),
        Err(e) => Err(Error::cancelled(format!("{subject} was cancelled: {e}"))),
    }
}

#[cfg(feature = "debug-handles")]
static LIVE_HANDLES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts one retained Python object while it lives: a command's callable, a store or recipe
/// provider object, a store or provider type's factory. Zero-sized and inert without
/// `debug-handles`.
pub(crate) struct LiveHandle(());

impl LiveHandle {
    pub(crate) fn new() -> Self {
        #[cfg(feature = "debug-handles")]
        LIVE_HANDLES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        LiveHandle(())
    }
}

#[cfg(feature = "debug-handles")]
impl Drop for LiveHandle {
    fn drop(&mut self) {
        LIVE_HANDLES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// How many Python objects the environments currently retain ([`LiveHandle`]s alive).
///
/// Test-only, behind `debug-handles`, for `RUNTIME05` and the store and recipe release tests:
/// Python cannot observe a Rust drop deterministically, so the count is what the tests assert on.
#[cfg(feature = "debug-handles")]
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(all(test, feature = "debug-handles"))]
mod tests {
    use super::*;

    #[test]
    fn live_handle_counts_create_and_drop() {
        // Other tests in this binary may hold handles; only the difference is asserted.
        let before = live_handle_count();
        let first = LiveHandle::new();
        let second = LiveHandle::new();
        assert!(live_handle_count() >= before + 2);
        drop(first);
        drop(second);
        assert!(live_handle_count() <= before + 2);
    }
}
