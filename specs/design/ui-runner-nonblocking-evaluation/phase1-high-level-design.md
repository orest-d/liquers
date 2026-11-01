# Phase 1: High-Level Design — ui-runner-nonblocking-evaluation

## Purpose

A Liquers UI in the browser stops while any of its queries waits. It cannot paint progress, take
clicks or show other elements' results until the slow query finishes
(`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`). This design lets the UI runner start an evaluation and keep
going, so a page built on `mountUI` stays responsive. That is the interactive data application of
`VISION.md`, running in the browser.

## Problem Example

A page registers a command that fetches data. It shows the command in a mount, next to a console:

```js
liquers.registerCommand({ name: "slow", async: true,
  run: async () => { await new Promise((r) => setTimeout(r, 3000)); return "done"; } });
const ui = await liquers.mountUI(app, { query: "hello/q/ns-lui/query_console" });
ui.show("slow");
```

**Today:**
- For three seconds the page shows nothing new. The progress element that the runner sets for
  `slow` is never painted.
- A click on the console is not processed.
- `ui.show("hello")` places its node, but `hello` is evaluated only after `slow` finishes.
- When `slow` resolves, everything lands at once.

**Wanted:** the progress element appears on the next tick, the console keeps working, `hello`
shows at once, and `done` replaces the progress element when it arrives.

Why it happens: on wasm the environment uses `ImmediateAssetManager`, which runs every evaluation
in the caller's task. `AppRunner::run` awaits each evaluation it starts, and the driver loop awaits
`run`.

## Scope and Acceptance Criteria

- **AC-1** Progress is painted while a query waits
  WHEN a node's query awaits a JavaScript Promise
  THEN the node's progress element is in the page before the Promise settles, and the result
  replaces it after.
- **AC-2** Other work continues
  WHEN one query waits
  THEN messages (clicks, `submit`, `show`, `dispatchEvent`) are processed, and other nodes are
  evaluated and painted, on the following ticks.
- **AC-3** Only the newest evaluation lands
  WHEN a node's source changes while its first evaluation is in flight
  THEN the first result is dropped and the node shows the second (today's stale-result rule,
  which still holds when the first evaluation finishes last).
- **AC-4** Stopping does not wait
  WHEN a mount is disposed while a query is in flight
  THEN `dispose` returns at once, nothing is painted afterwards, and the late result is dropped.
- **AC-5** Status order is unchanged
  WHEN a query is shown
  THEN its status events are still `processing`, then `ready` or `error`, delivered after the
  page update (the `web-ui-use` contract).
- **AC-6** Native behaviour is unchanged
  WHEN the runner drives an egui or native UI with `DefaultAssetManager`
  THEN results land, statuses arrive in order and the stale-result rule holds, as before, with no
  new thread or runtime requirement (`tests/ui_runner.rs`).

Non-goals:
- Making the `ImmediateAssetManager` itself spawn tasks: it stays spawn-free.
- Running evaluations in a Web Worker.
- Real cancellation of an abandoned evaluation (`WEB-CANCELLATION-INERT`): a dropped result is
  still computed to the end.

## Core Interactions

- **UI:** `AppRunner` (`liquers-lib/src/ui/runner.rs`): how `SubmitQuery`, pending nodes,
  `ApplyToInput` and `RequestAssetUpdates` start their evaluations, and when results land.
- **Assets:** no change. The runner still calls `EnvRef::evaluate`, `evaluate_immediately` and
  `apply`.
- **Bindings:** `liquers-web`'s mounts benefit without an API change.

## Crate Placement

All of it is in `liquers-lib` (`ui/runner.rs`, and `spawn_ui_task` in `ui/mod.rs`), because the
runner is where the waiting happens. `liquers-core` is untouched.

**`spawn_ui_task` on wasm:** it is `wasm_bindgen_futures::spawn_local`, a task on the page's own
thread. It is not a worker. The task runs when the current task yields, for example at an awaited
Promise, so the browser's event loop and the driver's 16 ms ticks keep running while it waits.
Nothing runs in parallel, so `AppState`, the runner and the asset manager need no new
synchronization. They must only not hold a lock across an await, which they already don't. On
native, it is `tokio::spawn`.

## Documentation Intent

- Reference: extend `specs/reference/JAVASCRIPT_INTEGRATION.md` §10. Remove the freeze from its
  limits, and say that evaluations run as tasks on the page's thread.
- Guide: `specs/guides/JAVASCRIPT_GUIDE.md` §11 and §13. Drop the freeze warning.
- Other documents: `liquers-web/README.md` (the `UIUSE` row note);
  `specs/reference/UI_INTERFACE_FSD.md` is not edited (being rewritten under
  `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API`).
- Documents to update: the above, plus `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` → closed.

## Design Dependencies

- Builds on `web-ui-use` (complete): the runner's `evaluating` map, the stale-result check, the
  event outbox and the stoppable mount.
- Overlaps `ui-asset-snapshot-boundary` (in review), which changes what
  `poll_evaluating_nodes` does with a result once it lands. This design changes how the result
  arrives. The two compose; whichever lands second adjusts one function.

## Open Questions

1. **Spawn in the runner, or make the manager spawn?** Recommended: the runner. It spawns each
   evaluation with `spawn_ui_task` and collects results through a channel that it drains every
   tick. The alternative, an `ImmediateAssetManager` that spawns on wasm, would also change
   `liquers.evaluate` and every inline caller, and would break the manager's documented
   "spawn-free" contract.
2. **What does `needs_repaint` mean afterwards?** Recommended: true while any evaluation is in
   flight, as today. A native driver keeps polling, and the browser driver ticks anyway.
3. **Do concurrent evaluations of the same query share one run?** On `ImmediateAssetManager` two
   tasks requesting one key or query may both compute it. Recommended: accept that for now, and
   check it in Phase 2 against the manager's asset map. Dedup is the manager's job, not the
   runner's.
4. **Same path on native?** Recommended: yes, so the native tests exercise it. A test that expects
   a result after a single `run` then needs `run_until_idle`, the helper `web-ui-use` added.

## References

- `specs/issues/WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS.md`
- `specs/design/web-ui-use/` (stale-result check, event outbox, `MountHandle::stop`)
- `specs/design/ui-asset-snapshot-boundary/`
- `liquers-core/src/assets.rs` `ImmediateAssetManager` ("Every evaluation runs in the caller's task")
- `liquers-lib/src/ui/mod.rs` `spawn_ui_task`
