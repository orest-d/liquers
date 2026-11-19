//! `lq.Accessor`: a settable value accessor from Python, over the same Rust accessor.
//!
//! An accessor is a live handle on the place a value lives (a stored key, a keyed asset, a part of
//! a document or of a recipe's query). `Environment.accessor(query)` evaluates the query and wraps
//! its accessor (any other value's asset is wrapped read-only); a command that returns an accessor
//! crosses into Python as `lq.Accessor`, and an `lq.Accessor` passed back crosses as the *same*
//! Rust accessor, never a copy.
//!
//! Each wrapper keeps the token it last read or wrote, and `set` is a compare-and-set on that
//! token, so two Python holders of one accessor conflict with each other (`StatusConflictError`,
//! whose `token` is the current one). Every blocking method has an `_async` form that never blocks
//! the caller's loop (`aio.rs`); the blocking forms release the GIL.
//!
//! Events follow `lq.UI`: a background task only queues them (at most 256, the oldest dropped), and
//! callbacks registered with `on` run only on a thread that asks: `process_events()`,
//! `wait_event()` or iterating `events()`. A raising callback goes to `sys.unraisablehook`.
//!
//! # Main components
//!
//! - [`Accessor`] (`lq.Accessor`): `get`, `snapshot`, `set`, `set_if`, `info`, `log`,
//!   `describe`, `refresh`, `project`, `on`, `wait_event`, `process_events`, and the `_async` forms.
//! - [`AccessorSnapshot`], [`AccessorInfo`], [`AccessorEvent`], [`PartDescription`],
//!   [`AccessorCapabilities`]: plain data.
//! - [`AccessorSubscription`]: an `on` callback, disposed by `dispose()` or a `with` block.
//!
//! # Where it is used
//!
//! - `environment.rs`: `Environment.accessor` and `Environment.memory_accessor`.
//! - `value.rs`: `ExtValue::Accessor` crosses both ways as `lq.Accessor`.
//!
//! # Related concepts
//!
//! - `liquers_core::accessor`: the Rust contract every method calls; `liquers-lib`'s `acc`
//!   commands make the accessors.
//! - `ui.rs`: the event and callback rules this module follows.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_ACCESSOR_use.py` (a Rust doctest cannot start the
//! interpreter):
//!
//! ```python
//! acc = env.accessor("-R-key/config.yaml/-/ns-acc/accessor/field-title")
//! snap = acc.snapshot()                      # snap.value == "Dashboard"
//! new_token = acc.set_if("Sales", snap.token)
//! acc.set_if("Stale", snap.token)            # raises lq.StatusConflictError, .token == new_token
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use liquers_core::accessor::{
    AccessorEvent as CoreEvent, AccessorInfo as CoreInfo, AccessorSnapshot as CoreSnapshot,
    Fingerprint, PartDescription as CorePart, SetOptions, ValueAccessor,
};
use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use liquers_core::query::Query as CoreQuery;
use liquers_core::value::ValueInterface;
use pyo3::prelude::*;
use tokio::sync::watch;

use crate::aio::{awaitable, check_blocking_call, PyConvert, Shut, ToPy};
use crate::environment::{block_on, PyEnvironment};
use crate::error::{error_to_pyerr, pyerr_to_error};
use crate::records::Scope;
use crate::runtime::runtime;
use crate::value::{py_to_value, value_to_py, ConversionMode, Value};

/// The most events a wrapper queues before it drops the oldest.
const QUEUE_LIMIT: usize = 256;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn json_to_py(py: Python<'_>, json: &serde_json::Value) -> Result<Py<PyAny>, Error> {
    value_to_py(py, &Value::try_from_json_value(json)?, &Scope::detached())
}

fn serialized_to_py<T: serde::Serialize>(py: Python<'_>, value: &T) -> Result<Py<PyAny>, Error> {
    let json = serde_json::to_value(value)
        .map_err(|e| Error::general_error(format!("cannot convert to JSON: {e}")))?;
    json_to_py(py, &json)
}

fn new_py<T: pyo3::PyClass + Into<pyo3::PyClassInitializer<T>>>(py: Python<'_>, value: T) -> Result<Py<PyAny>, Error> {
    Py::new(py, value).map(Py::into_any).map_err(|e| pyerr_to_error(py, &e))
}

fn parse_token(token: Option<&str>) -> Result<Option<Fingerprint>, Error> {
    token.map(Fingerprint::from_token).transpose()
}

/// The queued events and callbacks of one wrapper.
struct Events {
    queue: Mutex<VecDeque<CoreEvent>>,
    /// Bumped on every push; a waiter subscribes before it looks at the queue.
    arrived: watch::Sender<u64>,
    callbacks: Mutex<Vec<(u64, Py<PyAny>)>>,
    next_id: AtomicU64,
}

impl Events {
    fn new() -> Self {
        Events {
            queue: Mutex::new(VecDeque::new()),
            arrived: watch::channel(0).0,
            callbacks: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn push(&self, event: CoreEvent) {
        {
            let mut queue = lock(&self.queue);
            if queue.len() >= QUEUE_LIMIT {
                queue.pop_front();
            }
            queue.push_back(event);
        }
        self.arrived.send_modify(|n| *n = n.wrapping_add(1));
    }

    fn pop(&self) -> Option<CoreEvent> {
        lock(&self.queue).pop_front()
    }

    /// The next queued event, waiting up to `timeout` (forever when `None`).
    async fn next(&self, timeout: Option<Duration>) -> Option<CoreEvent> {
        let mut arrived = self.arrived.subscribe();
        let wait = async {
            loop {
                if let Some(event) = self.pop() {
                    return event;
                }
                if arrived.changed().await.is_err() {
                    futures::future::pending::<()>().await;
                }
            }
        };
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, wait).await.ok(),
            None => Some(wait.await),
        }
    }

    /// Converts `event` and runs every callback with it, outside any lock.
    fn dispatch(&self, py: Python<'_>, event: &CoreEvent) -> Result<Py<PyAny>, Error> {
        let object = new_py(py, AccessorEvent::new(py, event)?)?;
        let callbacks: Vec<Py<PyAny>> =
            lock(&self.callbacks).iter().map(|(_, c)| c.clone_ref(py)).collect();
        for callback in callbacks {
            if let Err(e) = callback.call1(py, (object.clone_ref(py),)) {
                e.write_unraisable(py, Some(callback.bind(py)));
            }
        }
        Ok(object)
    }
}

/// A settable value accessor.
#[pyclass(frozen, name = "Accessor", module = "liquers.accessors")]
pub struct Accessor {
    pub(crate) inner: Arc<dyn ValueAccessor<Value>>,
    scope: Scope,
    shut: Arc<Shut>,
    conversion: ConversionMode,
    pickle: bool,
    /// The token this wrapper last read or wrote.
    token: Mutex<Option<Fingerprint>>,
    events: Arc<Events>,
    /// Set when the forwarding task starts; dropping it (with the wrapper) ends the task.
    forwarding: OnceLock<watch::Sender<()>>,
}

impl Accessor {
    /// A wrapper over `inner`, converting values as `conversion` and `pickle` say.
    pub fn new(
        inner: Arc<dyn ValueAccessor<Value>>,
        scope: Scope,
        conversion: ConversionMode,
        pickle: bool,
    ) -> Self {
        let shut = scope
            .origin()
            .map(|origin| origin.shut().clone())
            .unwrap_or_else(|| Arc::new(Shut::default()));
        Accessor {
            inner,
            scope,
            shut,
            conversion,
            pickle,
            token: Mutex::new(None),
            events: Arc::new(Events::new()),
            forwarding: OnceLock::new(),
        }
    }

    /// A wrapper for a value crossing from Rust, with the default conversion.
    pub fn from_value(inner: Arc<dyn ValueAccessor<Value>>, scope: &Scope) -> Self {
        Accessor::new(inner, scope.clone(), ConversionMode::Permissive, false)
    }

    fn remember(&self, fingerprint: Fingerprint) {
        *lock(&self.token) = Some(fingerprint);
    }

    fn value_of(&self, value: &Bound<'_, PyAny>) -> Result<Value, Error> {
        py_to_value(value, self.conversion, self.pickle)
    }

    /// Starts the task that queues this accessor's events, once.
    fn ensure_forwarding(&self) -> Result<(), Error> {
        if self.forwarding.get().is_some() {
            return Ok(());
        }
        let runtime = runtime()?;
        let (closed_tx, mut closed_rx) = watch::channel(());
        if self.forwarding.set(closed_tx).is_err() {
            return Ok(());
        }
        let mut events_rx = self.inner.subscribe();
        events_rx.borrow_and_update();
        let events = self.events.clone();
        runtime.spawn(async move {
            loop {
                tokio::select! {
                    changed = events_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        let event = events_rx.borrow_and_update().clone();
                        events.push(event);
                    }
                    _ = closed_rx.changed() => break,
                }
            }
        });
        Ok(())
    }

    fn set_future(
        &self,
        value: Value,
        expected: Expected,
    ) -> impl std::future::Future<Output = Result<CoreSnapshot<Value>, Error>> + Send + 'static {
        let accessor = self.inner.clone();
        async move {
            match expected {
                Expected::LastSeen => accessor.set(value, SetOptions::default()).await,
                Expected::Token(token) => accessor.set_if(value, token, SetOptions::default()).await,
            }
        }
    }

    /// What a plain `set` expects: this wrapper's token, else the shared handle's last read.
    fn expected_for_set(&self) -> Expected {
        match *lock(&self.token) {
            Some(token) => Expected::Token(Some(token)),
            None => Expected::LastSeen,
        }
    }
}

enum Expected {
    LastSeen,
    Token(Option<Fingerprint>),
}

#[pymethods]
impl Accessor {
    /// The query that names this accessor (empty for one made in code).
    #[getter]
    fn address(&self) -> crate::query::Query {
        crate::query::Query(self.inner.address().clone())
    }

    /// Whether it can be written, and whether its source notifies changes.
    #[getter]
    fn capabilities(&self) -> AccessorCapabilities {
        let capabilities = self.inner.capabilities();
        AccessorCapabilities {
            write: capabilities.write,
            notify: capabilities.notify,
        }
    }

    /// The current value, converted. Raises the not-found error when the value does not exist.
    fn get(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "get")?;
        let accessor = self.inner.clone();
        let snapshot = block_on(py, &self.shut, async move { accessor.get().await })
            .map_err(|e| e.into_pyerr(py))?;
        self.remember(snapshot.fingerprint);
        let value = snapshot.state.value().map_err(|e| error_to_pyerr(py, &e))?;
        value_to_py(py, &value, &self.scope).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The current value with its token and state (`lq.AccessorSnapshot`).
    fn snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "snapshot")?;
        let accessor = self.inner.clone();
        let snapshot = block_on(py, &self.shut, async move { accessor.get().await })
            .map_err(|e| e.into_pyerr(py))?;
        self.remember(snapshot.fingerprint);
        AccessorSnapshot::from_core(py, &snapshot, &self.scope).map_err(|e| error_to_pyerr(py, &e))
    }

    /// Sets the value, checked against the token this wrapper last saw; returns the new token.
    fn set(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
        check_blocking_call(py, "set")?;
        let value = self.value_of(value).map_err(|e| error_to_pyerr(py, &e))?;
        let future = self.set_future(value, self.expected_for_set());
        let snapshot = block_on(py, &self.shut, future).map_err(|e| e.into_pyerr(py))?;
        self.remember(snapshot.fingerprint);
        Ok(snapshot.fingerprint.token())
    }

    /// Sets the value if its token is `token`; `None` creates a value that must not exist yet.
    /// Returns the new token; a mismatch raises `StatusConflictError` with the current `token`.
    #[pyo3(signature = (value, token))]
    fn set_if(&self, py: Python<'_>, value: &Bound<'_, PyAny>, token: Option<String>) -> PyResult<String> {
        check_blocking_call(py, "set_if")?;
        let expected = parse_token(token.as_deref()).map_err(|e| error_to_pyerr(py, &e))?;
        let value = self.value_of(value).map_err(|e| error_to_pyerr(py, &e))?;
        let future = self.set_future(value, Expected::Token(expected));
        let snapshot = block_on(py, &self.shut, future).map_err(|e| e.into_pyerr(py))?;
        self.remember(snapshot.fingerprint);
        Ok(snapshot.fingerprint.token())
    }

    /// Status, message, progress, log length and token, without reading the value.
    fn info(&self, py: Python<'_>) -> PyResult<AccessorInfo> {
        let accessor = self.inner.clone();
        let info = block_on(py, &self.shut, async move { accessor.info().await })
            .map_err(|e| e.into_pyerr(py))?;
        AccessorInfo::from_core(py, &info).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The log messages from index `since`, as `lq.LogEntry` objects.
    #[pyo3(signature = (since = 0))]
    fn log(&self, py: Python<'_>, since: usize) -> PyResult<Vec<crate::metadata::LogEntry>> {
        let accessor = self.inner.clone();
        let log = block_on(py, &self.shut, async move { accessor.log(since).await })
            .map_err(|e| e.into_pyerr(py))?;
        Ok(log.into_iter().map(|inner| crate::metadata::LogEntry { inner }).collect())
    }

    /// What the part is, for an editor (`lq.PartDescription`).
    fn describe(&self, py: Python<'_>) -> PyResult<PartDescription> {
        let accessor = self.inner.clone();
        let part = block_on(py, &self.shut, async move { accessor.describe().await })
            .map_err(|e| e.into_pyerr(py))?;
        PartDescription::from_core(py, &part).map_err(|e| error_to_pyerr(py, &e))
    }

    /// Pulls the source's pending changes (a stored key that was written directly, say).
    fn refresh(&self, py: Python<'_>) -> PyResult<()> {
        check_blocking_call(py, "refresh")?;
        let accessor = self.inner.clone();
        block_on(py, &self.shut, async move { accessor.refresh().await }).map_err(|e| e.into_pyerr(py))
    }

    /// The accessor of a part: this accessor's address with `actions` appended
    /// (`"ns-acc/recipe-0/query"`), evaluated in the environment it came from.
    fn project(&self, py: Python<'_>, actions: &str) -> PyResult<Accessor> {
        check_blocking_call(py, "project")?;
        let result = (|| -> Result<Accessor, crate::error::PyFailure> {
            let envref = self.envref()?;
            let query = projected_query(self.inner.address(), actions)?;
            let shut = self.shut.clone();
            let inner = block_on(py, &shut, async move { resolve(&envref, &query).await })?;
            Ok(Accessor::new(inner, self.scope.clone(), self.conversion, self.pickle))
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Registers `callback(event)`; it runs when events are processed on a thread that asks
    /// (`process_events`, `wait_event`, `events()`). Returns the `lq.AccessorSubscription`.
    fn on(&self, py: Python<'_>, callback: Bound<'_, PyAny>) -> PyResult<AccessorSubscription> {
        if !callback.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err("the callback must be callable"));
        }
        self.ensure_forwarding().map_err(|e| error_to_pyerr(py, &e))?;
        let id = self.events.next_id.fetch_add(1, Ordering::SeqCst);
        lock(&self.events.callbacks).push((id, callback.unbind()));
        Ok(AccessorSubscription {
            events: Arc::downgrade(&self.events),
            id,
        })
    }

    /// Runs the callbacks for every queued event; returns how many there were.
    fn process_events(&self, py: Python<'_>) -> PyResult<usize> {
        self.ensure_forwarding().map_err(|e| error_to_pyerr(py, &e))?;
        let mut count = 0;
        while let Some(event) = self.events.pop() {
            self.events.dispatch(py, &event).map_err(|e| error_to_pyerr(py, &e))?;
            count += 1;
        }
        Ok(count)
    }

    /// Waits up to `timeout` seconds (forever when `None`) for the next event, runs the callbacks
    /// with it and returns it; `None` when the time ran out.
    #[pyo3(signature = (timeout = None))]
    fn wait_event(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<Py<PyAny>>> {
        check_blocking_call(py, "wait_event")?;
        let timeout = seconds(timeout).map_err(|e| error_to_pyerr(py, &e))?;
        self.ensure_forwarding().map_err(|e| error_to_pyerr(py, &e))?;
        let events = self.events.clone();
        let event = block_on(py, &self.shut, async move { Ok(events.next(timeout).await) })
            .map_err(|e| e.into_pyerr(py))?;
        match event {
            Some(event) => self.events.dispatch(py, &event).map(Some).map_err(|e| error_to_pyerr(py, &e)),
            None => Ok(None),
        }
    }

    /// Awaitable `get`.
    fn get_async<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let accessor = this.inner.clone();
        let wrapper = slf.clone().unbind();
        let scope = this.scope.clone();
        awaitable(
            py,
            &this.shut,
            async move { Ok((accessor.get().await?, wrapper, scope)) },
            |result| {
                ToPy::With(result.map(|(snapshot, wrapper, scope)| -> PyConvert {
                    Box::new(move |py| {
                        wrapper.get().remember(snapshot.fingerprint);
                        value_to_py(py, snapshot.state.value()?.as_ref(), &scope)
                    })
                }))
            },
        )
    }

    /// Awaitable `snapshot`.
    fn snapshot_async<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let accessor = this.inner.clone();
        let wrapper = slf.clone().unbind();
        let scope = this.scope.clone();
        awaitable(
            py,
            &this.shut,
            async move { Ok((accessor.get().await?, wrapper, scope)) },
            |result| {
                ToPy::With(result.map(|(snapshot, wrapper, scope)| -> PyConvert {
                    Box::new(move |py| {
                        wrapper.get().remember(snapshot.fingerprint);
                        AccessorSnapshot::from_core(py, &snapshot, &scope)
                    })
                }))
            },
        )
    }

    /// Awaitable `set`.
    fn set_async<'py>(slf: &Bound<'py, Self>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let value = this.value_of(value).map_err(|e| error_to_pyerr(py, &e))?;
        let future = this.set_future(value, this.expected_for_set());
        set_awaitable(slf, future)
    }

    /// Awaitable `set_if`.
    #[pyo3(signature = (value, token))]
    fn set_if_async<'py>(
        slf: &Bound<'py, Self>,
        value: &Bound<'py, PyAny>,
        token: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let expected = parse_token(token.as_deref()).map_err(|e| error_to_pyerr(py, &e))?;
        let value = this.value_of(value).map_err(|e| error_to_pyerr(py, &e))?;
        let future = this.set_future(value, Expected::Token(expected));
        set_awaitable(slf, future)
    }

    /// Awaitable `info`.
    fn info_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let accessor = self.inner.clone();
        awaitable(py, &self.shut, async move { accessor.info().await }, |result| {
            ToPy::With(result.map(|info| -> PyConvert {
                Box::new(move |py| new_py(py, AccessorInfo::from_core(py, &info)?))
            }))
        })
    }

    /// Awaitable `refresh`.
    fn refresh_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let accessor = self.inner.clone();
        awaitable(py, &self.shut, async move { accessor.refresh().await }, ToPy::Unit)
    }

    /// Awaitable `project`.
    fn project_async<'py>(slf: &Bound<'py, Self>, actions: &str) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let envref = this.envref().map_err(|e| error_to_pyerr(py, &e))?;
        let query = projected_query(this.inner.address(), actions).map_err(|e| error_to_pyerr(py, &e))?;
        let (scope, conversion, pickle) = (this.scope.clone(), this.conversion, this.pickle);
        awaitable(
            py,
            &this.shut,
            async move { Ok((resolve(&envref, &query).await?, scope, conversion, pickle)) },
            |result| {
                ToPy::With(result.map(|(inner, scope, conversion, pickle)| -> PyConvert {
                    Box::new(move |py| new_py(py, Accessor::new(inner, scope, conversion, pickle)))
                }))
            },
        )
    }

    /// Awaitable `wait_event`.
    #[pyo3(signature = (timeout = None))]
    fn wait_event_async<'py>(slf: &Bound<'py, Self>, timeout: Option<f64>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let this = slf.get();
        let timeout = seconds(timeout).map_err(|e| error_to_pyerr(py, &e))?;
        this.ensure_forwarding().map_err(|e| error_to_pyerr(py, &e))?;
        let events = this.events.clone();
        let wrapper = slf.clone().unbind();
        awaitable(
            py,
            &this.shut,
            async move { Ok((events.next(timeout).await, wrapper)) },
            |result| {
                ToPy::With(result.map(|(event, wrapper)| -> PyConvert {
                    Box::new(move |py| match event {
                        Some(event) => wrapper.get().events.dispatch(py, &event),
                        None => Ok(py.None()),
                    })
                }))
            },
        )
    }

    /// An async iterator of the events, each run through the callbacks first.
    fn events<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.py()
            .import("liquers.accessors")?
            .call_method1("_events", (slf,))
    }

    fn __repr__(&self) -> String {
        let address = self.inner.address().encode();
        if address.is_empty() {
            "Accessor(<in memory>)".to_string()
        } else {
            format!("Accessor({address:?})")
        }
    }
}

impl Accessor {
    fn envref(&self) -> Result<EnvRef<PyEnvironment>, Error> {
        self.scope
            .origin()
            .and_then(|origin| origin.envref())
            .ok_or_else(|| {
                Error::general_error("the accessor's environment is gone or was never set".to_string())
            })
    }
}

fn set_awaitable<'py>(
    slf: &Bound<'py, Accessor>,
    future: impl std::future::Future<Output = Result<CoreSnapshot<Value>, Error>> + Send + 'static,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    let wrapper = slf.clone().unbind();
    awaitable(py, &slf.get().shut, async move { Ok((future.await?, wrapper)) }, |result| {
        ToPy::With(result.map(|(snapshot, wrapper)| -> PyConvert {
            Box::new(move |py| {
                wrapper.get().remember(snapshot.fingerprint);
                Ok(pyo3::types::PyString::new(py, &snapshot.fingerprint.token()).into_any().unbind())
            })
        }))
    })
}

fn seconds(timeout: Option<f64>) -> Result<Option<Duration>, Error> {
    match timeout {
        None => Ok(None),
        Some(t) if t.is_finite() && t >= 0.0 => Ok(Some(Duration::from_secs_f64(t))),
        Some(t) => Err(Error::general_error(format!("timeout must be a non-negative number, not {t}"))),
    }
}

/// `address` with `actions` appended as a new segment (`…/-/ns-acc/recipe-0`).
fn projected_query(address: &CoreQuery, actions: &str) -> Result<CoreQuery, Error> {
    let base = address.encode();
    if base.is_empty() {
        return Err(Error::general_error(
            "an accessor with no address cannot be projected by query".to_string(),
        ));
    }
    let actions = actions.trim_matches('/');
    liquers_core::parse::parse_query(&format!("{base}/{actions}"))
}

/// The accessor a query names (`liquers_core::accessor::resolve_accessor`).
pub(crate) async fn resolve(
    envref: &EnvRef<PyEnvironment>,
    query: &CoreQuery,
) -> Result<Arc<dyn ValueAccessor<Value>>, Error> {
    liquers_core::accessor::resolve_accessor(envref, query).await
}

/// Whether an accessor can be written, and whether its source notifies changes.
#[pyclass(frozen, get_all, name = "AccessorCapabilities", module = "liquers.accessors")]
pub struct AccessorCapabilities {
    /// Whether `set` can succeed: `False` for a read-only accessor (a computed asset).
    pub write: bool,
    /// Whether the root reports changes by itself; when `False`, call `refresh()` to see them.
    pub notify: bool,
}

#[pymethods]
impl AccessorCapabilities {
    fn __repr__(&self) -> String {
        format!(
            "AccessorCapabilities(write={}, notify={})",
            py_bool(self.write),
            py_bool(self.notify)
        )
    }
}

fn py_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// A read: the value (converted on first access), its `state`, its `token` and the root version.
#[pyclass(frozen, name = "AccessorSnapshot", module = "liquers.accessors")]
pub struct AccessorSnapshot {
    state: Py<crate::asset::State>,
    /// The token of this read, to pass to `set_if`.
    #[pyo3(get)]
    token: String,
    /// The root's version at this read: it changes with every write to the root, whichever part
    /// was written.
    #[pyo3(get)]
    root_version: String,
}

impl AccessorSnapshot {
    fn from_core(py: Python<'_>, snapshot: &CoreSnapshot<Value>, scope: &Scope) -> Result<Py<PyAny>, Error> {
        let state = crate::asset::State::from_core(py, &snapshot.state, scope)?;
        new_py(
            py,
            AccessorSnapshot {
                state,
                token: snapshot.fingerprint.token(),
                root_version: snapshot.root_version.to_string(),
            },
        )
    }
}

#[pymethods]
impl AccessorSnapshot {
    /// The value, converted on the first read.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.state.bind(py).getattr("value").map(Bound::unbind)
    }

    /// The part's `lq.State` (its metadata carries the root's status).
    #[getter]
    fn state(&self, py: Python<'_>) -> Py<crate::asset::State> {
        self.state.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        format!("AccessorSnapshot(token={:?})", self.token)
    }
}

/// The state behind the events: `status` (`Status.None` when the value does not exist),
/// `message`, the progress entries (dicts), `log_len` and `token` (`None` until the value has been
/// read, or when it does not exist).
#[pyclass(frozen, get_all, name = "AccessorInfo", module = "liquers.accessors")]
pub struct AccessorInfo {
    /// The root's status: `Status.None` when the value does not exist.
    pub status: crate::metadata::Status,
    /// The root's last message, empty when there is none.
    pub message: String,
    /// The root's primary progress, `None` when it reports none.
    pub primary_progress: Option<Py<PyAny>>,
    /// The root's secondary progress, `None` when it reports none.
    pub secondary_progress: Option<Py<PyAny>>,
    /// How many log entries there are, for `log(since=...)`.
    pub log_len: usize,
    /// The token of the last read, `None` until the value has been read or when it does not exist.
    pub token: Option<String>,
}

impl AccessorInfo {
    fn from_core(py: Python<'_>, info: &CoreInfo) -> Result<Self, Error> {
        let progress = |p: &Option<liquers_core::metadata::ProgressEntry>| -> Result<Option<Py<PyAny>>, Error> {
            p.as_ref().map(|p| serialized_to_py(py, p)).transpose()
        };
        Ok(AccessorInfo {
            status: crate::metadata::Status { inner: info.status },
            message: info.message.clone(),
            primary_progress: progress(&info.primary_progress)?,
            secondary_progress: progress(&info.secondary_progress)?,
            log_len: info.log_len,
            token: info.fingerprint.map(|f| f.token()),
        })
    }
}

#[pymethods]
impl AccessorInfo {
    fn __repr__(&self) -> String {
        format!(
            "AccessorInfo(status={:?}, log_len={}, token={:?})",
            self.status.inner, self.log_len, self.token
        )
    }
}

/// One event: `kind` (`"initial"`, `"status"`, `"progress"`, `"log"`, `"changed"`,
/// `"invalidated"`, `"removed"`) and its data: `status`, `primary` / `secondary` (progress
/// dicts), `count` (the log length) or `token` (the new token of `changed`).
#[pyclass(frozen, get_all, name = "AccessorEvent", module = "liquers.accessors")]
pub struct AccessorEvent {
    /// What happened: `"initial"`, `"status"`, `"progress"`, `"log"`, `"changed"`,
    /// `"invalidated"` or `"removed"`.
    pub kind: String,
    /// The new status, for a `"status"` event.
    pub status: Option<crate::metadata::Status>,
    /// The primary progress, for a `"progress"` event.
    pub primary: Option<Py<PyAny>>,
    /// The secondary progress, for a `"progress"` event.
    pub secondary: Option<Py<PyAny>>,
    /// How many log entries there are now, for a `"log"` event.
    pub count: Option<usize>,
    /// The new token, for an `"initial"` or `"changed"` event.
    pub token: Option<String>,
}

impl AccessorEvent {
    fn new(py: Python<'_>, event: &CoreEvent) -> Result<Self, Error> {
        let mut object = AccessorEvent {
            kind: event.kind().to_string(),
            status: None,
            primary: None,
            secondary: None,
            count: None,
            token: None,
        };
        match event {
            CoreEvent::Initial | CoreEvent::Invalidated | CoreEvent::Removed => {}
            CoreEvent::Status(status) => object.status = Some(crate::metadata::Status { inner: *status }),
            CoreEvent::PrimaryProgress(p) => object.primary = Some(serialized_to_py(py, p)?),
            CoreEvent::SecondaryProgress(p) => object.secondary = Some(serialized_to_py(py, p)?),
            CoreEvent::Log(count) => object.count = Some(*count),
            CoreEvent::Changed(fingerprint) => object.token = Some(fingerprint.token()),
        }
        Ok(object)
    }
}

#[pymethods]
impl AccessorEvent {
    fn __repr__(&self) -> String {
        match (&self.status, self.count, &self.token) {
            (Some(status), _, _) => format!("AccessorEvent({:?}, status={:?})", self.kind, status.inner),
            (None, Some(count), _) => format!("AccessorEvent({:?}, count={count})", self.kind),
            (None, None, Some(token)) => format!("AccessorEvent({:?}, token={token:?})", self.kind),
            (None, None, None) => format!("AccessorEvent({:?})", self.kind),
        }
    }
}

/// What a part is, for an editor: the step that made it, its type, the argument and command it
/// stands for (an action argument), whether a recipe overrides it, and whether it is read-only.
#[pyclass(frozen, get_all, name = "PartDescription", module = "liquers.accessors")]
pub struct PartDescription {
    /// The projection step that reaches the part, as a query action (`"field-title"`), or
    /// `"root"` for the root itself.
    pub step: String,
    /// The part's type identifier (`"Text"`).
    pub type_identifier: String,
    /// The command argument the part is, when it is one.
    pub argument: Option<Py<crate::command_metadata::ArgumentInfo>>,
    /// The command the part belongs to, when it is an action or an argument.
    pub command: Option<Py<crate::command_metadata::CommandKey>>,
    /// Whether a recipe's `arguments` or `links` names this argument, so its value in the query
    /// is not the one evaluation uses.
    pub overridden: bool,
    /// Whether the part cannot be set.
    pub read_only: bool,
}

impl PartDescription {
    fn from_core(py: Python<'_>, part: &CorePart) -> Result<Self, Error> {
        let argument = part
            .argument
            .clone()
            .map(|a| Py::new(py, crate::command_metadata::ArgumentInfo(a)))
            .transpose()
            .map_err(|e| pyerr_to_error(py, &e))?;
        let command = part
            .command
            .clone()
            .map(|c| Py::new(py, crate::command_metadata::CommandKey(c)))
            .transpose()
            .map_err(|e| pyerr_to_error(py, &e))?;
        Ok(PartDescription {
            step: part.step.clone(),
            type_identifier: part.type_identifier.clone(),
            argument,
            command,
            overridden: part.overridden,
            read_only: part.read_only,
        })
    }
}

#[pymethods]
impl PartDescription {
    fn __repr__(&self) -> String {
        format!(
            "PartDescription(step={:?}, type_identifier={:?}, read_only={})",
            self.step,
            self.type_identifier,
            py_bool(self.read_only)
        )
    }
}

/// A callback registered with `Accessor.on`; `dispose()` (or leaving a `with` block) removes it.
#[pyclass(frozen, name = "AccessorSubscription", module = "liquers.accessors")]
pub struct AccessorSubscription {
    events: std::sync::Weak<Events>,
    id: u64,
}

#[pymethods]
impl AccessorSubscription {
    /// Removes the callback; a second call does nothing.
    fn dispose(&self) {
        if let Some(events) = self.events.upgrade() {
            lock(&events.callbacks).retain(|(id, _)| *id != self.id);
        }
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, _exc: &Bound<'_, pyo3::types::PyTuple>) -> bool {
        self.dispose();
        false
    }
}

/// Registers the accessor classes in the native module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Accessor>()?;
    m.add_class::<AccessorCapabilities>()?;
    m.add_class::<AccessorSnapshot>()?;
    m.add_class::<AccessorInfo>()?;
    m.add_class::<AccessorEvent>()?;
    m.add_class::<PartDescription>()?;
    m.add_class::<AccessorSubscription>()?;
    Ok(())
}
