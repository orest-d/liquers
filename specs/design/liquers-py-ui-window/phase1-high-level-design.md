# Phase 1: High-Level Design — liquers-py-ui-window

## Purpose

Let a Python script open the Liquers desktop UI (the egui backend) in a window over the UI model of
[`liquers-py-ui`](../liquers-py-ui/DESIGN.md), alone or beside a background server: the window half
of the `UIUSE` feature (`UIUSE01`, `UIUSE08`) and the owner's "GUI" and "GUI plus server" usage
modes, for `VISION.md`'s UI roadmap item 1, the desktop egui application. Milestone M7.

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written and reviewed twice; question 1 is the owner's.
- **Automatic fixing:** `not-eligible`: an `L` feature that changes what every wheel contains.
- **Leading issue:** question 1, whether the published wheels carry egui. Source issue
  `PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW` (P3 L, filed with this design).
- **Explanation:** liquers-py links `liquers-lib` without `egui`, and `liquers-lib` has no reusable
  native window: each of the eight egui examples writes its own `eframe::App`, runtime and
  frame loop (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`, filed with this design). Python adds the main-thread
  rule, the GIL and Ctrl-C.
- **Open questions:**
  1. **Open design (blocking): egui in the wheels.** Recommended: every published wheel includes the
     egui backend, so `pip install liquers-py` can open a window. Phase 2 measures the size added
     to the 30.9 MB abi3 wheel, checks with `auditwheel show` and `ldd` that winit's X11 and Wayland
     and glow's GL are loaded only when a window opens (so the wheel imports on a server with no
     display libraries), and measures the build's disk use. Alternative: wheels without egui, a
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
- **AC-6** A build without egui says so (only if question 1 chooses that build)
  WHEN `ui.run()` is called in a build without the egui backend
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
- `liquers-py-web-server`: **requires**, for AC-5 only.
- `liquers-py-package` (complete): **changes** its wheel contents if question 1 includes egui.

## References

Guide §UIUSE and Appendix A; `liquers-lib/src/ui/web/app.rs` `mount_web`;
`liquers-lib/examples/ui_query_console_app.rs`; `liquers-py/src/environment.rs` `block_on` (signal checks).
