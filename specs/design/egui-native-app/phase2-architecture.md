# Phase 2: Solution & Architecture — egui-native-app

## Overview

A driver that owns the runner and the per-tick bookkeeping, used two ways: ticked by the window on
its own thread, as the examples do now, or by a task on a Tokio runtime with no window. The window
renders with `render_element`, as the examples do. A host hands one driver between the two:
`HeadlessHandle::stop` returns it, so liquers-py can run its model in the background, give it to a
window, and take it back.

Rejected: a window that only renders while a background task drives the model (the window's
`try_lock` would lose frames to the driver, and the examples' same-thread tick would go);
`eframe::App` as the public API (every host would still write the loop); a `Send` frame hook
(Python delivers its callbacks on the window's thread).

## Known-Issue Preflight

`EGUI-HAS-NO-REUSABLE-NATIVE-APP` (raised to P1, since two designs wait on it)
is the source and closes here. `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` (P2) is inherited, not
blocking: `AppRunner` awaits a `SubmitQuery` or `ApplyToInput` inline (`runner.rs`
`process_messages`), so a slow action delays the tick, and with it the window, as in the examples
today; documented. `ui-asset-snapshot-boundary` (in review) changes what widgets fetch, not the runner.

## Interfaces

`liquers-lib/src/ui/runner.rs`, additive:

```rust
impl<E> AppRunner<E> {
    pub fn take_quit_request(&mut self) -> bool;   // set by AppMessage::Quit
    pub fn has_pending_messages(&self) -> bool;    // !message_rx.is_empty()
}
```

`liquers-lib/src/ui/native.rs`, new, `#[cfg(not(target_arch = "wasm32"))]`, no egui:

```rust
pub struct NativeDriver<E: Environment> {          // owned; Send (the runner's future is Send)
    runner: AppRunner<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    current: EnvRef<E>,
    current_env: Option<Box<dyn FnMut() -> Option<EnvRef<E>> + Send>>,
    changes: Arc<AtomicU64>,
}
#[derive(Debug)]
pub struct Tick {
    pub invalidation: Invalidation, pub events: Vec<RunnerEvent>, pub quit: bool,
    pub evaluating: bool,      // AppRunner::has_evaluating
    pub monitoring: bool,      // AppRunner::has_monitoring
    pub settled: bool,         // no pending message, pending node or evaluation after the run
    pub changes: u64,          // the change counter, read before `settled` was computed
}

impl<E> NativeDriver<E> where E: Environment<Value = Value>, E::Payload: UIPayload + From<SimpleUIPayload> {
    pub fn new(envref: EnvRef<E>, app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
               receiver: AppMessageReceiver, sender: AppMessageSender) -> Self;   // AppRunner's order
    pub fn with_current_env(self, f: Box<dyn FnMut() -> Option<EnvRef<E>> + Send>) -> Self;
    pub fn app_state(&self) -> &Arc<tokio::sync::Mutex<dyn AppState>>;
    pub fn ui_context(&self) -> UIContext;          // what render_element and hosts need
    pub fn change_counter(&self) -> Arc<AtomicU64>; // a host increments it after each change it makes
    pub async fn tick(&mut self) -> Tick;
}
```

`tick` replaces the environment if `current_env` returned another one, runs the runner once, reads
`changes`, then computes `settled` (`!has_pending_messages()`, no `pending_nodes()`,
`!has_evaluating()`), and takes the invalidation (`None` while the lock is held elsewhere), the
events and the quit request. A host whose change completed before it incremented the counter knows
that a settled tick reporting that count has seen the change through (AC-9).

```rust
pub struct HeadlessHandle<E: Environment> {
    stop: tokio::sync::watch::Sender<bool>,
    wake: Arc<Notify>,
    task: Option<JoinHandle<NativeDriver<E>>>,
}
impl<E: Environment> Drop for HeadlessHandle<E> { fn drop(&mut self); }   // signals stop only
/// Ticks on `runtime` until stopped: `on_tick` after each tick, then waits for a wake, a stop, or
/// 16 ms while evaluating, 100 ms while monitoring, 250 ms otherwise.
pub fn spawn_headless<E>(driver: NativeDriver<E>, runtime: &tokio::runtime::Handle,
                         on_tick: Box<dyn FnMut(Tick) + Send>) -> HeadlessHandle<E>;
impl<E> HeadlessHandle<E> {
    pub fn waker(&self) -> Arc<Notify>;                        // notify_one: a wake mid-tick is kept
    pub async fn stop(mut self) -> Result<NativeDriver<E>, Error>; // takes `task`, awaits its tick
}
```

`liquers-lib/src/egui/app.rs`, new, behind `egui` and native only:

```rust
#[derive(Debug, Clone)]
pub struct WindowOptions { pub title: String, pub inner_size: [f32; 2], pub poll_interval: Duration }
impl WindowOptions { pub fn new(title: impl Into<String>) -> Self; }   // 900×700, 250 ms; also Default
#[derive(Clone, Default)] pub struct WindowCloser(Arc<CloserState>);   // Send + Sync
impl WindowCloser { pub fn close(&self); }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum FrameControl { Continue, Close }
pub fn run_window<E>(options: WindowOptions, driver: &mut NativeDriver<E>,
                     runtime: &tokio::runtime::Handle, closer: &WindowCloser,
                     on_frame: Option<&mut dyn FnMut(&Tick) -> FrameControl>) -> Result<(), Error>
where E: Environment<Value = Value>, E::Payload: UIPayload + From<SimpleUIPayload>;
```

`run_window` blocks until the window closes. It calls `eframe::run_native` (eframe's own context
setup) with an app creator that borrows the driver (`AppCreator<'_>`) and stores `cc.egui_ctx` in the
closer. A closer is reusable: `close` while `run_window` is opening or showing a window closes it
(a flag, checked every frame, plus `ViewportCommand::Close` and a repaint once the context is
known); `close` with no window does nothing.

The model work runs in `App::logic`, which eframe calls even while the window is hidden or
minimized; `App::ui` only renders. Each `logic` call runs `runtime.block_on(driver.tick())`, then
the hook, and closes on `quit`, a closer request or `FrameControl::Close`. `ui` renders each root
with `render_element(ui, handle, &driver.ui_context())`. It repaints at once while the tick changed
something or is evaluating, else after `poll_interval`, so the hook runs and changes made from other
threads show. `run_and_return` stays `true`, so the same thread can open a window again later.

## Integration Points

Examples: `ui_query_console_app.rs` and `ui_spec_demo.rs` move onto it; the other six keep their
loop (`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`, P3 S). No command changes; namespace `lui` only.
`ui/mod.rs` re-exports the driver types on native targets; `egui/mod.rs` declares `app` on native
targets. `Cargo.toml`: `libc` as an optional dependency on Linux and macOS, enabled by `egui`.

## Error Handling

Errors, all `Error::general_error` naming the cause, checked before eframe is called where possible:
- inside a Tokio runtime (`Handle::try_current().is_ok()`), where `block_on` would panic;
- a second concurrent window (a static flag);
- off the main thread: checked on Linux (`gettid() == getpid()`) and macOS (`pthread_main_np`)
  through `libc`; on Windows, winit's own panic, caught with `std::panic::catch_unwind`;
- winit's `RecreationAttempt`, or any panic caught from eframe: winit sets its "created" flag
  before such a panic, so no later window can open in this process; `run_window` records that and
  refuses every later call with that message instead of a second panic;
- `eframe::Error` creating the window (no display, no GPU): its message. eframe keeps its event loop
  per thread, so a retry on the same thread is possible.

A headless task that ended by panic is `Error::unexpected_error` from `stop`.

## Relevant Commands

None new. Namespace `lui`: the windows show `lui` elements (`query_console`, `ui_spec`).

## Documentation Architecture

Documents: `UI_INTERFACE_FSD.md` §Backend Comparison (a "native" row: `NativeDriver`, `run_window`,
`spawn_headless`), with a `## History` row; `docs/guides/rust/11-ui.md` an "Opening a window"
section; module docs as listed in Phase 1.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `ui/runner.rs`, `ui/mod.rs`, new `ui/native.rs`, `egui/mod.rs`, new `egui/app.rs`, two examples, `Cargo.toml` (`libc` on Linux and macOS, if not already direct) |
| Tests likely to change | none; new tests in `native.rs` (headless, AC-3's tick, AC-4, AC-5, AC-9) |
| Window tests | AC-1, AC-2, AC-6, AC-7 need a display: `#[ignore]`d tests run under `xvfb-run` in Phase 4; CI only if reliable |
| Compatibility | additive; the moved examples behave as before, except that *Quit* now works |
| Concurrency | the window ticks on its own thread; ownership keeps a headless task and a window from sharing a driver |
| Certainty | high for the driver; medium for the main-thread checks on Windows, checked in Phase 4 |
