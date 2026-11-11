# Phase 5: Documentation - liquers-py-ui-window

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #268)

## Implementation Summary

Built as approved, as the third commit group of PR #268:

- `liquers-py` has a default feature `gui` (`liquers-lib/egui`), so every wheel opens windows.
  **The owner has not yet confirmed this default** (Phase 1 question 1, decision log); dropping
  `gui` from `default` is the one-line alternative, after which `run` raises (AC-6).
- `UI.run(title, size)` borrows the model's driver, shows `liquers_lib::egui::app::run_window` on
  the main thread with the GIL released, delivers callbacks and checks signals between frames, and
  gives the driver back; `UI.close()` from any thread; `UI.window_open`.
- `tests/test_UIUSE_window.py` (marked `gui`, skipped without a display) and a CI run of the
  whole liquers-py loop under `xvfb-run`.

Changed from Phase 2: the `gui` tests skip without a display instead of being deselected, and
`close()` is kept until the first frame (Phase 3). From the code review: taking the driver from
the background task no longer checks signals, since an interrupted stop dropped the driver and
left the model stuck; a Ctrl-C meanwhile stays pending and the first frame raises it. The runtime
is fetched before any window state is set, and a failed restart disposes the model instead of
leaving it stuck.

## Documentation Delivered

### New Reference Documents
None: `PYTHON_INTEGRATION.md` §18 "The window" and §12 "The window".

### New Guide Documents
None: `PYTHON_GUIDE.md` §6 "Opening a window".

### Existing Documents Reviewed or Updated
Each with a History row and `reviewed: 2026-11-11`: `PYTHON_INTEGRATION.md` (§1, §12, §18),
`PYTHON_GUIDE.md` (§1, §6), `UI_INTERFACE_FSD.md` (Hosts), `GITHUB_ACTIONS_GUIDE.md`
(`liquers-py-tests.yml` under a virtual display), `LANGUAGE-INTEGRATION_GUIDE.md` §6,
`liquers-py/README.md` (wheels carry egui; `UIUSE01`, `08`), `specs/README.md`.

### Module Docs
`liquers-py/src/ui.rs` names the window in its main components; `check_api_docs.py`: 0 warnings.

### User-Guide Chapters
`docs/guides/python/11-ui.md` §11.4 Opening a window; `02-installing.md` (the window in every
package, a build without it, a virtual display); `15-troubleshooting.md`. The window example does
not run in the tests (it waits for its reader) and says so.

### Links and Capability Map
`specs/README.md`: "Liquers UI from Python (the model, and an egui window)" is built.

## Issues Filed

- `PY-UI-CHANGE-DOES-NOT-WAKE-AN-OPEN-WINDOW` (P3 S): a `show` from another thread waits for the
  window's next poll (up to 100 ms). Not eligible for a fix in this PR (it needs a repaint handle
  from liquers-lib); filed with the expected behaviour.

`PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW` is closed with a resolution note.

## Important Learning

- Ctrl-C needs no handler: the frame hook's `py.check_signals()` at most every 100 ms is enough, as
  `_thread.interrupt_main()` shows in the tests.
- A blocking wait that checks signals must not own anything it would drop when interrupted; the
  driver's stop is the case here.
- The window costs about 8 MB per wheel and loads its system libraries only when a window opens.

## Conformance and Remaining Work
Built as approved; the wheel default awaits the owner's confirmation. Phase 1 deferred two wheel
checks, both done. The import without display libraries is shown locally (`ldd` lists only libc,
libm and libgcc_s; the suite runs with no display). A manual run of `liquers-py-release.yml` on the
branch (run 43092722770, publishing nothing) built all fifteen wheels on five platforms, passed
`check-wheel.py` on each and tested each in a fresh venv. It first failed on Windows: the window
links nine system DLLs (`dwmapi`, `dxgi`, `gdi32`, `imm32`, `opengl32`, `setupapi`, `shlwapi`,
`uiautomationcore`, `uxtheme`), which `liquers-py/packaging/allowed-libraries.toml` now names
(2b55bc4); all are part of every Windows desktop install. A second attempt was needed only because
GitHub's artifact upload failed on the first.

## Validation
Window tests under `xvfb-run` (10 passed, five runs in a row); `cargo test -p liquers-py
--no-default-features --features debug-handles --lib` (the no-`gui` test); `xvfb-run -a bash
scripts/test-liquers-py.sh`; the PR's workflows, and the release run above.
