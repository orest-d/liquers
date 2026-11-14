//! A native UI driver: one owner for an `AppRunner`, its `AppState` and its per-tick bookkeeping,
//! ticked by an egui window or, with no window, by a task on a Tokio runtime.
//!
//! A host builds a [`NativeDriver`] over an environment and an `AppState`, changes the model
//! through [`NativeDriver::ui_context`] (or by editing the `AppState` directly), and lets something
//! tick it. [`spawn_headless`] ticks it in the background; `crate::egui::app::run_window` (feature
//! `egui`) ticks it from a window's frames. [`HeadlessHandle::stop`] returns the driver, so one
//! driver can move from the background to a window and back.
//!
//! # Main components
//!
//! - [`NativeDriver`]: owns the runner; [`NativeDriver::tick`] runs it once and returns a [`Tick`].
//! - [`Tick`]: what one tick did: invalidation, status events, a quit request, and whether the
//!   model has settled.
//! - [`spawn_headless`] and [`HeadlessHandle`]: tick a driver on a runtime until stopped.
//!
//! # Where it is used
//!
//! - `crate::egui::app::run_window`, the reusable egui window (feature `egui`).
//! - `liquers-py`'s `lq.UI`, which ticks a model in the background and lends it to a window.
//! - `liquers-lib/examples/ui_query_console_app.rs` and `ui_spec_demo.rs`, through the window.
//!
//! The other egui examples still drive `AppRunner` by hand (`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`).
//!
//! # Related concepts
//!
//! - [`AppRunner`]: the message and evaluation loop this module drives; it does one pass per
//!   `run` and leaves scheduling, repainting and quitting to its host.
//! - `crate::ui::web` `mount_web` / `run_headless`: the browser's driver, on wasm only, which
//!   schedules ticks with the browser's timers instead of a Tokio runtime.
//!
//! # Settled ticks
//!
//! A host that changes the model from another thread increments the
//! [change counter](NativeDriver::change_counter) *after* each change. A tick reads the counter
//! before it looks at the model, so a tick with `settled == true` whose `changes` is at least the
//! value a host left has seen that host's changes through: no message is waiting, no node is
//! pending and no evaluation is in flight.
//!
//! # Examples
//!
//! ```
//! # use std::sync::Arc;
//! # use liquers_core::context::Environment;
//! # use liquers_lib::environment::DefaultEnvironment;
//! # use liquers_lib::ui::{app_message_channel, AppState, DirectAppState, NativeDriver, SimpleUIPayload};
//! # use liquers_lib::value::Value;
//! # let rt = tokio::runtime::Runtime::new().unwrap();
//! # rt.block_on(async {
//! let envref = DefaultEnvironment::<Value, SimpleUIPayload>::new().to_ref();
//! let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
//!     Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
//! let (sender, receiver) = app_message_channel();
//! let mut driver = NativeDriver::new(envref, app_state, receiver, sender);
//! driver.ui_context().request_quit();
//! let tick = driver.tick().await;
//! assert!(tick.quit && tick.settled);
//! # });
//! ```
//!
//! More: the tests at the end of this file (a headless model evaluating a query, a stop that
//! returns the driver, environment replacement), and `crate::egui::app::run_window`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use crate::value::Value;

use super::app_state::{AppState, Invalidation};
use super::message::{AppMessageReceiver, AppMessageSender};
use super::payload::{SimpleUIPayload, UIPayload};
use super::runner::{AppRunner, RunnerEvent};
use super::ui_context::UIContext;

/// A closure that names the environment later evaluations should use, or `None` to keep the
/// current one. Called at the start of every tick.
pub type CurrentEnv<E> = Box<dyn FnMut() -> Option<EnvRef<E>> + Send>;

/// One owner for an [`AppRunner`], its `AppState` and its message sender.
///
/// Build it with [`NativeDriver::new`], tick it with [`NativeDriver::tick`], or hand it to
/// [`spawn_headless`] or `crate::egui::app::run_window`. See the [module example](self#examples).
pub struct NativeDriver<E: Environment> {
    runner: AppRunner<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    current: EnvRef<E>,
    current_env: Option<CurrentEnv<E>>,
    changes: Arc<AtomicU64>,
}

/// What one [`NativeDriver::tick`] did.
#[derive(Debug)]
pub struct Tick {
    /// The model's accumulated invalidation, taken by this tick; `None` when nothing changed or
    /// the `AppState` lock was held elsewhere (it is then taken by a later tick).
    pub invalidation: Invalidation,
    /// Status events recorded during the tick, in order.
    pub events: Vec<RunnerEvent>,
    /// An `AppMessage::Quit` arrived (a *Quit* control fired).
    pub quit: bool,
    /// Evaluations are in flight after the tick.
    pub evaluating: bool,
    /// Assets are monitored for updates after the tick.
    pub monitoring: bool,
    /// No message is waiting, no node is pending and no evaluation is in flight after the tick.
    pub settled: bool,
    /// The change counter, read before `settled` was computed (see the module's
    /// [settled ticks](self#settled-ticks)).
    pub changes: u64,
}

impl<E> NativeDriver<E>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    /// A driver over `envref` and `app_state`, reading messages from `receiver`; `sender` is the
    /// same channel's sending half (arguments in [`AppRunner::new`]'s order, `app_state` added).
    pub fn new(
        envref: EnvRef<E>,
        app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
        receiver: AppMessageReceiver,
        sender: AppMessageSender,
    ) -> Self {
        Self {
            runner: AppRunner::new(envref.clone(), receiver, sender.clone()),
            app_state,
            sender,
            current: envref,
            current_env: None,
            changes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Follows a host whose environment can be rebuilt: `current_env` is called at the start of
    /// every tick, and an environment other than the current one is used for every evaluation
    /// started from then on.
    pub fn with_current_env(mut self, current_env: CurrentEnv<E>) -> Self {
        self.current_env = Some(current_env);
        self
    }

    /// The shared `AppState`.
    pub fn app_state(&self) -> &Arc<tokio::sync::Mutex<dyn AppState>> {
        &self.app_state
    }

    /// A context over this driver's `AppState` and channel, with no current handle: what
    /// `render_element` and a host submitting queries need.
    pub fn ui_context(&self) -> UIContext {
        UIContext::new(self.app_state.clone(), self.sender.clone())
    }

    /// The counter a host increments after each change it makes to the model from outside a
    /// tick (see the module's [settled ticks](self#settled-ticks)).
    pub fn change_counter(&self) -> Arc<AtomicU64> {
        self.changes.clone()
    }

    /// The environment evaluations currently start in.
    pub fn envref(&self) -> &EnvRef<E> {
        &self.current
    }

    /// Runs the runner once and reports what happened.
    ///
    /// Takes the `AppState` lock briefly, never across the runner's pass. A submitted query is
    /// awaited inline by the runner, so a slow action delays the tick
    /// (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`).
    pub async fn tick(&mut self) -> Tick {
        self.follow_environment();
        // `run` reports no error of its own: evaluation errors become error elements and events.
        let _ = self.runner.run(&self.app_state).await;
        let changes = self.changes.load(Ordering::SeqCst);
        let (pending, invalidation) = {
            let mut state = self.app_state.lock().await;
            (!state.pending_nodes().is_empty(), state.take_invalidation())
        };
        let evaluating = self.runner.has_evaluating();
        Tick {
            invalidation,
            events: self.runner.take_events(),
            quit: self.runner.take_quit_request(),
            evaluating,
            monitoring: self.runner.has_monitoring(),
            settled: !self.runner.has_pending_messages() && !pending && !evaluating,
            changes,
        }
    }

    fn follow_environment(&mut self) {
        let Some(current_env) = self.current_env.as_mut() else {
            return;
        };
        if let Some(envref) = current_env() {
            if !Arc::ptr_eq(&envref.0, &self.current.0) {
                self.runner.replace_envref(envref.clone());
                self.current = envref;
            }
        }
    }
}

/// A driver ticking on a Tokio runtime, from [`spawn_headless`].
///
/// [`HeadlessHandle::stop`] ends the task and returns the driver. Dropping the handle only signals
/// the task to stop: the task ends after its current tick and drops the driver.
pub struct HeadlessHandle<E: Environment> {
    stop: watch::Sender<bool>,
    wake: Arc<Notify>,
    task: Option<JoinHandle<NativeDriver<E>>>,
}

/// How long the background task waits between ticks when nothing wakes it.
fn idle_delay(tick: &Tick) -> Duration {
    if tick.evaluating {
        Duration::from_millis(16)
    } else if tick.monitoring {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(250)
    }
}

/// Ticks `driver` on `runtime` until stopped.
///
/// After each tick `on_tick` receives the [`Tick`]; then the task waits for a
/// [wake](HeadlessHandle::waker), a stop, or 16 ms while evaluating, 100 ms while monitoring and
/// 250 ms otherwise. `on_tick` runs on a runtime thread: it must not block.
pub fn spawn_headless<E>(
    driver: NativeDriver<E>,
    runtime: &tokio::runtime::Handle,
    mut on_tick: Box<dyn FnMut(Tick) + Send>,
) -> HeadlessHandle<E>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    let (stop, mut stopped) = watch::channel(false);
    let wake = Arc::new(Notify::new());
    let woken = wake.clone();
    let task = runtime.spawn(async move {
        let mut driver = driver;
        while !*stopped.borrow() {
            let tick = driver.tick().await;
            let delay = idle_delay(&tick);
            on_tick(tick);
            tokio::select! {
                changed = stopped.changed() => {
                    if changed.is_err() {
                        break; // the handle is gone
                    }
                }
                _ = woken.notified() => {}
                _ = tokio::time::sleep(delay) => {}
            }
        }
        driver
    });
    HeadlessHandle {
        stop,
        wake,
        task: Some(task),
    }
}

impl<E: Environment> HeadlessHandle<E> {
    /// Wakes the task for an early tick; a wake that arrives during a tick is kept for the next
    /// wait (`Notify::notify_one`).
    pub fn waker(&self) -> Arc<Notify> {
        self.wake.clone()
    }

    /// Stops the task after its current tick and returns the driver.
    ///
    /// # Errors
    ///
    /// `Error::unexpected_error` when the task panicked or was cancelled; the driver is lost.
    pub async fn stop(mut self) -> Result<NativeDriver<E>, Error> {
        self.stop.send_replace(true);
        match self.task.take() {
            Some(task) => task.await.map_err(|e| {
                Error::unexpected_error(format!("the UI driver task ended abnormally: {e}"))
            }),
            None => Err(Error::unexpected_error(
                "the UI driver task was already stopped".to_string(),
            )),
        }
    }
}

impl<E: Environment> Drop for HeadlessHandle<E> {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use liquers_core::metadata::Status;
    use liquers_core::state::State;
    use liquers_macro::register_command;

    use super::*;
    use crate::environment::{CommandRegistryAccess, DefaultEnvironment};
    use crate::ui::app_state::DirectAppState;
    use crate::ui::element::ElementSource;
    use crate::ui::message::app_message_channel;

    type TestEnv = DefaultEnvironment<Value, SimpleUIPayload>;
    type CommandEnvironment = TestEnv;
    use liquers_core::context::Context;

    fn hello(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("hello"))
    }

    fn only_in_second(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("second"))
    }

    fn try_env(second: bool) -> Result<EnvRef<TestEnv>, Error> {
        let mut env = TestEnv::new();
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn hello(state) -> result)?;
        if second {
            register_command!(cr, fn only_in_second(state) -> result)?;
        }
        crate::register_lui_commands!(cr)?;
        Ok(env.to_ref())
    }

    fn env(second: bool) -> EnvRef<TestEnv> {
        try_env(second).unwrap()
    }

    fn new_driver(envref: EnvRef<TestEnv>) -> NativeDriver<TestEnv> {
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let (sender, receiver) = app_message_channel();
        NativeDriver::new(envref, app_state, receiver, sender)
    }

    async fn add_root(driver: &NativeDriver<TestEnv>, query: &str) -> crate::ui::UIHandle {
        let mut state = driver.app_state().lock().await;
        state
            .add_node(None, 0, ElementSource::Query(query.to_string()))
            .unwrap()
    }

    /// Ticks until settled, at most `max` times.
    async fn tick_until_settled(driver: &mut NativeDriver<TestEnv>, max: usize) -> Vec<Tick> {
        let mut ticks = Vec::new();
        for _ in 0..max {
            let tick = driver.tick().await;
            let settled = tick.settled;
            ticks.push(tick);
            if settled {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        ticks
    }

    #[tokio::test]
    async fn quit_message_is_reported_once() {
        let mut driver = new_driver(env(false));
        driver.ui_context().request_quit();
        assert!(driver.tick().await.quit);
        assert!(!driver.tick().await.quit);
    }

    #[tokio::test]
    async fn submitted_quit_query_is_reported() {
        // A control whose action is `quit` sends AppMessage::Quit (dispatch_action).
        let mut driver = new_driver(env(false));
        crate::ui::dispatch_action(&crate::ui::UiAction::Quit, &driver.ui_context(), None);
        assert!(driver.tick().await.quit);
    }

    #[tokio::test]
    async fn tick_evaluates_a_root_and_settles() {
        let mut driver = new_driver(env(false));
        let handle = add_root(&driver, "hello").await;
        driver.change_counter().fetch_add(1, Ordering::SeqCst);
        let ticks = tick_until_settled(&mut driver, 200).await;
        let last = ticks.last().unwrap();
        assert!(last.settled);
        assert_eq!(last.changes, 1);
        let statuses: Vec<Status> = ticks
            .iter()
            .flat_map(|t| t.events.iter())
            .filter(|e| e.handle == Some(handle))
            .map(|e| e.status)
            .collect();
        assert_eq!(statuses, vec![Status::Processing, Status::Ready]);
        let state = driver.app_state().lock().await;
        assert!(state.get_element(handle).unwrap().is_some());
    }

    #[tokio::test]
    async fn tick_is_not_settled_while_a_message_waits() {
        let mut driver = new_driver(env(false));
        let first = driver.tick().await;
        assert!(first.settled);
        driver.ui_context().submit_root_query("ns-lui/query_console/add-child");
        // The message is in the channel: a tick computing `settled` now must not report it.
        assert!(driver.runner.has_pending_messages());
        let ticks = tick_until_settled(&mut driver, 200).await;
        assert!(ticks.last().unwrap().settled);
        let state = driver.app_state().lock().await;
        assert_eq!(state.roots().len(), 1);
    }

    #[tokio::test]
    async fn driver_follows_a_replaced_environment() {
        let second = env(true);
        let next: Arc<Mutex<Option<EnvRef<TestEnv>>>> = Arc::new(Mutex::new(None));
        let source = next.clone();
        let mut driver = new_driver(env(false)).with_current_env(Box::new(move || {
            source.lock().ok().and_then(|n| n.clone())
        }));
        *next.lock().unwrap() = Some(second.clone());
        let handle = add_root(&driver, "only_in_second").await;
        let ticks = tick_until_settled(&mut driver, 200).await;
        assert!(Arc::ptr_eq(&driver.envref().0, &second.0));
        let last = ticks
            .iter()
            .flat_map(|t| t.events.iter())
            .filter(|e| e.handle == Some(handle))
            .last()
            .map(|e| e.status);
        assert_eq!(last, Some(Status::Ready));
    }

    #[test]
    fn headless_task_evaluates_and_stop_returns_the_driver() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _runtime = rt.enter(); // building an environment spawns its asset manager's tasks
        let driver = new_driver(env(false));
        let app_state = driver.app_state().clone();
        let counter = driver.change_counter();
        let events: Arc<Mutex<Vec<(Option<crate::ui::UIHandle>, Status)>>> = Arc::default();
        let settled_at: Arc<Mutex<Option<u64>>> = Arc::default();
        let (seen, settled) = (events.clone(), settled_at.clone());
        let handle = spawn_headless(
            driver,
            rt.handle(),
            Box::new(move |tick: Tick| {
                seen.lock().unwrap().extend(tick.events.iter().map(|e| (e.handle, e.status)));
                *settled.lock().unwrap() = tick.settled.then_some(tick.changes);
            }),
        );
        let root = rt.block_on(async {
            let h = app_state
                .lock()
                .await
                .add_node(None, 0, ElementSource::Query("hello".to_string()))
                .unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
            h
        });
        handle.waker().notify_one();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while *settled_at.lock().unwrap() != Some(1) {
            assert!(std::time::Instant::now() < deadline, "the model never settled");
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut driver = rt.block_on(handle.stop()).unwrap();
        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[(Some(root), Status::Processing), (Some(root), Status::Ready)]
        );
        // The returned driver still works.
        driver.ui_context().request_quit();
        assert!(rt.block_on(driver.tick()).quit);
    }

    #[test]
    fn dropping_the_handle_ends_the_task() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _runtime = rt.enter();
        let envref = env(false);
        let weak = Arc::downgrade(&envref.0);
        let handle = spawn_headless(new_driver(envref), rt.handle(), Box::new(|_| {}));
        drop(handle);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while weak.strong_count() > 0 {
            assert!(std::time::Instant::now() < deadline, "the task kept the driver");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
