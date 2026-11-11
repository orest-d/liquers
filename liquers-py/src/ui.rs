//! `lq.UI`: a Liquers UI model held by a Python environment, driven in the background and observed
//! from Python.
//!
//! `env.ui()` makes a model: an `AppState`, a message channel and a `liquers_lib::ui::NativeDriver`
//! that a task on liquers-py's runtime ticks (`spawn_headless`), so elements keep evaluating
//! between Python calls. Python shows queries as elements, submits queries as a control's action
//! does, lists the elements, and follows their statuses through callbacks.
//!
//! # Who runs the callbacks
//!
//! The background task only queues status events (at most 1024; the oldest is dropped). A
//! callback runs on a thread that asked for it: in `wait_idle`, on the event loop of
//! `wait_idle_async`, in `process_events()`, or in the frame hook of a window (`run`), and never
//! while the `AppState` lock is held, so a callback may call back into the UI. While a window is
//! open only its frame hook delivers: `wait_idle` waits without delivering and `process_events`
//! returns 0. A callback that raises is reported through `sys.unraisablehook`; delivery goes on.
//!
//! # Idle
//!
//! `show` and `submit` count each change after making it (the driver's change counter). A tick
//! that finds no waiting message, pending node or evaluation records the count it read; the model
//! is idle when that count is the current one, so every change has been through a whole tick.
//!
//! # Main components
//!
//! - [`UI`] (`lq.UI`): `show`, `submit`, `elements`, `on`, `wait_idle`, `wait_idle_async`,
//!   `process_events`, `asset`, `dispose`; and the window: `run` (feature `gui`; it borrows the
//!   driver from the background task and gives it back), `close` (any thread), `window_open`.
//!   `command.rs` marks every Python command's thread, so `wait_idle` inside one raises: the UI
//!   may be waiting for that command.
//! - [`UIElement`] (`lq.UIElement`): a UI element value as Python sees it; it crosses back as the
//!   same element.
//! - [`UIElementInfo`], [`UIStatusEvent`], [`UISubscription`]: what `elements()` lists, what a
//!   callback receives, what `on` returns.
//! - [`UiModel`]: the shared state behind a `UI`, which the environment and the exit hook dispose.
//!
//! # Where it is used
//!
//! - `environment.rs`: `Environment.ui()`; `shutdown()` disposes the environment's models.
//! - `exit.rs`: interpreter exit stops every model's task.
//! - `value.rs`: `ExtValue::UIElement` to and from [`UIElement`]; `context.rs`: `Context.element`.
//! - `command.rs`: marks a thread running a Python command (and the model whose action runs it),
//!   so `wait_idle` there raises instead of waiting for itself.
//!
//! # Related concepts
//!
//! - `liquers_lib::ui::NativeDriver` and `spawn_headless`: the driver this module ticks; the
//!   window (`liquers_lib::egui::app::run_window`) ticks the same driver.
//! - `liquers-web/src/ui.rs`: the browser counterpart, whose API this mirrors.
//! - `specs/reference/PYTHON_INTEGRATION.md` §18.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_UIUSE_model.py` (a Rust doctest cannot start the
//! interpreter):
//!
//! ```python
//! ui = env.ui()
//! with ui.on("status", lambda e: print(e.handle, e.status)):
//!     handle = ui.show("hello")
//!     assert ui.wait_idle(timeout=5)
//! assert ui.elements()[0].status == "Ready"
//! ui.dispose()
//! ```

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use liquers_core::context::Context as CoreContext;
use liquers_core::error::Error;
use liquers_lib::ui::{
    app_message_channel, AppMessage, AppMessageSender, AppState, DirectAppState, ElementSource,
    HeadlessHandle, InsertionPoint, NativeDriver, RunnerEvent, Tick, UIHandle, UIPayload,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::MutexExt;
use tokio::sync::watch;

use crate::aio::{check_blocking_call, shut_down_while_waiting};
use crate::environment::{block_on, shut_down_error, to_query, EnvCell, PyEnvironment};
use crate::error::{error_to_pyerr, PyFailure};
use crate::metadata::Status;
use crate::runtime::runtime;

/// How many undelivered status events a model keeps; the oldest is dropped beyond it.
const MAX_PENDING_EVENTS: usize = 1024;
/// How long `wait_idle` waits between two signal checks.
const WAIT_SLICE: Duration = Duration::from_millis(100);
/// How long `dispose()` waits for the background task to finish its tick.
const DISPOSE_DEADLINE: Duration = Duration::from_secs(5);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Who holds the model's driver.
pub(crate) enum DriverSlot {
    /// Ticked by a task on the runtime.
    Background(HeadlessHandle<PyEnvironment>),
    /// Lent to a window, or between two owners.
    Lent,
    /// The model was disposed.
    Disposed,
}

/// A status callback, with its subscription id.
struct Subscriber {
    id: u64,
    callback: Py<PyAny>,
}

/// The shared state behind an `lq.UI`.
pub struct UiModel {
    cell: Arc<EnvCell>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    /// The driver's change counter: incremented after each `show` / `submit`.
    changes: Arc<AtomicU64>,
    driver: Mutex<DriverSlot>,
    /// The `changes` count of the last tick that settled, `None` after one that did not.
    idle_at: Mutex<Option<u64>>,
    pending: Mutex<VecDeque<RunnerEvent>>,
    statuses: Mutex<HashMap<UIHandle, liquers_core::metadata::Status>>,
    subscribers: Mutex<Vec<Subscriber>>,
    next_subscriber: AtomicU64,
    /// Serialises deliveries, so callbacks see events in order.
    delivery: Mutex<()>,
    /// Counts ticks; `wait_idle` waits for it to change.
    ticks: watch::Sender<u64>,
    /// The thread a window shows on, while `run` blocks (`liquers-py-ui-window`).
    pub(crate) window_thread: Mutex<Option<ThreadId>>,
    pub(crate) window_open: AtomicBool,
    /// Closes the window `run` shows, from any thread.
    #[cfg(feature = "gui")]
    window: Mutex<Option<liquers_lib::egui::app::WindowCloser>>,
    /// Set by `close()` under the `window` lock, so a close that arrives before the window's first
    /// frame is not lost: the frame hook reads it.
    #[cfg(feature = "gui")]
    close_requested: AtomicBool,
    disposed: AtomicBool,
}

thread_local! {
    /// One entry per Python command running on this thread: the address of the `AppState` whose
    /// action runs it, or `NO_UI` for a command outside a UI action (an element's own evaluation,
    /// a plain query).
    static IN_ACTION: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// The models (by address) this thread is delivering status events for, so a callback's own
    /// `wait_idle` or `process_events` on the same model does not deliver the rest out of order.
    static DELIVERING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The `IN_ACTION` entry of a command that runs for no UI action.
const NO_UI: usize = 0;

fn state_address(app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> usize {
    Arc::as_ptr(app_state) as *const () as usize
}

/// Marks the current thread as running a Python command, and for which UI action, while it lives.
///
/// The runner awaits an action's query inline, so a `wait_idle` on the same model from inside it
/// would wait for the tick that is waiting for it. An element's own evaluation carries no payload,
/// so any command's `wait_idle` is refused: the model may be waiting for that very command.
pub(crate) struct ActionMark(usize);

impl ActionMark {
    pub(crate) fn enter(context: &CoreContext<PyEnvironment>) -> Self {
        let address = context
            .get_payload_clone()
            .map(|payload| state_address(&payload.app_state()))
            .unwrap_or(NO_UI);
        IN_ACTION.with(|marks| marks.borrow_mut().push(address));
        ActionMark(address)
    }
}

impl Drop for ActionMark {
    fn drop(&mut self) {
        let address = self.0;
        IN_ACTION.with(|marks| {
            let mut marks = marks.borrow_mut();
            if let Some(index) = marks.iter().rposition(|a| *a == address) {
                marks.remove(index);
            }
        });
    }
}

struct Delivering(usize);

impl Delivering {
    /// `None` when this thread already delivers for the model at `address`.
    fn enter(address: usize) -> Option<Self> {
        DELIVERING.with(|d| {
            let mut models = d.borrow_mut();
            if models.contains(&address) {
                None
            } else {
                models.push(address);
                Some(Delivering(address))
            }
        })
    }
}

impl Drop for Delivering {
    fn drop(&mut self) {
        DELIVERING.with(|d| {
            let mut models = d.borrow_mut();
            if let Some(index) = models.iter().rposition(|a| *a == self.0) {
                models.remove(index);
            }
        });
    }
}

/// Every model of the process, for the exit hook.
static MODELS: Mutex<Vec<Weak<UiModel>>> = Mutex::new(Vec::new());

fn disposed_error() -> Error {
    Error::general_error("the UI has been disposed".to_string())
}

impl UiModel {
    /// A model over `cell`'s environment, ticked in the background from the start.
    fn new(py: Python<'_>, cell: &Arc<EnvCell>) -> Result<Arc<Self>, Error> {
        let envref = cell.envref(py)?;
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let (sender, receiver) = app_message_channel();
        let source = cell.clone();
        let driver = NativeDriver::new(envref, app_state.clone(), receiver, sender.clone())
            .with_current_env(Box::new(move || source.current_without_gil().ok().flatten()));
        let model = Arc::new(UiModel {
            cell: cell.clone(),
            app_state,
            sender,
            changes: driver.change_counter(),
            driver: Mutex::new(DriverSlot::Lent),
            idle_at: Mutex::new(None),
            pending: Mutex::new(VecDeque::new()),
            statuses: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(Vec::new()),
            next_subscriber: AtomicU64::new(1),
            delivery: Mutex::new(()),
            ticks: watch::Sender::new(0),
            window_thread: Mutex::new(None),
            window_open: AtomicBool::new(false),
            #[cfg(feature = "gui")]
            window: Mutex::new(None),
            #[cfg(feature = "gui")]
            close_requested: AtomicBool::new(false),
            disposed: AtomicBool::new(false),
        });
        cell.add_ui(Arc::downgrade(&model))?;
        {
            let mut all = lock(&MODELS);
            all.retain(|weak| weak.strong_count() > 0);
            all.push(Arc::downgrade(&model));
        }
        model.start_background(driver)?;
        Ok(model)
    }

    /// Ticks `driver` on the runtime; the task holds only a `Weak` of the model.
    pub(crate) fn start_background(self: &Arc<Self>, driver: NativeDriver<PyEnvironment>) -> Result<(), Error> {
        let runtime = runtime()?;
        let weak = Arc::downgrade(self);
        let handle = liquers_lib::ui::spawn_headless(
            driver,
            runtime.handle(),
            Box::new(move |tick: Tick| {
                if let Some(model) = weak.upgrade() {
                    model.record_tick(&tick);
                }
            }),
        );
        let mut slot = lock(&self.driver);
        match &*slot {
            DriverSlot::Disposed => {} // disposed meanwhile: dropping `handle` stops the task
            DriverSlot::Lent | DriverSlot::Background(_) => *slot = DriverSlot::Background(handle),
        }
        Ok(())
    }

    /// The bookkeeping of one tick, shared by the background task and a window's frame hook.
    pub(crate) fn record_tick(&self, tick: &Tick) {
        {
            let mut statuses = lock(&self.statuses);
            for event in &tick.events {
                if let Some(handle) = event.handle {
                    statuses.insert(handle, event.status);
                }
            }
        }
        if !self.disposed.load(Ordering::SeqCst) {
            let mut pending = lock(&self.pending);
            for event in &tick.events {
                if pending.len() >= MAX_PENDING_EVENTS {
                    pending.pop_front();
                }
                pending.push_back(event.clone());
            }
        }
        *lock(&self.idle_at) = tick.settled.then_some(tick.changes);
        self.ticks.send_modify(|count| *count = count.wrapping_add(1));
    }

    pub(crate) fn has_pending_events(&self) -> bool {
        !lock(&self.pending).is_empty()
    }

    fn is_idle(&self) -> bool {
        *lock(&self.idle_at) == Some(self.changes.load(Ordering::SeqCst))
    }

    fn check_open(&self) -> Result<(), Error> {
        if self.disposed.load(Ordering::SeqCst) {
            return Err(disposed_error());
        }
        Ok(())
    }

    /// Counts a change made from outside a tick and wakes the background task.
    fn note_change(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
        if let DriverSlot::Background(handle) = &*lock(&self.driver) {
            handle.waker().notify_one();
        }
    }

    fn in_own_action(&self) -> bool {
        let address = state_address(&self.app_state);
        IN_ACTION.with(|marks| marks.borrow().contains(&address))
    }

    /// Locks the `AppState` from a Python thread, with the GIL released while waiting.
    fn with_state<T: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut dyn AppState) -> Result<T, Error> + Send,
    ) -> Result<T, Error> {
        let app_state = self.app_state.clone();
        py.detach(move || {
            futures::executor::block_on(async move {
                let mut state = app_state.lock().await;
                f(&mut *state)
            })
        })
    }

    fn show(&self, py: Python<'_>, query: String, handle: Option<u64>) -> Result<u64, Error> {
        self.check_open()?;
        self.cell.envref(py)?; // builds a late registration before the query runs
        let placed = self.with_state(py, move |state| {
            let point = match handle {
                Some(h) => {
                    let h = UIHandle(h);
                    if !state.node_exists(h) {
                        return Err(Error::general_error(format!("no UI element {}", h.0)));
                    }
                    InsertionPoint::Instead(h)
                }
                None => InsertionPoint::Root,
            };
            state.insert_source(&point, ElementSource::Query(query))
        })?;
        self.note_change();
        Ok(placed.0)
    }

    fn submit(&self, py: Python<'_>, query: String, handle: Option<u64>) -> Result<(), Error> {
        self.check_open()?;
        self.cell.envref(py)?;
        let handle = match handle {
            Some(h) => {
                let h = UIHandle(h);
                if !self.with_state(py, move |state| Ok(state.node_exists(h)))? {
                    return Err(Error::general_error(format!("no UI element {}", h.0)));
                }
                Some(h)
            }
            None => None,
        };
        self.sender
            .send(AppMessage::SubmitQuery { handle, query })
            .map_err(|_| disposed_error())?;
        self.note_change();
        Ok(())
    }

    /// Delivers the queued events to the callbacks; `from_window` is the window's frame hook.
    pub(crate) fn deliver(&self, py: Python<'_>, from_window: bool) -> PyResult<usize> {
        if !from_window && self.window_open.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let Some(_delivering) = Delivering::enter(self as *const Self as usize) else {
            return Ok(0); // a callback's own call: the outer delivery continues in order
        };
        let _serial = self
            .delivery
            .lock_py_attached(py)
            .map_err(|_| error_to_pyerr(py, &Error::unexpected_error("UI delivery lock poisoned".to_string())))?;
        // Checked again under the lock: a window that opened meanwhile delivers from now on.
        if !from_window && self.window_open.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let events: Vec<RunnerEvent> = lock(&self.pending).drain(..).collect();
        if events.is_empty() {
            return Ok(0);
        }
        for event in &events {
            let ids: Vec<u64> = self.subscriber_ids(py);
            let payload = Py::new(py, UIStatusEvent::new(py, event))?;
            for id in ids {
                // Looked up again per call: a subscription disposed by a callback runs no more.
                let Some(callback) = self.subscriber(py, id) else {
                    continue;
                };
                if let Err(error) = callback.call1(py, (payload.clone_ref(py),)) {
                    error.write_unraisable(py, Some(callback.bind(py)));
                }
            }
        }
        Ok(events.len())
    }

    fn subscriber_ids(&self, py: Python<'_>) -> Vec<u64> {
        match self.subscribers.lock_py_attached(py) {
            Ok(subscribers) => subscribers.iter().map(|s| s.id).collect(),
            Err(poisoned) => poisoned.into_inner().iter().map(|s| s.id).collect(),
        }
    }

    fn subscriber(&self, py: Python<'_>, id: u64) -> Option<Py<PyAny>> {
        let subscribers = match self.subscribers.lock_py_attached(py) {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.callback.clone_ref(py))
    }

    fn unsubscribe(&self, py: Python<'_>, id: u64) {
        let removed: Vec<Subscriber> = {
            let mut subscribers = match self.subscribers.lock_py_attached(py) {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let (gone, kept): (Vec<_>, Vec<_>) =
                std::mem::take(&mut *subscribers).into_iter().partition(|s| s.id == id);
            *subscribers = kept;
            gone
        };
        drop(removed); // after the guard: a callable's __del__ may call back into the UI
    }

    fn refuse_unreturnable_wait(&self) -> Result<(), Error> {
        if *lock(&self.window_thread) == Some(std::thread::current().id()) {
            return Err(Error::general_error(
                "wait_idle cannot be called on the window's thread while the window shows; \
                 the window delivers the events itself"
                    .to_string(),
            ));
        }
        if self.in_own_action() {
            return Err(Error::general_error(
                "wait_idle cannot be called from a command run by this UI's own action: \
                 the UI waits for the command to finish"
                    .to_string(),
            ));
        }
        if IN_ACTION.with(|marks| !marks.borrow().is_empty()) {
            return Err(Error::general_error(
                "wait_idle cannot be called from inside a command: a UI showing the command's \
                 query would wait for the command to finish"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn check_waitable(&self) -> Result<(), Error> {
        self.check_open()?;
        if self.cell.shut.is_set() {
            return Err(shut_down_while_waiting());
        }
        Ok(())
    }

    fn wait_idle(&self, py: Python<'_>, timeout: Option<f64>) -> Result<bool, PyFailure> {
        self.check_open()?;
        self.refuse_unreturnable_wait()?;
        let deadline = deadline_after(timeout)?;
        let runtime = runtime()?;
        loop {
            let mut ticks = self.ticks.subscribe();
            self.deliver(py, false).map_err(PyFailure::Python)?;
            if self.is_idle() {
                self.deliver(py, false).map_err(PyFailure::Python)?;
                return Ok(true);
            }
            self.check_waitable()?;
            let slice = match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Ok(false);
                    }
                    (deadline - now).min(WAIT_SLICE)
                }
                None => WAIT_SLICE,
            };
            py.detach(|| {
                runtime.block_on(async {
                    let _ = tokio::time::timeout(slice, ticks.changed()).await;
                })
            });
            py.check_signals().map_err(PyFailure::Python)?;
        }
    }

    /// Marks the model disposed and takes what it owned; `None` when it already was.
    fn take_for_dispose(&self) -> Option<(DriverSlot, Vec<Subscriber>)> {
        if self.disposed.swap(true, Ordering::SeqCst) {
            return None;
        }
        let slot = std::mem::replace(&mut *lock(&self.driver), DriverSlot::Disposed);
        self.close_window();
        lock(&self.pending).clear();
        let subscribers = std::mem::take(&mut *lock(&self.subscribers));
        self.ticks.send_modify(|count| *count = count.wrapping_add(1));
        Some((slot, subscribers))
    }

    /// Disposes the model, stops its task and waits (up to 5 s) for the tick in progress, unless
    /// this thread runs an action of this UI, which that tick is waiting for.
    pub(crate) fn dispose(&self, py: Python<'_>) {
        let Some((slot, subscribers)) = self.take_for_dispose() else {
            return;
        };
        drop(subscribers); // no lock held: a callable's __del__ may call back into the UI
        if let DriverSlot::Background(handle) = slot {
            if self.in_own_action() {
                drop(handle);
                return;
            }
            if let Ok(runtime) = runtime() {
                py.detach(|| {
                    runtime.block_on(async {
                        let _ = tokio::time::timeout(DISPOSE_DEADLINE, handle.stop()).await;
                    })
                });
            }
        }
    }

    /// Disposes the model without waiting: the task is only signalled. For a collected handle
    /// and interpreter exit.
    pub(crate) fn dispose_without_waiting(&self) {
        if let Some((slot, subscribers)) = self.take_for_dispose() {
            drop(subscribers);
            drop(slot);
        }
    }

    /// Closes the window `run` shows, if any; any thread.
    fn close_window(&self) {
        #[cfg(feature = "gui")]
        {
            let window = lock(&self.window);
            if let Some(closer) = window.as_ref() {
                self.close_requested.store(true, Ordering::SeqCst);
                closer.close();
            }
        }
    }

    /// Takes the driver from the background task for a window: `Lent` until handed back.
    #[cfg(feature = "gui")]
    fn borrow_driver(&self, py: Python<'_>) -> Result<NativeDriver<PyEnvironment>, PyFailure> {
        let handle = {
            let mut slot = lock(&self.driver);
            match std::mem::replace(&mut *slot, DriverSlot::Lent) {
                DriverSlot::Background(handle) => handle,
                DriverSlot::Lent => {
                    return Err(Error::general_error("a window is already open on this UI".to_string()).into())
                }
                DriverSlot::Disposed => {
                    *slot = DriverSlot::Disposed;
                    return Err(disposed_error().into());
                }
            }
        };
        // The lock is released: the task's last tick may need the model (`record_tick`). The wait
        // checks no signals: dropping `stop` half-way would drop the driver with the task. A
        // Ctrl-C meanwhile stays pending, and the window's first frame raises it.
        let runtime = runtime()?;
        let stopped = py.detach(|| runtime.block_on(handle.stop()));
        match stopped {
            Ok(driver) => Ok(driver),
            Err(error) => {
                // The task panicked and took the driver with it: the model cannot go on.
                self.dispose_without_waiting();
                Err(error.into())
            }
        }
    }

    /// Shows the model in a window on this (the main) thread until it closes; see `UI.run`.
    #[cfg(feature = "gui")]
    fn run_window(self: &Arc<Self>, py: Python<'_>, title: String, size: (f32, f32)) -> Result<(), PyFailure> {
        use liquers_lib::egui::app::{run_window, FrameControl, WindowCloser, WindowOptions};

        self.check_open()?;
        let threading = py.import("threading").map_err(PyFailure::Python)?;
        let current = threading.call_method0("current_thread").map_err(PyFailure::Python)?;
        let main = threading.call_method0("main_thread").map_err(PyFailure::Python)?;
        if !current.is(&main) {
            return Err(Error::general_error("UI.run must be called on the main thread".to_string()).into());
        }
        let runtime = runtime()?;
        if self.window_open.swap(true, Ordering::SeqCst) {
            return Err(Error::general_error("a window is already open on this UI".to_string()).into());
        }
        // The closer is in place before the driver is taken, so a `close()` from now on is kept.
        let closer = WindowCloser::default();
        {
            let mut window = lock(&self.window);
            *window = Some(closer.clone());
            self.close_requested.store(false, Ordering::SeqCst);
        }
        *lock(&self.window_thread) = Some(std::thread::current().id());
        let mut driver = match self.borrow_driver(py) {
            Ok(driver) => driver,
            Err(error) => {
                self.end_window();
                return Err(error);
            }
        };

        let mut options = WindowOptions::new(title);
        options.inner_size = [size.0, size.1];
        options.poll_interval = WAIT_SLICE;
        let model: &UiModel = self;
        let mut kept: Option<PyErr> = None;
        let mut last_attached = Instant::now();
        let outcome = {
            let kept = &mut kept;
            let mut hook = move |tick: &Tick| -> FrameControl {
                model.record_tick(tick);
                if model.close_requested.load(Ordering::SeqCst) {
                    return FrameControl::Close;
                }
                if !model.has_pending_events() && last_attached.elapsed() < WAIT_SLICE {
                    return FrameControl::Continue;
                }
                last_attached = Instant::now();
                Python::attach(|py| {
                    let checked = model.deliver(py, true).and_then(|_| py.check_signals());
                    match checked {
                        Ok(()) => FrameControl::Continue,
                        Err(error) => {
                            *kept = Some(error);
                            FrameControl::Close
                        }
                    }
                })
            };
            let closer = &closer;
            let driver = &mut driver;
            // Closed or disposed while the driver was being taken: open nothing.
            if self.close_requested.load(Ordering::SeqCst) || self.is_disposed() {
                Ok(())
            } else {
                py.detach(move || run_window(options, driver, runtime.handle(), closer, Some(&mut hook)))
            }
        };

        self.end_window();
        if !self.is_disposed() {
            if let Err(error) = self.start_background(driver) {
                self.dispose_without_waiting();
                return Err(error.into());
            }
        }
        // Events of the last frames, if the hook did not run after them.
        self.deliver(py, false).map_err(PyFailure::Python)?;
        if let Some(error) = kept {
            return Err(PyFailure::Python(error));
        }
        outcome.map_err(PyFailure::from)
    }

    /// Clears the window state `run_window` set, whichever way it ends.
    #[cfg(feature = "gui")]
    fn end_window(&self) {
        {
            let mut window = lock(&self.window);
            *window = None;
            self.close_requested.store(false, Ordering::SeqCst);
        }
        *lock(&self.window_thread) = None;
        self.window_open.store(false, Ordering::SeqCst);
    }

    pub(crate) fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }
}

fn deadline_after(timeout: Option<f64>) -> Result<Option<Instant>, Error> {
    match timeout {
        None => Ok(None),
        Some(seconds) => Duration::try_from_secs_f64(seconds)
            .map(|d| Some(Instant::now() + d))
            .map_err(|_| {
                Error::general_error(format!("timeout must be a number of seconds, not {seconds}"))
            }),
    }
}

/// Disposes every model of the process without waiting: the interpreter is exiting.
pub(crate) fn stop_all_uis(py: Python<'_>) {
    // `py` proves the caller holds the interpreter: the callables are released here.
    let models: Vec<Arc<UiModel>> = lock(&MODELS).drain(..).filter_map(|w| w.upgrade()).collect();
    for model in models {
        model.dispose_without_waiting();
    }
    let _ = py;
}

#[cfg(feature = "gui")]
fn run_model(py: Python<'_>, model: &Arc<UiModel>, title: String, size: (f32, f32)) -> Result<(), PyFailure> {
    model.run_window(py, title, size)
}

#[cfg(not(feature = "gui"))]
fn run_model(_py: Python<'_>, _model: &Arc<UiModel>, _title: String, _size: (f32, f32)) -> Result<(), PyFailure> {
    Err(no_window_support().into())
}

/// The error of `run` in a build without the `gui` feature.
#[cfg(not(feature = "gui"))]
fn no_window_support() -> Error {
    Error::general_error(
        "this build has no window support: build liquers-py with the gui feature".to_string(),
    )
}

/// The query text a `show` or `submit` argument stands for: a `str` (parsed, so a syntax error
/// raises here) or an `lq.Query`.
fn query_text(query: &Bound<'_, PyAny>) -> Result<String, Error> {
    Ok(to_query(query)?.encode())
}

/// A Liquers UI model on an environment, as `env.ui()` returns it.
///
/// `show` and `submit` change it from any thread; its elements evaluate in the background.
/// Status callbacks (`on("status", …)`) run only on a thread that asks for them: `wait_idle`,
/// `wait_idle_async`, `process_events()`, or a window's frames. A context manager: leaving the
/// block disposes it. See `PYTHON_GUIDE.md` "Show a Liquers UI".
#[pyclass(frozen, name = "UI", module = "liquers")]
pub struct UI {
    pub(crate) model: Arc<UiModel>,
}

impl UI {
    pub(crate) fn new(py: Python<'_>, cell: &Arc<EnvCell>) -> PyResult<Self> {
        UiModel::new(py, cell)
            .map(|model| UI { model })
            .map_err(|e| error_to_pyerr(py, &e))
    }
}

impl Drop for UI {
    fn drop(&mut self) {
        // Nothing else can reach a collected UI: signal its task and drop its callbacks. The
        // model itself goes when the last reference (the task's tick in progress) does.
        self.model.dispose_without_waiting();
    }
}

#[pymethods]
impl UI {
    /// Shows `query`'s value as an element and returns its handle: a new root, or in place of
    /// element `handle`. Returns at once; the element evaluates in the background.
    #[pyo3(signature = (query, handle = None))]
    fn show(&self, py: Python<'_>, query: &Bound<'_, PyAny>, handle: Option<u64>) -> PyResult<u64> {
        let result = query_text(query).and_then(|query| self.model.show(py, query, handle));
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Runs `query` as a control's action does: with the UI payload, bound to element `handle`
    /// (or to none). Its commands change the UI themselves (`lui/add`). Returns at once.
    #[pyo3(signature = (query, handle = None))]
    fn submit(&self, py: Python<'_>, query: &Bound<'_, PyAny>, handle: Option<u64>) -> PyResult<()> {
        let result = query_text(query).and_then(|query| self.model.submit(py, query, handle));
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The elements, depth first from the roots, with their last status.
    fn elements(&self, py: Python<'_>) -> PyResult<Vec<UIElementInfo>> {
        self.model.check_open().map_err(|e| error_to_pyerr(py, &e))?;
        let rows = self.model.with_state(py, |state| {
            let mut rows = Vec::new();
            let mut stack: Vec<UIHandle> = state.roots().into_iter().rev().collect();
            while let Some(handle) = stack.pop() {
                let element = state.get_element(handle).ok().flatten();
                rows.push((
                    handle,
                    state.parent(handle).ok().flatten(),
                    element.map(|e| e.type_name().to_string()),
                    element.map(|e| e.title()),
                    element.is_some(),
                ));
                let children = state.children(handle).unwrap_or_default();
                stack.extend(children.into_iter().rev());
            }
            Ok(rows)
        });
        let rows = rows.map_err(|e| error_to_pyerr(py, &e))?;
        let statuses = lock(&self.model.statuses).clone();
        Ok(rows
            .into_iter()
            .map(|(handle, parent, type_name, title, has_element)| {
                let status = match statuses.get(&handle) {
                    Some(status) => *status,
                    None if has_element => liquers_core::metadata::Status::Ready,
                    None => liquers_core::metadata::Status::None,
                };
                UIElementInfo {
                    handle: handle.0,
                    parent: parent.map(|p| p.0),
                    type_name,
                    title,
                    status: Status { inner: status },
                }
            })
            .collect())
    }

    /// Subscribes `callback` to status events (`kind` must be `"status"`). The callback gets a
    /// `UIStatusEvent`; see the class doc for the thread it runs on.
    fn on(&self, py: Python<'_>, kind: &str, callback: Bound<'_, PyAny>) -> PyResult<UISubscription> {
        self.model.check_open().map_err(|e| error_to_pyerr(py, &e))?;
        if kind != "status" {
            return Err(PyValueError::new_err(format!(
                "unknown UI event kind {kind:?}; the only kind is \"status\""
            )));
        }
        if !callback.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err("the callback must be callable"));
        }
        let id = self.model.next_subscriber.fetch_add(1, Ordering::SeqCst);
        let mut subscribers = self
            .model
            .subscribers
            .lock_py_attached(py)
            .map_err(|_| error_to_pyerr(py, &Error::unexpected_error("UI subscriber lock poisoned".to_string())))?;
        // Checked again under the lock, which `dispose` takes too: no callable outlives the model.
        if self.model.is_disposed() {
            drop(subscribers);
            return Err(error_to_pyerr(py, &disposed_error()));
        }
        subscribers.push(Subscriber { id, callback: callback.unbind() });
        Ok(UISubscription { model: Arc::downgrade(&self.model), id })
    }

    /// Waits until every change made so far has been through a whole tick and no element is
    /// evaluating, delivering status events meanwhile. Returns `False` when `timeout` (seconds)
    /// passes first. The GIL is released while waiting; Ctrl-C raises `KeyboardInterrupt`.
    #[pyo3(signature = (timeout = None))]
    fn wait_idle(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<bool> {
        check_blocking_call(py, "wait_idle")?;
        self.model.wait_idle(py, timeout).map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `wait_idle`: delivers the events on the awaiting event loop's thread.
    #[pyo3(signature = (timeout = None))]
    fn wait_idle_async<'py>(slf: &Bound<'py, Self>, timeout: Option<f64>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        slf.get().model.refuse_unreturnable_wait().map_err(|e| error_to_pyerr(py, &e))?;
        py.import("liquers._ui")?.call_method1("wait_idle_async", (slf, timeout))
    }

    /// Delivers the queued status events now and returns how many; 0 while a window shows.
    fn process_events(&self, py: Python<'_>) -> PyResult<usize> {
        self.model.deliver(py, false)
    }

    /// Whether the model is idle (`wait_idle`'s condition), without waiting or delivering.
    fn _idle(&self, py: Python<'_>) -> PyResult<bool> {
        self.model.check_waitable().map_err(|e| error_to_pyerr(py, &e))?;
        Ok(self.model.is_idle())
    }

    /// Resolves after the next tick, or after `timeout` seconds (for `wait_idle_async`).
    fn _next_tick_async<'py>(&self, py: Python<'py>, timeout: f64) -> PyResult<Bound<'py, PyAny>> {
        let slice = Duration::try_from_secs_f64(timeout.max(0.0)).unwrap_or(WAIT_SLICE);
        let mut ticks = self.model.ticks.subscribe();
        runtime().map_err(|e| error_to_pyerr(py, &e))?;
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let _ = tokio::time::timeout(slice, ticks.changed()).await;
            Ok(())
        })
    }

    /// The asset of element `handle`'s query: its log, progress and `cancel()`.
    fn asset(&self, py: Python<'_>, handle: u64) -> PyResult<crate::asset::AssetRef> {
        let result = (|| -> Result<_, PyFailure> {
            self.model.check_open()?;
            let h = UIHandle(handle);
            let query = self.model.with_state(py, move |state| match state.get_source(h) {
                Ok(ElementSource::Query(query)) => Ok(query.clone()),
                Ok(ElementSource::None) | Ok(ElementSource::Recipe(_)) => Err(Error::general_error(
                    format!("UI element {handle} has no query"),
                )),
                Err(_) => Err(Error::general_error(format!("no UI element {handle}"))),
            })?;
            let cell = &self.model.cell;
            let envref = cell.envref(py)?;
            let scope = crate::records::Scope::from_envref(&envref, cell.shut.clone());
            let asset = block_on(py, &cell.shut, async move { envref.evaluate(&query).await })?;
            Ok(crate::asset::AssetRef::new(asset, cell.shut.clone(), scope))
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Shows the model in a desktop window (the egui backend) and returns when it closes.
    ///
    /// Must be called on the main thread; it blocks with the GIL released, so commands keep
    /// running on other threads. Between frames the window delivers status callbacks on this
    /// thread and checks for signals: Ctrl-C closes it and raises `KeyboardInterrupt`. `close()`
    /// from any thread, a *Quit* control, or closing the window ends it; the model keeps running in
    /// the background afterwards. A build without the `gui` feature raises `lq.LiquersError`.
    #[pyo3(signature = (title = "Liquers".to_string(), size = (900.0, 700.0)))]
    fn run(slf: &Bound<'_, Self>, title: String, size: (f32, f32)) -> PyResult<()> {
        let py = slf.py();
        check_blocking_call(py, "run")?;
        run_model(py, &slf.get().model, title, size).map_err(|e| e.into_pyerr(py))
    }

    /// Closes the window `run` shows; any thread. Does nothing without a window.
    fn close(&self) {
        self.model.close_window();
    }

    /// Whether `run` shows a window now.
    #[getter]
    fn window_open(&self) -> bool {
        self.model.window_open.load(Ordering::SeqCst)
    }

    /// Disposes the model: its task stops, its callbacks are released and every later call
    /// raises. Idempotent.
    fn dispose(&self, py: Python<'_>) {
        self.model.dispose(py);
    }

    /// Whether `dispose()` (or `env.shutdown()`) ended the model.
    #[getter]
    fn disposed(&self) -> bool {
        self.model.is_disposed()
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, py: Python<'_>, _exc: &Bound<'_, pyo3::types::PyTuple>) -> bool {
        self.dispose(py);
        false
    }

    fn __repr__(&self) -> String {
        let state = if self.model.is_disposed() { "disposed" } else { "running" };
        format!("<liquers.UI {state}>")
    }
}

/// One element, as `UI.elements()` lists it.
#[pyclass(frozen, get_all, name = "UIElementInfo", module = "liquers")]
pub struct UIElementInfo {
    /// The element's handle.
    handle: u64,
    /// Its parent's handle, `None` for a root.
    parent: Option<u64>,
    /// The element's type (`"AssetViewElement"`, `"QueryConsoleElement"`), `None` while pending.
    type_name: Option<String>,
    /// The element's title, `None` while pending.
    title: Option<String>,
    /// The last status reported for it: `Ready` for an element shown without an evaluation,
    /// `None` for one still pending.
    status: Status,
}

#[pymethods]
impl UIElementInfo {
    fn __repr__(&self) -> String {
        fn py_str(text: &Option<String>) -> String {
            match text {
                Some(text) => format!("{text:?}"),
                None => "None".to_string(),
            }
        }
        let parent = match self.parent {
            Some(parent) => parent.to_string(),
            None => "None".to_string(),
        };
        format!(
            "UIElementInfo(handle={}, parent={parent}, type_name={}, title={}, status={:?})",
            self.handle,
            py_str(&self.type_name),
            py_str(&self.title),
            format!("{:?}", self.status.inner)
        )
    }
}

/// A status change of one element, as a status callback receives it.
#[pyclass(frozen, get_all, name = "UIStatusEvent", module = "liquers")]
pub struct UIStatusEvent {
    /// The element, or `None` for a root query that made no element.
    handle: Option<u64>,
    /// `Processing` when an evaluation starts, `Ready` or `Error` when it ends.
    status: Status,
    /// The error (an `lq.LiquersError`) when `status` is `Error`.
    error: Option<Py<PyAny>>,
}

impl UIStatusEvent {
    fn new(py: Python<'_>, event: &RunnerEvent) -> Self {
        UIStatusEvent {
            handle: event.handle.map(|h| h.0),
            status: Status { inner: event.status },
            error: event
                .error
                .as_ref()
                .map(|error| error_to_pyerr(py, error).value(py).clone().into_any().unbind()),
        }
    }
}

#[pymethods]
impl UIStatusEvent {
    fn __repr__(&self) -> String {
        format!("UIStatusEvent(handle={:?}, status={:?})", self.handle, format!("{:?}", self.status.inner))
    }
}

/// A status subscription, as `UI.on` returns it. `dispose()` (or leaving its `with` block) stops
/// its callback and releases it.
#[pyclass(frozen, name = "UISubscription", module = "liquers")]
pub struct UISubscription {
    model: Weak<UiModel>,
    id: u64,
}

#[pymethods]
impl UISubscription {
    /// Stops the callback. Idempotent; nothing to do after the UI was disposed.
    fn dispose(&self, py: Python<'_>) {
        if let Some(model) = self.model.upgrade() {
            model.unsubscribe(py, self.id);
        }
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, py: Python<'_>, _exc: &Bound<'_, pyo3::types::PyTuple>) -> bool {
        self.dispose(py);
        false
    }
}

/// A UI element value, as Python receives it (`env.evaluate("hello/q/ns-lui/query_console")`).
///
/// A live handle on the element, not data: passing it back to Liquers gives the same element, and
/// a store refuses to write it.
#[pyclass(frozen, name = "UIElement", module = "liquers")]
pub struct UIElement {
    pub(crate) value: Arc<dyn liquers_lib::ui::UIElement>,
}

impl UIElement {
    pub(crate) fn new(value: Arc<dyn liquers_lib::ui::UIElement>) -> Self {
        UIElement { value }
    }
}

#[pymethods]
impl UIElement {
    /// The element's Rust type name, for example `"QueryConsoleElement"`.
    #[getter]
    fn type_name(&self) -> String {
        self.value.type_name().to_string()
    }

    /// The element's title.
    #[getter]
    fn title(&self) -> String {
        self.value.title()
    }

    fn __repr__(&self) -> String {
        format!("<liquers.UIElement {} {:?}>", self.value.type_name(), self.value.title())
    }
}

/// The model list of an environment, closed by `shutdown()`.
#[derive(Default)]
pub(crate) struct UiList {
    closed: bool,
    models: Vec<Weak<UiModel>>,
}

impl UiList {
    pub(crate) fn add(&mut self, model: Weak<UiModel>) -> Result<(), Error> {
        if self.closed {
            return Err(shut_down_error());
        }
        self.models.retain(|weak| weak.strong_count() > 0);
        self.models.push(model);
        Ok(())
    }

    /// Closes the list and returns the live models.
    pub(crate) fn close(&mut self) -> Vec<Arc<UiModel>> {
        self.closed = true;
        std::mem::take(&mut self.models).into_iter().filter_map(|w| w.upgrade()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-6 of `liquers-py-ui-window`: a build without `gui` says how to get a window. Runs in
    /// `cargo test -p liquers-py --no-default-features --lib`, which has no `gui`.
    #[cfg(not(feature = "gui"))]
    #[test]
    fn a_build_without_gui_explains_run() {
        let error = no_window_support();
        assert_eq!(error.error_type, liquers_core::error::ErrorType::General);
        assert!(error.message.contains("no window support"), "{}", error.message);
        assert!(error.message.contains("gui feature"), "{}", error.message);
    }

    #[test]
    fn deadline_after_refuses_a_negative_timeout() {
        assert!(deadline_after(Some(-1.0)).is_err());
        assert!(deadline_after(None).is_ok_and(|d| d.is_none()));
        assert!(deadline_after(Some(0.5)).is_ok_and(|d| d.is_some()));
    }
}
