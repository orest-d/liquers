# Phase 3: Examples & Use-cases — liquers-py-ui

## Overview Table

All tests are in `liquers-py/tests/test_UIUSE_model.py` unless named otherwise; the names below
drop the `test_UIUSE` prefix.

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | a report without a window | `show`, `on`, `wait_idle`, `elements`, `asset` | AC-1, AC-4 |
| 2 | Example | an action that knows its element | `submit` to a handle, `context.element` | AC-3 |
| 3 | Test | `_show_a_query_without_a_window` | the problem example, statuses and the element | AC-1, AC-4 |
| 4 | Test | `02_a_ui_value_reaches_python` | `lq.UIElement`, `type_name`, `title`; `store_set` refuses it | AC-2 |
| 5 | Test | `02_a_ui_value_crosses_back` | a command returning an `lq.UIElement` shows that element | AC-2 |
| 6 | Test | `03_an_action_reaches_the_owning_element` | one call, `context.element == handle` | AC-3 |
| 7 | Test | `03_context_element_is_none_outside_a_ui` | `None` outside a UI | AC-3 |
| 8 | Test | `04_statuses_arrive_in_order_on_the_waiting_thread` | per-element order; callbacks on the waiting thread | AC-4 |
| 9 | Test | `04_process_events_delivers_on_the_caller` | `process_events` delivers on its caller | AC-4 |
| 10 | Test | `04_wait_idle_async_delivers_on_the_event_loop` | the awaiting loop's thread | AC-4 |
| 11 | Test | `05_disposing_a_subscription_releases_its_callback` | the callable is collected | AC-5 |
| 12 | Test | `05_dispose_ends_the_model` | no callbacks; later calls raise; idempotent | AC-5 |
| 13 | Test | `05_a_with_block_disposes` | context manager | AC-5 |
| 14 | Test | `05_shutdown_disposes_the_environments_models` | `env.shutdown()` | AC-5 |
| 15 | Test | `06_a_stale_result_is_ignored` | A finishes after B; B stays | AC-6 |
| 16 | Test | `07_an_unparsable_query_raises` | `lq.ParseError` from `show` and `submit` | AC-7 |
| 17 | Test | `07_a_failing_query_is_an_error_event` | an `Error` event carrying `lq.LiquersError` | AC-7 |
| 18 | Test | `07_unknown_handle_and_kind` | "no UI element 99"; `ValueError` for a kind | AC-7 |
| 19 | Test | `07_a_raising_callback_is_reported_and_delivery_goes_on` | `sys.unraisablehook`; the next callback runs | AC-7 |
| 20 | Test | `_a_callback_may_call_back_into_the_ui` | `show` from a callback | AC-8 |
| 21 | Test | `_the_ui_follows_late_registrations` | a command registered after `env.ui()` | AC-9 |
| 22 | Test | `_wait_idle_inside_an_own_action_raises` | the own-action refusal | AC-3 |
| 23 | Test | `_wait_idle_times_out` | `False` after the timeout | AC-4 |
| 24 | Test | `_default_environment_ui` | `lq.ui()` | AC-1 |
| 25 | Rust test | `liquers-py/src/ui.rs` `tests::deadline_after_refuses_a_negative_timeout` | timeout parsing | AC-4 |
| 26 | Suite | `test_STUBS_declarations.py`, `test_DOCS_docstrings.py` | the stubs and docstrings of the new names | AC-1, AC-2 |

## Example 1: a report without a window

```python
import liquers as lq
env = lq.Environment()

@env.command
def report() -> str:
    return "rows: 3"

with env.ui() as ui:
    with ui.on("status", lambda e: print(e.handle, e.status)):
        handle = ui.show("report")
        ui.wait_idle(timeout=10)
    [info] = ui.elements()
    print(info.type_name, info.status, ui.asset(handle).get())
```

Prints `1 Processing`, `1 Ready`, then `AssetViewElement Ready rows: 3`. `show` inserts a root whose
source is the query and wakes the background task; the task's ticks evaluate it; `wait_idle`
delivers the queued events on the calling thread until a settled tick has seen the `show`. Status
names are those of `lq.Status` (capitalised), so Phase 1's lowercase spelling is corrected in the
documents. `report` validates with `--command report`.

## Example 2: an action that knows its element

```python
@env.command
def refresh(state, context) -> str:
    return f"refreshed element {context.element}"

handle = ui.show("report")
ui.submit("refresh", handle)        # as a control of element `handle` would
ui.wait_idle(timeout=10)
```

`submit` sends `AppMessage::SubmitQuery` with the handle; the runner evaluates it with a
`SimpleUIPayload` naming that handle, so `context.element == handle`. Outside a UI it is `None`. A
command that takes only `context` raises the existing "context-only" warning, so the examples take
`state` too.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| a callback raises | reported through `sys.unraisablehook`; the other callbacks still run | 19 |
| a callback calls `show` or `wait_idle` | `show` works; a nested `wait_idle` does not deliver out of order | 20 |
| `wait_idle` inside a command this model's action runs | raises "own action" instead of deadlocking | 22 |
| a slow query A replaced by B | A's result is dropped by `AppRunner`'s stale-source rule | 15 |
| more than 1024 queued events with nobody waiting | the oldest are dropped; `elements()` still shows the latest status | none: needs no new code beyond the bound, covered by review |
| `env.shutdown()` with a model alive | the model is disposed before the asset managers stop | 14 |
| a collected `lq.UI` | its task is signalled; no wait | none: needs GC timing (the exit hook path is covered by 14) |
| Ctrl-C in `wait_idle` | `KeyboardInterrupt` within 100 ms | the window suite's `WINDOW04` proves the same 100 ms check |

## Test Plan

- Python: `liquers-py/tests/test_UIUSE_model.py`, tests 3 to 24 above, each citing its scenario in a
  comment. Runs with `bash scripts/test-liquers-py.sh` and needs no display.
- Rust: `liquers-py/src/ui.rs` `tests::deadline_after_refuses_a_negative_timeout`, in the script's
  `cargo test -p liquers-py --no-default-features --features debug-handles --lib` step.
- Stubs: `stubtest` through `test_STUBS_declarations.py` sees `UI`, `UIElement`, `UIElementInfo`,
  `UIStatusEvent`, `UISubscription`, `Environment.ui`, `lq.ui` and `Context.element`.
- The whole suite: 638 passed, 33 skipped (the 10 window tests among them without a display),
  1 xpassed (pre-existing) on CPython 3.13, 2026-11-11.

## Learning Log

- The payload change touched no existing test: evaluations outside a UI pass no payload, as Phase 2
  predicted from `WebEnvironment`.
- `register_lui_commands!` expects `CommandEnvironment` and `Context` in scope and uses `?`; a test
  helper must return `Result`.
- `wait_idle` waits on a `tokio::sync::watch` tick counter instead of Phase 2's `activity: Notify`:
  a `Notify` notified between the idle check and the wait is lost, a watch version is not. The
  driver's waker comes from `HeadlessHandle::waker`, so the model holds no separate `wake`.
- `wait_idle_async` is a small Python coroutine (`liquers/_ui.py`) over two private methods
  (`_idle`, `_next_tick_async`): the loop must deliver on its own thread, which a Rust future
  completing on the runtime cannot do.
- `command.rs` gained the own-action mark (`ActionMark::enter` in `call_py_command`), which
  Phase 1's crate placement did not name; `_env.py` and `__init__.py` gained the new names.
- Delivery is serialised by one lock and marked per thread, so a callback's own `wait_idle` or
  `process_events` returns without delivering the rest out of order.
