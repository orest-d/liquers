# Phase 4: Implementation Plan — egui-native-app

## Overview

The runner change first (two accessors and the quit request), then the driver, then the window on
the driver, then the examples on the window, then documents. No prerequisite issue. Lands as the
first commit group of the Python UI PR (Phase 1, Scope Changes); `liquers-py-ui` builds on it.

## Progress

- [x] Step 1: the runner's quit request and pending-message check (9789773)
- [x] Step 2: the native driver and the headless task (9789773)
- [x] Step 3: the egui window (9789773)
- [x] Step 4: the window test target (9789773)
- [x] Step 5: the two examples on the window (9789773)
- [x] Step 6: documents and module docs (the docs commit of PR #268)

## Implementation Steps

### Step 1: the runner's quit request and pending-message check
- Files / symbols: `liquers-lib/src/ui/runner.rs` `AppRunner` (`quit_requested`, the `Quit` arm,
  `take_quit_request`, `has_pending_messages`); its missing module doc.
- Change: additive, as in Phase 2.
- Depends on: none.
- Proof: `cargo test -p liquers-lib --lib ui::` (existing runner tests unchanged).
- Rollback: revert the file; nothing else uses the accessors yet.

### Step 2: the native driver and the headless task
- Files / symbols: new `liquers-lib/src/ui/native.rs` (`NativeDriver`, `CurrentEnv`, `Tick`,
  `spawn_headless`, `HeadlessHandle`); `ui/mod.rs` declares it and re-exports the names, both
  `#[cfg(not(target_arch = "wasm32"))]`.
- Change: Phase 2's interfaces; `NativeDriver::envref` added for hosts and tests.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib ui::native` (Phase 3 tests 3 to 9).
- Rollback: remove the module and its re-exports.

### Step 3: the egui window
- Files / symbols: new `liquers-lib/src/egui/app.rs` (`WindowOptions`, `WindowCloser`,
  `FrameControl`, `run_window`, the private `WindowApp`); `egui/mod.rs` declares `app` on native
  targets; `Cargo.toml` `libc` (optional, Linux and macOS) in the `egui` feature.
- Change: Phase 2's interfaces. `WindowApp` has two lifetimes, one for its borrows and one for the
  hook's trait object.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-lib --lib egui::app` (tests 10 to 13).
- Rollback: remove the module and the dependency.

### Step 4: the window test target
- Files / symbols: new `liquers-lib/tests/egui_window.rs`; `Cargo.toml` `[[test]] egui_window`,
  `harness = false`, `required-features = ["egui"]`.
- Depends on: Step 3.
- Proof: `xvfb-run -a cargo test -p liquers-lib --test egui_window` (tests 14 to 17) with
  `libxkbcommon-x11-0` and `mesa-vulkan-drivers` installed; without a display it prints that it is
  skipped and passes.
- Rollback: remove the target.

### Step 5: the two examples on the window
- Files / symbols: `liquers-lib/examples/ui_query_console_app.rs`, `ui_spec_demo.rs`: their
  `eframe::App` goes; `main` builds a driver and calls `run_window`. The spec demo loses its
  "UISpec Demo" heading, which the window title now carries.
- Depends on: Step 3.
- Proof: `cargo build -p liquers-lib --example ui_query_console_app --example ui_spec_demo`; a
  15-second `xvfb-run` of each that the timeout ends (exit 124), not a panic.
- Rollback: restore the two files.

### Step 6: documents and module docs
- Files: listed under Documentation Updates.
- Depends on: Steps 1 to 5.
- Proof: `python3 scripts/check_api_docs.py` on the five modules; `python3 scripts/docs_index.py --check`;
  `cargo run -p liquers-docs -- check`.
- Rollback: revert the documents.

## Testing Plan

Each step's proof as it lands; then `cargo test -p liquers-lib --lib --tests`,
`cargo test -p liquers-lib --doc -- ui::native egui::app`, `cargo clippy -p liquers-lib --all-targets`
on the touched files, and `bash scripts/check-build-matrix.sh` (a new `cfg` and an optional
dependency), whose wasm rows check that `native` stays out of wasm builds.

## Rollback Plan

Each step reverts on its own. If the window cannot be finished, Steps 1 and 2 stand alone: the
Python model needs only the driver, and the window remainder becomes an issue.

## Documentation Updates

- `specs/reference/UI_INTERFACE_FSD.md` §Backend Comparison: a native row (`NativeDriver`,
  `run_window`, `spawn_headless`); `## History` row; `reviewed:` bumped.
- `docs/guides/rust/11-ui.md`: "Opening a window", with the console snippet and the main-thread
  rule (`USER_DOCS_EDITING_GUIDE.md`; `docs/guides/rust/HISTORY.md` row).
- `specs/README.md`: the native driver line moves from "designing" to built.
- Module docs (`API_DOCUMENTATION_GUIDE.md` §3): `ui/native.rs`, `egui/app.rs` new;
  `ui/runner.rs` gains one; `ui/mod.rs` and `egui/mod.rs` gain the new names.
- Issues: `EGUI-HAS-NO-REUSABLE-NATIVE-APP` closed with a resolution note.
- `specs/index.csv`, `index.md` regenerated. No command changes, so no registry export.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
