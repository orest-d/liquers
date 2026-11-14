# Phase 1: High-Level Design — egui-native-app

## Purpose

Give native hosts what the browser has in `mount_web` and `run_headless`
(`liquers-lib/src/ui/web/app.rs`): one library call that drives a UI model, with or without an egui
window. Today each of the eight egui examples writes its own runtime, `AppRunner` loop, repaint
policy and `eframe::App`, and a host that is not a Rust `main`, such as liquers-py, has nothing to
call.

## Problem Example

```rust
let (sender, receiver) = app_message_channel();
let app_state: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(Mutex::new(DirectAppState::new()));
let mut driver = NativeDriver::new(envref, app_state, receiver, sender);
driver.ui_context().submit_root_query("ns-lui/query_console/add-child");
liquers_lib::egui::app::run_window(WindowOptions::new("Reports"), &mut driver, runtime.handle(), &WindowCloser::default(), None)?;
```

**Today** this needs the 230-line `liquers-lib/examples/ui_query_console_app.rs`, and its *Quit*
menu item does nothing: `AppRunner::run` drops `AppMessage::Quit` and no example handles it.
**After** the snippet opens the console, *Quit* or closing the window returns `Ok(())`, and the
same driver runs without a window under `spawn_headless`. The query validates against the registry.

## Scope and Acceptance Criteria

- **AC-1** A window from one call
  WHEN `run_window` is called on the main thread with a driver whose model has a root query
  THEN the window renders the roots, ticks the driver, and returns `Ok(())` when closed
- **AC-2** Close from any thread
  WHEN `WindowCloser::close` is called from another thread while the window shows
  THEN the window closes within one frame and `run_window` returns
- **AC-3** Quit closes the window
  WHEN a control whose action is `quit` fires
  THEN the window closes; the driver's tick reports `quit`
- **AC-4** A model without a window
  WHEN `spawn_headless` runs a driver whose model has a root query
  THEN the element evaluates, `on_tick` receives its status events in order, and `stop` returns the
  driver for later use
- **AC-5** A rebuilt environment is followed
  WHEN the driver's `current_env` returns an environment other than the current one
  THEN evaluations started from then on use it
- **AC-6** A per-frame hook
  WHEN `run_window` has an `on_frame` hook
  THEN it runs after every tick, with the tick, no `AppState` lock held, at least every
  `poll_interval` even while the window is hidden, and returning `FrameControl::Close` closes it
- **AC-7** Failures are errors, not panics
  WHEN no display can be opened, a window is already open, the call is made off the main thread or
  inside a Tokio runtime
  THEN `run_window` returns an `Error` naming the cause, and a later call on the main thread can
  still open a window when the cause was not winit's
- **AC-8** The examples use it
  WHEN the egui examples are built
  THEN `ui_query_console_app` and `ui_spec_demo` run on `NativeDriver` and `run_window`
- **AC-9** Settled ticks
  WHEN a host counts its changes with the driver's change counter
  THEN a tick reports `settled` with the count it read only when no message, pending node or
  evaluation remains for those changes

Non-goals: several windows at once; a window on a background thread; moving the other six examples
(`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`); the browser backend.

## Core Interactions

UI: `AppRunner` (a quit request and a pending-message check), a new native driver over it, and an
egui window around the driver. Examples: two of the egui examples move onto the window.

## Crate Placement

`liquers-lib` only: `src/ui/native.rs` (new, native targets, no egui), `src/ui/runner.rs`,
`src/egui/app.rs` (new, feature `egui`, native only), the two examples and a window test. No
command changes.

## Documentation Intent

- Reference: `UI_INTERFACE_FSD.md` §Backend Comparison gains the native driver and window.
- User guide: `docs/guides/rust/11-ui.md` (an outline) gains a section on opening a window.
- Other documents: `specs/README.md` capability map.
- API docs: the module docs of the new `liquers-lib/src/ui/native.rs` and
  `liquers-lib/src/egui/app.rs`, and of `liquers-lib/src/egui/mod.rs`, `liquers-lib/src/ui/mod.rs`
  (re-exports) and `liquers-lib/src/ui/runner.rs` (the quit request and the new accessors).

## Design Readiness

- **Readiness:** `ready` (Phase 4 review, 2026-11-11). Leading issue: None.
- **Explanation:** the driver and the window are built and tested (Phase 3), and nothing in them
  depends on the open wheel question of `liquers-py-ui-window`.
- **Decision log (pre-approval, 2026-11-11):** None needs the owner. Settled by the repository: the
  window tests are their own `harness = false` target (Scope Changes); `libc` joins the `egui`
  feature on Linux and macOS for the main-thread check.
- **Automatic fixing:** `not-eligible`: a new library API.
- **Found while designing:** `AppMessage::Quit` is dropped by `AppRunner::run` (`runner.rs`, the
  `Quit` arm), so *Quit* menu items do nothing. It belongs here (triage case 1): AC-3.
- **Overlap:** `ui_query_console_app.rs` is also edited by `command-parameter-editor` and
  `query-editor-live-validation` (in review). Moving it here is a small diff at its `eframe::App`;
  whichever lands second rebases.

## Scope Changes

- 2026-11-11 (Phase 3): converted from the compact to the full form, since Phases 3 and 4 would
  take it past the compact size limit. The window tests became their own test target
  (`tests/egui_window.rs`, no libtest harness), because libtest never runs a test on the main
  thread. The implementation lands as the first commit group of the Python UI PR instead of a PR
  of its own: one branch carries M7's UI work.
