# Phase 2: Solution & Architecture — liquers-py-ui

## Overview

`PyEnvironment` gains the UI payload and the `lui` built-ins. `env.ui()` makes a model: an
`AppState`, a channel and a `NativeDriver` (from [`egui-native-app`](../egui-native-app/DESIGN.md))
ticked in the background by `spawn_headless` on liquers-py's runtime. Python sees an `lq.UI` that
shows and submits queries, lists elements and delivers status events to callbacks. A UI element
value crosses as an `lq.UIElement` holding the same `Arc`.

Question 3 is decided here. **The delivery rule:** callbacks run only on a thread that asked for
them, in `wait_idle`, on the event loop of `wait_idle_async`, in `process_events()`, or in the
window's frame hook, never under the `AppState` lock. While a window is open only its frame hook
delivers: `wait_idle` and `wait_idle_async` wait without delivering and `process_events` returns 0,
so callbacks stay on the main thread. There is no delivery thread. Questions 1, 2, 4 and 5 keep
their proposed resolutions.

Rejected: a delivery thread (it would race the script's threads); ticking only inside `wait_idle`
(elements would stop between calls); a liquers-py driver loop (the window needs the same driver).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `EGUI-HAS-NO-REUSABLE-NATIVE-APP` | draft | P1 | the driver comes from it | yes | yes | `egui-native-app`, own PR first |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | the runner awaits each submitted query inline: a slow action delays the next tick | no | no | widened to native hosts; documented |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | AC-9 follows it | no | no | none |
| `UI-ELEMENT-NESTED-IN-CONSOLE-HAS-NO-HANDLE` | draft | P3 | such elements are missing from `elements()` | no | no | documented |
| `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API` | draft | P2 | the new FSD text describes only what is built | no | no | follow it |

## Interfaces

Rust, `liquers-py`; `environment.rs`'s `PyBuilder` and `default_environment_builder` take the payload too:

```rust
pub type PyEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;
pub(crate) struct UiModel {                          // ui.rs; behind Arc
    cell: Arc<EnvCell>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>, sender: AppMessageSender,
    changes: Arc<AtomicU64>, wake: Arc<Notify>,      // the driver's change counter and waker
    driver: Mutex<DriverSlot>,
    idle_at: Mutex<Option<u64>>,                     // `changes` of the last settled tick
    pending: Mutex<VecDeque<RunnerEvent>>,           // at most 1024; the oldest is dropped
    statuses: Mutex<HashMap<UIHandle, Status>>,      // latest per element
    subscribers: Mutex<Vec<(u64, Py<PyAny>)>>,
    activity: Arc<Notify>,                           // notified after every tick
    window_open: AtomicBool, disposed: AtomicBool,
}
pub(crate) enum DriverSlot { Background(HeadlessHandle<PyEnvironment>), Lent, Disposed }
```

`show` and `submit` make their change (insert into `AppState`, or send the message), then increment
`changes` and notify `wake`. The `on_tick` closure holds a `Weak<UiModel>`, so the task never keeps
the model alive; it queues `tick.events`, updates `statuses`, sets `idle_at` to `tick.changes` when
`tick.settled` (else `None`) and notifies `activity`. The model is **idle** when
`idle_at == Some(changes)`. Stale results (AC-6) need no model code: `AppRunner` drops a result
whose element's source has changed. The window takes the driver (`Lent`).

Python (`__init__.pyi`; `frozen` pyclasses, safe on free-threaded builds):

```python
class Environment:
    def ui(self) -> UI: ...                       # a new model each call
def ui() -> UI: ...                               # default environment
class UI:
    def show(self, query: str | Query, handle: int | None = None) -> int: ...
    def submit(self, query: str | Query, handle: int | None = None) -> None: ...
    def elements(self) -> list[UIElementInfo]: ...
    def on(self, kind: Literal["status"], callback: Callable[[UIStatusEvent], object]) -> UISubscription: ...
    def wait_idle(self, timeout: float | None = None) -> bool: ...       # False on timeout
    async def wait_idle_async(self, timeout: float | None = None) -> bool: ...
    def process_events(self) -> int: ...          # delivers the queue; returns the count
    def asset(self, handle: int) -> AssetRef: ...
    def dispose(self) -> None: ...                # idempotent; also __exit__
class UIElementInfo:   handle: int; parent: int | None; type_name: str | None; title: str | None; status: Status
class UIStatusEvent:   handle: int | None; status: Status; error: LiquersError | None
class UISubscription:  def dispose(self) -> None: ...  # also a context manager
class UIElement:       type_name: str; title: str      # a live value; store_set refuses it
class Context:         element: int | None             # the element running the command
```

Sync and async: `show`, `submit`, `elements` and `process_events` return at once, taking the
`AppState` lock with the GIL released, so they work from any thread, a command included. `wait_idle`
waits on `activity` with the GIL released, waking at least every 100 ms to deliver and check signals
(Ctrl-C raises `KeyboardInterrupt`). It raises when it could never return: on the window's thread
while `run` blocks, and inside a command that this model's tick is awaiting (the Python command
adapter marks the thread while a command with this model's payload runs). `wait_idle_async` returns
a coroutine. `show` and `submit` call `cell.envref(py)` first, so a late
registration is built before the query runs; the driver's `with_current_env` reads
`current_without_gil` (AC-9).

Locks and lifetime: `subscribers` is locked with `lock_py_attached`; delivery copies the callables
under the lock and calls them, and drops any `Py`, after it is released. `dispose()` swaps the slot
to `Disposed` under the lock, then stops the driver outside it through `environment.rs` `block_on`
(GIL released, signals checked). A collected `lq.UI` only signals its task; the exit hook gives
each model 5 s. A callback that refers to its `UI` keeps it alive until `dispose()`
or the end of a `with` block; the guide says so.

## Integration Points

- `environment.rs`: the payload type; `EnvCell` keeps `Weak<UiModel>`s in a list closed at
  `shutdown()`, disposed after the servers and before the asset managers stop (as `ServerList`).
- `builtins.rs`: `liquers_lib::register_lui_commands!(cr)?`.
- `value.rs`: `ExtValue::UIElement` ↔ `lq.UIElement`, the same `Arc` both ways.
- `context.rs`: `element`, from `get_payload_clone()` and `UIPayload::handle`; the command adapter's
  thread mark for `wait_idle`.
- `ui.rs` (new): the model and classes; `stop_all_uis`, which `exit.rs` runs after stopping servers
  and before cancelling commands.
- `lib.rs`, `__init__.py`, the stubs: the new names.

## Error Handling

| Case | Raised |
|---|---|
| unparsable query in `show` / `submit` | the parse error as `lq.ParseError` |
| unknown or removed handle | `Error::general_error("no UI element N")` |
| a call after `dispose()` / `env.shutdown()` | `Error::general_error("the UI has been disposed")` / `shut_down_error()` |
| `wait_idle` where it could never return | `Error::general_error` naming the case |
| `on` with a kind other than `"status"` | `ValueError` |
| an element's query fails | not raised: an `error` event carries it (AC-7) |
| a callback raises | `sys.unraisablehook` (`PyErr::write_unraisable`); delivery goes on |
| `store_set` of an `lq.UIElement` | the store's refusal (no data format) |
| a legacy `UiCommand` / `Widget` value | `ConversionError` (question 5) |

## Relevant Commands

No new command. Namespace `lui` joins the built-ins (already in `specs/command_registry.yaml`).

## Documentation Architecture

| Document | Change |
|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | new §18 User interface: model, delivery rule, lifetime, errors; History row |
| `specs/guides/PYTHON_GUIDE.md` | new section "Show a Liquers UI"; §2 names; History row |
| `docs/guides/python/11-ui.md` | rewritten: the model |
| `UI_INTERFACE_FSD.md`, `PAYLOAD_GUIDE.md` | the Python host |
| `liquers-py/README.md`, `LANGUAGE-INTEGRATION_GUIDE.md` §6 | `UIUSE` PARTIAL, deviations |
| module docs | `ui.rs` new; `environment.rs`, `value.rs`, `context.rs`, `builtins.rs`, `exit.rs`, `lib.rs` |

`specs/README.md`: the "Liquers UI from Python" line under User interface.

## Risks

| Assessment | Finding |
|---|---|
| Existing tests likely to change | none: evaluations outside a UI pass no payload |
| New validation | `tests/test_UIUSE_model.py`; `stubtest` covers the names |
| Concurrency | no callback under a lock; the task holds a `Weak`; one driver owner at a time |
| Certainty | high; the payload change is the one `WebEnvironment` made (`web-ui-use` Decision 1) |
