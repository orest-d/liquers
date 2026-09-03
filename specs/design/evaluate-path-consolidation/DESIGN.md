---
id: EVALUATE-PATH-CONSOLIDATION
kind: design
title: One evaluation path for every entry point
workflow: liquers-project
status: draft
phase: examples
area: [core/assets, core/plan]
gh_pr: []
issues: [CORE-EVALUATE-PATH-CONSOLIDATION, ASSETS-FIX1, INLINE-PATH-LACKS-EXECUTE-ONCE, ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED, ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY, REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED]
affects_docs: []
created: 2026-09-02
superseded_by:
---
# evaluate-path-consolidation Design Tracking

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-03)
- [x] Phase 2: Solution & Architecture (approved 2026-09-03)
- [x] Phase 3: Examples & Testing (drafted, reviewed)
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Designs the fix for `CORE-EVALUATE-PATH-CONSOLIDATION` (P1, L): `AssetRef::evaluate_and_store` and
`AssetRef::evaluate_immediately` are two independent evaluation bodies reached through four run
harnesses and six manager entry points, diverging on delegation, payload admission, status
finalization, persistence and dependency recording. Target: one body plus policy, entry points as
thin wrappers.

Discussion resolved most of Phase 1's questions before Phase 2. The "policy axes" framing was
rejected: dependency recording, status finalization, key-owner delegation and the payload
precondition are invariants of the one body; persistence is *derived* from a reproducibility
predicate (no payload, no supplied initial state, not volatile) rather than switchable; only
queued-vs-inline and queue characteristics are genuine manager policy, the latter out of scope.
The same predicate governs map reuse, loadable persistence, and eligibility to be a dependency —
which is why `Context::apply` records no edge. The unified body therefore takes only the asset.

All Phase 1 questions resolved on 2026-09-03: non-keyed (query) assets are not stored, so the
write predicate is the existing `AssetRef::bound_owner_key()`; the harness mapping is settled in
Phase 1's method-mapping tables (3 evaluation bodies to 1, 4 run entry points to 2, 6 manager
evaluation entry points to 4); `INLINE-PATH-LACKS-EXECUTE-ONCE` is co-delivered with this work.

Added requirement (2026-09-03): an asset must record that its evaluation depended on a payload,
and expose it in metadata and `AssetInfo`. The fields exist but nothing sets them — filed as
`ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` and scheduled here; the requirement is resolved before
evaluation, symmetrically with volatility.

Phase 2 settles that decision: `apply` evaluates inline on both managers, extending the rationale
already written for payload dependencies. Phase 2 also corrects Phase 1 on two points — the write
predicate is a recorded `store_target`, not `bound_owner_key` (which returns `None` for volatile
keyed assets, which must keep writing), and the payload requirement lives in metadata only, not in
a duplicated `AssetData` field.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
