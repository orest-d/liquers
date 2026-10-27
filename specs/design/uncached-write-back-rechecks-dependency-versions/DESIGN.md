---
id: UNCACHED-WRITE-BACK-RECHECKS-DEPENDENCY-VERSIONS
kind: design
title: An uncached key's write-back re-checks its inputs, so an expiry during the evaluation is not overwritten
form: compact
status: complete
gh_pr: [206]
readiness: ready
autofix: eligible
area: [core/assets]
issues: [UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION]
created: 2026-10-20
---
# An uncached key's write-back re-checks its inputs, so an expiry during the evaluation is not overwritten

> **Complete on 2026-10-27**: implemented by automatic fix in #206.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix). Origin: `record-streams` (complete), implementation-review finding
C1, which introduced the store-side expiry of an uncached key.

## Phase 1: High-Level Design

### Purpose

A `cached: false` key is never registered, so a change upstream expires its **stored copy**
(`expire_stored_copy`) instead of a live asset. An evaluation of that key already in flight then
writes `Ready` over the mark, and after a restart the stored copy is served as valid although it
was computed from the old input. The write-back must not leave such a copy `Ready`.

### Problem Example

Verified at HEAD in `liquers-core/src/assets.rs`: `AssetManager::expire_dependencies_result` calls
`expire_stored_copy` for a key with no registered asset; `complete_evaluation` →
`persist_with_status_tracking` → `save_to_store` writes with a plain `store.set`, checking only for
`Cancelled`; nothing compares the evaluation's dependency records with the dependency manager.

Scenario: `data/src.txt` holds `a` (a `Source`); `data/u.txt` has recipe
`-R/data/src.txt/-/gated_upper/u.txt` with `cached: false`
(`liquers-validate --command gated_upper -- '-R/data/src.txt/-/gated_upper/u.txt'`: `Ok`, a
`GetAsset` then an `Action`).

1. `get(data/u.txt)` starts; the `GetAsset` step reads `a` (version `v(a)` recorded); `gated_upper`
   waits;
2. `set_state(data/src.txt, "b")` registers `v(b)` and cascades to `data/u.txt`: no registered
   asset, and no stored copy yet (or an older one), so `expire_stored_copy` marks nothing that
   survives;
3. `gated_upper` returns `A`; the asset finishes `Ready` and `save_to_store` writes `A` as `Ready`.

Today: the store holds `A` with status `Ready` and a dependency record `src = v(a)`. In this process
the fast-track version check refuses it; a fresh process serves `A`. Should be: the stored copy is
`Expired` with reason `Direct { StaleDependency { dependency: -R/data/src.txt } }`, and a fresh
process recomputes `B`.

### Scope and Acceptance Criteria

- **AC-1** Input changed during the evaluation (queued manager)
  WHEN an upstream key changes after an uncached keyed evaluation read it and before the evaluation's store write
  THEN after the write the stored copy is `Expired` with a `StaleDependency` reason naming that upstream key
- **AC-2** The same on the inline manager
  WHEN the scenario of AC-1 runs under `ImmediateEnvironment`
  THEN the stored copy is `Expired` with the same reason
- **AC-3** A fresh process recomputes
  WHEN a new environment over the same store evaluates the key after AC-1
  THEN the recipe runs again and returns the value of the new input
- **AC-4** No change, no mark
  WHEN nothing upstream changes during the evaluation
  THEN the stored copy is `Ready`, as today, and the existing `stored_cached_flags` and `cache_strategy` suites pass
- **AC-5** Change after the write
  WHEN the upstream key changes after the write-back
  THEN the cascade marks the stored copy `Expired`, as today

Non-goals: registered (`cached: true`) keys — an in-flight registered dependent that a cascade
reaches is refused by `mark_expired_status` and can finish `Ready` on a superseded input; that is
filed as `CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED`. A late write after a removal or a
replacement (`REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`). The in-memory status of
the unregistered asset (nothing reuses it).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-core/src/assets.rs` (one private helper and
  its call in the persist path) restoring what `ASSETS.md` §cached promises ("a fresh process does
  not fast-track data the graph knows is stale"); no `pub`, type, command or format change.
- **Leading issue:** None
- **Explanation:** the dependency manager's version map already holds the evidence (a record
  version that differs from the current one), and `expire_stored_copy` already writes the right
  mark. Only the ordering argument (Phase 2) needed working out.
- **Open questions:** None.

### Design Dependencies

- `record-streams` — origin (complete, E4; not extended).
- `dependency-edge-superseded-version` (complete) — **overlaps**: the same comparison at edge
  recording time (`record_dependency_on_asset`); this design repeats it after the write.
- `store-conditional-write` — **overlaps** (in_review): a conditional store write would be another
  way to order the two writers; not needed in-process. No order.
- `CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED`, `REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`
  (issues filed with this design) — **overlaps**: other late-write-back races; different causes
  and change sites (`mark_expired_status`, `remove` / `set_state`).

### Consolidated Findings

- Ordering, not locking, closes the race. The upstream change registers its new version in the
  dependency manager *before* the cascade runs `expire_stored_copy`. The re-check runs *after* the
  write. So either the re-check sees the new version and marks the copy, or the cascade runs after
  the write and marks it. Taking `key_mutation_lock` around the write would not help: an expiry
  that came first would still be overwritten, and the cascade is sometimes called with that lock
  held (`remove`), so taking it there would deadlock.
- The re-check reads the records the evaluation wrote into its own metadata (direct dependencies
  only, since 2026-10-08), so its cost is one map read per direct dependency per uncached write.
- An unknown version on either side is no evidence, as in `dependency-edge-superseded-version`.

## Phase 2: Architecture

### Solution

New private `AssetRef::expire_written_copy_if_inputs_superseded(&self)`, called at the end of the
write path of `AssetRef::persist_with_status_tracking` when `save_to_store` returned
`Ok(SaveOutcome::Written)` — inside the spawned task for a background save, inline otherwise
(native inline mode and wasm32).

It acts only for an uncached, non-volatile keyed asset (`metadata.cached()` false, `is_volatile`
false, `key` set) — the case where no registered asset can be reached by a cascade:

1. Read `key` and the dependency records from the asset's metadata (one read lock, released).
2. For each record with a known version, `dm.get_version(&record.key)`; the first `Some(current)`
   with a known `current != record.version` is the stale input.
3. If one is found: `expire_stored_copy(&*manager, store, &key, &ExpiryReason::Direct { cause:
   ExpiryCause::StaleDependency { dependency } })`. It rewrites a stored `Ready` copy as `Expired`
   with the reason, and does nothing for any other stored status.

No asset lock is held across the dependency-manager reads (lock order as in
`record_dependency_on_asset`).

Rejected alternatives:

- **Order expiry and write through `key_mutation_lock`** (the issue's second suggestion): an
  expiry that precedes the write is still overwritten; deadlock risk (Consolidated Findings).
- **Check before the write and write `Expired` directly:** leaves the window between the check and
  the write; the post-write check is needed anyway, so the pre-check adds nothing.
- **Make the in-flight unregistered asset reachable by the cascade:** needs a new registry of
  in-flight uncached assets, a structural change for the same outcome.

Known-issue preflight: none blocks. `DEPENDENCY-MANAGER-VERSION-NOT-OBSERVABLE-FROM-TESTS` (P3)
does not matter here: the tests observe the stored metadata.

### Changes

- `liquers-core/src/assets.rs`: new private `AssetRef::expire_written_copy_if_inputs_superseded`;
  `persist_with_status_tracking` calls it after a `Written` outcome (three branches).
- No `pub` item, command, registry, serialized format or other crate change. Async, no new
  spawn.
- Documents: `specs/reference/ASSETS.md` (uncached paragraph: replace "The race … is
  `UNCACHED-…`" with the re-check rule) and `specs/reference/DEPENDENCIES_STATUS.md` (the same
  bullet), each with a History row and `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A correct copy is marked because a record version is stale for another reason (e.g. a record from an earlier evaluation merged in) | records are replaced per evaluation for keyed assets; unknown versions are ignored; AC-4 | medium |
| Extra cost per uncached write | one map read per direct dependency; uncached keys are a minority | high |
| The background-save task now does more work after `record_persistence_result` | no lock held; failures only logged, as in `expire_stored_copy` | high |
| Test timing (background save) | the test polls the stored status with a bound, as `ac01` polls stored bytes | high |

Recovery: remove the call; behaviour returns to today's.

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-1's test. Its recipe list (`data/recipes.yaml` in a memory store, served
by `DefaultRecipeProvider`): `-R/data/src.txt/-/gated_upper/u.txt` with `cached: false`;
`data/src.txt` is installed with `set_state`.

### Tests

In `liquers-core/tests/stored_cached_flags.rs`, both managers, with an async `gated_upper` command
that counts entries, waits on a latch, and upper-cases its input:

- `uncached_copy_written_after_input_change_is_expired_default` — `get(data/u.txt)` in a task; wait
  for the entry; `set_state(data/src.txt, "b")`; open the latch; poll the store until
  `data/u.txt` has data (bounded); assert stored status `Expired` and
  `expiry_reason() == Some(Direct { StaleDependency { dependency: -R/data/src.txt } })`. Fails at
  HEAD (`Ready`). Covers AC-1.
- `uncached_copy_written_after_input_change_is_expired_immediate` — the same on
  `ImmediateEnvironment` (the evaluation runs in a spawned task). Covers AC-2.
- `…_default` continued: a second environment over the same store evaluates `-R/data/u.txt`, the
  counter rises and the value is `B`. Covers AC-3.
- Existing `cached_false_asset_is_not_reused_*`, `stored_false_*`, and `cache_strategy` suites.
  Cover AC-4.
- Existing `expire_stored_copy_calls_record_expiry` and
  `expire_dependencies_result_assigns_cascaded_reason_per_key` (`assets.rs`). Cover AC-5.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test stored_cached_flags`, then
`cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/stored_cached_flags.rs` — `gated_upper` fixture and the two tests; both fail at HEAD — `cargo test -p liquers-core --test stored_cached_flags uncached_copy`. Rollback: delete the tests.
- [x] 2. `liquers-core/src/assets.rs` — `AssetRef::expire_written_copy_if_inputs_superseded` and its calls in `persist_with_status_tracking` — depends on 1 — step 1 tests pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert the commit.
- [x] 3. Docs: `ASSETS.md`, `DEPENDENCIES_STATUS.md` (History rows, `reviewed:`); close `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (records use
`cached: false`); `bash scripts/check-build-matrix.sh` (wasm32 rows: the sync persist branch);
`python3 scripts/docs_index.py --check`; diff review for edits outside `assets.rs`, the test file and
the two references.

### Implementation notes (2026-10-27)

- The version comparison is written inline in `expire_written_copy_if_inputs_superseded`. The
  same comparison is `DependencyManager::superseded_record` in
  `runtime-dependency-record-superseded-check`, implemented in parallel; whichever merges second
  can call the other's helper.
