# Phase 4: Implementation Plan — liquers-py-ui-window

## Overview

The third commit group of the Python UI PR, after [`egui-native-app`](../egui-native-app/DESIGN.md)
and [`liquers-py-ui`](../liquers-py-ui/DESIGN.md): the `gui` feature, then `run` / `close`, then the
tests and the CI job, then the wheel checks deferred by Phase 1, then documents.

## Progress

- [x] Step 1: the `gui` feature (8465894)
- [x] Step 2: `run`, `close` and `window_open` (8465894)
- [x] Step 3: the window tests and the no-`gui` test (8465894)
- [x] Step 4: the CI job under a virtual display (8465894)
- [x] Step 5: the wheel checks (2b55bc4: the Windows allow-list names the window's DLLs; release run 43092722770, fifteen wheels built, checked and tested)
- [x] Step 6: documents and module docs (the docs commit of PR #268)

## Implementation Steps

### Step 1: the `gui` feature
- Files / symbols: `liquers-py/Cargo.toml` `[features] default`, `gui`.
- Change: `default = ["async_store", "extension-module", "gui"]`, `gui = ["liquers-lib/egui"]`.
- Depends on: `egui-native-app` Step 3.
- Proof: `maturin develop` builds; `cargo test -p liquers-py --no-default-features --lib` builds
  without egui.
- Rollback: drop `gui` from `default` (the decision log's fallback): the wheels lose the window and
  `run` raises (AC-6).

### Step 2: `run`, `close` and `window_open`
- Files / symbols: `liquers-py/src/ui.rs` `UiModel::{run_window, borrow_driver, close_window,
  end_window}`, the `window` and `close_requested` fields, `run_model`, `no_window_support`;
  `UI.run`, `UI.close`, `UI.window_open`; the stubs.
- Change: Phase 2's `run`, in order; `dispose` closes the window first.
- Depends on: Step 1, `liquers-py-ui` Step 3.
- Proof: `maturin develop` builds; the model suite still passes. Tests 3 to 12 prove it with Step 3.
- Rollback: `run` raises as without `gui`.

### Step 3: the window tests and the no-`gui` test
- Files / symbols: `liquers-py/tests/test_UIUSE_window.py`; `tests/conftest.py` (`gui` marker,
  `has_display`, the skip); `ui.rs` `tests::a_build_without_gui_explains_run`.
- Depends on: Step 2.
- Proof: `xvfb-run -a python -m pytest tests/test_UIUSE_window.py`; the suite without `DISPLAY`
  skips them; `cargo test -p liquers-py --no-default-features --features debug-handles --lib ui::`.
- Rollback: delete the file; the marker skips nothing.

### Step 4: the CI job under a virtual display
- Files / symbols: `.github/workflows/liquers-py-tests.yml`, a Linux step or job that installs
  `xvfb libxkbcommon-x11-0 mesa-vulkan-drivers` and runs the window tests under `xvfb-run -a`;
  `specs/guides/GITHUB_ACTIONS_GUIDE.md`.
- Depends on: Step 3.
- Proof: the workflow on the PR.
- Rollback: remove the step; the tests stay runnable by hand.

### Step 5: the wheel checks
- Files / symbols: none changed unless a check fails.
- Change: confirm what Phase 1 deferred. Locally: `ldd` of the built extension lists no display
  library, and the whole suite imports and runs with `DISPLAY` unset. On the PR:
  `liquers-py-package.yml` (it runs when `liquers-py/Cargo.toml` changes) builds the Linux x86_64
  abi3 wheel with the default features, checks it with `scripts/check-wheel.py` (allowed libraries,
  the 90 MB limit) and imports it in a fresh venv on a runner without a display; a manual run of
  `liquers-py-release.yml` on the branch (which publishes nothing) does the same for the fifteen
  wheels on five platforms.
- Depends on: Step 1.
- Proof: `ldd $(python -c "import liquers._native as n; print(n.__file__)")`;
  `env -u DISPLAY -u WAYLAND_DISPLAY python -m pytest tests -q`; both workflows green, recorded in
  Phase 5.
- Rollback: Step 1's rollback.

### Step 6: documents and module docs
- Files / symbols: Phase 2's documentation table; the `ui.rs` module doc names the window.
- Depends on: Steps 1 to 5.
- Proof: `python3 scripts/check_api_docs.py liquers-py/src/ui.rs`; `docs_index.py --check`;
  `cargo run -p liquers-docs -- check`.
- Rollback: documents only.

## Testing Plan

Steps 2 and 3 run the window tests under `xvfb-run` after each `maturin develop`; the last step
runs `bash scripts/test-liquers-py.sh` (which skips the window tests without a display) and then the
window tests under `xvfb-run`. `liquers-lib`'s `egui` feature is unchanged in this design, so the
build matrix runs once, for `egui-native-app`.

## Rollback Plan

Step 1's rollback removes the window from the wheels without touching the model. Reverting the
whole group leaves `liquers-py-ui` working.

## Documentation Updates

`PYTHON_INTEGRATION.md` §18 "The window" and §12 Packaging; `PYTHON_GUIDE.md` "Open a window" and
§1; `docs/guides/python/11-ui.md` and `02-installing.md`; `GITHUB_ACTIONS_GUIDE.md`;
`UI_INTERFACE_FSD.md`; `liquers-py/README.md` and `LANGUAGE-INTEGRATION_GUIDE.md` §6 (`UIUSE`
COMPLETE with deviations); `specs/README.md`. History rows and `reviewed:` bumps. Close
`PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW`; regenerate `specs/index.csv`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
