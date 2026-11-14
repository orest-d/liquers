# Phase 3: Examples & Use-cases — egui-native-app

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | the query console in a window | `NativeDriver` + `run_window`; *Quit* closes | AC-1, AC-3, AC-8 |
| 2 | Example | a model with no window | `spawn_headless`, `on_tick`, `stop` | AC-4, AC-9 |
| 3 | Test | `ui::native::tests::quit_message_is_reported_once` | a quit request reaches one tick | AC-3 |
| 4 | Test | `ui::native::tests::submitted_quit_query_is_reported` | the `quit` action reaches the tick | AC-3 |
| 5 | Test | `ui::native::tests::tick_evaluates_a_root_and_settles` | statuses in order, settled with the count | AC-4, AC-9 |
| 6 | Test | `ui::native::tests::tick_is_not_settled_while_a_message_waits` | a queued message keeps a tick unsettled | AC-9 |
| 7 | Test | `ui::native::tests::driver_follows_a_replaced_environment` | `with_current_env` | AC-5 |
| 8 | Test | `ui::native::tests::headless_task_evaluates_and_stop_returns_the_driver` | background ticks, events, `stop` | AC-4, AC-9 |
| 9 | Test | `ui::native::tests::dropping_the_handle_ends_the_task` | the task releases the driver | AC-4 |
| 10 | Test | `egui::app::tests::run_window_refuses_inside_a_tokio_runtime` | error, no panic | AC-7 |
| 11 | Test | `egui::app::tests::run_window_refuses_off_the_main_thread` | error, no panic | AC-7 |
| 12 | Test | `egui::app::tests::closer_without_a_window_does_nothing` | a closer is reusable | AC-2 |
| 13 | Test | `egui::app::tests::window_options_defaults` | 900×700, 250 ms | AC-1 |
| 14 | Window test | `tests/egui_window.rs` `window_renders_and_the_hook_closes_it` | a real window, the hook, `Close` | AC-1, AC-6 |
| 15 | Window test | `closer_from_another_thread_closes` | `WindowCloser::close` from a thread | AC-2 |
| 16 | Window test | `quit_closes_the_window` | `AppMessage::Quit` closes it | AC-3 |
| 17 | Window test | `second_window_is_refused_and_a_later_one_opens` | a nested call is refused; the next opens | AC-7 |
| 18 | Build | `cargo build --example ui_query_console_app --example ui_spec_demo` | both examples on the window | AC-8 |

## Example 1: the query console in a window

`ui_query_console_app.rs` keeps its environment and YAML spec; its `eframe::App` and `main` become:

```rust
let mut driver = runtime.block_on(async {
    /* environment, store, commands as before */
    let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(Mutex::new(DirectAppState::new()));
    let (sender, receiver) = app_message_channel();
    Ok::<_, Error>(NativeDriver::new(envref, app_state, receiver, sender))
})?;
driver.ui_context().submit_root_query("main_spec/ns-lui/ui_spec/add-child");
run_window(WindowOptions::new("Query Console App"), &mut driver, runtime.handle(), &WindowCloser::default(), None)
```

The first frame's tick runs the root query, which adds the `ui_spec` element; File › Quit sends
`AppMessage::Quit`, the next tick reports `quit`, and `run_window` returns `Ok(())`. Both queries
validate (`main_spec` declared with `--command`).

## Example 2: a model with no window

```rust
let counter = driver.change_counter();
let handle = spawn_headless(driver, runtime.handle(), Box::new(move |tick| {
    for event in &tick.events { eprintln!("{:?} {:?}", event.handle, event.status); }
}));
app_state.lock().await.add_node(None, 0, ElementSource::Query("hello".into()))?;
counter.fetch_add(1, Ordering::SeqCst);   // after the change
handle.waker().notify_one();
// … on_tick sees Processing then Ready, then a tick with settled == true and changes == 1
let driver = handle.stop().await?;         // back for a window or another task
```

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| a quit request with no window | the tick reports it once; a headless host decides | 3 |
| a message sent just before the tick checks `settled` | not settled until a later tick drains it | 6 |
| the environment is rebuilt between ticks | later evaluations use the new one | 7 |
| the handle is dropped instead of stopped | the task ends and drops the driver | 9 |
| `run_window` inside a runtime, off the main thread, twice at once | `Error`, no panic | 10, 11, 17 |
| `close()` before or after a window | nothing carries over to the next window | 12 |
| winit cannot start (no display, a missing system library) | `Error`; later calls report the same cause | AC-7, conceptual: a test cannot uninstall a library from its own process; shown by hand (Learning Log) |
| the window is hidden (minimised) | the hook still runs every `poll_interval`, since eframe calls `App::logic` while hidden | AC-6, conceptual: Xvfb cannot minimise a window; eframe 0.36's documented `logic` contract |
| a slow submitted query | the window waits for it (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`) | documented only |

## Test Plan

- Unit: `liquers-lib/src/ui/native.rs` tests 3 to 9; `liquers-lib/src/egui/app.rs` tests 10 to 13.
  `cargo test -p liquers-lib --lib -- ui::native egui::app`.
- Window: `liquers-lib/tests/egui_window.rs`, tests 14 to 17. Its own `main` (`harness = false`),
  since winit needs the main thread and libtest runs tests on worker threads. It skips itself
  without `DISPLAY` or `WAYLAND_DISPLAY`. `xvfb-run -a cargo test -p liquers-lib --test egui_window`.
- Build: 18, and `scripts/check-build-matrix.sh` (a new `cfg`, an optional dependency).

## Learning Log

- Prototype, 2026-11-11: all four window tests pass under `xvfb-run` on Linux with wgpu, after
  installing `libxkbcommon-x11-0` (winit's X11 keyboard) and `mesa-vulkan-drivers` (lavapipe, a
  software Vulkan). Without the first, winit panics with "Library libxkbcommon-x11.so could not be
  loaded"; `run_window` catches it, returns the message and refuses later calls, as designed. A CI
  job needs `xvfb libxkbcommon-x11-0 mesa-vulkan-drivers`.
- Building an environment spawns the asset manager's tasks, so a synchronous test enters its
  runtime first (`let _guard = runtime.enter();`).
- `NativeDriver::envref` and the `CurrentEnv` alias were added beyond Phase 2's interfaces: a host
  (liquers-py) needs the driver's environment, and the boxed closure type is long to repeat.
- The window function is `liquers_lib::egui::app::run_window`, not `egui::run_window` as Phase 1
  first wrote: it lives in the `app` module.
- `register_lui_commands!` expands to `?` and expects `CommandEnvironment` and `Context` in scope.
- The root query of Phase 1's problem example needs `add-child` to become an element:
  `ns-lui/query_console/add-child`.
