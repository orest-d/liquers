# Phase 2: Solution & Architecture — liquers-py-ui-window

## Overview

Every standard wheel carries the egui backend: liquers-py gains a default feature `gui`
(`liquers-lib/egui`). `UI.run()` takes the model's driver from its background task, opens
`liquers_lib::egui::app::run_window` (from [`egui-native-app`](../egui-native-app/DESIGN.md)) on the main
thread with the GIL released, and hands the driver back to a new background task when the window
closes. Between frames a hook re-attaches the GIL to deliver the model's queued status events and to
check signals. `UI.close()` works from any thread through a `WindowCloser`.

Phase 1 answers, as decided here:
- **Question 1, egui in the wheels: recommended yes; the owner confirms at this gate.** The owner
  wants egui in the standard wheels unless it costs too much space (2026-11-06), the measured cost
  is +8.0 MB compressed (+27%), and the request of 2026-11-10 is "egui support". `gui` stays a
  feature, so a source build can leave it out (AC-6). The no-display import and `auditwheel` checks
  run in Phase 4; if either fails, the wheels drop `gui` and AC-6 is the documented path.
- **Questions 2 to 5** keep their proposed resolutions; question 2 is `egui-native-app`.

Rejected: a window on a background thread (winit refuses it on macOS, and one event loop per process
must stay on one thread); a window that only renders while the background task keeps ticking (lost
frames under lock contention, `egui-native-app` Solution); a second extension module with egui (it
would link its own `liquers-lib`, and an environment cannot cross between the two).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `EGUI-HAS-NO-REUSABLE-NATIVE-APP` | draft | P1 | the window comes from it | yes | yes | `egui-native-app`, own PR first |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | a slow action freezes the window for its duration, as in the egui examples today | no | no | widened to native hosts; documented |
| `PY-NO-PUBLISHED-WHEELS` | draft | P2 | the first release ships the larger wheel | no | no | note in the release checklist |

## Interfaces

Rust, `liquers-py/src/ui.rs` (behind `gui` where marked):

```rust
impl UiModel {
    /// The bookkeeping of one tick, shared by the background task and the window's frame hook.
    fn record_tick(&self, tick: &Tick);
}
pub(crate) enum DriverSlot { Background(HeadlessHandle<PyEnvironment>), Lent, Disposed } // from liquers-py-ui
// UiModel gains: window: Mutex<Option<WindowCloser>>   (gui)
```

Python:

```python
class UI:
    def run(self, title: str = "Liquers", size: tuple[float, float] = (900.0, 700.0)) -> None: ...
    def close(self) -> None: ...          # any thread; nothing to do without a window
    @property
    def window_open(self) -> bool: ...
```

`run`, in order:
1. Refuses off the main thread (`threading.current_thread() is threading.main_thread()`), when
   another window is open, and after `dispose()`: before winit is reached (AC-2).
2. Takes the driver: swaps the slot from `Background` to `Lent` under its lock, releases the lock,
   then awaits `HeadlessHandle::stop` through `environment.rs` `block_on` (GIL released, signals
   checked); sets `window_open`.
3. Calls `run_window` with the GIL released, `poll_interval` 100 ms and a frame hook that calls
   `record_tick` and attaches the GIL only when events are pending or 100 ms have passed since it
   last did, to deliver them (AC-3) and run `py.check_signals()`. A signal error is kept and
   returns `FrameControl::Close` (AC-4).
4. After the window closes, spawns a new background task with the driver, unless the model was
   disposed meanwhile, and raises the kept error, if any.

Delivery follows the model's rule ([`liquers-py-ui` Phase 2](../liquers-py-ui/phase2-architecture.md)):
while a window shows, only its frame hook delivers, so callbacks stay on the main thread;
`wait_idle` elsewhere waits without delivering, and on the window's own thread (from a callback)
raises. `dispose()` and `env.shutdown()` from another thread close the window first.

GUI with a background server (AC-5): `env.serve(background=True)` from
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) (complete) and the window read the
same `EnvCell`, so both evaluate through one `EnvRef` and one asset manager, and an asset computed
by one is found by the other. Sync only: `run` blocks by nature, and an async variant
would still have to own the main thread.

Cargo: `liquers-py/Cargo.toml` `[features] default = ["async_store", "extension-module", "gui"]`,
`gui = ["liquers-lib/egui"]`. Without `gui`, `run` raises (AC-6) and `close` does nothing.

## Integration Points

- `liquers-py/Cargo.toml`: the `gui` feature.
- `ui.rs`: `run`, `close`, `window_open`, `record_tick`; `dispose` closes the window.
- `__init__.pyi`, `_native.pyi`: the three names; `stubtest` sees them in every build because they
  exist without `gui` too.
- `pyproject.toml`: a `gui` pytest marker, deselected by default (`-m "not gui"`).
- `.github/workflows/liquers-py-tests.yml`: a Linux job running `-m gui` under `xvfb-run`, if Phase
  4 shows it is reliable; otherwise the window tests stay manual.
- `.github/workflows/liquers-py-release.yml`: no change of steps; its wheels grow by about 8 MB.

## Error Handling

| Case | Raised |
|---|---|
| `run` off the main thread | `Error::general_error("UI.run must be called on the main thread")` as `lq.LiquersError` |
| a second window, or `run` after `dispose()` | `Error::general_error` naming the cause |
| `wait_idle` on the window's thread while `run` blocks | `Error::general_error`, from `liquers-py-ui` |
| no display (`DISPLAY` and `WAYLAND_DISPLAY` unset on Linux, or the window cannot be created) | the `run_window` error as `lq.LiquersError`, with eframe's message |
| a build without `gui` | `Error::general_error("this build has no window support: build liquers-py with the gui feature")` |
| Ctrl-C | `KeyboardInterrupt`, after the window closed |
| a callback raises | `sys.unraisablehook`, as in `liquers-py-ui`; the window stays |

## Relevant Commands

None new. Namespace `lui` (the console and `ui_spec` elements the window shows).

## Documentation Architecture

| Document | Change |
|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | §18 User interface gains "The window"; §12 Packaging: the wheels carry egui, the `gui` feature; History row |
| `specs/guides/PYTHON_GUIDE.md` | "Show a Liquers UI" gains "Open a window"; §1 Installing: what `gui` adds; History row |
| `docs/guides/python/11-ui.md` | the window section, with the main-thread rule and Ctrl-C |
| `docs/guides/python/02-installing.md` | the wheels open windows; a source build without `gui` does not |
| `specs/guides/GITHUB_ACTIONS_GUIDE.md` | the `gui` job under `xvfb-run`, if Phase 4 adds it |
| `specs/reference/UI_INTERFACE_FSD.md` | the Python host row names the window |
| `liquers-py/README.md`, `LANGUAGE-INTEGRATION_GUIDE.md` §6 | `UIUSE` COMPLETE with its deviations; the wheel size |
| module doc | `ui.rs`: the window in main components and an example |

`affects_docs` as in `DESIGN.md`. `specs/README.md`: the Python UI line names the window.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/Cargo.toml`, `src/ui.rs`, stubs, `pyproject.toml`, the tests workflow |
| Crates and workflows affected | liquers-py; the release wheels' contents and build time (+1 min on four cores) |
| Existing tests likely to change | none |
| New validation | `tests/test_UIUSE_window.py` (`gui`); a CI import check on the Linux abi3 wheel with no display variables set |
| Wheels | display libraries load at run time (`ldd` shows none new); `auditwheel show` and the other four platforms are checked on the PR by the release workflow in Phase 4 |
| Renderer | liquers-lib's eframe uses wgpu (its default). Under Xvfb with no GPU it needs a software Vulkan or GL; if CI cannot run it, the window tests stay manual and `glow` is filed as an option |
| Concurrency | the driver has one owner at a time (`DriverSlot`); callbacks run only on the window's thread while it shows |
| Certainty | medium: the main-thread rule and run-and-return are eframe's documented behaviour; Ctrl-C through a 100 ms frame hook is to be shown in Phase 3 |
