# Phase 1: High-Level Design — liquers-py-ui

## Purpose

Let Python hold a Liquers UI model on its environment: show the query console, a `ui_spec`
dashboard or any element a query produces, submit queries to elements, follow their status and
dispose of them, with or without a window. This is the model half of the `UIUSE` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` and of the owner's "GUI" usage mode (`VISION.md`: "the same queries
drive the egui desktop UI"). The window is [`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md),
which builds on this; the browser counterpart is [`web-ui-use`](../web-ui-use/DESIGN.md), whose API
this mirrors. Milestone M7.

## Design Readiness

- **Readiness:** `needs-decision` until the Phase 2 gate, and reassessed at the Phase 4 review.
  Phase 1 approved 2026-11-07; Phase 2 answers question 3 (callbacks are delivered by the thread
  that waits; no delivery thread).
- **Automatic fixing:** `not-eligible`: an `L` feature that changes `PyEnvironment`'s type.
- **Leading issue:** question 1, the payload change. Source issue `PY-CANNOT-USE-A-LIQUERS-UI`
  (P3 L, filed with this design).
- **Explanation:** the model needs no egui. `AppState`, `AppRunner` (status events, the stale-source
  rule, `replace_envref`), `SimpleUIPayload`, `UIElement` and the `lui` commands are not gated on
  the `egui` feature, and native windowless environments already use a UI payload
  (`liquers-agent/src/lib.rs`, `liquers-docs/src/env.rs`). liquers-py lacks the payload, the `lui`
  built-ins, a Python form of a UI element and something that drives the runner without a window.
  Split from the window on 2026-11-06 (review), so the wheel question does not hold this up.
- **Open questions:**
  1. **Proposed resolution: the UI payload.** `PyEnvironment` becomes
     `DefaultEnvironment<Value, SimpleUIPayload>`, as `WebEnvironment` did (`web-ui-use` Decision 1),
     and the `lui` commands join the built-ins. Evaluations outside a UI pass no payload. Inside a
     command, `context.element` is the handle of the element whose action ran it (UIUSE03).
  2. **Proposed resolution: the API.** `env.ui()` (and `lq.ui()`) returns an `lq.UI`:
     `show(query, handle=None)` makes the query's value an element and returns its handle;
     `submit(query, handle=None)` runs it as a control's action does; `elements()`;
     `on("status", callback)` returns an `lq.UISubscription` (`dispose()`, context manager);
     `wait_idle(timeout)` and `wait_idle_async`; `dispose()`. A UI element result reaches Python as
     an `lq.UIElement` (`type_name`, `title`), a live handle that cannot be stored: `store_set` of
     one raises.
  3. **Proposed resolution: who drives the model.** A task on liquers-py's runtime runs
     `AppRunner` while the model lives, so elements progress between Python calls. Status events
     are queued and delivered to callbacks on a Python thread, batched per tick and never under the
     `AppState` lock, so a callback may call back into the UI (`web-ui-use` Decision 3): during
     `wait_idle`, or by the window between frames (`liquers-py-ui-window`). Phase 2 decides whether
     a background script with neither gets a delivery thread.
  4. **Proposed resolution: lifetime.** The model ends at `dispose()`, `env.shutdown()` or
     interpreter exit; a collected `lq.UI` is disposed, since nothing else can reach it. A rebuild
     replaces the runner's environment for later queries (AC-9).
  5. **Proposed resolution: logs and cancellation.** An element's log and progress are read
     through its asset (`ui.asset(handle)`, an `lq.AssetRef`), and `asset.cancel()` cancels it;
     no new UI-level API. The legacy `UiCommand` and `Widget` values, egui-only, raise `ConversionError`
     when they reach Python.
- **Deviations from Appendix A** (copied to the conformance matrix): UIUSE03 reads
  `context.element`, not `context.query`; UIUSE04 observes statuses (`processing` … `ready`), not
  progress fractions; UIUSE06 gives an element query B while A runs, as `web-ui-use` does, instead of
  generations; UIUSE01 and UIUSE08 are in `liquers-py-ui-window`.

## Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.command
def report(context) -> str:
    return "rows: 3"

ui = env.ui()
with ui.on("status", lambda e: print(e.handle, e.status)):
    h = ui.show("report")
    ui.wait_idle(timeout=5)
print(ui.elements())    # [UIElementInfo(handle=1, type_name="AssetViewElement", status="ready", …)]
```

**Today** `env.ui` does not exist, the `lui` commands are unknown, and a UI element result has no
Python form. **After** the script prints `1 processing` and `1 ready`, then the element. `report`
validates with `report` declared; `hello/q/ns-lui/query_console` validates against the registry.

## Scope and Acceptance Criteria

- **AC-1** Show a query
  WHEN `ui.show("report")` and `ui.wait_idle()` run, with no window and no display
  THEN `ui.elements()` lists one element holding `rows: 3` with status `ready` (problem example)
- **AC-2** A UI value reaches Python
  WHEN `env.evaluate("hello/q/ns-lui/query_console")` produces a UI element
  THEN Python receives an `lq.UIElement` with `type_name` and `title`, not a conversion error (UIUSE02)
- **AC-3** An action reaches the right Python command
  WHEN a control's action is a query ending in a Python command, and `ui.submit` triggers it once
  THEN the command runs once, and its `context.element` is the owning element's handle (UIUSE03)
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
  THEN the first raises `lq.LiquersError`, and the second leaves the error in the element and
  delivers an `error` status event carrying it (UIUSE07)
- **AC-8** Callbacks may re-enter
  WHEN a status callback calls `ui.show`
  THEN the call succeeds and no deadlock occurs (addition)
- **AC-9** The UI follows late registrations
  WHEN a command is registered after `env.ui()`
  THEN a later query in the UI can use it (addition)

Non-goals: the window (`liquers-py-ui-window`); elements written in Python (`UIDEF`); the HTML backend
or a Jupyter widget; saving and loading the element tree.

## Core Interactions

UI: `AppState` and `AppRunner`. Commands: `lui` in the built-ins. Environment: the UI payload.
Value types: `lq.UIElement`. Bindings: `Environment.ui`, `lq.UI`, `lq.UISubscription`, `Context.element`.

## Crate Placement

`liquers-py` only: `environment.rs` (payload), `builtins.rs` (`lui`), `value.rs` (`UIElement`),
`context.rs` (`element`), `exit.rs` (the exit hook), `lib.rs`, a new `src/ui.rs`; the stubs and
`__init__.py`. The driver is a `liquers-lib` change in its own design (Scope Changes).

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §User interface; `UI_INTERFACE_FSD.md` and
  `PAYLOAD_GUIDE.md` gain the Python host.
- Guide: a "Show a Liquers UI" chapter in `PYTHON_GUIDE.md`, as the JavaScript guide has.
- Other documents: the `UIUSE` row and deviations in `liquers-py/README.md`;
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `specs/README.md`.
- Documents to update: user guide chapter `docs/guides/python/11-ui.md` ("Not available from
  Python"), which `chapters-for` does not list; the others it lists.
- API docs: the module docs of the new `liquers-py/src/ui.rs`, and of `liquers-py/src/environment.rs`,
  `builtins.rs`, `value.rs`, `context.rs`, `exit.rs` and `lib.rs`; docstrings on every new Python name.

## Design Dependencies

- `egui-native-app`: **requires** (lands first): the background driver (scope change below).
- `liquers-py-ui-window`: **required-by**.
- `language-late-registration-without-rebuild`, `post-init-command-registration` (in review):
  **overlap** AC-9.
- `language-store-config-function`, `records-arrow-c-data-export`, `language-bridge-extended-scalars`
  (in review): **overlap**; they change `PyEnvironment` or `liquers-py/src/value.rs` too.
- `ui-asset-snapshot-boundary` (in review): **overlaps** the widgets' own asset fetching.
- M6 (PRs #257, #259–#263): built after it merges; the closure asset is generic over the payload.

## Scope Changes

- 2026-11-10 (Phase 2): the driver that ticks the model is liquers-lib's `NativeDriver` and
  `spawn_headless` from `egui-native-app`, shared with the window, instead of a liquers-py loop.
  Crate placement gains that liquers-lib change, in its own PR.

## References

Guide §UIUSE and Appendix A; `specs/design/web-ui-use/`; `liquers-lib/src/ui/runner.rs` `AppRunner`;
`liquers-lib/src/ui/commands.rs`; `liquers-agent/src/lib.rs` (a windowless UI payload).
