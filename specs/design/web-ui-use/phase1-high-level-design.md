# Phase 1: High-Level Design — web-ui-use

## Design Readiness

Decision log of the pre-approved run (`proceed all`, 2026-10-30).

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds exported JavaScript classes and changes
  `WebEnvironment`'s type (rules 3 and 4); complexity `L`.
- **Leading issue:** **Proposed resolution, the page API:** `show` and `UISubscription`
  (Scope Changes, 2026-10-30).
- **Explanation:** Phases 2–4 specify a working solution under the four Phase 1 decisions. What is
  left are API refinements that Phase 2 found and the user has not yet seen.
- **Open questions:**
  1. **Proposed resolution, `submit` versus `show`:** `submit(query, handle?)` runs a query as a
     control's action does, and its commands place results. `show(query, handle?)` makes the
     query's value an element (a new root, or a replacement) and returns its handle.
  2. **Proposed resolution, subscriptions:** `on` returns a `UISubscription` with `dispose()`,
     because a returned function would be a `Closure` that leaks if never called.
  3. **Proposed resolution, stale results:** the runner drops a pending node's result when the
     node's source changed while it was evaluating. This is a fix to shared backend code, and it
     changes egui as well (only the newest query lands).
  4. **Proposed resolution, status names:** events use core's `Status` names, the same strings
     `Asset.status` reports. Progress is therefore `"processing"`; core has no `"evaluating"`.
  5. **Limitation, not blocking:** on wasm, a query awaiting a Promise freezes the UI loop, so
     progress is never painted (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`, filed 2026-10-30, needs its
     own design).

## Purpose

Let a page use the Liquers UI that already runs in the browser: mount the query console, a
`ui_spec` dashboard or any element a query produces into a DOM element, submit queries to it, follow
element status and errors, and dispose of it. This is the `UIUSE` feature of
`guides/LANGUAGE-INTEGRATION_GUIDE.md` for the Browser JavaScript profile, and the browser half of
VISION's "same query, any front end": the backend exists in `liquers-lib`, only the JavaScript
surface is missing.

## Problem Example

```js
await liquers.init();
const ui = await liquers.mountUI(document.getElementById("app"), {
  query: "hello/q/ns-lui/query_console",
});
```

**Today.** There is no `mountUI`. The backend is reachable only from a Rust crate that builds its own
environment (`liquers-lib/examples-web/ui_spec_demo`), because:

- `WebEnvironment` is `DefaultEnvironment<Value, ()>` (`liquers-web/src/environment.rs`), while
  `mount_web` and `AppRunner` require `E::Payload: UIPayload + From<SimpleUIPayload>`;
- the web builtins (`liquers-web/src/builtins.rs`) do not register the `lui` commands, so
  `await liquers.evaluate("hello/q/ns-lui/query_console")` rejects with an unknown command;
- an `ExtValue::UIElement` result has no JavaScript form (`default_value.rs` `to_js_custom`).

**Should be.**

```js
const ui = await liquers.mountUI(app, { query: "hello/q/ns-lui/query_console" });
const sub = ui.on("status", (e) => log(e.handle, e.status));   // "processing" ... "ready"
ui.show("commands_doc");          // a second root element showing the query's result
ui.elements();                    // [{ handle: 1, typeName: "QueryConsoleElement", title, status }, …]
sub.dispose();                    // no further callbacks
ui.dispose();                     // listeners detached, driver loop stopped, container emptied
```

## Scope and Acceptance Criteria

- **AC-1** Mount from a query or a spec (`UIUSE01`)
  WHEN a page calls `mountUI(container, { query })` or `mountUI(container, { spec })` after `init()`
  THEN the resulting element is rendered into `container` and the returned `UIMount` reports `running`
- **AC-2** A UI value reaches JavaScript (`UIUSE02`)
  WHEN `evaluate` produces an `ExtValue::UIElement`
  THEN the page receives a `UIElement` handle exposing `typeName`, `title` and `renderHtml()`,
  not a conversion error
- **AC-3** An action reaches the right command and element (`UIUSE03`)
  WHEN the user triggers a control whose action is a query ending in a JavaScript command
  THEN that command runs once, and its context names the element that owns the control
- **AC-4** Status arrives in order (`UIUSE04`)
  WHEN a page subscribes with `ui.on("status", cb)` and an element evaluates a query
  THEN `cb` receives that element's statuses in order, ending in `ready` or `error`, after the DOM
  for that tick is applied
- **AC-5** Unsubscribing releases callbacks (`UIUSE05`)
  WHEN the page disposes the subscription `on` returned, or calls `ui.dispose()`
  THEN no further callbacks run and the retained-handle count returns to its earlier value
- **AC-6** A stale result is ignored (`UIUSE06`)
  WHEN an element is submitted query B while its query A is still evaluating
  THEN A's late result never replaces B's in the element or the DOM
- **AC-7** Errors map through `ERROR` (`UIUSE07`)
  WHEN `mountUI` gets an unparsable query, or an element's query fails
  THEN the first rejects with a `LiquersError` carrying the core error type, and the second shows
  the error in the element and delivers it as an `error` status event
- **AC-8** Shutdown is clean (`UIUSE08`)
  WHEN `ui.dispose()` runs, including from inside one of the mount's callbacks
  THEN listeners detach, the driver loop ends, the container is emptied, and any later call on the
  mount rejects with a `LiquersError`; `mountUI` without a DOM (a Worker, Node) rejects the same way
- **AC-9** A mount follows registration
  WHEN a page registers a command after mounting a UI on the global environment
  THEN later UI queries can use that command
- **AC-10** Show a query's result
  WHEN a page calls `ui.show(query)` or `ui.show(query, handle)`
  THEN the query's value appears as a new root element, or replaces that element, and `show`
  returns the element's handle

Non-goals: egui in the browser (`eframe`'s web runner; liquers-web is built without `egui`), saving
and loading the element tree (`AppMessage::Serialize`), server-side rendering from JavaScript
(`render_app_ssr`), cancellation (inert on wasm, `WEB-CANCELLATION-INERT`), and JavaScript-defined
elements or renderers (`web-ui-define`).

## Core Interactions

- **UI**: `mount_web` gets a stoppable loop and follows a rebuild; `AppRunner` reports status changes.
- **Commands**: the web builtins register the `lui` namespace.
- **Environment**: the payload becomes a UI payload; evaluations outside a mount pass none.
- **Value types**: `ExtValue::UIElement` gets an outbound JavaScript handle.
- **Bindings**: `UIMount` and `UIElement` classes; `mountUI` on the module and on `Environment`.

## Crate Placement

`liquers-lib/src/ui/web/app.rs` and `runner.rs` for the backend changes, which every wasm app
benefits from; `liquers-web` (new `src/ui.rs`, plus `environment.rs`, `builtins.rs`,
`default_value.rs`) for the JavaScript surface. No `liquers-core` change; the dependency flow is
unchanged.

## Documentation Intent

- Reference: extend `reference/JAVASCRIPT_INTEGRATION.md` with a UI section.
- Guide: extend `guides/JAVASCRIPT_GUIDE.md` with "Show a Liquers UI".
- Other documents: the `UIUSE` row of `liquers-web/README.md`.
- Documents to update: `guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Browser JavaScript. The stale
  `reference/UI_INTERFACE_FSD.md` is filed separately (`UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`).

## Design Dependencies

- **requires (AC-3 only)** `JS-COMMAND-CANNOT-ACCESS-CONTEXT`: a JavaScript command has no context
  today, so it cannot learn which element triggered it. Every other criterion is independent.
- **overlaps** `ui-events` and `console-mounts-nested-element` (both in review, backend-internal):
  this design forwards whatever the backend does and needs neither first.
- **required-by** `web-ui-define`.

Defects found in the backend and owned by this design: `mount_web`'s `spawn_local` loop never ends,
so dropping `MountHandle` keeps the runner, the environment and the `AppState` alive; and its
listeners are not removed either, so they throw once their `Closure`s are dropped (AC-5,
AC-8). `AppRunner` keeps the `EnvRef` it was built with, so a mount outlives an environment
rebuild (AC-9). A pending node's late result overwrites a newer source (AC-6).

## Open Questions

None. Every recommendation was accepted by the maintainer on 2026-10-27, and is recorded below as a decision.

0. **Environment (standing rules, 2026-10-17 and 2026-10-19).** A mount uses the environment it was
   created from and remembers it, as other handles do. A mount made from the global handle follows a
   rebuild (AC-9), and one made from `new Environment()` stays with that instance.
1. **Payload type.** `WebEnvironment` becomes `DefaultEnvironment<Value, SimpleUIPayload>`, the type
   `mount_web` already accepts. `evaluate` passes no payload, so nothing outside a mount changes.
2. **AC-3 and the JavaScript context.** `JS-COMMAND-CANNOT-ACCESS-CONTEXT` is implemented first, and
   its context exposes `context.element`. Every other criterion can ship without waiting for it.
3. **Event delivery.** Status events are batched once per driver tick and delivered after the DOM
   is updated and the `AppState` lock is released. A callback may therefore re-enter the mount.

## Scope Changes

- **2026-10-30, Phase 2 (pre-approved).** `submit` runs a query the way a control's action does,
  and commands place their own results (`AppRunner::process_messages`). So it cannot "render its
  result", as the problem example claimed. Added `show` (AC-10), which places a query's value
  through `AppState::insert_source`, and corrected the example. `on` returns a `UISubscription`
  with `dispose()`, not a function: a function would be a `Closure` that leaks when the page never
  calls it. Size unchanged. Phases 1 and 2 updated. Recorded in the decision log as a proposed
  resolution.

## References

- `guides/LANGUAGE-INTEGRATION_GUIDE.md` §UIUSE and Appendix A `UIUSE`
- `specs/design/webui/`, `specs/design/webui-fixes/` (the backend), `specs/design/ui-events/`
- `liquers-lib/examples-web/ui_spec_demo` (the Rust-side equivalent of AC-1)
- Maintainer decisions: one web environment; ergonomic single-call APIs (2026-10-17)
