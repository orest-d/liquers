# webui-fixes Design Tracking

**Created:** 2026-07-25

**Status:** Phase 1 (narrowed to rendering/invalidation) — awaiting approval

## Scope

Rendering and invalidation for the retained-mode (web) backend: make the DOM follow what changed in
the model, without destroying focus and caret.

| ID | Issue | Outcome |
|----|-------|---------|
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | Per-handle invalidation; any model change reaches the DOM |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor`; close the record |

Moved out to [`specs/ui-events/`](../ui-events/DESIGN.md) — they need an inbound-interaction
vocabulary, which nothing here does:

| ID | Issue |
|----|-------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED |
| W5 | Menu accelerators are egui-only (found in review) |

## Phase Status

- [ ] Phase 1: High-Level Design — narrowed to W3 + W4; awaiting approval
- [ ] Phase 2: Solution & Architecture — *draft only* (superseded, see note)
- [ ] Phase 3: Examples & Testing — *draft only* (superseded, see note)
- [ ] Phase 4: Implementation Plan — *draft only* (superseded, see note)
- [ ] Implementation Complete

> **Note on the drafts.** Phases 2–4 were produced in a single ungated pass before Phase 1 was
> reviewed. They cover the old, wider scope and encode the point-fix answer that review rejected —
> including a global dirty flag where the decision is now per-handle invalidation. Reference only;
> they will be re-derived for the narrowed scope once Phase 1 is approved.

## Notes

- `liquers-core` untouched; everything lives in `liquers-lib/src/ui` (`runner.rs`, `app_state.rs`,
  `web/app.rs`) plus tests and `examples-web/`. No command signatures change, so
  `register_lui_commands!`, `liquers-py` and `liquers-axum` are unaffected.
- `examples-web/ui_spec_demo` repaints today only because its one menu action creates a *pending*
  node, leaving an evaluation in flight for a tick. An action resolving fully inline would not
  repaint — this feature is what makes the demo work by construction rather than by luck.
- Snapshot delivery already computes a per-element `UpdateResponse::NeedsRepaint` that the runner
  discards: a ready-made invalidation source.
- W4 needs no code — `async-wasm-refactor` (2026-07-23) shipped `ImmediateAssetManager`, reduced
  wasm tokio to `["sync"]`, and proved the browser path with a green Playwright run. Only the
  `specs/ISSUES.md` entry is stale.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [ui-events](../ui-events/DESIGN.md) — the interaction half (W1, W2, W5)
- [Original webui feature](../webui/DESIGN.md)
- [async-wasm-refactor](../async-wasm-refactor/DESIGN.md)
