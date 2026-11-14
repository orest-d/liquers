# Phase 5: Documentation - egui-native-app

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #268)

## Implementation Summary

Built as approved, as the first commit group of PR #268 (Phase 1, Scope Changes):

- `AppRunner` keeps a quit request (`take_quit_request`) and reports waiting messages
  (`has_pending_messages`), so *Quit* reaches its host instead of being dropped.
- `liquers-lib/src/ui/native.rs`: `NativeDriver` owns the runner, the `AppState` and a change
  counter; `tick` returns a `Tick` (invalidation, events, quit, settled). `spawn_headless` ticks a
  driver in the background; `HeadlessHandle::stop` returns it.
- `liquers-lib/src/egui/app.rs`: `run_window` shows a driver in an eframe window on the main
  thread, with `WindowOptions`, a thread-safe `WindowCloser`, an `on_frame` hook and
  `FrameControl`. It returns errors for a window it cannot open (a winit panic is caught and
  recorded), off the main thread, inside a runtime, or while another window is open.
- `ui_query_console_app` and `ui_spec_demo` use it; the other egui examples keep their own loop
  (`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`).

Added beyond Phase 2: `NativeDriver::envref` and the `CurrentEnv` alias (Phase 3, Learning Log).

## Documentation Delivered

### New Reference Documents
None: the contract is a section of `UI_INTERFACE_FSD.md`.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
- `specs/reference/UI_INTERFACE_FSD.md`: new section Hosts (the browser mount, the native driver,
  the window, Python), reviewed against the code; History row, `reviewed: 2026-11-11`.
- `docs/guides/rust/11-ui.md`: 11.3 rewritten around `run_window`; HISTORY row.
- `specs/README.md`: the native driver line moves to built.

### Module Docs
`liquers-lib/src/ui/native.rs`, `liquers-lib/src/egui/app.rs` (new), and the formerly missing
docs of `liquers-lib/src/ui/runner.rs`, `liquers-lib/src/ui/mod.rs` and
`liquers-lib/src/egui/mod.rs`, each with an executed (or, for a window, compiled) example.
`python3 scripts/check_api_docs.py` on all five: 0 warnings, 0 errors; their doctests pass.

### User-Guide Chapters
`chapters-for` lists `python/11-ui.md` (rewritten by `liquers-py-ui-window`), `rust/11-ui.md`
(updated) and `web/11-ui.md` (reviewed: the browser mount is unchanged, no edit).

### Links and Capability Map
`specs/README.md` User interface: "Native UI driver and egui window" is built.

## Issues Filed
None. `EGUI-HAS-NO-REUSABLE-NATIVE-APP` is closed with a resolution note.

## Important Learning

- libtest never runs a test on the main thread, so window tests need a `harness = false` target
  that skips itself without a display (`liquers-lib/tests/egui_window.rs`).
- Under Xvfb with no GPU, wgpu needs lavapipe (`mesa-vulkan-drivers`), and winit loads
  `libxkbcommon-x11` at run time; a missing library is a winit panic, which `run_window` turns into
  an error and remembers for later calls.
- eframe 0.36 opens another window after one closed in the same process.
- The settled-tick rule (count changes after making them, read the count before looking) is what
  lets a host wait for "idle" without a race; liquers-py's `wait_idle` relies on it.

## Conformance and Remaining Work
Requested, approved and built agree. Nothing remains in this design; the other examples' move is
`EGUI-EXAMPLES-HAND-WRITE-THE-FRAME-LOOP`.

## Validation
`cargo test -p liquers-lib --lib --tests` (all suites pass), `cargo test -p liquers-lib --doc --
ui::runner ui::native egui::app` (17 passed), `xvfb-run -a cargo test -p liquers-lib --test
egui_window` (4 passed), both examples build, `check_api_docs.py`, `docs_index.py --check`,
`cargo run -p liquers-docs -- check`.
