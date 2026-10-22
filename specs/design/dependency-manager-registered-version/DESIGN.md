---
id: DEPENDENCY-MANAGER-REGISTERED-VERSION
kind: design
title: A read-only public accessor for the version the dependency manager holds for a key
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [DEPENDENCY-MANAGER-VERSION-NOT-OBSERVABLE-FROM-TESTS]
created: 2026-10-20
---
# A read-only public accessor for the version the dependency manager holds for a key

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

An integration test cannot ask whether a key is a dependency-graph node and with which version.
`DependencyManager` is `pub`, and integration tests reach it through
`AssetManager::dependency_manager()` (`DependencyManagerAccess` is a public supertrait of
`AssetManager`). But every method except `new` is `pub(crate)`, `get_version` included
(`liquers-core/src/dependencies.rs`, verified at HEAD 18732dd). So
`liquers-core/tests/runtime_volatility.rs` `rv03` asserts a proxy (the asset's metadata has no
version) for `runtime-volatility` AC-3 ("the key registers no version in the dependency graph").
The header of `tests/dependency_manager_integration.rs` says the same: "DependencyManager and
dependency_manager() are pub(crate)", which is no longer true of the type and accessor.

### Problem Example

```rust
let dm = envref.get_asset_manager().dependency_manager();
let v = dm.get_version(&DependencyKey::from(&parse_key("n.txt")?)).await;
// today: error[E0624]: method `get_version` is private
// should: compiles; `v == None` for the runtime-volatile key of rv03
```

### Scope and Acceptance Criteria

- **AC-1** A test can read the registered version
  WHEN an integration test calls `dependency_manager().registered_version(&key)`
  THEN it gets `Some(version)` for a tracked `Ready` key and `None` for a key never registered
- **AC-2** rv03 checks the graph directly
  WHEN `rv03` runs on both managers
  THEN it asserts `registered_version(n.txt) == None` in addition to the metadata check
- **AC-3** Read-only
  WHEN the accessor is called
  THEN it changes no graph state and expires nothing

Non-goals: exposing edges or a mutating API.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Rule 4: a new `pub` method in `liquers-core`.
- **Leading issue:** **Open design question - public method or test-only feature:** recommended:
  a public read-only method.
- **Explanation:** The method is a one-line delegation to `get_version`. The only open point is
  whether the graph's version map should be part of the public surface at all.
- **Open questions:**
  1. **Open design question - surface.** Recommended: `pub async fn registered_version(&self, key:
     &DependencyKey) -> Option<Version>`. It is read-only, useful for diagnostics (an external
     manager or a debug UI can show why a key did or did not cascade), and it costs no feature
     flag. The alternative is a `test-support` feature exposing `get_version`. It keeps the surface
     closed, but it is a new feature to maintain in the build matrix and the default test loop
     would need to enable it.

### Design Dependencies

None. Tests of `keyed-run-replaces-dependency-edges` and `set-binary-replaces-dependency-edges` do
not need it (they observe expiry), so there is no ordering.

## Phase 2: Architecture

### Solution

In `liquers-core/src/dependencies.rs`:

```rust
/// The version this manager holds for `key`, or `None` when `key` is not a graph node.
/// Read-only: a snapshot, which a concurrent write may change at once.
pub async fn registered_version(&self, key: &DependencyKey) -> Option<Version> {
    self.get_version(key).await
}
```

`get_version` stays `pub(crate)` for internal callers, so no call site changes. A `Version(0)`
(unknown) entry is returned as `Some(Version::unknown())`, which distinguishes "registered with an
unknown version" from "absent", as `missing_versions` does.

Rejected: making `get_version` itself `pub` (a second name for the same thing later); a
`test-support` feature (open question 1).

Known-issue preflight: none.

### Changes

- `dependencies.rs`: `registered_version`; the `DependencyManager` doc says it is the one public
  read.
- `liquers-core/tests/runtime_volatility.rs` `rv03`: the extra assertion.
- `liquers-core/tests/dependency_manager_integration.rs`: correct the header note.
- `specs/reference/DEPENDENCIES_STATUS.md`: one sentence where the version map is described,
  History, `reviewed:`. `liquers-py` does not wrap `DependencyManager` (grep at step 1).

### Risks

Public surface grows by one read-only method. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example, then `assert_eq!(v, None)`.

### Tests

- `liquers-core/tests/dependency_manager_integration.rs`
  `registered_version_reports_tracked_and_untracked_keys`. `set_state` a key: its version is
  `Some`, and an unknown key gives `None`. Both calls leave a later cascade unchanged (a dependent
  still expires on the next write). Covers AC-1, AC-3.
- `rv03` (both managers), with the extra assertion. Covers AC-2.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_manager_integration --test
runtime_volatility`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `dependencies.rs` `registered_version`. Proof: `cargo test -p liquers-core --lib`.
- [ ] 2. Tests above. Proof: the run command.
- [ ] 3. Header note, `DEPENDENCIES_STATUS.md`; close the issue. Proof:
  `python3 scripts/docs_index.py --check`.

### Validation

`cargo doc -p liquers-core --no-deps`; `cargo test -p liquers-core --lib --tests`; diff review.
