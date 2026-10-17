---
id: REPLACING-WRITE-DROPS-STALE-DEPENDENCY-EDGES
kind: design
title: A write that replaces a keyed asset replaces its incoming dependency edges
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES]
created: 2026-10-17
---
# A write that replaces a keyed asset replaces its incoming dependency edges

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`set_state` installs a value whose metadata lists the dependencies that value was built from.
`DependencyManager::track_keyed_asset` (`liquers-core/src/dependencies.rs`) loads those records
with `load_from_records`, which only adds or updates edges. An edge the key held before and the new
metadata no longer lists survives, so a later change of that old dependency expires the key for no
reason. The result is an unneeded recomputation or re-read, never a stale value.

### Problem Example

```rust
// data/k.txt was installed with metadata recording a dependency on -R/data/d.txt.
manager.set_state(&k, state_with_dependencies(&[d])).await?;   // edge d -> k
// It is replaced with a value that records no dependencies.
manager.set_state(&k, state_with_dependencies(&[])).await?;    // edge d -> k survives
manager.set_binary(&d, b"changed", meta).await?;
// Today: k is expired (Cascaded, root -R/data/d.txt). Should be: k stays Override.
```

### Scope and Acceptance Criteria

- **AC-1** A dropped dependency no longer expires the key
  - WHEN `set_state` replaces a key whose old metadata listed `d` with metadata that does not
  - THEN a later write of `d` does not expire the key
- **AC-2** A kept dependency still does
  - WHEN the new metadata lists `e`
  - THEN a later write of `e` expires the key
- **AC-3** The key's own dependents are untouched
  - WHEN the key is replaced
  - THEN the edges *from* the key to its dependents are handled as today (the new version is
    published and contradicted dependents expire)
- **AC-4** External managers get the same behaviour
  - WHEN an external manager installs a value through `AssetManager::track_installed`
  - THEN AC-1 and AC-2 hold

Non-goals: the evaluation path (`AssetRef` completion calls `track_asset` / `track_keyed_asset`
with records that may not include schedule-time attribution edges, so replacing there needs its
own analysis); `set_binary`, which does not load metadata records today.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in `liquers-core` (`dependencies.rs`, `assets.rs`
  method bodies), one new `pub(crate)` method, no `pub` signature change, one crate.
- **Leading issue:** None
- **Explanation:** A write is the one place where the new metadata is, by definition, the whole
  truth about what the value depends on, so replacing the key's incoming edges there is safe. The
  issue's open note (reverse index or scan) is an implementation detail, settled below.
- **Open questions:** None.

### Design Dependencies

- `overlaps` `dependent-asset-weak-reference-pruning` (weak: same file).

### Consolidated Findings

Write path only. The scan over `keyed_dependents` is O(number of dependency keys) per write, the
same order as `missing_versions_for`; a reverse map is deferred until a measurement asks for it.
The evaluation path keeps additive edges (non-goal above).

## Phase 2: Architecture

### Solution

Add `pub(crate) async fn DependencyManager::clear_incoming_edges(&self, dependent: &DependencyKey)`:
`keyed_dependents.retain_async` over every dependency entry, removing `dependent` from its inner
map and dropping an inner map that becomes empty (as `remove_edge` does). Add
`pub(crate) async fn track_replacing_asset(&self, asset)`: for a keyed asset (the
`bound_owner_key` branch of `track_asset`), call `clear_incoming_edges` before `track_keyed_asset`;
otherwise behave as `track_asset`.

Call it in place of `track_asset` in `DefaultAssetManager::set_state`,
`ImmediateAssetManager::set_state` (`liquers-core/src/assets.rs`, step 9 of each) and the default
body of `AssetManager::track_installed`. The edge clear runs under the key-mutation lock that
`set_state` already holds, after `publish_version` (AC-3 is unaffected: outgoing edges live under
the key's own entry, which is not touched).

Rejected: replacing in `track_keyed_asset` for every caller (the evaluation path, see non-goals);
a reverse index field (more state to keep consistent in every expiry path, for a write-path cost
that has not been measured).

Known-issue preflight: none open touches `track_keyed_asset` or `load_from_records`.

### Changes

- `liquers-core/src/dependencies.rs`: `clear_incoming_edges`, `track_replacing_asset`.
- `liquers-core/src/assets.rs`: three call sites; `track_installed` doc comment ("replaces the
  key's previous incoming edges").
- `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`: the `track_installed` line; History row.

### Risks

A schedule-time edge for the key added by a still-running evaluation would be dropped; `set_state`
cancels that evaluation first (`cancel_for_replacement`), so none should remain. Tests most likely
touched: `liquers-core/tests/external_asset_manager.rs`, `dependency_manager_integration.rs`.
Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example.

### Tests

- `dependencies::tests::clear_incoming_edges_removes_only_that_dependent` — edges `d→k`, `d→j`,
  `e→k`; clearing `k` leaves `d→j` and drops the empty entry of `e` — AC-1 (unit)
- `liquers-core/tests/dependency_manager_integration.rs`
  `replacing_set_state_drops_unlisted_dependency_edges` — the Problem Example on both built-in
  managers (`manager_parametric` style), plus a write of `e` that does expire — AC-1, AC-2, AC-3
- `liquers-core/tests/external_asset_manager.rs`
  `external_manager_replacing_install_drops_unlisted_edges` — the same through
  `MinimalInlineAssetManager` — AC-4

Run: `cargo test -p liquers-core --lib dependencies::tests::clear_incoming_edges` and
`cargo test -p liquers-core --test dependency_manager_integration --test external_asset_manager`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. The three failing tests — commands above (AC-1, AC-4 fail; AC-2, AC-3 pass)
- [ ] 2. `liquers-core/src/dependencies.rs` `clear_incoming_edges`, `track_replacing_asset` —
  unit test passes
- [ ] 3. `liquers-core/src/assets.rs` — both `set_state` step-9 call sites and the
  `track_installed` default body use `track_replacing_asset` — integration tests pass. Rollback:
  revert the three call sites.
- [ ] 4. Guide line and History row; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`. Final diff
review for scope.
