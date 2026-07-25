# Phase 1: High-Level Design - webui-fixes

## Feature Name

webui-fixes — browser-interaction correctness for the `webui` backend

## Purpose

Close the four open `webui` items in `specs/ISSUES.md` so the browser backend behaves like the
egui reference backend: Enter submits in the query console, the console keeps the query the user
actually submitted, and any state change repaints the DOM. The fourth item (wasm async engine)
is already fixed by `async-wasm-refactor` and only needs its issue record closed.

## Scope (issues addressed)

| ID | Issue | Outcome |
|----|-------|---------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Fix: Enter on the query input dispatches `Apply` |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | Fix: the submitted query becomes the console's state |
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | Fix: repaint after any state-changing `AppRunner::run` |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor`; close record |

## Core Interactions

### Query System

No change. Queries remain opaque strings carried by `UiAction` / `AppMessage`.

### Store System

Not involved.

### Command System

`lui/submit` keeps its signature and stays synchronous. Its behaviour becomes correct because the
console learns the submitted query from the asset snapshot it monitors, not from the command.

### Asset System

`AssetSnapshot` gains the query that produced the monitored asset, so an element can reconcile its
displayed query with what `AppRunner` actually monitors. `AssetManager` and the asset lifecycle
are untouched.

### Value Types

None.

### Web/API

No HTTP surface. `QueryConsoleElement::render_web` markup and the delegated DOM listener in
`ui/web/app.rs` change (W1); the browser render loop gains a dirty-driven repaint (W3).

### UI

`QueryConsoleElement` (state sync), `AppRunner` (repaint signal), web driver (event dispatch).
egui behaviour is unchanged; egui apps may additionally consume the new repaint signal.

## Crate Placement

All production changes live in `liquers-lib`: `src/ui/message.rs`, `src/ui/runner.rs`,
`src/ui/web/app.rs`, `src/ui/widgets/query_console_element.rs`; tests in `liquers-lib/tests/` and
`liquers-lib/examples-web/`. `liquers-core` is untouched, which keeps the fixes out of the
Python/axum blast radius.

## Open Questions

1. W2: reconcile through the snapshot, or mutate the console from `lui/submit` (which would need
   element downcasting or an async command)? — resolved in Phase 2 in favour of the snapshot.
2. W1: suppress click-dispatch on text inputs, or move the action to a toolbar ancestor? —
   resolved in Phase 2 (suppression; an ancestor fires on stray toolbar clicks).
3. Should the browser demo grow a query console so W1/W2 get Playwright coverage? — yes
   (Phase 3/4); otherwise both fixes are only unit-testable.

## References

- `specs/ISSUES.md` (W1–W4 records)
- `specs/webui/` (Phases 1–4 of the original webui feature)
- `specs/async-wasm-refactor/DESIGN.md` (W4 resolution evidence)
- PR #10 review comments (source of W1–W3)
