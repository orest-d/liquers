---
id: STALE-DEPENDENCY-STATUS-FINALIZATION
kind: design
title: Status is finalized before persistence for a stale-dependency evaluation
workflow: liquers-project
status: draft
phase: high-level
area: [core/assets]
gh_pr: []
issues: [ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY]
affects_docs: [ASSET_LIFECYCLE, ASSETS, DOC_03_ASSETS_EXECUTION_LIFECYCLE]
created: 2026-09-04
superseded_by:
---
# stale-dependency-status-finalization Design Tracking

**Created:** 2026-09-04

## Phase Status

- [ ] Phase 1: High-Level Design (drafted 2026-09-04, awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Designs the fix for `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` (P2, M). The defect was found
during Phase 3 of `evaluate-path-consolidation`, which states the ordering invariant, makes the
violation visible, and explicitly scoped the fix out (its Phase 5 §"What was omitted"). That design
is finished and its PR merged, so the remainder is this folder rather than a reopening.

**Verified live at HEAD before drafting Phase 1.** `evaluate` (`assets.rs:2528`) finalizes status
at `:2553` and persists at `:2572`; the stale-dependency rule runs in `finish_run_with_result`
(`:2251`), after both, on both harnesses, with no save afterwards. The issue's own file:line
citations predate the consolidation and are all stale — correcting them is part of this work.

Two facts widen the issue as written and are carried into Phase 1's open questions: moving the rule
before `evaluate`'s step 8 would stop `DependencyManager::track_asset` registering the asset (it
early-returns for `Expired`), and the relabel bypasses `expire()`/`mark_expired_status`, which
already persists `Expired` for a keyed asset, notifies, and cascades to dependents.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
