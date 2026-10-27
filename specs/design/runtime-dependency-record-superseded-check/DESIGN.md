---
id: RUNTIME-DEPENDENCY-RECORD-SUPERSEDED-CHECK
kind: design
title: A dependency record a command adds is checked against the registered version when the run completes
form: compact
status: complete
gh_pr: [205]
readiness: ready
autofix: eligible
area: [core/assets]
issues: [RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED]
created: 2026-10-20
---
# A dependency record a command adds is checked against the registered version when the run completes

> **Complete on 2026-10-27**: implemented by automatic fix in #205.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`dependency-edge-superseded-version` (complete) made a dependent that records an edge against a
superseded version take the stale-dependency route, but only in
`AssetRef::record_dependency_on_asset`, the path for dependencies a run *waits for*. A record a
command adds itself (`Context::add_dependency` / `add_dependencies`, used by `ns-search` in
`liquers-lib/src/search/`) reaches the graph only at completion: `AssetRef::complete_evaluation`
merges it into the metadata, then `track_keyed_asset` → `load_from_records` → `add_dependency`
(records, never compares) or, for a non-keyed asset, `track_asset` → `add_dependent_asset` (no
comparison). Verified at HEAD (18732dd) by reading; the window is narrow and was not reproduced.

A write of the dependency between the command's read and the completion cascades over an edge that
does not exist yet, so the dependent finishes `Ready` on stale content. It is expired only when the
dependency changes again.

### Problem Example

Recipe `data/k.txt` = `record_d` (validated with `--command record_d`). The command reads the
version `v1` of `data/d.txt`, records `-R/data/d.txt` at `v1`, then waits on a test gate.

```text
get k.txt            record_d records (-R/data/d.txt, v1) and blocks
set_binary d.txt     registers v2; its cascade finds no edge to k
release the gate     today: k Ready, edge d→k expecting v1, until d changes again
                     should: k finishes Expired, Direct { StaleDependency { dependency: -R/data/d.txt } }
```

### Scope and Acceptance Criteria

- **AC-1** A record superseded before completion takes the stale route
  WHEN a run's added record names a concrete version and, when it completes, the manager holds a concrete different version
  THEN the asset finishes `Expired` with `Direct { StaleDependency }` naming that dependency, and its value is returned once
- **AC-2** A record superseded during completion is caught
  WHEN the dependency is written after the completion check and before the edge exists
  THEN the asset is expired with `Direct { StaleDependency }` once its edges are registered
- **AC-3** Matching or unknown versions change nothing
  WHEN the versions are equal, or either side is unknown or absent
  THEN the asset finishes `Ready`, as today
- **AC-4** Non-keyed assets too
  WHEN the dependent is a query asset
  THEN AC-1 holds for it (it is not a graph node, so nothing cascades from it)
- **AC-5** Waited dependencies behave as before
  WHEN the existing `edge_against_superseded_version_*` tests run
  THEN they pass unchanged

Non-goals: a check at `Context::add_dependency` time (the race is after it); verifying store bytes
(that is `verify_versions`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. A bug fix that applies the documented stale-at-birth rule
  (`DEPENDENCIES_STATUS.md`) to the second recording path. It adds two crate-private helpers, no
  `pub` change, one crate.
- **Leading issue:** None
- **Explanation:** The rule, its evidence test (two concrete, different versions) and its outcome
  (the stale route) are already decided by `dependency-edge-superseded-version`. This design adds
  the comparison at the one place every added record passes.
- **Open questions:** None. Settled in Phase 2: compare all records, not only command-added ones
  (a waited one that changed since is equally stale), and check twice to cover the persistence
  window.

### Design Dependencies

- `dependency-edge-superseded-version`, `stale-dependency-status-finalization`, `search-read-dependencies`
  (complete, E4): the rule, the route and the motivating consumer.
- `root-listing-dependency-key-spelling`: **overlaps** (weak). A record spelled `-R-dir/` misses
  the comparison until that design canonicalises keys. A miss, never a false positive. No ordering.
- `keyed-run-replaces-dependency-edges`: **overlaps** (weak; the other end of a keyed run).

## Phase 2: Architecture

### Solution

`liquers-core/src/dependencies.rs`:

```rust
/// The first record whose concrete version differs from a concrete version this manager holds.
pub(crate) async fn superseded_record(&self, records: &[DependencyRecord]) -> Option<DependencyKey>;
```

It runs `get_version` per record and skips unknown on either side. No I/O and no lock beyond the
`scc` read.

`liquers-core/src/assets.rs`, `AssetRef::complete_evaluation`:

1. **Before the status is finalized.** Before taking the `data` write lock that merges
   `dependencies`, compute `dm.superseded_record(&dependencies)`. Inside that lock, while the run is
   still in flight, a `Some(key)` adds the warning `note_expired_dependency` writes and sets
   `stale_dependency` if unset. This goes through a new private `AssetData::note_stale_dependency(key)`,
   which `note_expired_dependency` then calls too, so the text stays one. `finalize_status_with_version`
   then takes the existing route (`Expired`, `Direct { StaleDependency }`, the keyed branch
   `track_keyed_asset` with `StaleDependency` for its dependents). This covers AC-1 and AC-4.
2. **After the edges exist.** In the ordinary (not stale, not volatile) branch, after
   `dm.track_asset(self)`, compute `superseded_record` over the merged metadata records again. On
   `Some(key)`, call `self.expire_without_cascade(Direct { StaleDependency { dependency: key } })`
   and, for a keyed asset, `manager.cascade_expire_dependents(&dep_key, StaleDependency { .. })`.
   A write landing after this point finds the edge, so nothing is left uncovered (AC-2).

Rejected: comparing inside `load_from_records` (it is documented as a pure recorder, and it also
serves reloads, where a mismatch is the audit's business); comparing in `Context::add_dependency`
(before the race).

Known-issue preflight: none open touches `complete_evaluation`'s finalization.
`keyed-run-replaces-dependency-edges` edits `evaluate_recipe` and `finish_run_with_result`, which are
different functions.

### Changes

- `dependencies.rs`: `superseded_record`.
- `assets.rs`: `complete_evaluation` (two checks), `AssetData::note_stale_dependency` (private),
  `note_expired_dependency` uses it.
- `specs/reference/DEPENDENCIES_STATUS.md`: the stale-at-birth bullet names both recording paths.
  History row and `reviewed:`.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| False positive: a record's version and the map's differ legitimately | both are content versions of one key; the same comparison already runs for waited dependencies. AC-3, AC-5 | high |
| `ns-search` records a store version the map holds differently | the map holds what the last write published, which is what the store holds. Run `cargo test -p liquers-lib --lib --tests` | medium |
| Extra cost per completion | one `scc` read per record | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example. `record_d` and its gate follow `register_late_dependency_commands` /
`LateGate` in `liquers-core/tests/common/manager_scenarios.rs`. The command reads
`m.version(&d)`, calls `context.add_dependency`, signals `entered` and awaits `release`.

### Tests

Scenarios in `manager_scenarios.rs`, run on both built-in managers from
`liquers-core/tests/dependency_manager_integration.rs`:

- `added_record_superseded_before_completion_marks_stale` (the Problem Example). Covers AC-1.
- `added_record_with_current_version_stays_ready` (no write while gated) and
  `added_record_with_unknown_version_stays_ready`. Cover AC-3.
- `query_asset_with_superseded_added_record_is_expired` (the query `record_d/upper`, with a
  test-registered `upper`; validated with `--command record_d --command upper`). Covers AC-4.

Unit tests in `liquers-core/src/assets.rs` `tests`:

- `superseded_after_tracking_expires_the_asset`: a `Ready` keyed asset whose records hold `-R/a.txt`
  at `v1` and an edge expecting `v1`; `register_version(a, v2)` without cascade (the edge is added
  afterwards in the real window, so the test adds it after), then `expire_if_superseded(records)`:
  the asset is `Expired` with `Direct { StaleDependency }`. Covers AC-2.
- `edge_against_superseded_version_*`: existing, unchanged. Covers AC-5.

Unit, `dependencies::tests::superseded_record_skips_unknown_and_absent`. Covers AC-3.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_manager_integration`,
`--lib assets::tests`, `--lib dependencies::tests::superseded`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. Tests above. AC-1, AC-2 and AC-4 fail at HEAD. Proof: the run commands.
- [x] 2. `dependencies.rs` `superseded_record` and its unit test.
- [x] 3. `assets.rs` `AssetData::note_stale_dependency`, used by `note_expired_dependency`. Proof:
  `edge_against_superseded_version_*` pass.
- [x] 4. `complete_evaluation` check 1. Proof: AC-1, AC-3, AC-4 tests.
- [x] 5. `complete_evaluation` check 2, factored as a private `expire_if_superseded(&self, records)`
  so the AC-2 unit test can call it. Proof: AC-2 test. Rollback: revert steps 4-5.
- [x] 6. `DEPENDENCIES_STATUS.md`; close the issue. Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (search
commands); diff review.

### Implementation notes (2026-10-27)

- AC-4 is tested on the query `record_d` itself rather than `record_d/upper`: with `upper`
  appended, the planner cuts `record_d` off as a predecessor asset, so the asset holding the record
  would again be `record_d`'s.
- An `Expired` asset's `get` reports an error, so the scenario reads the run's value with
  `get_any_status`, as `scenario_stale_dependency` does.
