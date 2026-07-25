# webui-fixes Design Tracking

**Created:** 2026-07-25

**Status:** Phase 1 (re-derived) — awaiting approval

## Scope

Retained-mode support in the `ui` module: inbound interaction, a declared interaction surface, and
invalidation. The open `webui` issues from `specs/ISSUES.md` are the acceptance criteria:

| ID | Issue | Gap it demonstrates |
|----|-------|---------------------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT | Intent in: a gesture with no reachable action is dropped |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED | Value in: user-entered state never reaches the element that owns it |
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | Change out: the backend is not told the model changed |
| W5 | Menu accelerators are egui-only (found in review) | Intent in: shortcut matching lives inside `show_in_egui` |
| W4 | webui async engine does not run on wasm | None — already resolved by `async-wasm-refactor`; close the record |

## Phase Status

- [ ] Phase 1: High-Level Design — revised after review: demonstrating flows, the common
      denominator, an explicit answer on `ui`-module design work, and the six review decisions;
      awaiting approval
- [ ] Phase 2: Solution & Architecture — *draft only* (superseded, see note)
- [ ] Phase 3: Examples & Testing — *draft only* (superseded, see note)
- [ ] Phase 4: Implementation Plan — *draft only* (superseded, see note)
- [ ] Implementation Complete

> **Note on the drafts.** Phases 2–4 were produced in a single ungated pass before Phase 1 was
> reviewed, so they encode one particular answer to Phase 1's open questions: three narrow point
> fixes rather than a general interaction contract. Review decision 1 rejected that answer, so they
> are now reference material only and will be re-derived once Phase 1 is approved.

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
