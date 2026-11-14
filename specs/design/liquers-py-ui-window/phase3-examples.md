# Phase 3: Examples & Use-cases — liquers-py-ui-window

## Overview Table

Window tests are in `liquers-py/tests/test_UIUSE_window.py`, marked `gui`; the names drop the
`test_UIUSE_WINDOW` prefix.

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | the console in a window | `show`, `run`, a command typed into the console | AC-1 |
| 2 | Example | GUI plus server | `serve` in the background, `run` in the foreground | AC-5 |
| 3 | Test | `01_run_shows_the_model_and_close_ends_it` | `run` returns after `close()`; the element kept its value | AC-1, AC-2 |
| 4 | Test | `01_the_model_runs_on_after_the_window` | the driver goes back to the background; a second `run` opens | AC-1 |
| 5 | Test | `02_run_off_the_main_thread_raises` | "main thread", no window | AC-2 |
| 6 | Test | `02_run_after_dispose_raises` | "disposed" | AC-2 |
| 7 | Test | `02_dispose_from_another_thread_closes` | `dispose()` from a thread closes the window | AC-2 |
| 8 | Test | `03_callbacks_run_on_the_main_thread` | `Processing`, `Ready`, both on the main thread | AC-3 |
| 9 | Test | `03_wait_idle_on_the_window_thread_raises` | from a callback: refused, names the window | AC-3 |
| 10 | Test | `03_wait_idle_elsewhere_waits_without_delivering` | a thread's `wait_idle` returns; callbacks stay on the main thread | AC-3 |
| 11 | Test | `04_ctrl_c_closes_the_window_and_raises` | `_thread.interrupt_main()`; `KeyboardInterrupt`; the model goes on | AC-4 |
| 12 | Test | `05_a_background_server_beside_the_window` | the server answers from the asset the window computed | AC-5 |
| 13 | Rust test | `liquers-py/src/ui.rs` `tests::a_build_without_gui_explains_run` | the no-`gui` error names the feature | AC-6 |
| 14 | Suite | the whole suite without a display | imports and runs; the `gui` tests skip | AC-6 |

## Example 1: the console in a window

```python
import liquers as lq
env = lq.Environment()

@env.command
def report() -> str:
    return "rows: 3"

ui = env.ui()
ui.show("hello/q/ns-lui/query_console")
ui.run(title="Reports")     # blocks; type `report` into the console, close the window
```

`run` takes the driver from the model's background task, opens `liquers_lib::egui::app::run_window`
on the main thread with the GIL released, and gives the driver back when the window closes. The
console's own evaluation goes through the same environment, so `report` resolves. Phase 1's example
declared `report(context)`, which raises the context-only warning; the documents use `report()`.

## Example 2: GUI plus server

```python
with env.serve(port=8080) as server:    # background by default
    ui.show("report")
    ui.run()                            # the browser at server.url and the window share assets
```

Both read the same `EnvCell`, so `report` runs once: test 12 counts the calls.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| `close()` before the first frame | kept by `close_requested` under the window lock, the first frame closes | 3, 7 (the helper may close early) |
| `dispose()` or `env.shutdown()` while the window shows | the window closes first; `run` returns; the driver is dropped | 7 |
| a callback raises while the window shows | `sys.unraisablehook`; the window stays | the model suite's `07_a_raising_callback…` (same `deliver`) |
| no display | `run_window`'s error as `lq.LiquersError`; a missing `libxkbcommon-x11` panic is caught and later windows refuse with the recorded reason | `liquers-lib/tests/egui_window.rs` and `egui::app::tests` |
| a second `run` while one shows (from a callback) | "a window is already open on this UI" | review: the same `window_open` swap as test 5's path |
| Ctrl-C | the frame hook's `check_signals` keeps the error, closes the window, `run` raises it | 11 |

## Test Plan

- Window: `xvfb-run -a python -m pytest tests/test_UIUSE_window.py` (tests 3 to 12). Without a
  display, `conftest.py` skips the `gui` marker with "no display: run under xvfb-run". This changes
  Phase 2's "deselected by default": a developer with a display runs them in the ordinary loop, and
  CI needs no `-m` flag, only a virtual display (Phase 4 adds the job).
- Ubuntu needs `xvfb libxkbcommon-x11-0 mesa-vulkan-drivers`: winit loads `libxkbcommon-x11` at run
  time, and wgpu needs a software Vulkan (lavapipe) when there is no GPU.
- Rust: test 13 runs in `cargo test -p liquers-py --no-default-features --features debug-handles --lib`,
  the build without `gui`.
- Proven 2026-11-11 on CPython 3.13 under Xvfb: 10 passed, 5 runs in a row, about 3.6 s each.

## Learning Log

- eframe 0.36's `run_native` returns when the window closes and can open another window later in
  the same process: test 4 opens two in a row.
- Ctrl-C works through the frame hook alone: `_thread.interrupt_main()` sets the flag, and the
  hook's `py.check_signals()` (at most every 100 ms) turns it into `KeyboardInterrupt` and closes the
  window.
- A `WindowCloser` forgets a close made before its window starts, which is right for liquers-lib.
  For `UI.close()` the model keeps its own `close_requested` flag, set under the `window` lock and
  read by the hook, so a close from a helper thread cannot be lost between `run` taking the driver
  and the first frame.
- No `Quit` command exists for queries (`quit` is a control action), so the Python tests close the
  window with `close()`; liquers-lib's window tests cover *Quit*.
