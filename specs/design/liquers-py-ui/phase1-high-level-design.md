# Phase 1: High-Level Design — liquers-py-ui

## Purpose

Let a Python script open the Liquers desktop UI (the egui backend) on its own environment, show the
query console, a `ui_spec` dashboard or any element a query produces, and drive and observe it from
Python: the `UIUSE` feature of `LANGUAGE-INTEGRATION_GUIDE.md` and the owner's "GUI" usage mode. It
serves `VISION.md`'s "the same queries drive the egui desktop UI" (UI roadmap item 1) for Python
users. Milestone M7, with [`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) and
[`liquers-py-modules`](../liquers-py-modules/DESIGN.md). The browser counterpart is
[`web-ui-use`](../web-ui-use/DESIGN.md) (complete), whose API this design mirrors.

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written; question 1 is the owner's to decide.
- **Automatic fixing:** `not-eligible`: `XL`, it changes `PyEnvironment`'s type and what every
  wheel contains.
- **Leading issue:** question 1, whether the published wheels carry egui. Source issue
  `PY-CANNOT-USE-A-LIQUERS-UI` (P3 XL, filed with this design).
- **Explanation:** the backend exists (`AppState`, `AppRunner` with status events and the
  stale-result rule, the `lui` commands), but there is no reusable native window: every egui
  example writes its own `eframe::App` (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`, filed with this design).
  liquers-py links `liquers-lib` without `egui` and its environment has no UI payload.
- **Open questions:**
  1. **Open design (blocking): egui in the wheels.** Recommended: every published wheel includes
     the egui backend, so `pip install liquers-py` can open a window. Phase 2 measures the size
     added to the 30.9 MB abi3 wheel and checks that the wheel still imports on a Linux machine
     with no display libraries (they are loaded only when a window opens). Alternative: wheels
     without egui, a source build with a `gui` feature for those who want it, and `ui.run()`
     raising a typed error otherwise. A separate `liquers-py-gui` package is not possible: a
     second native module would link its own copy of `liquers-lib`, and an environment cannot
     cross between the two.
  2. **Proposed resolution: a reusable native app first.** `liquers-lib` gains the egui
     counterpart of `mount_web`: run a window over an `EnvRef` and an `AppState`, with a close
     request usable from another thread, status events and environment replacement. It has its
     own compact design and PR, and the examples move onto it (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`).
  3. **Proposed resolution: the UI payload.** `PyEnvironment` becomes
     `DefaultEnvironment<Value, SimpleUIPayload>`, as `WebEnvironment` did (`web-ui-use`
     Decision 1), and the `lui` and egui commands join the built-ins. Evaluations outside a UI
     pass no payload, so nothing else changes. Inside a command, `context.element` gives the
     handle of the element whose action ran it (UIUSE03).
  4. **Proposed resolution: a model separate from the window.** `env.ui()` returns an `lq.UI`
     that holds the elements and runs queries without any window: `show(query, handle=None)`,
     `submit(query, handle=None)`, `elements()`, `on("status", callback)`, `wait_idle(timeout)`,
     `dispose()`. `ui.run(title=…, size=…)` then opens a window on the same model and returns when
     it is closed. Scripts, notebooks and the tests use the model alone; only `run` needs a display.
  5. **Proposed resolution: threads.** `run` must be called on the main thread (winit requires it
     on macOS, and the rule is the same everywhere) and blocks it with the GIL released, so Python
     commands keep running on their worker threads. There is no background window; a background
     server (`liquers-py-web-server`) is the background part of the GUI-plus-server usage mode.
     `ui.close()` may be called from any thread and only requests the close.
  6. **Proposed resolution: callbacks.** Status callbacks run on the thread that drives the model
     (`run`'s main thread between frames, or the caller of `wait_idle`), batched once per tick and
     after the `AppState` lock is released, so a callback may call back into the UI (`web-ui-use`
     Decision 3). `on` returns an `lq.UISubscription` with `dispose()`, usable as a context manager.
  7. **Proposed resolution: tests.** The model tests (UIUSE02–07) run in the normal suite. The
     window tests (UIUSE01, UIUSE08) are marked `gui` and deselected by default; CI runs them on
     Linux under a virtual display, if Phase 2 shows that is reliable.

## Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.command
def report(context) -> str:
    return "rows: 3"

ui = env.ui()
ui.show("hello/q/ns-lui/query_console")
with ui.on("status", lambda e: print(e.handle, e.status)):
    ui.run(title="Reports")        # a window; type `report` into the console
```

**Today** `env.ui` does not exist, `ns-lui/query_console` is an unknown command, and a UI element
result has no Python form. A desktop UI needs a Rust `eframe` program
(`liquers-lib/examples/ui_query_console_app.rs`) that cannot call `report`. **After** the window
shows the console, running `report` in it shows `rows: 3`, and the script prints `processing`, then
`ready`. The query validates against the registry (`liquers-validate`, `lui` commands present).

## Scope and Acceptance Criteria

- **AC-1** Open a window
  WHEN `ui.run()` is called on the main thread with a display
  THEN a window shows the model's root elements and `run` returns after it is closed (UIUSE01,
  problem example)
- **AC-2** A UI value reaches Python
  WHEN `env.evaluate("hello/q/ns-lui/query_console")` produces a UI element
  THEN Python receives an `lq.UIElement` with `type_name` and `title`, not a conversion error (UIUSE02)
- **AC-3** An action reaches the right Python command
  WHEN a control's action is a query ending in a Python command, triggered once
  THEN the command runs once, and its `context.element` is the handle of the element that owns the
  control (UIUSE03)
- **AC-4** Status arrives in order
  WHEN `ui.on("status", cb)` is subscribed and an element evaluates a query
  THEN `cb` receives that element's statuses in order, ending in `ready` or `error` (UIUSE04)
- **AC-5** Disposing releases callbacks
  WHEN a subscription is disposed, or `ui.dispose()` runs
  THEN no further callbacks run and the retained callables are released (UIUSE05)
- **AC-6** Stale results are ignored
  WHEN an element is given query B while its query A is still evaluating
  THEN A's late result never replaces B's (UIUSE06)
- **AC-7** Errors are typed
  WHEN `ui.show` gets an unparsable query, or an element's query fails
  THEN the first raises `lq.LiquersError`, and the second shows the error in the element and
  delivers an `error` status event carrying it (UIUSE07)
- **AC-8** Thread affinity
  WHEN `ui.run()` is called from a thread other than the main one, or `ui.close()` from any thread
  THEN the first raises `lq.LiquersError`, and the second closes the window, after which `run`
  returns (UIUSE08)
- **AC-9** The model works without a window
  WHEN `ui.show(query)` and `ui.wait_idle()` are called with no window and no display
  THEN the element is created and `ui.elements()` lists it with its status
- **AC-10** The UI follows late registrations
  WHEN a command is registered after `env.ui()` was created
  THEN a later query in the UI can use it
- **AC-11** GUI with a background server
  WHEN `env.serve(background=True)` runs while `ui.run()` shows a query
  THEN both answer from the same environment, and a result computed by one is reused by the other
- **AC-12** A build without egui says so (only if question 1 chooses that build)
  WHEN `ui.run()` is called in a build without the egui backend
  THEN it raises `lq.LiquersError` naming the missing feature

Non-goals: elements or renderers written in Python (`UIDEF`); the HTML backend or a Jupyter widget;
a window on a background thread; saving and loading the element tree; egui in the browser.

## Core Interactions

UI: the new native app, `AppState`, `AppRunner` events. Commands: `lui` and egui commands in the
built-ins. Environment: the UI payload; a rebuild replaces the runner's `EnvRef`. Value types: an
outbound `lq.UIElement` wrapper. Bindings: `Environment.ui`, `lq.UI`, `lq.UIElement`,
`lq.UISubscription`, `Context.element`.

## Crate Placement

`liquers-lib/src/egui/` (feature `egui`) for the native app, its own PR first (question 2).
`liquers-py`: `Cargo.toml` enables `egui`; `environment.rs` (payload), `builtins.rs`, `value.rs`
(`UIElement`), `context.rs` (`element`), a new `src/ui.rs`; stubs and `__init__.py`.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §User interface.
- Guide: a "Show a Liquers UI" chapter in `PYTHON_GUIDE.md`, as the JavaScript guide has.
- Other documents: the `UIUSE` row of `liquers-py/README.md`; `LANGUAGE-INTEGRATION_GUIDE.md`
  §6 Python; `liquers-py/README.md` wheel contents, if question 1 adds egui; `specs/README.md`.
- Documents to update: the user guide chapters `docs_index.py chapters-for` lists.
- API docs: the module docs of the new `liquers-py/src/ui.rs` and the new `liquers-lib` native
  app module, and of `environment.rs`, `builtins.rs` and `context.rs`; docstrings on every new
  Python name.

## Design Dependencies

- `EGUI-HAS-NO-REUSABLE-NATIVE-APP`: **requires** (question 2), its own design and PR, first.
- `liquers-py-web-server`: **requires** for AC-11 only.
- `ui-asset-snapshot-boundary` (in review): **overlaps**; widgets that fetch assets themselves
  are its subject, and this design uses whatever the widgets do.
- `ad-hoc-closure-asset` (M6, PR #259, open): **overlaps**; the payload change (question 3)
  touches the code it adds, so this design is built after M6 merges.
- `package` (`liquers-py-package`, complete): **changes** the wheel contents if question 1
  includes egui.

## References

- Guide §UIUSE and Appendix A `UIUSE`; `specs/design/web-ui-use/` (Decisions 1 and 3)
- `liquers-lib/src/ui/runner.rs` `AppRunner`, `take_events`, `replace_envref`;
  `liquers-lib/src/ui/web/app.rs` `mount_web`; `liquers-lib/examples/ui_query_console_app.rs`
