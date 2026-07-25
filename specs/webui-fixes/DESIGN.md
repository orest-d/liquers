# webui-fixes Design Tracking

**Created:** 2026-07-25

**Status:** Design complete — awaiting approval to implement

## Scope

The open `webui` issues from `specs/ISSUES.md`:

| ID | Issue | Plan |
|----|-------|------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | `data-lq-action` on the query `<input>` + suppress click-dispatch on text entry |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | `AssetSnapshot.query` + `QueryConsoleElement::adopt_query` |
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | `AppRunner::take_repaint_request()` consumed by the render loops |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor`; close the record |

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

- No `liquers-core` changes; everything lives in `liquers-lib/src/ui` plus tests and the browser
  example. `register_lui_commands!` and therefore `liquers-py` / `liquers-axum` are unaffected.
- `lui/submit` deliberately stays a synchronous command: routing the submitted query through the
  asset snapshot avoids downcasting `dyn UIElement` and avoids making the command async.
- W4 needs no code — `async-wasm-refactor` (2026-07-23) shipped `ImmediateAssetManager`, reduced
  wasm tokio to `["sync"]`, and proved the browser path with a green Playwright run. Only the
  `specs/ISSUES.md` entry is stale.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Original webui feature](../webui/DESIGN.md)
- [async-wasm-refactor](../async-wasm-refactor/DESIGN.md)
