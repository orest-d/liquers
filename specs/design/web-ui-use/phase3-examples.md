# Phase 3: Examples & Use-cases — web-ui-use

All queries below are checked with `liquers-validate` against `specs/command_registry.yaml`.
`hello/q/ns-lui/query_console`, `ns-lui/roots` and `commands_doc` return `Ok`. `action_probe` is a
page command and returns `Ok` with `--command action_probe`. `nosuchcommand` is meant to fail, and
it does (`ActionNotRegistered`). `a~` is meant not to parse, and it does not (`ParseError`).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Mount the query console | mount, status events, `show`, dispose | AC-1, AC-4, AC-5, AC-8, AC-10 |
| 2 | Example | A menu action reaches a page command | `UISpec` button → JavaScript command → `context.element` | AC-3 |
| 3 | Test | `liquers-lib/tests/ui_runner.rs` (native; extends the existing 7 tests) | runner events, environment replacement, stale results | AC-4, AC-6, AC-7, AC-9 |
| 4 | Test | `liquers-web/tests/ui_UIUSE.rs` (Node loop) | element handles, refusals, `lui` registration | AC-2, AC-7, AC-8 |
| 5 | Test | `liquers-web/tests/ui_browser_UIUSE.rs` (`browser-tests`) | the mount in a real DOM | AC-1, AC-3 to AC-10 |
| 6 | Test | `liquers-web/tests/e2e/ui.spec.ts` (Playwright) | the delivered package in a page | AC-1, AC-10 |

## Example 1: Mount the query console

A page shows the console and logs status changes, adds a second root element showing a query's
value, and tears everything down.

```js
await liquers.init();
const events = [];
const ui = await liquers.mountUI(document.getElementById("app"),
                                 { query: "hello/q/ns-lui/query_console" });
const sub = ui.on("status", (e) => events.push([e.handle, e.status]));
const h = ui.show("commands_doc");          // returns the new root's handle
// after a tick: #app contains #ui-element-1 (the console) and #ui-element-<h>
// events: [[1, "processing"], [1, "ready"], [h, "processing"], [h, "ready"]]
// (the first pair may arrive before `on` subscribes, if the first tick has already run)
sub.dispose();
ui.dispose();                               // #app is empty; ui.running === false
ui.show("commands_doc");                    // throws LiquersError: the UI mount has been disposed
```

## Example 2: A menu action reaches a page command

```js
let seen = null;
liquers.registerCommand({ name: "action_probe", arguments: [{ name: "context" }],
                          run: (context) => { seen = context.element; return "ok"; } });
const ui = await liquers.mountUI(app, { spec: `
menu:
  items:
  - !button
    label: Probe
    action:
      query: "action_probe"
layout: vertical` });
app.querySelector("[data-lq-action]").click();
// after a tick: seen === 1, the spec element's handle
```

The button's `UiAction::Query` goes through `dispatch_action` with the handle of its own element,
then through `SubmitQuery`, so the payload names the spec element. `js-command-context` exposes
that handle as `context.element`.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| `{ query, spec }` both given | rejects with `General` | `uiuse07c_mount_refuses_query_and_spec` |
| `{ query: "a~" }` cannot be parsed | rejects with the parse error, before any DOM check | `uiuse07_mount_rejects_unparsable_query` |
| a query naming an unknown command | the element shows an error, and an `error` event carries a `LiquersError` | `uiuse07b_failed_element_reports_error_event` |
| no DOM (Node) | rejects with `NotSupported` | `uiuse08_mount_without_dom_is_refused` |
| a callback calls `ui.dispose()` | the loop exits after the tick, and no later callback runs | `uiuse08b_dispose_from_inside_a_callback` |
| a callback throws | `console.error`, and later callbacks still run | `uiuse04b_throwing_callback_does_not_stop_delivery` |
| `show(B, h)` while A is awaiting on `h` | `h` ends with B's value | `uiuse06_stale_result_is_ignored` |
| `registerCommand` after mounting | a later `show` can use the command | `uiuse09_mount_follows_rebuild` |
| `free()` on a `UIElement` handle, then passing it back | `ConversionError` naming the freed `UIElement` | `uiuse02c_freed_ui_element_is_refused` |

## Test Plan

- Native, `liquers-lib/tests/ui_runner.rs` (`DefaultEnvironment<Value, SimpleUIPayload>`, as in
  `ui_invalidation.rs`):
  - `runner_events_record_processing_then_ready` (AC-4): a pending query node yields
    `Processing`, then `Ready`, in that order, and `take_events` drains them.
  - `runner_events_record_errors_with_and_without_handle` (AC-7): a failing pending node gives
    `Error` with its handle; a failing root `SubmitQuery` gives `handle: None`.
  - `runner_drops_result_when_source_changed` (AC-6): the source is replaced between `evaluate`
    and `poll_evaluating_nodes`; the node ends with the new query's element, and the new query
    starts on the next tick without waiting for the old one.
  - `runner_replace_envref_used_by_later_evaluations` (AC-9): a command registered only in the
    second environment works after `replace_envref`.
- Node loop, `liquers-web/tests/ui_UIUSE.rs`
  (`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test ui_UIUSE`):
  - `uiuse02_ui_element_value_reaches_js_as_handle` (AC-2): evaluating
    `hello/q/ns-lui/query_console` gives a `UIElement` whose `typeName` is
    `QueryConsoleElement` and whose `renderHtml()` contains `lq-`.
  - `uiuse02b_ui_element_handle_crosses_back_as_same_arc` (AC-2): a JavaScript command that
    returns the handle it got gives Rust the same `Arc` (`Arc::ptr_eq`, through a Rust check
    command).
  - `uiuse02c_freed_ui_element_is_refused` (AC-2).
  - `uiuse07_mount_rejects_unparsable_query` and `uiuse07c_mount_refuses_query_and_spec` (AC-7).
  - `uiuse08_mount_without_dom_is_refused` (AC-8).
  - `uiuse_lui_commands_are_registered`: `ns-lui/roots` fails because no UI payload is available,
    not because the command is unknown.
- Browser, `liquers-web/tests/ui_browser_UIUSE.rs` (`--features browser-tests,debug-handles`,
  `CHROMEDRIVER=$(which chromedriver)`). Each test mounts into a fresh `<div>` and waits for ticks
  with a 50 ms timer promise.
  - `uiuse01_mount_from_query_and_spec` (AC-1).
  - `uiuse03_action_reaches_command_with_element` (AC-3): Example 2.
  - `uiuse04_status_events_in_order` and `uiuse04b_throwing_callback_does_not_stop_delivery`
    (AC-4).
  - `uiuse05_dispose_releases_callbacks` (AC-5): `live_handle_count` returns to its value before
    the mount, after `sub.dispose()` and after `ui.dispose()`.
  - `uiuse06_stale_result_is_ignored` (AC-6): A is an async page command that awaits a promise the
    test resolves after calling `show(B, h)`.
  - `uiuse07b_failed_element_reports_error_event` (AC-7).
  - `uiuse08b_dispose_from_inside_a_callback` and `uiuse08c_calls_after_dispose_reject` (AC-8).
  - `uiuse09_mount_follows_rebuild` (AC-9).
  - `uiuse10_show_adds_and_replaces` (AC-10): `show` returns a `number`, not a `bigint`.
  - `uiuse01b_object_spec_with_menu` (AC-1): a spec passed as an object, with a `button` menu item.
  - `uiuse08d_dispose_while_a_query_awaits` (AC-8): nothing is painted into the container after
    `dispose()`.
- Playwright, `liquers-web/tests/e2e/ui.spec.ts`: on a quickstart page that mounts the console,
  `mounts the console and shows a query` (AC-1, AC-10).
- Commands to run: `cargo test -p liquers-lib --lib --tests`; the two liquers-web commands above;
  `cd liquers-web/tests/e2e && npx playwright test ui.spec.ts`.

## Learning Log

- `submit` and `show` are different operations. A query sent to an element runs with it as the
  current element, and its commands place results (`lui/add`). A query *shown* in an element
  becomes that element's source. Phase 1's example confused the two.
- Pending nodes are the runner's own way to evaluate a query into an element. `mountUI({ query })`
  needs nothing else.
- On wasm, a slow query blocks the whole UI loop (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`), and that
  is also how a stale result overwrote a newer source.
