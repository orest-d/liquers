//! A reusable native egui window over a [`NativeDriver`]: one call opens it, ticks the model from
//! its frames, renders the root elements, and returns when it closes.
//!
//! [`run_window`] blocks the calling thread, which must be the process's main thread, until the
//! window closes: by the user, by a *Quit* control (`AppMessage::Quit`), by
//! [`WindowCloser::close`] from any thread, or by the frame hook returning
//! [`FrameControl::Close`]. The driver is borrowed, so the host keeps it afterwards and can tick it
//! headless again ([`crate::ui::spawn_headless`]).
//!
//! # Main components
//!
//! - [`run_window`]: open a window over a driver and block until it closes.
//! - [`WindowOptions`]: title, size and the idle poll interval.
//! - [`WindowCloser`]: closes the window from any thread.
//! - [`FrameControl`]: what the per-frame hook returns.
//!
//! # Where it is used
//!
//! - `liquers-lib/examples/ui_query_console_app.rs`: the query console in a window.
//! - `liquers-lib/examples/ui_spec_demo.rs`: a `ui_spec` dashboard in a window.
//! - `liquers-py`'s `UI.run()`, which lends its background model to a window.
//!
//! The other egui examples still write their own `eframe::App`
//! (`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`).
//!
//! # Related concepts
//!
//! - [`crate::ui::NativeDriver`]: the model and its tick; this module only adds a window and
//!   frames around it, and [`crate::ui::spawn_headless`] ticks the same driver with no window.
//! - `crate::ui::web` `mount_web`: the browser counterpart, which renders the same elements to the
//!   DOM instead of egui.
//!
//! # Errors
//!
//! [`run_window`] returns an `Error` instead of panicking when it is called inside a Tokio runtime,
//! while another window is open, off the main thread (checked on Linux and macOS; on Windows
//! winit's own panic is caught), or when eframe cannot create the window (no display, no GPU).
//! winit allows one event loop per process: once a call fails in winit's event-loop creation,
//! every later call returns that error.
//!
//! # Examples
//!
//! Opening a window needs a display, so this example is compiled but not run:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use liquers_core::context::Environment;
//! # use liquers_lib::environment::DefaultEnvironment;
//! # use liquers_lib::ui::{app_message_channel, AppState, DirectAppState, NativeDriver, SimpleUIPayload};
//! # use liquers_lib::value::Value;
//! use liquers_lib::egui::app::{run_window, WindowCloser, WindowOptions};
//! # fn main() -> Result<(), liquers_core::error::Error> {
//! let runtime = tokio::runtime::Runtime::new().map_err(|e| liquers_core::error::Error::general_error(e.to_string()))?;
//! let envref = runtime.block_on(async { DefaultEnvironment::<Value, SimpleUIPayload>::new().to_ref() });
//! let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
//! let (sender, receiver) = app_message_channel();
//! let mut driver = NativeDriver::new(envref, app_state, receiver, sender);
//! driver.ui_context().submit_root_query("ns-lui/query_console/add-child");
//! run_window(WindowOptions::new("Reports"), &mut driver, runtime.handle(), &WindowCloser::default(), None)
//! # }
//! ```
//!
//! Runnable programs: `cargo run --example ui_query_console_app -p liquers-lib` and
//! `cargo run --example ui_spec_demo -p liquers-lib`. The window tests, which need a display and the main thread,
//! are `liquers-lib/tests/egui_window.rs` (`xvfb-run cargo test -p liquers-lib --test egui_window`).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use liquers_core::context::Environment;
use liquers_core::error::Error;

use crate::ui::{
    render_element, try_sync_lock, Invalidation, NativeDriver, SimpleUIPayload, Tick, UIContext,
    UIPayload,
};
use crate::value::Value;

/// How a window opened by [`run_window`] looks and how often it ticks while idle.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    /// The window title (also eframe's application name).
    pub title: String,
    /// The initial inner size in logical points.
    pub inner_size: [f32; 2],
    /// The longest wait between two ticks while nothing changes and nothing evaluates.
    pub poll_interval: Duration,
}

impl WindowOptions {
    /// Options with `title`, 900×700 points and a 250 ms poll interval.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            inner_size: [900.0, 700.0],
            poll_interval: Duration::from_millis(250),
        }
    }
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self::new("Liquers")
    }
}

/// What the per-frame hook of [`run_window`] asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameControl {
    /// Keep the window open.
    Continue,
    /// Close the window; `run_window` then returns.
    Close,
}

#[derive(Default)]
struct CloserState {
    /// A `run_window` call is using this closer.
    active: bool,
    requested: bool,
    context: Option<egui::Context>,
}

/// Closes the window of a [`run_window`] call from any thread.
///
/// Reusable: [`WindowCloser::close`] closes the window of the call currently using it (one being
/// opened included) and does nothing when there is none.
#[derive(Clone, Default)]
pub struct WindowCloser(Arc<Mutex<CloserState>>);

impl WindowCloser {
    fn state(&self) -> MutexGuard<'_, CloserState> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Closes the window within one frame; does nothing without a window.
    pub fn close(&self) {
        let mut state = self.state();
        if !state.active {
            return;
        }
        state.requested = true;
        if let Some(ctx) = &state.context {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        }
    }

    fn begin(&self) {
        *self.state() = CloserState {
            active: true,
            ..CloserState::default()
        };
    }

    fn attach(&self, context: egui::Context) {
        self.state().context = Some(context);
    }

    fn requested(&self) -> bool {
        self.state().requested
    }

    fn end(&self) {
        *self.state() = CloserState::default();
    }
}

/// Set while a window is open in this process.
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
/// Why no window can open any more in this process (winit's event loop could not be created).
static UNAVAILABLE: Mutex<Option<String>> = Mutex::new(None);

fn unavailable() -> MutexGuard<'static, Option<String>> {
    match UNAVAILABLE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Clears [`WINDOW_OPEN`] when `run_window` returns, by any path.
struct OpenWindow;

impl Drop for OpenWindow {
    fn drop(&mut self) {
        WINDOW_OPEN.store(false, Ordering::SeqCst);
    }
}

#[cfg(target_os = "linux")]
fn on_main_thread() -> bool {
    // SAFETY: both calls only read the calling thread's and process's ids.
    unsafe { libc::gettid() == libc::getpid() }
}

#[cfg(target_os = "macos")]
fn on_main_thread() -> bool {
    // SAFETY: reads whether the calling thread is the process's main thread.
    unsafe { libc::pthread_main_np() == 1 }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn on_main_thread() -> bool {
    true // winit's own check panics, and `run_window` catches it.
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_string()
    }
}

/// Opens a window over `driver` and blocks until it closes.
///
/// Every frame ticks the driver on `runtime` (`Handle::block_on`, so `runtime` should be a
/// multi-thread runtime), runs `on_frame` with the tick and no `AppState` lock held, and renders
/// the model's roots with `render_element`. Ticks continue at least every
/// `options.poll_interval` while the window is hidden. The window closes on a quit request, on
/// `closer.close()`, or when `on_frame` returns [`FrameControl::Close`]. See the
/// [module example](self#examples).
///
/// # Errors
///
/// `Error::general_error` naming the cause: inside a Tokio runtime, another window open, off the
/// main thread, winit unavailable in this process, or eframe failing to create the window.
pub fn run_window<E>(
    options: WindowOptions,
    driver: &mut NativeDriver<E>,
    runtime: &tokio::runtime::Handle,
    closer: &WindowCloser,
    on_frame: Option<&mut dyn FnMut(&Tick) -> FrameControl>,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    if let Some(reason) = unavailable().clone() {
        return Err(Error::general_error(format!(
            "no window can be opened in this process: {reason}"
        )));
    }
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::general_error(
            "run_window must not be called inside a Tokio runtime".to_string(),
        ));
    }
    if !on_main_thread() {
        return Err(Error::general_error(
            "run_window must be called on the main thread".to_string(),
        ));
    }
    if WINDOW_OPEN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(Error::general_error(
            "a Liquers window is already open".to_string(),
        ));
    }
    let _open = OpenWindow;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(options.inner_size)
            .with_title(options.title.clone()),
        ..Default::default()
    };
    let ui_context = driver.ui_context();
    let poll_interval = options.poll_interval;
    closer.begin();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        eframe::run_native(
            &options.title,
            native_options,
            Box::new(|cc| {
                closer.attach(cc.egui_ctx.clone());
                Ok(Box::new(WindowApp {
                    driver,
                    runtime,
                    closer,
                    on_frame,
                    poll_interval,
                    ui_context,
                }))
            }),
        )
    }));
    closer.end();

    match outcome {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            let message = error.to_string();
            if format!("{error:?}").contains("RecreationAttempt") {
                *unavailable() = Some(message.clone());
            }
            Err(Error::general_error(format!(
                "cannot open a window: {message}"
            )))
        }
        Err(payload) => {
            // winit marks its event loop as created before such a panic, so it cannot be retried.
            let message = panic_message(payload.as_ref());
            *unavailable() = Some(message.clone());
            Err(Error::general_error(format!(
                "cannot open a window: {message}"
            )))
        }
    }
}

/// The `eframe::App` of [`run_window`]: ticks in `logic`, which eframe also calls while the window
/// is hidden, and only renders in `ui`.
struct WindowApp<'a, 'f, E: Environment> {
    driver: &'a mut NativeDriver<E>,
    runtime: &'a tokio::runtime::Handle,
    closer: &'a WindowCloser,
    on_frame: Option<&'a mut (dyn FnMut(&Tick) -> FrameControl + 'f)>,
    poll_interval: Duration,
    ui_context: UIContext,
}

impl<E> eframe::App for WindowApp<'_, '_, E>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
{
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let tick = self.runtime.block_on(self.driver.tick());
        let mut close = tick.quit || self.closer.requested();
        if let Some(hook) = self.on_frame.as_mut() {
            match hook(&tick) {
                FrameControl::Close => close = true,
                FrameControl::Continue => {}
            }
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let changed = !matches!(tick.invalidation, Invalidation::None) || !tick.events.is_empty();
        if close || changed || tick.evaluating {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(self.poll_interval);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let roots = match try_sync_lock(self.driver.app_state()) {
            Ok(state) => state.roots(),
            Err(_) => {
                // Held by a command or a host thread: show a spinner and try again next frame.
                ui.spinner();
                ui.ctx().request_repaint();
                return;
            }
        };
        if roots.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
        } else {
            for handle in roots {
                render_element(ui, handle, &self.ui_context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::DefaultEnvironment;
    use crate::ui::{app_message_channel, AppState, DirectAppState};

    type TestEnv = DefaultEnvironment<Value, SimpleUIPayload>;
    type CommandEnvironment = TestEnv;
    use liquers_core::context::Context;

    fn try_driver(runtime: &tokio::runtime::Runtime) -> Result<NativeDriver<TestEnv>, Error> {
        let mut env = TestEnv::new();
        let envref = runtime.block_on(async {
            let cr = crate::environment::CommandRegistryAccess::get_mut_command_registry(&mut env);
            crate::register_lui_commands!(cr)?;
            Ok::<_, Error>(env.to_ref())
        })?;
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
        let (sender, receiver) = app_message_channel();
        Ok(NativeDriver::new(envref, app_state, receiver, sender))
    }

    #[test]
    fn window_options_defaults() {
        let options = WindowOptions::new("Reports");
        assert_eq!(options.title, "Reports");
        assert_eq!(options.inner_size, [900.0, 700.0]);
        assert_eq!(options.poll_interval, Duration::from_millis(250));
        assert_eq!(WindowOptions::default().title, "Liquers");
    }

    #[test]
    fn closer_without_a_window_does_nothing() {
        let closer = WindowCloser::default();
        closer.close();
        assert!(!closer.requested());
        closer.begin();
        assert!(!closer.requested(), "a close before the window opened must not carry over");
        closer.close();
        assert!(closer.requested());
        closer.end();
        assert!(!closer.requested());
    }

    #[test]
    fn run_window_refuses_inside_a_tokio_runtime() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut driver = try_driver(&runtime).unwrap();
        let _inside = runtime.enter();
        let error = run_window(
            WindowOptions::default(),
            &mut driver,
            runtime.handle(),
            &WindowCloser::default(),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Tokio runtime"), "{error}");
    }

    #[test]
    fn run_window_refuses_off_the_main_thread() {
        // Test threads are never the process's main thread.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let mut driver = try_driver(&runtime).unwrap();
        let error = run_window(
            WindowOptions::default(),
            &mut driver,
            runtime.handle(),
            &WindowCloser::default(),
            None,
        )
        .unwrap_err();
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            assert!(error.to_string().contains("main thread"), "{error}");
        }
    }
}
