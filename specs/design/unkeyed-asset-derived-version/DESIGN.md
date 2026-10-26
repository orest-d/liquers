---
id: UNKEYED-ASSET-DERIVED-VERSION
kind: design
title: A keyless query asset carries a version derived from its recipe and its dependencies' versions
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, records]
issues: [UNKEYED-ASSET-HAS-NO-VERSION]
created: 2026-10-26
---
# A keyless query asset carries a version derived from its recipe and its dependencies' versions

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Origin: `external-index-reconciliation` (its question 4) and
`external-index-sync/indexation-policy.md` §4-§5. The source was filed `L`; Phase 2 confines it
to `liquers-core`, so it is `M` and uses the compact form.

## Phase 1: High-Level Design

### Purpose

A query result that is never stored (an ad-hoc report, a manifest chunk given as a query, a
predecessor boundary) has no version, so nothing can tell whether it changed without
re-evaluating and comparing content. Its inputs are versioned already: every non-volatile
evaluation records each dependency it read and each command's metadata and implementation
version. A hash over those records and the recipe is a version that changes exactly when an input
does, with no serialization (VISION: "Trust the result"; records and search, priority 2).

### Problem Example

A manifest chunk is the unkeyed query `-R/data/sales.csv/-/ns-pl/eq-region-EU` (validated:
`GetAsset[data, sales.csv]`, one `Action`). `ManifestSource::describe_chunk` asks
`EnvResolver::metadata`, which evaluates it; at HEAD (`cd6d5da`) the result's
`metadata.version()` is `None` because `AssetRef::prepare_version` returns `None` for any asset
without a key (`liquers-core/src/assets.rs`). Reconciliation (`external-index-reconciliation`)
therefore puts the chunk on its time-based regime and re-feeds it on a schedule. Should be: the
version is `Some(v)`, `v` is the same after the asset is dropped and recomputed from the same
`sales.csv`, and different once `sales.csv` (or `eq`'s implementation version) changes, so an
unchanged chunk is recognized from its metadata alone.

### Scope and Acceptance Criteria

- **AC-1** A keyless query result is versioned
  WHEN a non-volatile query with no key and no input state finishes `Ready`
  THEN its `metadata.version()` is `Some`, with bit 127 clear
- **AC-2** Stable over recomputation
  WHEN the same query is evaluated again after the cached asset is removed, with no input changed
  THEN the version is equal
- **AC-3** Changes with a data dependency
  WHEN a key the query read is written with different content and the query is re-evaluated
  THEN the version differs
- **AC-4** Changes with a command implementation version
  WHEN the same query runs in an environment where a command it uses has a different `version:`
  THEN the version differs
- **AC-5** Changes with the query
  WHEN two queries differ only in a parameter (`…/upper` and `…/lower` of the same input, or one argument override)
  THEN their versions differ
- **AC-6** No version where none is sound
  WHEN the asset is volatile, was applied to an input state, or recorded a data or query dependency at `Version::unknown()`
  THEN `metadata.version()` is `None`, as today
- **AC-7** Nested keyless dependencies are versioned
  WHEN a query's predecessor boundary is a keyless asset
  THEN the outer asset's dependency record for it carries that asset's derived version, not `Version::unknown()`
- **AC-8** The dependency graph is unchanged
  WHEN a keyed asset depends on a keyless query
  THEN the dependency manager's edge to that query still records `Version::unknown()`, and no keyless version is registered in it
- **AC-9** Record sources see it
  WHEN `describe_chunk` describes a `ChunkId::Query` chunk through `EnvResolver`
  THEN the descriptor's metadata has the derived version

Non-goals: computing the version **before** evaluation from the plan (question 1); registering
keyless assets as dependency-graph nodes; versions for volatile assets (time-based by design,
`indexation-policy.md` §6); deduplicating two spellings of one computation.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new observable metadata for every keyless query result and
  a change to what the dependency manager records (rule 2: not a bug fix; rule 4: the
  `q/version` answer changes); open decisions (rule 5).
- **Leading issue:** **Open design - when the version is computed** (question 1).
- **Explanation:** every input the version needs is already in the asset's metadata when
  `prepare_version` runs (dependency records are merged first), so the change is local to
  finalization plus one guard on dependency-manager edges; Phases 3-4 follow the recommendations.
- **Open questions:**
  1. **Open design - after evaluation, not before:** the version is derived at finalization from
     the dependency records the evaluation actually made. Recommended: a plan-only version
     (`indexation-policy.md` §5, "offline") misses run-time dependencies a command adds with
     `context.get_dependency_state` / `context.evaluate`, so it could call a changed chunk
     unchanged — an index silently stale. A pre-evaluation estimate can be added later as a
     separate, weaker field. Consequence: deciding staleness still evaluates an uncached chunk,
     but a re-evaluated unchanged chunk keeps its version, so a sink is not re-fed.
  2. **Open design - not distinguishable from a time-based version:** bit 127 stays clear, so
     `Version::kind()` reports `Timestamp` for it. Recommended: `Version` is opaque and only
     equality matters (`metadata.rs`); `kind()` only words a log line. Alternative: reserve a bit
     for derived versions — a new persisted meaning for stored versions, for no current consumer.
  3. **Open design - unversioned commands:** a command-metadata or command-implementation record
     at `Version::unknown()` (a command that declares no `version:`) is left out of the hash, as
     a keyed asset already treats it (it never invalidates anything). A data, listing or query
     dependency at `Version::unknown()` makes the version `None` (AC-6). Recommended.
  4. **Proposed resolution - identity:** the hash covers the recipe's `query`, `arguments`
     (canonical JSON, sorted keys), `links` (sorted) and `cwd` — not `title` / `description` —
     then every record sorted by key, under a domain tag `liquers-derived-version-v1`.
  5. **Proposed resolution - edges stay unknown:** `DEPENDENCIES_STATUS.md` requires that a
     concrete version reach a dependency-manager edge only for a graph node; a keyless query is
     not one, so its edges keep `Version::unknown()` (AC-8) while metadata records the derived
     version (AC-7).

### Design Dependencies

- `external-index-reconciliation` — **required-by** (soft): works without this (time-based
  regime); with it, unkeyed manifest chunks reconcile by version. Its tests use a volatile chunk
  for the time-based path, so none change.
- `runtime-dependency-record-superseded-check` — **overlaps**: compares run-time records with
  *registered* versions only; keyless versions are never registered (AC-8), so no interaction.
- `recipe-dependency-versions` — **overlaps**: gives recipe keys a version; a derived version then
  covers recipe edits through the recorded `Recipe` dependency with no extra work.
- `asset-event-kinds` — **overlaps**, weak: announces new versions of keyed assets only.

### Consolidated Findings

- `evaluate` merges the evaluation's dependency records into the metadata before
  `prepare_version`, and `finalize_plan_expanded` seeds command metadata and implementation
  records for every non-volatile plan, keyless included — so no plan access is needed.
- The volatile test must include the metadata's `expires` (finalization marks an asset volatile
  from either), or a volatile asset would receive a version just before turning volatile.
- `record_dependency_on_asset` and `DependencyManager::load_from_records` turn recorded versions
  into graph edges; without the AC-8 guard, a keyless dependency's derived version would become an
  edge expectation and reintroduce the spurious expiry `DEPENDENCIES_STATUS.md` forbids.
- No persisted format changes: keyed assets' stored dependency records may now hold a concrete
  version for a query key, which the on-load audit skips (`is_store_resolvable` is false).

## Phase 2: Architecture

### Solution

In `AssetRef::prepare_version` (`liquers-core/src/assets.rs`), the branch that returns `None` for
a keyless asset computes a derived version instead, from the recipe and the merged dependency
records already in `lock.metadata`. It returns `PreparedVersion { version, binary: None,
serialization_error: None }`, installed by `finalize_status_with_version` as today. The edge
funnels in `dependencies.rs` and `record_dependency_on_asset` record `Version::unknown()` for a
query dependency key.

Rejected: hashing the serialized value (the cost the original `None` avoided); a plan-only
version (question 1); registering keyless versions in the dependency manager (makes every query a
graph node and grows the map with every ad-hoc query).

Known-issue preflight: `RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED` and
`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` touch neighbouring code but not `prepare_version`'s
keyless branch; either order.

### Changes

```rust
// liquers-core/src/assets.rs, private
/// `None` when any data, listing or query record is unknown; unknown command records are skipped.
fn derived_version(recipe: &Recipe, records: &[DependencyRecord]) -> Option<Version>;
```

- `prepare_version`: `lock.key.is_none()` → `None` if `lock.is_volatile`,
  `lock.metadata.expires().is_volatile()`, or `lock.initial_state` is not none (applied), else
  `derived_version(&lock.recipe, lock.metadata.get_dependencies())` as the prepared version. The
  keyed path is unchanged.
- `derived_version`: `Version::from_bytes` (unflagged) over `liquers-derived-version-v1\n`, the
  identity of question 4, then `key=hex(version)\n` per record sorted by key.
- `record_dependency_on_asset`: when the dependency was identified by query (the `else if` arm
  that builds `DependencyKey::from(query)`), the metadata record keeps the version and the
  `dependency_manager().add_dependency(..)` edge gets `Version::unknown()`.
- `liquers-core/src/dependencies.rs` `load_from_records` (and any other path turning a
  `DependencyRecord` into an edge): a private `edge_expectation(key, version)` returns
  `Version::unknown()` for a query key (none of `is_pure_key`, `is_dir_key`, `is_recipe_key`,
  `is_command_metadata`, `is_command_implementation`).

No new `pub` item; no command; no serialized format. Sync except the existing async lock.
Documents: `specs/reference/DEPENDENCIES_STATUS.md` (the "non-keyed (query) asset gets none"
bullet and the edge invariant), `specs/reference/ASSETS.md` (versions), and
`specs/reference/WEB_API_SPECIFICATION.md` (`q/version` of a finished keyless asset), each with a
History row and `reviewed:`.

### Risks

| Assessment | Finding |
|---|---|
| Files | `liquers-core/src/assets.rs`, `liquers-core/src/dependencies.rs`; tests |
| Existing tests | any asserting `version() == None` for a query asset, or an unknown edge for one; none found by search, re-checked in step 1 |
| Compatibility | Observable: `q/version`, `AssetInfo`, metadata of keyless results. Graph semantics unchanged by AC-8 |
| Correctness | A non-volatile command that is in fact nondeterministic keeps its version across recomputation — the same promise caching already relies on |
| Performance | One hash over a few records per keyless finalization; no serialization |
| Recovery | Revert `prepare_version`'s branch; the edge guard is harmless alone |
| Certainty | Medium-high; the edge funnels are enumerated in step 1 |

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-9's shape. A core-only form: a memory store holding `data/a.txt` =
`hello`, `upper` (`version: "1"`) registered, and the query `-R/data/a.txt/-/upper` (validated
with `liquers-validate --command upper`: `GetAsset[data, a.txt]`, `Action{upper}`). Its first
evaluation has `version() == Some(v)`; after `remove` of the query asset and re-evaluation, `v`
again; after writing `data/a.txt` = `bye` and re-evaluating, `w != v`.

### Tests

New `liquers-core/tests/unkeyed_derived_version.rs`, both managers (`SimpleEnvironment` queued and
`ImmediateEnvironment` inline) through one generic scenario body each:
- `keyless_result_has_unflagged_version` — AC-1.
- `version_stable_over_recomputation` — AC-2.
- `version_changes_with_input_content` — AC-3.
- `version_changes_with_command_version` — two environments, `upper` at `version: "1"` and `"2"`. AC-4.
- `version_differs_by_query_and_override` — `upper` vs `lower`; one `apply_queued` with an argument override. AC-5.
- `no_version_when_volatile_applied_or_unknown_input` — a `volatile: true` command; `apply` on a state; a command recording a dependency at `Version::unknown()`. AC-6.
- `predecessor_boundary_record_carries_derived_version` — a two-segment query whose prefix is cut; the outer metadata's record for the prefix query equals the prefix asset's version. AC-7.
- `graph_edge_to_query_stays_unknown` — a keyed recipe over a keyless query: the dependency manager edge records `Version::unknown()` and `get_version(query key)` is `None`. AC-8.

Unit, `liquers-core/src/assets.rs` `mod tests`: `derived_version_is_order_independent_and_skips_unknown_commands` — AC-2, AC-6.

`liquers-records/src/sources.rs` `mod tests`: `describe_query_chunk_has_derived_version` — a
manifest with one `ChunkId::Query` chunk and `EnvResolver` over a core environment. AC-9.

Run: `cargo test -p liquers-core --test unkeyed_derived_version`, then
`cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-records --all-features --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Enumerate every path that turns a `DependencyRecord` or a dependency's metadata version into a dependency-manager edge (`record_dependency_on_asset`, `DependencyManager::load_from_records`, `add_dependency` callers in `assets.rs` / `context.rs`) and every test asserting a keyless `None` version; record the list here — no code change.
- [ ] 2. `dependencies.rs` `edge_expectation`, applied in each path from step 1, and `record_dependency_on_asset`'s query arm; test `graph_edge_to_query_stays_unknown` — `cargo test -p liquers-core --lib --tests` (all green: no keyless version exists yet, so behaviour is unchanged). Rollback: revert.
- [ ] 3. `assets.rs` `derived_version` and its unit test; `prepare_version`'s keyless branch — depends on 2 — `cargo test -p liquers-core --test unkeyed_derived_version`. Rollback: restore the `None` branch.
- [ ] 4. Remaining tests of Phase 3, including the records test — `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-records --all-features --lib --tests`.
- [ ] 5. Docs: `DEPENDENCIES_STATUS.md`, `ASSETS.md`, `WEB_API_SPECIFICATION.md` (History rows, `reviewed:`); close `UNKEYED-ASSET-HAS-NO-VERSION` with a resolution note; note in `external-index-reconciliation` Phase 1 question 4 that unkeyed chunks are now versioned — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (UI and
record commands read versions); `cargo test -p liquers-axum` (`q/version`); `cargo test -p
liquers-records --all-features --lib --tests`; `python3 scripts/docs_index.py --check`; diff
review: only the two core source files, tests and the three documents.
