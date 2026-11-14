# Phase 1: High-Level Design — liquers-py-ui-window

## Purpose

Let a Python script open the Liquers desktop UI (the egui backend) in a window over the UI model of
[`liquers-py-ui`](../liquers-py-ui/DESIGN.md), alone or beside a background server: the window half
of the `UIUSE` feature (`UIUSE01`, `UIUSE08`) and the owner's "GUI" and "GUI plus server" usage
modes, for `VISION.md`'s UI roadmap item 1, the desktop egui application. Milestone M7.

## Design Readiness

- **Readiness:** `ready`. Question 1 stayed open through the implementation, which assumed "yes",
  the recommended answer; Orest accepted it as built on 2026-11-14 ("complete and merge the
  design").
- **Automatic fixing:** `not-eligible`: an `L` feature that changes what every wheel contains.
- **Leading issue:** question 1, whether the published wheels carry egui. Source issue
  `PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW` (P3 L, filed with this design).
- **Explanation:** liquers-py links `liquers-lib` without `egui`, and `liquers-lib` has no reusable
  native window: each of the eight egui examples writes its own `eframe::App`, runtime and
  frame loop (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`, filed with this design). Python adds the main-thread
  rule, the GIL and Ctrl-C.
- **Open questions:**
  1. **Decided 2026-11-14 (yes): egui in the wheels.** Recommended, assumed by Phase 2 pending the
     owner's confirmation at its gate: every published wheel includes the egui backend, so `pip install liquers-py` can open a window. **Measured 2026-11-06** (Linux
     x86_64, `--profile wheel --features abi3`, with a probe that links `eframe::run_native`,
     `render_element` and the egui commands): the library grows from 85.3 MB to 104.4 MB
     (+19.2 MB, +22%), and its deflate-compressed size, which approximates the wheel, from 29.6 MB
     to 37.7 MB (+8.0 MB, +27%); the build takes 13m40s instead of 12m41s on four cores. `ldd`
     lists no new library (only libc, libm, libgcc_s in both), so X11, Wayland and GL are loaded
     at run time. Deferred to Phase 4: importing the egui wheel on a machine without display
     libraries and `auditwheel show`, on the other platforms too; if either fails, the wheels
     drop egui and the AC-6 source-build path is the fallback. Owner's wish (2026-11-06): web UI, egui
     and ratatui in the standard wheels unless that costs too much space. Alternative: wheels without egui, a
     source build with a `gui` feature, and `ui.run()` raising a typed error otherwise (AC-6). A
     separate `liquers-py-gui` package is not possible: a second native module links its own copy
     of `liquers-lib`, and an environment cannot cross between the two.
  2. **Proposed resolution: a reusable native app first.** `liquers-lib` gains the egui counterpart
     of `mount_web` behind the `egui` feature: a window over an `EnvRef` and an `AppState` with
     options, a close request usable from any thread (`ViewportCommand::Close` plus a repaint),
     repaints while evaluations are in flight, status events and environment replacement. Its
     own compact design and PR; the examples move onto it.
  3. **Proposed resolution: threads.** `ui.run(title=…, size=…)` must be called on the main thread
     (winit requires it on macOS, and the rule is the same everywhere); elsewhere it raises before
     winit is reached, since winit panics instead of returning an error. It blocks with the GIL
     released, so Python commands keep running on their worker threads, and returns when the
     window closes. There is no background window: the background part of "GUI plus server" is a
     background server (`liquers-py-web-server`). `ui.close()` works from any thread. The plan's
     `lq.run_gui(env, …)` becomes `env.ui().run()`.
  4. **Proposed resolution: callbacks and Ctrl-C.** Between frames the window delivers queued status
     callbacks on the main thread (`liquers-py-ui` question 3) and checks for signals, so Ctrl-C
     closes the window and `run` raises `KeyboardInterrupt`, as foreground `serve` does.
  5. **Proposed resolution: tests.** The window tests are marked `gui` and deselected by default;
     CI runs them on Linux under a virtual display if Phase 2 shows that is reliable.
- **Decision log (pre-approval, 2026-11-11):**
  1. **Decided 2026-11-14: egui in every wheel** (question 1). Assumed yes, `gui` a default
     feature, and accepted as built when Orest completed the design. The code is the same either way; "no" is one line, dropping `gui` from
     `default` (Phase 4, Step 1 rollback), after which `run` raises as AC-6 says.
  2. Settled by the repository: the `gui` tests skip without a display instead of being deselected
     (Phase 3, Test Plan); `close()` is kept until the first frame (Phase 3, Learning Log).
- **Deviations from Appendix A** (copied to the conformance matrix): UIUSE01 is foreground
  `env.ui().run()`, not `env.ui.start(backend="egui", background=True)`; UIUSE08 allows `close()`
  from any thread and refuses `run()` off the main thread, where the sample refuses a shutdown from
  the wrong thread.

## Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.command
def report(context) -> str:
    return "rows: 3"

ui = env.ui()
ui.show("hello/q/ns-lui/query_console")
ui.run(title="Reports")         # a window; type `report` into the console
```

**Today** no window can be opened from Python; a desktop UI needs a Rust `eframe` program
(`liquers-lib/examples/ui_query_console_app.rs`) that cannot call `report`. **After** the console
opens in a window, `report` typed into it shows `rows: 3`, and `run` returns when the window is
closed. The query validates against the registry.

## Scope and Acceptance Criteria

- **AC-1** Open a window
  WHEN `ui.run()` is called on the main thread with a display
  THEN a window shows the model's root elements, and `run` returns after it is closed (UIUSE01,
  problem example)
- **AC-2** Thread affinity
  WHEN `ui.run()` is called from another thread, and `ui.close()` from a worker thread while `run`
  shows the window
  THEN the first raises `lq.LiquersError`, and the second closes the window, after which `run`
  returns (UIUSE08)
- **AC-3** Callbacks run on the main thread
  WHEN a status subscription exists while the window shows an evaluating element
  THEN its callback runs on the main thread, after the frame's lock is released
- **AC-4** Ctrl-C
  WHEN the user presses Ctrl-C while `run` blocks
  THEN the window closes and `run` raises `KeyboardInterrupt`
- **AC-5** GUI with a background server
  WHEN `env.serve(background=True)` runs while `ui.run()` shows a query
  THEN both answer from the same environment, and a result computed by one is reused by the other
- **AC-6** A build without egui says so
  WHEN `ui.run()` is called in a source build without the `gui` feature
  THEN it raises `lq.LiquersError` naming the missing feature

Non-goals: a window on a background thread; egui in the browser; the model itself (`liquers-py-ui`).

## Core Interactions

UI: the new native egui app. Build: the wheels' contents. Bindings: `UI.run`, `UI.close`.

## Crate Placement

`liquers-lib/src/egui/` (feature `egui`): the native app, its own PR first (question 2). `liquers-py`:
`Cargo.toml` enables `egui` (question 1), `src/ui.rs` gains `run` and `close`, plus the stubs; the
release workflow (`.github/workflows/liquers-py-release.yml`) if the wheels change.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md` §User interface (from `liquers-py-ui`) gains the window.
- Guide: the "Show a Liquers UI" chapter of `PYTHON_GUIDE.md` gains "Open a window".
- Other documents: `liquers-py/README.md` (the `UIUSE` row, wheel contents); `UI_INTERFACE_FSD.md`;
  `specs/README.md`.
- Documents to update: user guide chapter `docs/guides/python/11-ui.md`; the chapters
  `chapters-for` lists.
- API docs: the module doc of the new `liquers-lib` native app module and of `liquers-py/src/ui.rs`;
  docstrings on `run` and `close`.

## Design Dependencies

- `liquers-py-ui`: **requires** (lands first).
- `EGUI-HAS-NO-REUSABLE-NATIVE-APP`: **requires** (question 2). Its change site,
  `ui_query_console_app.rs`, is also edited by `command-parameter-editor` and
  `query-editor-live-validation` (in review): **overlap**.
- `liquers-py-web-server` (complete): **requires**, for AC-5 only.
- `liquers-py-package` (complete): **changes** its wheel contents if question 1 includes egui.

## Scope Changes

- 2026-11-11 (Phase 3): `egui-native-app` lands as the first commit group of the same PR as this
  design and `liquers-py-ui`, not a PR of its own (one branch carries M7's UI work).
- 2026-11-10 (Phase 2): `UI.window_open` added; `run` borrows the model's driver from its
  background task and hands it back when the window closes (`DriverSlot::Lent`).

## References

Guide §UIUSE and Appendix A; `liquers-lib/src/ui/web/app.rs` `mount_web`;
`liquers-lib/examples/ui_query_console_app.rs`; `liquers-py/src/environment.rs` `block_on` (signal checks).
