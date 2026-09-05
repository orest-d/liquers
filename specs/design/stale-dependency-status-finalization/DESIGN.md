---
id: STALE-DEPENDENCY-STATUS-FINALIZATION
kind: design
title: Status is finalized before persistence for a stale-dependency evaluation
workflow: liquers-project
status: draft
phase: architecture
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
- [ ] Phase 4: Implementation Plan (drafted, **not approvable** — returned to Phase 2)
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

## Phase 4 review: three blocking findings, design returned to Phase 2 (2026-09-04)

Two reviewers passed the plan clean. The holistic pass over all four documents did not, and its
central finding is verified against source:

**B1 — finalizing `Expired` before persistence stops the value being written at all.**
`evaluate` installs `lock.data` and never `lock.binary`, so `save_to_store`'s `binary_unchecked()`
returns `None` and it falls through to `serialize_to_binary` (`assets.rs:2718`) — which calls
`self.poll_state()`, and `poll_state` returns `None` for `Expired` (`:1199`, `metadata.rs:368`).
So the write fails with "Failed to obtain binary value for storing of the asset" and **nothing
reaches the store**. The comment already in the source at `assets.rs:2552` states the constraint
outright: *"Must happen before persistence so poll_state() returns Some for serialization."*

Phase 3's "Verified Setup Facts" asserted the opposite — that `save_to_store` has no status gate —
and Phase 3 pitfall P4 repeated it, telling implementers not to worry about this exact thing. That
verification checked `save_to_store` and stopped one call short of `serialize_to_binary`. It was
written into a table labelled binding, which is the worst place for a wrong fact.

The fix is principled and already has two precedents in this repository: `serialize_to_binary`
needs the ungated read (`poll_state_any_status`), for the same reason `save_to_store` already uses
`binary_unchecked` — *persisting is not a read of the exposed value*. It is the same defect class as
`ASSET-EXPIRED-CACHED-BINARY-READ` and `DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE`: a gate was
added to a read and an internal caller that needed the ungated one was not moved across. But it
changes the persistence path, so it is a Phase 2 decision, not a Phase 4 detail.

**B2 — Step 3 uses the wrong key derivation, and the delegated case falls in the gap.**
`track_asset` uses `bound_owner_key()` (ownership-aware, `None` for a keyed non-owner);
Phase 4 Step 3 uses `lock.key` (ownership-blind by design). A *delegating* asset can carry
`stale_dependency` — delegation goes through `wait_for_dependency`, the same call that sets it —
and `evaluate` skips persistence for `delegated` but runs the DM step unconditionally. So the new
branch would cascade on a key the asset does not own, tearing down the real owner's version and
edges. Today that asset reaches `track_asset`, gets `None`, and does nothing.

**B3 — the cascade is not "`track_asset`'s invalidation without the version registration".**
That description is what the Phase 2 gate decision was approved on, and it is wrong three ways.
`track_asset` also calls `load_from_records`, registering *incoming* edges, which the cascade drops.
And `expire_internal` (`dependencies.rs:596-635`) skips the cascade when the stored version
`is_unknown()` — `Version(0)`, which is what the evaluate path registers — while removing
`keyed_dependents[K]` **regardless**. So for a previously-tracked key the dependents are *not*
expired and *lose their edges*: a new invalidation hole rather than the preservation of one. For a
never-tracked key it expires every transitive dependent. The branch is both broader and narrower
than today depending on history, which no phase noticed. It also removes `versions[K]`, which makes
`try_fast_track`'s version guard vacuous in-process — the very mechanism the P1 raise rests on.

Also raised, for the owner: staleness now propagates transitively within a run (a parent polling a
stale-dependency child sees `Expired` instead of a brief `Ready`), and the issue's "or not written
at all" option needs deciding explicitly given B1.

Filed separately from this design: `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` and
`SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED`.

## Where this stands, and how to resume (2026-09-05)

**Blocked, deliberately, on `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS` (P1, L).** This
folder is published as preparatory design work so the reasoning survives the gap; it is not ready to
implement.

### Why it is blocked

The Phase 4 review established that computed keyed assets never carry a content version, which makes
keyed-to-keyed expiry propagation inert. The project owner has decided that is an old omission to be
fixed — computed assets should take their version from the hash of their serialized bytes, with a
timestamp fallback — and that it is separate work with its own design. This design waits because its
own dependency-manager decision (C2: leave `track_asset` alone) is *correct given today's
versionless behaviour* and is worth revisiting once versions are real: `register_version` would then
see a genuine change and cascade, which is the behaviour Phase 2 originally wanted and could not get.

### State of each phase

| Phase | State |
|---|---|
| 1 High-level | Approved 2026-09-04. Unaffected by the review; no correction owed |
| 2 Architecture | Approved 2026-09-04, then **invalidated in three places**. Not edited in place; the corrections are listed in `phase2-architecture.md` §"Corrections owed before this phase is re-approved" |
| 3 Examples | Approved 2026-09-04. Two entries are marked wrong in place — the `save_to_store` row of "Verified Setup Facts", and pitfall P4 — because a wrong fact in a table labelled binding is worth leaving visible. The rest stands, including the cross-process test design and the `SharedMemoryStore` pattern |
| 4 Implementation | Drafted, **not approvable**. Steps 1, 2 and 4–8 survive the corrections; Step 3 is deleted by C2 and Step 2 gains the `serialize_to_binary` change from C1 |

### Resuming

1. Land `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`.
2. Apply C1–C5 to `phase2-architecture.md` and take Phase 2 back through its gate. Reconsider C2
   in the light of real versions — that is the one decision the versions work may change.
3. Re-run the Phase 3 tests against the corrected architecture; the `I2` store-status assertion is
   the one that could not have passed before C1.
4. Rebuild Phase 4 Step 2 and drop Step 3.

### Issues this work produced

| Issue | P | Relationship |
|---|---|---|
| `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` | P1 | The one being fixed; raised from P2 on cross-process evidence found here |
| `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS` | P1 | **The blocker.** Carries the owner's decision on computed-asset versions |
| `EXPIRY-RECORDS-NO-REASON` | P2 | Requested by the owner; every route into `Expired` is silent |
| `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` | P2 | The C1 defect, filed independently since it predates this design |
| `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` | P2 | Adjacent, found while tracing the persistence path |
| `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` | P2 | Adjacent, same trace |
| `DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS` | P2 | Unrelated; found while regenerating the index. Half fixed upstream since |
