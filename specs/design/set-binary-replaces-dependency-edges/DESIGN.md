---
id: SET-BINARY-REPLACES-DEPENDENCY-EDGES
kind: design
title: set_binary replaces a key's incoming dependency edges with the records it writes
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [SET-BINARY-KEEPS-PREVIOUS-DEPENDENCY-EDGES]
created: 2026-10-20
---
# set_binary replaces a key's incoming dependency edges with the records it writes

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`set_state` replaces a key's incoming edges with the records of the new metadata
(`replacing-write-drops-stale-dependency-edges`, complete). `set_binary` does not, on both built-in
managers (`DefaultAssetManager::set_binary`, `ImmediateAssetManager::set_binary` in
`liquers-core/src/assets.rs`, verified at HEAD 18732dd): step 6 calls only `publish_version`. Two
effects follow. An old edge survives and expires the key spuriously. A record the caller supplied
is not registered, so a real dependency does not expire the key in this process, although it will
after a restart, because the fast track loads the stored records (`load_from_records`).

### Problem Example

Uses the `replacing_install_store` recipes (`dash.txt` has a recipe):

```rust
install_with_dependencies(&envref, &k, "from d", &[&d]).await?;           // set_state: edge d -> k
m.set_binary(&k, b"from e", text_metadata_with_dependencies(&[&e])).await?; // Override, records e
m.set_state(&d, State::new().with_data(Value::from("d2"))).await?;
// Today: k's stored copy is marked Expired and the next get recomputes it.  Should: k stays Override.
m.set_state(&e, State::new().with_data(Value::from("e2"))).await?;
// Today: k stays Override.  Should: k's stored copy is marked Expired.
```

### Scope and Acceptance Criteria

- **AC-1** A dependency the written metadata does not list no longer expires the key
  WHEN `set_binary` replaces a key that had an edge from `d`, with metadata that does not list `d`
  THEN a later write of `d` leaves the key `Override` (or `Source`)
- **AC-2** A dependency the written metadata lists expires the key
  WHEN the metadata passed to `set_binary` lists `e`
  THEN a later write of `e` expires the key, as it would after a restart
- **AC-3** Only values that are graph nodes are tracked
  WHEN `set_binary` writes with status `Expired`, `Error` or `Volatile`
  THEN the graph is left as it is today (the same rule as `set_state`)
- **AC-4** External managers get the same behaviour
  WHEN an external manager's `set_binary` calls the new primitive after `publish_version`
  THEN AC-1 and AC-2 hold for it

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Rule 4: AC-4 needs a new provided method on the public
  `AssetManager` trait.
- **Leading issue:** **Open design question - external managers:** add a provided trait method
  (recommended), or fix the built-in managers only.
- **Explanation:** The rule and its safety argument are the ones `set_state` already applies. A
  write's metadata is the whole truth about the bytes it installs. Only the reach to external
  managers is a contract choice.
- **Open questions:**
  1. **Open design question - a provided `AssetManager::track_installed_records`.** Recommended:
     yes. `DependencyManager`'s methods are crate-private, so without it an external manager cannot
     match the built-in ones (the parity `track_installed` gave `set_state`). The alternative, the
     built-in managers only, would be eligible for automatic fixing but leaves the guide's
     `set_binary` row describing two behaviours.

### Design Dependencies

- `replacing-write-drops-stale-dependency-edges` (complete, E4): origin of the rule; listed this as
  a non-goal.
- `keyed-run-replaces-dependency-edges`: **overlaps** (weak; the same invariant on the evaluation
  path). `overlap.md` T2 was considered and rejected: the contract here was already decided by the
  completed design, and the change sites differ. No ordering.
- `external-manager-evaluation-session-sharing`: **overlaps** (weak; another external-manager
  primitive in the same guide table). No ordering.

## Phase 2: Architecture

### Solution

In `liquers-core/src/dependencies.rs`, add the key-based form of `track_replacing_asset`:

```rust
/// The key's incoming edges become exactly `records` (no asset involved, as for `set_binary`).
pub(crate) async fn replace_incoming_edges(
    &self,
    dependent: &DependencyKey,
    records: &[DependencyRecord],
) -> ExpiredDependents<E>;
```

Body: `self.remove_dependent_edges(dependent).await; self.load_from_records(dependent, records).await`.

In `liquers-core/src/assets.rs`, a provided method on `AssetManager`, next to `track_installed`:

```rust
/// [`Self::track_installed`] for a write that installs no asset (`set_binary`): `key`'s incoming
/// edges become `records`. Graph only; safe under the key-mutation lock. Call it after
/// `publish_version`, for `Ready`, `Source` and `Override` writes only.
async fn track_installed_records(&self, key: &Key, records: &[DependencyRecord], version: Version);
```

Default body: `replace_incoming_edges(&DependencyKey::from(key), records)`, then
`expire_dependencies_result(expired, ExpiryCause::Updated { version })`. `load_from_records` records
without comparing, so this expires nothing in practice. The cause is nominal, as in `track_installed`.

Both built-in `set_binary` step 6: inside the existing `Ready | Source | Override` branch, after
`publish_version`, call `self.track_installed_records(key, &metadata.dependencies, version)`.
`MinimalInlineAssetManager::set_binary` (`liquers-core/tests/common/minimal_manager.rs`) does the
same.

Rejected: putting the replacement inside `publish_version` (`set_state` also calls it, before its
asset is mapped); a temporary `AssetRef` to reuse `track_installed` (allocates an asset for nothing).

Known-issue preflight: `root-listing-dependency-key-spelling` canonicalises record keys inside
`load_from_records`, so it covers this path too. Nothing blocks this.

### Changes

- `dependencies.rs`: `replace_incoming_edges`.
- `assets.rs`: `AssetManager::track_installed_records` (provided), both `set_binary` bodies, the
  `set_binary` trait doc ("replaces the key's incoming edges with the supplied records").
- `tests/common/minimal_manager.rs`: its `set_binary`.
- `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`: the `set_binary` row and a new primitive row,
  History. `specs/reference/DEPENDENCIES_STATUS.md`: the write rule names both writes, History.
- No command, registry or format change. `liquers-py` / `liquers-web` do not implement
  `AssetManager` (checked by grep at step 1).

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| A self-record in supplied metadata | `load_from_records` skips the cycle error, as on reload | high |
| An external manager ignoring the new method | unchanged behaviour for it; the guide states the call | high |
| O(graph) scan per write | the same cost `set_state` already pays | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example. `text_metadata_with_dependencies(keys)` is `provenance_text_metadata()` with
`dependencies` set to records at each key's registered version.

### Tests

In `liquers-core/tests/common/manager_scenarios.rs`, `scenario_binary_install_replaces_dependency_edges`
(the Problem Example; the key's status is read with `m.get(&k)`: `Override` from the store, or
recomputed `Ready` when wrongly expired). It runs from:

- `dependency_manager_integration.rs` `binary_install_replaces_dependency_edges_default` and
  `_inline`. Covers AC-1, AC-2.
- `external_asset_manager.rs` `external_manager_binary_install_replaces_dependency_edges`. Covers AC-4.

`dependency_manager_integration.rs` `binary_install_with_error_status_leaves_edges`: `set_binary` with
`status: Error` keeps the old edge (a later write of `d` still reaches the key). Covers AC-3.

Unit, `dependencies::tests::replace_incoming_edges_keeps_other_dependents`: edges `d→k`, `d→j`,
`e→k`; replacing `k` with `[f]` leaves `d→j` and `f→k` only. Covers AC-1, AC-2.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_manager_integration --test
external_asset_manager` and `--lib dependencies::tests::replace_incoming`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Scenario and tests above. At HEAD the AC-1, AC-2 and AC-4 tests fail, and the AC-3 test
  passes. Proof: the run commands.
- [ ] 2. `dependencies.rs` `replace_incoming_edges`. Proof: the unit test.
- [ ] 3. `assets.rs` `track_installed_records` and both `set_binary` call sites. Proof: integration
  tests. Rollback: revert the two call sites.
- [ ] 4. `minimal_manager.rs` `set_binary`. Proof: `external_asset_manager` test.
- [ ] 5. Guide and reference rows, History, `reviewed:`; close the issue. Proof:
  `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`; diff review.
