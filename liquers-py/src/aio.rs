//! Async support: the loops `async def` commands run on, the awaitable results of the `_async`
//! methods, and the check that keeps a blocking call off an event loop.
//!
//! # The loop pool
//!
//! An `async def` command's coroutine runs on one of the environment's own asyncio loops, never on
//! the caller's: an `Environment` owns a [`LazyLoopPool`] of `async_loops` threads, each running
//! one loop forever, started by the first async command. A command picks a loop round-robin and
//! hands its coroutine over with `asyncio.run_coroutine_threadsafe`. A loop's thread is flagged
//! ([`ON_LOOP`]), so a blocking liquers call made there raises instead of stalling every coroutine
//! on that loop ([`check_blocking_call`]).
//!
//! The pool stops when the environment shuts down, when the interpreter exits (an `atexit` hook
//! holding a `Weak`), or, failing both, when the last handle drops (on a thread of its own). A
//! stop cancels what is still running and gives it [`STOP_GRACE`] to finish.
//!
//! # Awaitables
//!
//! [`awaitable`] spawns a core future on the liquers-py runtime and wraps its `JoinHandle` in an
//! `asyncio.Future` of the caller's loop. Cancelling or dropping that future drops the handle,
//! which detaches the evaluation rather than cancelling it; `AssetRef.cancel_async` is the explicit
//! cancel. The result is converted to Python by [`ToPy`] on a blocking thread, so no runtime worker
//! takes the GIL.

use std::cell::Cell;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use liquers_core::assets::AssetRef as CoreAssetRef;
use liquers_core::error::Error;
use liquers_core::state::State as CoreState;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::{MutexExt, PyOnceLock};
use pyo3::types::PyModule;
use tokio::sync::Notify;

use crate::context::Context;
use crate::environment::PyEnvironment;
use crate::error::{error_to_pyerr, pyerr_to_error};
use crate::runtime::runtime;
use crate::value::{value_to_py, Value};

/// How long a stopping loop waits for its cancelled coroutines before it closes anyway.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(5);

/// How long starting a loop thread may take before the start counts as failed.
const START_TIMEOUT: Duration = Duration::from_secs(10);

thread_local! {
    /// Set on the threads of a [`LoopPool`].
    static ON_LOOP: Cell<bool> = const { Cell::new(false) };
}

/// Refuses or flags a blocking liquers call (AC-3).
///
/// On a pool thread the call would stop every coroutine on that loop until it returned, and a
/// nested evaluation needing the same loop would never finish, so it raises `RuntimeError`. Under
/// any other running loop it works, with a `RuntimeWarning` naming the `_async` variant.
pub(crate) fn check_blocking_call(py: Python<'_>, call: &str) -> PyResult<()> {
    if ON_LOOP.with(|flag| flag.get()) {
        return Err(PyRuntimeError::new_err(format!(
            "{call}() would block the liquers event loop it is called on; use `await {call}_async(...)` in an async command"
        )));
    }
    let running = py.import("asyncio")?.call_method0("_get_running_loop")?;
    if !running.is_none() {
        let warning = py.get_type::<pyo3::exceptions::PyRuntimeWarning>();
        py.import("warnings")?.call_method1(
            "warn",
            (
                format!("{call}() blocks the running event loop until it returns; use `await {call}_async(...)`"),
                warning,
                2,
            ),
        )?;
    }
    Ok(())
}

/// Refuses `shutdown()` on a pool thread, where stopping the pool would wait for its own loop.
pub(crate) fn refuse_shutdown_on_loop() -> PyResult<()> {
    if ON_LOOP.with(|flag| flag.get()) {
        return Err(PyRuntimeError::new_err(
            "shutdown() cannot be called from an async command: it stops the loop the command runs on",
        ));
    }
    Ok(())
}

/// The shut signal of an environment: the flag a blocking wait polls, and the notification an
/// async wait awaits.
#[derive(Default)]
pub struct Shut {
    flag: AtomicBool,
    notify: Notify,
}

impl Shut {
    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Sets the flag and wakes every async wait.
    pub fn fire(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Returns once the signal fires, at once if it already has.
    pub async fn wait(&self) {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        // Registered before the flag is read, so a `fire` between the two is not missed.
        notified.as_mut().enable();
        if self.is_set() {
            return;
        }
        notified.await;
    }
}

/// The error a wait ends with when the environment is shut down under it.
pub(crate) fn shut_down_while_waiting() -> Error {
    Error::cancelled("the environment was shut down while this call was waiting".to_string())
}

/// What an awaitable resolves to, converted to Python on a blocking thread.
pub(crate) enum ToPy {
    Value(Result<Arc<Value>, Error>),
    State(Result<CoreState<Value>, Error>),
    Asset(Result<(CoreAssetRef<PyEnvironment>, Arc<Shut>), Error>),
    Unit(Result<(), Error>),
    Bytes(Result<Vec<u8>, Error>),
    Metadata(Result<liquers_core::metadata::Metadata, Error>),
    Bool(Result<bool, Error>),
    Strings(Result<Vec<String>, Error>),
    Recipe(Result<Option<liquers_core::recipes::Recipe>, Error>),
    Plan(Result<liquers_core::plan::Plan, Error>),
    ResourceNames(Result<Vec<liquers_core::query::ResourceName>, Error>),
}

impl<'py> IntoPyObject<'py> for ToPy {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let converted = match self {
            ToPy::Value(result) => result.and_then(|value| value_to_py(py, value.as_ref())),
            ToPy::State(result) => result
                .and_then(|state| crate::asset::State::from_core(py, &state))
                .map(Py::into_any),
            ToPy::Asset(result) => result.and_then(|(asset, shut)| {
                Py::new(py, crate::asset::AssetRef::new(asset, shut))
                    .map(Py::into_any)
                    .map_err(|e| pyerr_to_error(py, &e))
            }),
            ToPy::Unit(result) => result.map(|()| py.None()),
            ToPy::Bytes(result) => result.map(|data| pyo3::types::PyBytes::new(py, &data).into_any().unbind()),
            ToPy::Metadata(result) => result.and_then(|metadata| {
                Py::new(py, crate::metadata::Metadata { inner: metadata })
                    .map(Py::into_any)
                    .map_err(|e| pyerr_to_error(py, &e))
            }),
            ToPy::Bool(result) => result.map(|b| pyo3::types::PyBool::new(py, b).to_owned().into_any().unbind()),
            ToPy::Strings(result) => result.and_then(|names| {
                pyo3::types::PyList::new(py, names)
                    .map(|list| list.into_any().unbind())
                    .map_err(|e| pyerr_to_error(py, &e))
            }),
            ToPy::Recipe(result) => result.and_then(|recipe| match recipe {
                Some(inner) => Py::new(py, crate::recipes::Recipe { inner })
                    .map(Py::into_any)
                    .map_err(|e| pyerr_to_error(py, &e)),
                None => Ok(py.None()),
            }),
            ToPy::Plan(result) => result.and_then(|plan| {
                Py::new(py, crate::plan::Plan(plan))
                    .map(Py::into_any)
                    .map_err(|e| pyerr_to_error(py, &e))
            }),
            ToPy::ResourceNames(result) => result.and_then(|names| {
                let names: PyResult<Vec<Py<crate::query::ResourceName>>> =
                    names.into_iter().map(|n| Py::new(py, crate::query::ResourceName(n))).collect();
                names
                    .and_then(|names| pyo3::types::PyList::new(py, names))
                    .map(|list| list.into_any().unbind())
                    .map_err(|e| pyerr_to_error(py, &e))
            }),
        };
        converted
            .map(|object| object.into_bound(py))
            .map_err(|e| error_to_pyerr(py, &e))
    }
}

/// An `asyncio.Future` on the caller's running loop that resolves to `future`'s result.
///
/// `future` runs as its own task on the liquers-py runtime, raced against `shut`. Cancelling the
/// returned future drops only the wait. Raises `RuntimeError` if no loop is running.
pub(crate) fn awaitable<'py, T, F>(
    py: Python<'py>,
    shut: &Arc<Shut>,
    future: F,
    wrap: fn(Result<T, Error>) -> ToPy,
) -> PyResult<Bound<'py, PyAny>>
where
    T: Send + 'static,
    F: Future<Output = Result<T, Error>> + Send + 'static,
{
    let runtime = runtime().map_err(|e| error_to_pyerr(py, &e))?;
    // Taken before anything is spawned, so a call with no running loop starts no work.
    let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
    let shut = shut.clone();
    let handle = runtime.spawn(async move {
        tokio::select! {
            result = future => result,
            () = shut.wait() => Err(shut_down_while_waiting()),
        }
    });
    pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
        let result = match handle.await {
            Ok(result) => result,
            Err(e) => Err(Error::unexpected_error(format!(
                "an awaitable liquers call ended abnormally: {e}"
            ))),
        };
        Ok(wrap(result))
    })
}

/// The code each pool thread runs, as a module built once per process.
const LOOP_CODE: &std::ffi::CStr = cr#"
import asyncio


def run(loop, grace):
    """Runs `loop` until it is stopped, then cancels its tasks and closes it.

    Returns how many tasks were still running after `grace` seconds.
    """
    asyncio.set_event_loop(loop)
    try:
        loop.run_forever()
        tasks = asyncio.all_tasks(loop)
        for task in tasks:
            task.cancel()
        pending = ()
        if tasks:
            _, pending = loop.run_until_complete(asyncio.wait(tasks, timeout=grace))
        loop.run_until_complete(loop.shutdown_asyncgens())
        return len(pending)
    finally:
        loop.close()
"#;

fn loop_module(py: Python<'_>) -> PyResult<&Py<PyModule>> {
    static MODULE: PyOnceLock<Py<PyModule>> = PyOnceLock::new();
    MODULE.get_or_try_init(py, || {
        PyModule::from_code(py, LOOP_CODE, c"liquers/_loop.py", c"liquers._loop").map(Bound::unbind)
    })
}

/// How a loop thread ended: the number of tasks that ignored cancellation, or why it failed.
type LoopOutcome = Result<usize, Error>;

/// One asyncio loop running forever on its own thread.
pub(crate) struct LoopThread {
    event_loop: Py<PyAny>,
    thread: Mutex<Option<JoinHandle<()>>>,
    done: Mutex<Option<mpsc::Receiver<LoopOutcome>>>,
}

impl LoopThread {
    fn spawn(py: Python<'_>, index: usize) -> Result<LoopThread, Error> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Py<PyAny>, Error>>();
        let (done_tx, done_rx) = mpsc::channel::<LoopOutcome>();
        let thread = std::thread::Builder::new()
            .name(format!("liquers-py-loop-{index}"))
            .spawn(move || {
                ON_LOOP.with(|flag| flag.set(true));
                let outcome = crate::exit::attach_tracked(|py| -> LoopOutcome {
                    let started = py
                        .import("asyncio")
                        .and_then(|asyncio| asyncio.call_method0("new_event_loop"))
                        .and_then(|event_loop| loop_module(py).map(|module| (event_loop, module)));
                    let (event_loop, module) = match started {
                        Ok(started) => started,
                        Err(e) => {
                            let _ = ready_tx.send(Err(pyerr_to_error(py, &e)));
                            return Ok(0);
                        }
                    };
                    let _ = ready_tx.send(Ok(event_loop.clone().unbind()));
                    module
                        .bind(py)
                        .call_method1("run", (event_loop, STOP_GRACE.as_secs_f64()))
                        .and_then(|pending| pending.extract::<usize>())
                        .map_err(|e| pyerr_to_error(py, &e))
                });
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| Error::general_error(format!("could not start an event loop thread: {e}")))?;
        let ready = py.detach(move || ready_rx.recv_timeout(START_TIMEOUT));
        let event_loop = match ready {
            Ok(Ok(event_loop)) => event_loop,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(Error::general_error(
                    "an event loop thread did not start".to_string(),
                ))
            }
        };
        Ok(LoopThread {
            event_loop,
            thread: Mutex::new(Some(thread)),
            done: Mutex::new(Some(done_rx)),
        })
    }

    pub(crate) fn event_loop<'py>(&self, py: Python<'py>) -> &Bound<'py, PyAny> {
        self.event_loop.bind(py)
    }

    /// Asks the loop to stop; harmless if it already has.
    fn request_stop(&self, py: Python<'_>) {
        let event_loop = self.event_loop.bind(py);
        let stop = event_loop.getattr("stop");
        if let Ok(stop) = stop {
            // Fails only when the loop is already closed.
            let _ = event_loop.call_method1("call_soon_threadsafe", (stop,));
        }
    }
}

/// Locks a pool mutex without keeping the interpreter while it waits.
fn lock<'a, T>(py: Python<'_>, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
    match mutex.lock_py_attached(py) {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The event loops of one environment.
pub(crate) struct LoopPool {
    loops: Vec<LoopThread>,
    next: AtomicUsize,
    stopped: AtomicBool,
}

impl LoopPool {
    /// Starts `size` loop threads and registers the `atexit` stop.
    pub(crate) fn start(py: Python<'_>, size: NonZeroUsize) -> Result<LoopPool, Error> {
        let mut loops = Vec::with_capacity(size.get());
        for index in 0..size.get() {
            match LoopThread::spawn(py, index) {
                Ok(thread) => loops.push(thread),
                Err(e) => {
                    let partial = LoopPool::from_loops(loops);
                    // The error that matters is the start failure; a warning from stopping the
                    // partial pool would only hide it.
                    let _ = partial.stop(py);
                    return Err(e);
                }
            }
        }
        Ok(LoopPool::from_loops(loops))
    }

    fn from_loops(loops: Vec<LoopThread>) -> LoopPool {
        LoopPool {
            loops,
            next: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
        }
    }

    /// The next loop, round-robin.
    pub(crate) fn pick(&self) -> &LoopThread {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.loops.len();
        &self.loops[index]
    }

    /// Stops every loop: their tasks are cancelled and given [`STOP_GRACE`] to finish.
    ///
    /// A loop whose thread does not end in time (a coroutine blocking its thread) is left running
    /// detached, with a `RuntimeWarning`; so is one where a coroutine ignored cancellation.
    /// Idempotent.
    pub(crate) fn stop(&self, py: Python<'_>) -> PyResult<()> {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        stop_loops(py, &self.loops)
    }
}

fn stop_loops(py: Python<'_>, loops: &[LoopThread]) -> PyResult<()> {
    for event_loop in loops {
        event_loop.request_stop(py);
    }
    let deadline = Instant::now() + STOP_GRACE + Duration::from_secs(1);
    let mut stubborn = 0;
    let mut stuck = 0;
    for event_loop in loops {
        let done = lock(py, &event_loop.done).take();
        let outcome = match done {
            Some(done) => py.detach(move || {
                done.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }),
            None => continue,
        };
        match outcome {
            Ok(Ok(pending)) => {
                stubborn += pending;
                if let Some(thread) = lock(py, &event_loop.thread).take() {
                    let _ = py.detach(|| thread.join());
                }
            }
            Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(thread) = lock(py, &event_loop.thread).take() {
                    let _ = py.detach(|| thread.join());
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Dropping the handle detaches the thread.
                stuck += 1;
                lock(py, &event_loop.thread).take();
            }
        }
    }
    if stubborn > 0 || stuck > 0 {
        let warning = py.get_type::<pyo3::exceptions::PyRuntimeWarning>();
        py.import("warnings")?.call_method1(
            "warn",
            (
                format!(
                    "liquers event loops stopped with {stubborn} coroutine(s) ignoring cancellation and {stuck} loop thread(s) still blocked after {} s; they were left behind",
                    STOP_GRACE.as_secs()
                ),
                warning,
                2,
            ),
        )?;
    }
    Ok(())
}

impl Drop for LoopPool {
    /// A pool nobody stopped is stopped on a thread of its own, never on a runtime worker.
    fn drop(&mut self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let loops = std::mem::take(&mut self.loops);
        let _ = std::thread::Builder::new()
            .name("liquers-py-loop-stop".to_string())
            .spawn(move || {
                crate::exit::attach_tracked(|py| {
                    let _ = stop_loops(py, &loops);
                });
            });
    }
}

/// The `atexit` hook of a pool: stops it if it is still alive when the interpreter exits.
#[pyclass(frozen)]
struct StopAtExit {
    pool: Weak<LoopPool>,
}

#[pymethods]
impl StopAtExit {
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        match self.pool.upgrade() {
            Some(pool) => pool.stop(py),
            None => Ok(()),
        }
    }
}

enum PoolSlot {
    Idle,
    Running(Arc<LoopPool>),
    Closed,
}

/// An environment's loop pool, started by the first async command that needs it.
///
/// A failed start is not kept, so the next command tries again. Shared by every rebuild of the
/// environment, since it lives on the handle's cell.
pub struct LazyLoopPool {
    size: NonZeroUsize,
    slot: Mutex<PoolSlot>,
}

impl LazyLoopPool {
    pub(crate) fn new(size: NonZeroUsize) -> Self {
        LazyLoopPool {
            size,
            slot: Mutex::new(PoolSlot::Idle),
        }
    }

    pub(crate) fn get(&self, py: Python<'_>) -> Result<Arc<LoopPool>, Error> {
        self.get_with(py, LoopPool::start)
    }

    fn get_with(
        &self,
        py: Python<'_>,
        start: impl FnOnce(Python<'_>, NonZeroUsize) -> Result<LoopPool, Error>,
    ) -> Result<Arc<LoopPool>, Error> {
        let mut slot = lock(py, &self.slot);
        match &*slot {
            PoolSlot::Running(pool) => return Ok(pool.clone()),
            PoolSlot::Closed => return Err(shut_down_while_waiting()),
            PoolSlot::Idle => {}
        }
        let pool = Arc::new(start(py, self.size)?);
        let hook = StopAtExit {
            pool: Arc::downgrade(&pool),
        };
        py.import("atexit")
            .and_then(|atexit| atexit.call_method1("register", (hook,)))
            .map_err(|e| pyerr_to_error(py, &e))?;
        *slot = PoolSlot::Running(pool.clone());
        Ok(pool)
    }

    /// Stops the pool if it runs, and refuses to start one afterwards. Idempotent.
    pub(crate) fn close(&self, py: Python<'_>) -> PyResult<()> {
        let previous = std::mem::replace(&mut *lock(py, &self.slot), PoolSlot::Closed);
        match previous {
            PoolSlot::Running(pool) => pool.stop(py),
            PoolSlot::Idle | PoolSlot::Closed => Ok(()),
        }
    }
}

/// What an async command's cancellation guard can reach.
#[derive(Default)]
pub(crate) struct Pending {
    /// Set when the guard drops.
    cancelled: bool,
    /// The `concurrent.futures.Future` of the running coroutine.
    future: Option<Py<PyAny>>,
    /// The context handed to the coroutine, invalidated when the command ends.
    context: Option<Context>,
}

impl Pending {
    /// Keeps the context; `false` (and the context invalidated) if the guard already dropped.
    pub(crate) fn keep_context(&mut self, context: Context) -> bool {
        if self.cancelled {
            context.invalidate();
            return false;
        }
        self.context = Some(context);
        true
    }

    /// Keeps the future, or hands it back to be cancelled if the guard already dropped.
    pub(crate) fn store(&mut self, future: Py<PyAny>) -> Option<Py<PyAny>> {
        if self.cancelled {
            return Some(future);
        }
        self.future = Some(future);
        None
    }

    /// Takes the future and invalidates the context, once the command has its result.
    pub(crate) fn finish(&mut self) -> Option<Py<PyAny>> {
        if let Some(context) = self.context.take() {
            context.invalidate();
        }
        self.future.take()
    }
}

/// Locks a [`Pending`], whether or not the thread is attached.
pub(crate) fn lock_pending(pending: &Mutex<Pending>) -> MutexGuard<'_, Pending> {
    match pending.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Cancels an async command's coroutine when the command's future is dropped (AC-7): the asset
/// was cancelled or replaced, or the environment shut down.
///
/// Made before the coroutine exists, so a cancel that arrives while the call is being prepared is
/// applied as soon as the future is stored.
pub(crate) struct CancelOnDrop(pub(crate) Arc<Mutex<Pending>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        // No Python call is made under this lock anywhere, so taking it here is never a wait on
        // the interpreter.
        let future = {
            let mut pending = lock_pending(&self.0);
            pending.cancelled = true;
            pending.finish()
        };
        if let Some(future) = future {
            cancel_on_blocking_thread(future);
        }
    }
}

/// Stores the coroutine's future, cancelling it at once if the guard already dropped. The cancel
/// runs after the lock is released, since it calls the future's done callbacks.
pub(crate) fn store_future(py: Python<'_>, pending: &Mutex<Pending>, future: Py<PyAny>) -> PyResult<()> {
    let refused = lock_pending(pending).store(future);
    match refused {
        Some(future) => future.bind(py).call_method0("cancel").map(|_| ()),
        None => Ok(()),
    }
}

/// Cancels a `concurrent.futures.Future` from a thread that may not take the GIL.
pub(crate) fn cancel_on_blocking_thread(future: Py<PyAny>) {
    match runtime() {
        Ok(runtime) => {
            runtime.spawn_blocking(move || {
                crate::exit::attach_tracked(|py| {
                    let _ = future.bind(py).call_method0("cancel");
                })
            });
        }
        // No runtime means nothing runs the coroutine's command either; dropping is all there is.
        Err(_) => drop(future),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attached<T>(f: impl for<'py> FnOnce(Python<'py>) -> T) -> T {
        Python::initialize();
        Python::attach(f)
    }

    fn size(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap_or(NonZeroUsize::MIN)
    }

    #[test]
    fn check_blocking_call_raises_only_on_pool_threads() -> Result<(), Box<dyn std::error::Error>> {
        attached(|py| -> Result<(), Box<dyn std::error::Error>> {
            assert!(check_blocking_call(py, "evaluate").is_ok());
            let pool = LoopPool::start(py, size(1))?;
            let (tx, rx) = mpsc::channel();
            let event_loop = pool.pick().event_loop(py).clone();
            let probe = pyo3::types::PyCFunction::new_closure(py, None, None, move |args, _kwargs| {
                let py = args.py();
                let _ = tx.send(check_blocking_call(py, "evaluate").is_err());
            })?;
            event_loop.call_method1("call_soon_threadsafe", (probe,))?;
            let on_pool = py.detach(move || rx.recv_timeout(Duration::from_secs(10)))?;
            assert!(on_pool, "a blocking call on a pool thread must raise");
            pool.stop(py)?;
            Ok(())
        })
    }

    #[test]
    fn pool_picks_round_robin_and_stop_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        attached(|py| -> Result<(), Box<dyn std::error::Error>> {
            let pool = LoopPool::start(py, size(3))?;
            let picked: Vec<usize> = (0..6)
                .map(|_| pool.pick().event_loop(py).as_ptr() as usize)
                .collect();
            assert_eq!(picked[0..3], picked[3..6]);
            assert_ne!(picked[0], picked[1]);
            assert_ne!(picked[1], picked[2]);
            pool.stop(py)?;
            pool.stop(py)?;
            for thread in &pool.loops {
                assert!(lock(py, &thread.thread).is_none());
                assert!(thread.event_loop(py).call_method0("is_closed")?.extract::<bool>()?);
            }
            Ok(())
        })
    }

    #[test]
    fn failed_pool_start_is_retried() -> Result<(), Box<dyn std::error::Error>> {
        attached(|py| -> Result<(), Box<dyn std::error::Error>> {
            let lazy = LazyLoopPool::new(size(1));
            let failed = lazy.get_with(py, |_, _| Err(Error::general_error("no loop".to_string())));
            assert!(failed.is_err());
            let pool = lazy.get(py)?;
            assert!(Arc::ptr_eq(&pool, &lazy.get(py)?));
            lazy.close(py)?;
            assert!(lazy.get(py).is_err());
            Ok(())
        })
    }

    #[test]
    fn pending_cancels_a_future_stored_after_drop() -> Result<(), Box<dyn std::error::Error>> {
        attached(|py| -> Result<(), Box<dyn std::error::Error>> {
            let pending = Arc::new(Mutex::new(Pending::default()));
            drop(CancelOnDrop(pending.clone()));
            let future = py
                .import("concurrent.futures")?
                .getattr("Future")?
                .call0()?;
            store_future(py, &pending, future.clone().unbind())?;
            assert!(lock_pending(&pending).future.is_none());
            assert!(future.call_method0("cancelled")?.extract::<bool>()?);
            Ok(())
        })
    }
}
