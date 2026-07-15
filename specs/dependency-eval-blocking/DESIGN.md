# dependency-eval-blocking Design Tracking

**Created:** 2026-07-15

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (anchor, reconstructed from plan20260707 WP-1 Phase 2)
- [~] Phase 2: Solution & Architecture (drafted, awaiting user approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

- Feature = remaining work of `plan20260707.md` WP-1 "Phase 2 — scheduler and delegation
  rework": replace inline blocking recipe delegation (`assets.rs:1447-1455`) with a
  suspend/resume model that frees the parent's job-queue slot.
- Already done on `main` (out of scope): WP-1 Phase 1 dependency recording; WP-1 Phase 2C
  event-driven `JobQueue` (`Notify`, `with_capacity`, `shutdown`).
- In scope: WP-1 Phase 2A (non-blocking delegation, `EvaluationOutcome`), 2B (`waiting_on`),
  2D (re-entry / duplicate-submit rules via wait-generation counter).
- All changes confined to `liquers-core/src/assets.rs`.
- rust-best-practices skill not installed in this environment; Rust-idiom checks applied
  manually.

### Phase 2 multi-agent review (completed)
- Reviewer A (Phase 1 conformity): conformant; flagged the `evaluate_recipe` shim tension →
  shim removed (only caller is `evaluate_and_store`).
- Reviewer B (codebase alignment): confirmed most claims; fixed 5 items — `WeakAssetRef`
  already exists (reuse, real shape carries `id`), use `manager.job_queue.submit` (no
  `manager.submit`), `subscribe_to_notifications` on `AssetRef` is async, use `is_finished()`
  (covers Error+Cancelled), watch `changed()` does not replay current value (pre-check guards
  the race). No blocking issues.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
