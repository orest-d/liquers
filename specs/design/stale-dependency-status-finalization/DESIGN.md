---
id: STALE-DEPENDENCY-STATUS-FINALIZATION
kind: design
title: Status is finalized before persistence for a stale-dependency evaluation
workflow: liquers-project
status: draft
phase: implementation
area: [core/assets]
gh_pr: []
issues: [ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY, EXPIRY-RECORDS-NO-REASON]
affects_docs: [ASSET_LIFECYCLE, ASSETS, DOC_03_ASSETS_EXECUTION_LIFECYCLE]
created: 2026-09-04
superseded_by:
---
# stale-dependency-status-finalization Design Tracking

**Created:** 2026-09-04

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-04)
- [x] Phase 2: Solution & Architecture (approved 2026-09-04)
- [x] Phase 3: Examples & Testing (approved 2026-09-04)
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

**Phase 2 drafted 2026-09-04.** Settles all five Phase 1 questions: the rule moves into the status
authority (renamed `try_to_set_ready` → `finalize_status`) so it is decided under the same write
lock, before persistence; volatility keeps precedence over the stale-dependency label; the
`expire()`/`mark_expired_status` route is rejected because it writes metadata only for a key the
store already has, which at finalization time it does not. The one judgement call is the
dependency-manager branch: rather than let `track_asset` silently early-return for `Expired`, a
stale-dependency **keyed** asset calls `cascade_expire_dependents`, keeping the dependent
invalidation `register_version` performs today without advertising an uncacheable value as the
key's current version.

Phase 2 also verified an exposure the issue does not record: `try_fast_track` skips its recorded
dependency-version check when the dependency manager holds no version for that key, which is every
key in a fresh process. A stale-dependency asset stored as `Ready` is therefore **served without
recomputation after a restart**. Recommended (not applied) priority change P2 → P1.

**Phase 2 review (2 reviewers).** Reviewer A (Phase 1 conformity) — no blocking findings; the
documentation plan matches Phase 1 exactly and every Phase 1 question is answered, but open
question 5 was only listed as evidence to collect rather than confirmed. Fixed: Phase 2 now carries
§"The `expired-binary-read-safety` regression is preserved, and stops being racy", which shows the
position is not weakened and that deciding the status before the notification closes the window B1
called racy. Reviewer B (codebase alignment) — every file:line claim verified against HEAD with no
correction; no consumer outside `liquers-core` breaks (`liquers-axum`'s query handlers already have
explicit `Status::Expired` arms at `handlers.rs:113` and `:261`); no existing test asserts the
buggy behaviour, so none breaks. No fixer agent was needed for a single advisory.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Phase 2 gate decisions (2026-09-04)

- **DM branch: `cascade_expire_dependents`** — confirmed by the project owner. A stale-dependency
  keyed asset invalidates its dependents instead of registering an uncacheable value as the key's
  current version.
- **Diagnostics** — the owner asked that an expiry record its reason ("expired due to dependency X
  expiring while evaluating Y"), and that the requirement be treated as general. Filed as
  `EXPIRY-RECORDS-NO-REASON` (P2, S): `mark_expired_status` logs nothing on any route, so a cascade
  expires assets silently, and `note_expired_dependency` names the dependency by its runtime asset
  id. Not absorbed into this design — it spans four expiry routes and carries an `info`/`warning`
  choice that is not this design's to make.
- **Priority raised P2 → P1** on `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` (owner), and applied
  to the issue with the cross-process reasoning that earned it.
- **Rename confirmed** (owner): `try_to_set_ready` → `finalize_status`.

**Phase 2 approved 2026-09-04.** All four gate questions resolved; no open decision carried into
Phase 3.

## Phase 3 review (2026-09-04)

Three drafting agents, then two reviewers.

**Drafting produced more corrections than content.** The pitfalls table survived nearly whole; most
drafted test code did not. Verification against the source found nine wrong assumptions, recorded
as Phase 3's binding "Verified Setup Facts" table. Two mattered: `set_value` already persists, so a
draft testing "the reason is recorded before persistence" had persisted in its own setup; and
`AsyncMemoryStore` is not shareable by cloning, so the cross-process test — the fix's whole payoff —
could not be built the drafted way. A draft claimed that pattern was "verified feasible against
existing tests"; the test it cited builds one environment.

**Reviewer 1 (Phase 1/2 conformity)** found two approved Phase 2 decisions with no test. Decision
(d), `expiration_time` parity, was a real gap — U7 added. Decision (g), that no `Expired`
notification is sent, gets no test and Phase 3 now explains why: the channel is
`tokio::sync::watch`, which retains only the latest value, so an absence assertion would pass on a
broken implementation. Also cross-indexed all ten pitfalls to tests.

**Reviewer 2 (codebase alignment)** verified all nine corrections against source with no
correction of its own, and supplied two facts that make Phase 4 executable: `AsyncStore` has only
two required methods (so the shared-store wrapper is two forwarding bodies), and the mid-evaluation
expiry gate at `expiration_integration.rs:749` achieves its timing with a bounded poll until the
child is `Ready`, not a sleep.
