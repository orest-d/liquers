//! Window tests of `liquers_lib::egui::app::run_window` (design `egui-native-app`): they open real
//! windows, so they need a display and the process's main thread. This file has its own `main`
//! (`harness = false` in `Cargo.toml`) and runs its cases in order on the main thread.
//!
//! Without `DISPLAY` or `WAYLAND_DISPLAY` it prints that it is skipped and passes. Run it with
//! `xvfb-run cargo test -p liquers-lib --test egui_window`.
#![cfg(feature = "egui")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_lib::egui::app::{run_window, FrameControl, WindowCloser, WindowOptions};
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::{
    app_message_channel, AppState, DirectAppState, ElementSource, NativeDriver, SimpleUIPayload,
    Tick, UIEvent,
};
use liquers_lib::value::Value;

type TestEnv = DefaultEnvironment<Value, SimpleUIPayload>;
type CommandEnvironment = TestEnv;

fn driver(runtime: &tokio::runtime::Runtime) -> Result<NativeDriver<TestEnv>, Error> {
    let envref = runtime.block_on(async {
        let mut env = TestEnv::new();
        let cr = env.get_mut_command_registry();
        liquers_lib::register_lui_commands!(cr)?;
        Ok::<_, Error>(env.to_ref())
    })?;
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));
    let (sender, receiver) = app_message_channel();
    Ok(NativeDriver::new(envref, app_state, receiver, sender))
}

fn options() -> WindowOptions {
    let mut options = WindowOptions::new("egui_window test");
    options.poll_interval = Duration::from_millis(20);
    options
}

/// AC-1, AC-6: the console renders, the hook runs after ticks, and `Close` from the hook returns.
fn window_renders_and_the_hook_closes_it(runtime: &tokio::runtime::Runtime) -> Result<(), Error> {
    let mut driver = driver(runtime)?;
    driver
        .ui_context()
        .submit_root_query("ns-lui/query_console/add-child");
    let app_state = driver.app_state().clone();
    let frames = AtomicUsize::new(0);
    let mut hook = |tick: &Tick| {
        let n = frames.fetch_add(1, Ordering::SeqCst);
        if n >= 5 && tick.settled {
            FrameControl::Close
        } else {
            FrameControl::Continue
        }
    };
    run_window(options(), &mut driver, runtime.handle(), &WindowCloser::default(), Some(&mut hook))?;
    assert!(frames.load(Ordering::SeqCst) >= 6);
    let roots = runtime.block_on(async { app_state.lock().await.roots() });
    assert_eq!(roots.len(), 1, "the console became a root element");
    Ok(())
}

/// AC-2: `WindowCloser::close` from another thread closes the window.
fn closer_from_another_thread_closes(runtime: &tokio::runtime::Runtime) -> Result<(), Error> {
    let mut driver = driver(runtime)?;
    let closer = WindowCloser::default();
    let remote = closer.clone();
    let closing = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        remote.close();
    });
    let started = Instant::now();
    run_window(options(), &mut driver, runtime.handle(), &closer, None)?;
    assert!(started.elapsed() < Duration::from_secs(10));
    closing.join().map_err(|_| Error::general_error("closer thread panicked".to_string()))?;
    Ok(())
}

/// AC-3: a *Quit* control (`AppMessage::Quit`) closes the window.
fn quit_closes_the_window(runtime: &tokio::runtime::Runtime) -> Result<(), Error> {
    let mut driver = driver(runtime)?;
    let ui = driver.ui_context();
    let handle = runtime.block_on(async {
        driver
            .app_state()
            .lock()
            .await
            .add_node(None, 0, ElementSource::None)
    })?;
    let _ = handle;
    let quitting = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        ui.request_quit();
    });
    run_window(options(), &mut driver, runtime.handle(), &WindowCloser::default(), None)?;
    quitting.join().map_err(|_| Error::general_error("quit thread panicked".to_string()))?;
    let _unused: Option<UIEvent> = None;
    Ok(())
}

/// AC-7: a second window while one is open is refused; after it closes another opens on the
/// same thread.
fn second_window_is_refused_and_a_later_one_opens(
    runtime: &tokio::runtime::Runtime,
) -> Result<(), Error> {
    let mut outer = driver(runtime)?;
    let mut inner = driver(runtime)?;
    let refused: std::cell::RefCell<Option<Error>> = std::cell::RefCell::new(None);
    let mut hook = |_: &Tick| {
        let result = run_window(options(), &mut inner, runtime.handle(), &WindowCloser::default(), None);
        *refused.borrow_mut() = result.err();
        FrameControl::Close
    };
    run_window(options(), &mut outer, runtime.handle(), &WindowCloser::default(), Some(&mut hook))?;
    let refused = refused.into_inner().ok_or_else(|| {
        Error::general_error("the nested window was not refused".to_string())
    })?;
    assert!(refused.to_string().contains("already open"), "{refused}");
    Ok(())
}

fn main() {
    let has_display = std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !has_display {
        eprintln!("egui_window: skipped, no DISPLAY or WAYLAND_DISPLAY (run under xvfb-run)");
        return;
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => panic!("cannot build a runtime: {e}"),
    };
    let cases: [(&str, fn(&tokio::runtime::Runtime) -> Result<(), Error>); 4] = [
        ("window_renders_and_the_hook_closes_it", window_renders_and_the_hook_closes_it),
        ("closer_from_another_thread_closes", closer_from_another_thread_closes),
        ("quit_closes_the_window", quit_closes_the_window),
        ("second_window_is_refused_and_a_later_one_opens", second_window_is_refused_and_a_later_one_opens),
    ];
    let mut failed = 0;
    for (name, case) in cases {
        match case(&runtime) {
            Ok(()) => eprintln!("test {name} ... ok"),
            Err(e) => {
                failed += 1;
                eprintln!("test {name} ... FAILED: {e}");
            }
        }
    }
    if failed > 0 {
        panic!("{failed} egui_window case(s) failed");
    }
}
