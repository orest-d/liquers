---
id: CASCADE-MARKS-IN-FLIGHT-DEPENDENT-STALE
kind: design
title: An expiry cascade that reaches a running dependent marks it stale instead of being dropped
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED]
created: 2026-10-26
---
# An expiry cascade that reaches a running dependent marks it stale instead of being dropped

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

When a dependency changes or expires, `AssetManager::expire_dependencies_result` expires every
dependent the graph reports. A dependent still evaluating refuses (`mark_expired_status` returns
`StatusConflict` for in-flight statuses) and the refusal is ignored, so the dependent finishes
`Ready` on an input the system already knows is out of date, and stays so in memory and in the
store until that input changes again. The cascade must leave a mark the run honours when it
finishes.

### Problem Example

Verified by reading HEAD (`cd6d5da`): `expire_dependencies_result` → `expire_without_cascade` →
`mark_expired_status`, result discarded with `let _ =`.

`runtime-dependency-record-superseded-check` (ready, not implemented) will catch the case where the
dependency's *version* changed (its completion check compares records with the version map). It
cannot catch a cascade that changes no version: an explicit `expire`, a deadline, `remove`, or a
**transitive** cascade, where the intermediate node expires but keeps its version.

Keys, queued manager (validated: `liquers-validate --command upper --command gated_upper --
'-R/data/c.txt/-/upper' '-R/data/q.txt/-/gated_upper'`: both `Ok`):

- `data/c.txt` a `Source` holding `a`;
- `data/q.txt` = `-R/data/c.txt/-/upper` (registered, `Ready` with `A`);
- `data/p.txt` = `-R/data/q.txt/-/gated_upper`; `gated_upper` reads its input, then waits on a gate.

1. `get(data/p.txt)` reads `q = A` and waits; edges `c → q → p` exist;
2. `set_state(data/c.txt, "b")`: `c` gets a new version; the cascade expires `q` (`Cascaded`,
   version unchanged) and reaches `p`, which is `Processing`: refused, dropped;
3. the gate opens; `p` finishes.

Today: `p` is `Ready` with `AA` in memory and in the store, and `get(data/p.txt)` serves it; its
record `q @ v(q)` matches the map, so neither the runtime check nor the fast track sees anything.
Should be: `p` takes the stale-dependency route — `Expired` with
`Direct { StaleDependency { dependency: -R/data/q.txt } }` — and the next `get` recomputes `BB`.

### Scope and Acceptance Criteria

- **AC-1** Transitive cascade to a running dependent
  WHEN a cascade reaches a keyed dependent that is `Dependencies`, `Processing` or `Partial` through a dependency its run has recorded
  THEN the dependent finishes `Expired` with `Direct { StaleDependency { dependency } }` naming that dependency, in memory and in the store
- **AC-2** Version-less direct cascade
  WHEN an explicit `expire` of a dependency the running dependent has read cascades to it
  THEN the same as AC-1
- **AC-3** A dependency not read by the run does not mark it
  WHEN the cascade arrives through a dependency the run never records (an edge left by a previous run, or one it schedules only after the cascade)
  THEN the dependent finishes `Ready`
- **AC-4** Not started yet
  WHEN the cascade reaches a dependent in `None`, `Recipe` or `Submitted`
  THEN nothing is marked, and it reads the current inputs when it runs (finishes `Ready`)
- **AC-5** Finished dependents unchanged
  WHEN the cascade reaches a `Ready` or `Override` dependent
  THEN it is expired with a `Cascaded` reason, as today
- **AC-6** Non-keyed running dependents
  WHEN the cascade reaches a running query asset through `expired.assets`
  THEN AC-1 holds for it
- **AC-7** Both managers
  WHEN AC-1 runs under `ImmediateEnvironment` (the cascade issued from another task)
  THEN the same outcome

Non-goals: re-running the dependent with the new input (the stale-dependency policy deliberately
never recomputes mid-run); what a top-level `get()` waiter receives from a stale-route completion
(decided by `stale-dependency-status-finalization`, unchanged); `Storing` (not an in-flight status,
still refused as today).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — rule 5 (one open behaviour decision); rule 3 would also need
  judgement (a private field on `AssetData`).
- **Leading issue:** **Open design question - outcome of a running dependent the cascade reached.**
  Recommended: the stale-dependency route (`Expired` at birth), the documented outcome for "this
  evaluation consumed an input that was out of date" (`ASSETS.md` §Expiry is uniform). Alternative:
  finish `Ready`, then expire with `Cascaded` right after. That gives a `get()` waiter the value
  more often, but the window between the two is a read of a value the system knows is stale, the
  stored copy is written `Ready` first, and it is a second policy for the same situation.
- **Explanation:** the existing stale route already has every piece (a flag read at finalization,
  the `Expired` status and reason, persistence, and registration of the new version); only the
  cascade's side of it is missing.
- **Open questions:**
  1. **Open design question - outcome** (above). Recommended: the stale route.
  2. **Proposed resolution - honour the mark only for a recorded dependency.** The cascade's `via`
     key is kept on the asset and honoured at finalization only if the run's dependency records
     name it (AC-3). Marking unconditionally is simpler but over-expires a run that had not read
     the input yet.

### Design Dependencies

- `stale-dependency-status-finalization`, `dependency-edge-superseded-version` (complete, E4): reused.
- `runtime-dependency-record-superseded-check` — **overlaps** (T4 partial, finished Phase 4, not
  edited): covers the direct, version-changing case of this issue at completion; this design covers
  the version-less and transitive cases. Both set `stale_dependency` (first wins); no order.
- `uncached-write-back-rechecks-dependency-versions` — **overlaps**: the `cached: false` analogue
  (no live asset to mark); edits `persist_with_status_tracking`, not `expire_dependencies_result`.
- `keyed-run-replaces-dependency-edges` — **overlaps** (weak): it removes a key's old incoming edges
  at run start, which shrinks the edges a cascade can arrive through; AC-3 holds either way.

### Consolidated Findings

- The mark must be set under the dependent's `data` write lock together with the in-flight test,
  and finalization reads it under the same lock (`finalize_status_with_version`): either the
  cascade sees the asset in flight and the run sees the mark, or the cascade sees it finished and
  expires it normally. No window.
- Records are merged into the metadata (`merge_dependencies` in `complete_evaluation`) before
  `finalize_status_with_version`, so finalization can test "was `via` recorded" without new plumbing.
- Inherited records (a non-keyed dependency's own records, `Context::wait_for_dependency_recording`)
  make an attribution edge `x → k` recorded too, so AC-1 holds for expression-mediated edges.
- `expire_without_cascade` is a documented contract for external managers and keeps its behaviour;
  the cascade uses a new crate-private method.

## Phase 2: Architecture

### Solution

1. `AssetData` (`liquers-core/src/assets.rs`) gains a private field
   `cascaded_through: Vec<DependencyKey>` (initialised empty in `new_ext`), next to
   `stale_dependency`.
2. New crate-private method on `AssetRef`:

   ```rust
   /// Expire this asset for a cascade that reached it through `via`. A running asset (in
   /// `Dependencies`, `Processing` or `Partial`) is not expired: `via` is kept, and the run takes
   /// the stale-dependency route at finalization if it recorded `via`. A not-started asset is left
   /// alone. Otherwise as `expire_without_cascade`.
   pub(crate) async fn expire_from_cascade(&self, reason: ExpiryReason, via: &DependencyKey) -> Result<(), Error>;
   ```

   Under one `data` write lock it matches `status` explicitly: `Dependencies | Processing | Partial`
   → push `via` (deduplicated) and log an info entry naming it, return `Ok`; `None | Recipe |
   Submitted` → `Ok`; every other status → release the lock and call
   `self.mark_expired_status(reason)` (whose refusals stay as today).
3. `AssetManager::expire_dependencies_result` (default method): call
   `ar.expire_from_cascade(reason, &expired_key.via)` for keyed dependents and
   `ar.expire_from_cascade(reason_for(via, None), via)` for `expired.assets`, instead of
   `expire_without_cascade`. The stored-copy branch is unchanged.
4. `AssetRef::finalize_status_with_version`, before the existing `stale_dependency` branch: when
   `stale_dependency` is `None` and `cascaded_through` is not empty, take the first key in
   `cascaded_through` that one of `metadata.get_dependencies()` names and set it as
   `stale_dependency`, with the warning `note_expired_dependency` writes ("Dependency … of …
   expired during evaluation…"), shared through a private helper so the text stays one. The
   existing branch then does the rest (status, reason, persistence, and in `complete_evaluation`
   the `track_keyed_asset` + `StaleDependency` cascade to its own dependents).

Rejected alternatives:

- **Compare records with the version map at completion** (the issue's second option): that is
  `runtime-dependency-record-superseded-check`; it cannot see a cascade that changes no version.
- **Mark unconditionally:** over-expires runs that had not read the input (open question 2).
- **Expire right after `Ready`:** open question 1.
- **Change `mark_expired_status` to accept in-flight statuses:** it also serves explicit `expire`,
  whose refusal of a running asset is documented (`StatusConflict`).

### Changes

- `liquers-core/src/assets.rs`: `AssetData` field; `AssetRef::expire_from_cascade` (crate-private);
  `AssetManager::expire_dependencies_result` (default method body);
  `finalize_status_with_version`; a private helper for the stale warning shared with
  `note_expired_dependency`.
- `specs/reference/DEPENDENCIES_STATUS.md` (§Current contract, stale-dependency bullets) and
  `ASSETS.md` §Why an asset is `Expired` (`StaleDependency` row: "or a cascade reached it while it
  ran"): History rows and `reviewed:`.
- No `pub` signature change; no command, registry, format or binding change. Async; no new lock.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| A cascade and the finalization interleave | both under the asset's `data` write lock (Consolidated Findings) | high |
| Over-expiry when `via` was recorded before the cascade but re-read after it | the mark is honoured; costs one recompute on next access, never a stale read | high |
| `test_dependency_expiring_during_parent_evaluation_is_allowed` (non-keyed parent) changes | it asserts only the value the run returns, which the stale route still produces; re-run it | medium |
| Lock order: `expire_from_cascade` takes the dependent's lock while the cascade runs | the cascade already takes it through `mark_expired_status`; no other lock held | high |

Recovery: revert Phase 4 step 3 (the call site).

## Phase 3: Examples and Tests

### Examples

The Problem Example. `gated_upper` is test-registered as in the `late_dep` / `LateGate` helper of
`liquers-core/tests/common/manager_scenarios.rs`: it uppercases its input, notifies `entered`,
awaits `release`. Recipes in `data/recipes.yaml`: `q.txt` = `-R/data/c.txt/-/upper/q.txt`,
`p.txt` = `-R/data/q.txt/-/gated_upper/p.txt`.

### Tests

Scenarios in `liquers-core/tests/common/manager_scenarios.rs`, run on both built-in managers from
`liquers-core/tests/dependency_manager_integration.rs` (each wait under a timeout):

- `transitive_cascade_marks_running_dependent_stale` — the Problem Example: `p` ends `Expired`,
  reason `Direct { StaleDependency { dependency: -R/data/q.txt } }`, stored status `Expired`; a
  second `get(data/p.txt)` returns `BB`. Covers AC-1, AC-7.
- `explicit_expire_marks_running_dependent_stale` — `c.txt` = `make_text/c.txt` (a recipe key, since
  `expire` refuses a `Source`), `p.txt` = `-R/data/c.txt/-/gated_upper/p.txt`; while `p` is gated,
  `manager.expire(data/c.txt)`; `p` ends `Expired` with `StaleDependency { -R/data/c.txt }`.
  Covers AC-2.
- `cascade_through_unrecorded_dependency_does_not_mark` — `p` evaluated once reading `c`, then its
  recipe source is switched (test command reading a flag) to read only `d`; on the re-run, while
  gated, `set_state(c)` reaches `p` through the old edge; `p` ends `Ready`. Covers AC-3.
- `cascade_to_submitted_dependent_is_ignored` — a one-slot queue keeps `p` `Submitted` behind a
  gated job; `set_state(c)`; `p` runs and ends `Ready` with the new value. Covers AC-4.
- Existing `expiry_provenance_integration.rs` cascade-reason tests pass. Covers AC-5.
- `query_dependent_cascade_marks_stale` — the top-level query `-R/data/q.txt/-/gated_upper` (not
  keyed) under the Problem Example: its asset ends `Expired`. Covers AC-6.

Unit tests in `liquers-core/src/assets.rs` `tests`: `expire_from_cascade_marks_processing_asset`
(status `Processing` → `cascaded_through == [via]`, status unchanged) and
`finalize_honours_cascade_only_for_recorded_dependency` (two records; mark on a recorded and on an
unrecorded key). Cover AC-1, AC-3 at unit level.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_manager_integration`,
`--test expiration_integration`, `--test expiry_provenance_integration`, `--lib assets::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `manager_scenarios.rs` + `dependency_manager_integration.rs` — `gated_upper` and the five
  scenarios; AC-1, AC-2, AC-6 fail at HEAD — `cargo test -p liquers-core --test dependency_manager_integration cascade`
- [ ] 2. `assets.rs` — `AssetData::cascaded_through`, `AssetRef::expire_from_cascade`, unit test
  `expire_from_cascade_marks_processing_asset` — `cargo test -p liquers-core --lib expire_from_cascade`
- [ ] 3. `assets.rs` `AssetManager::expire_dependencies_result` — call `expire_from_cascade` —
  `cargo test -p liquers-core --test expiry_provenance_integration`. Rollback: revert this step.
- [ ] 4. `assets.rs` `finalize_status_with_version` + shared warning helper; unit test
  `finalize_honours_cascade_only_for_recorded_dependency`; step-1 scenarios pass —
  `cargo test -p liquers-core --lib --tests`
- [ ] 5. `DEPENDENCIES_STATUS.md`, `ASSETS.md` (History, `reviewed:`); issue closed with a note —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`; diff review
(explicit `match` on `Status`, no `_ =>`; no `println!`).
