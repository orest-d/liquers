---
id: WEBUI-FIXES
kind: design
title: Invalidation tracking for retained-mode rendering
status: complete
area: [lib/ui]
gh_pr: [12]
issues: []
created: 2026-03-02
superseded_by:
---
# webui-fixes Design Tracking

**Created:** 2026-07-25


## Scope

Rendering and invalidation for the retained-mode (web) backend: make the DOM follow what changed in
the model, without destroying focus and caret.

| ID | Issue | Outcome |
|----|-------|---------|
| W3 | WEBUI-REPAINT-AFTER-SYNC-MUTATION | Per-handle invalidation; any model change reaches the DOM |
| W4 | webui async engine does not run on wasm | Already resolved by `async-wasm-refactor`; close the record |

Moved out to [`specs/design/ui-events/`](../ui-events/DESIGN.md) — they need an inbound-interaction
vocabulary, which nothing here does:

| ID | Issue |
|----|-------|
| W1 | WEBUI-QUERY-CONSOLE-ENTER-KEY-SUBMIT |
| W2 | WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED |
| W5 | Menu accelerators are egui-only (found in review) |

## Phase Status

- [x] Phase 1: High-Level Design — narrowed to W3 + W4
- [x] Phase 2: Solution & Architecture — recorded `UIChange`s, container opt-in, mutation contract
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan — 11 steps, staged
- [x] Implementation Complete — all 11 steps executed, native suite and 3 browser cases green

## Outcome

Stage 1 (steps 1–7) made rendering follow recorded model changes; stage 2 (steps 8–10) turned
inserts and removals into real DOM operations behind the `data-lq-children` opt-in. Both stages
were checked in the failing direction — each new browser case fails against the behaviour it
replaces — which is how the pre-fix measurement below was found.

## Notes

- `liquers-core` untouched; everything lives in `liquers-lib/src/ui` (`runner.rs`, `app_state.rs`,
  `web/app.rs`) plus tests and `examples-web/`. No command signatures change, so
  `register_lui_commands!`, `liquers-py` and `liquers-axum` are unaffected.
- **Measured during implementation:** `examples-web/ui_spec_demo` did not repaint at all after a
  menu click on the pre-fix build — clicking *Add Dashboard* left the DOM unchanged, because the
  inline asset manager finishes the evaluation inside the same `run()` that started it. The demo's
  Playwright test passed only because `#app` contains the menu label "Add Dashboard", which already
  satisfies a `toContainText('Dashboard')` assertion before any click. Earlier phases assumed the
  demo worked "by accident"; it did not work. Both e2e cases now count elements rather than match
  text.
- Snapshot delivery already computes a per-element `UpdateResponse::NeedsRepaint` that the runner
  discards: a ready-made invalidation source.
- W4 needs no code — `async-wasm-refactor` (2026-07-23) shipped `ImmediateAssetManager`, reduced
  wasm tokio to `["sync"]`, and proved the browser path with a green Playwright run. Only the
  `specs/archive/2026-08-08-issues.md` entry is stale.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [ui-events](../ui-events/DESIGN.md) — the interaction half (W1, W2, W5)
- [Original webui feature](../webui/DESIGN.md)
- [async-wasm-refactor](../async-wasm-refactor/DESIGN.md)
