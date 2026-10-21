# Phase 3: Examples & Use-cases — One call reconciles a running asset manager with its store

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Restore from backup on a running server | the call and the report | AC-1, AC-2, AC-3 |
| 2 | Example | Dry run before acting | `ReportOnly` | AC-5 |
| 3 | Test | `store_sync::sync01_outside_byte_edit_is_applied` | external change policy, unmapping, cascade | AC-1, AC-7 |
| 4 | Test | `store_sync::sync02_deleted_stored_copy_unmaps_registration` | live-registration check, deletion | AC-2, AC-7 |
| 5 | Test | `store_sync::sync03_metadata_version_change_unmaps_registration` | live-registration check, version | AC-2, AC-7 |
| 6 | Test | `store_sync::sync04_stale_stored_dependent_is_expired` | stored-records walk | AC-3, AC-7 |
| 7 | Test | `store_sync::sync05_report_only_changes_nothing` | dry run | AC-5, AC-7 |
| 8 | Test | `store_sync::sync06_consistent_store_is_a_noop` | no false positives | AC-6, AC-7 |
| 9 | Test | `store_sync::sync07_unwritten_value_is_not_unmapped` | `stored: false` and non-serializable skipped | AC-2 |
| 10 | Test | `liquers_records::provider::tests::invalidate_caches_clears_manifests` | provider hook | AC-4 |
| 11 | Test | `recipes::tests::chain_forwards_invalidate_caches` | chain forwarding | AC-4 |

## Example 1: restore from backup on a running server

```rust
let m = envref.get_asset_manager();
let report = m.synchronize_with_store(&parse_key("data")?, AuditMode::Expire).await?;
for (key, action) in &report.versions.changed { eprintln!("{key}: {action:?}"); }
for key in &report.unmapped { eprintln!("{key}: no longer backed by the store, will reload"); }
for key in &report.stored_audit.expired { eprintln!("{key}: stale, recomputed on next request"); }
```

The resource queries a client uses afterwards are unchanged. `-R/data/report.csv` and
`-R/data/summary.txt` validate as `GetAsset[data, report.csv]` and `GetAsset[data, summary.txt]`.
The next request for either reads the store or recomputes.

## Example 2: dry run

`synchronize_with_store(&root, AuditMode::ReportOnly)` returns the same findings with
`repaired: None` and `commands: []`. An operator inspects it, then runs `Expire`.

## Edge and Error Cases

- **A key registered but outside `root`** → not checked (prefix filter) → sync02 keeps a registration
  under `other/` and asserts it is still mapped.
- **A value never persisted** (`stored: false`, non-serializable) → not unmapped → sync07.
- **A store error in a step** → the call returns `Err`, earlier steps have taken effect, and a
  retry is idempotent → covered by the existing `verify_stored_versions` error tests. No new test,
  because the composition only propagates errors.
- **Concurrent manager write** → consistent by construction (Phase 1 Q2). Not tested directly:
  the outcome depends on scheduling, and both outcomes are correct.

## Test Plan

New file `liquers-core/tests/store_sync.rs`. Every `syncNN` test runs on `SimpleEnvironment` and
`ImmediateEnvironment` (AC-7) with an `AsyncMemoryStore`. A test makes an outside change by writing
through the store directly (`store.set`, `store.remove`, `store.set_metadata`), bypassing the
manager. The recipes come from `recipes.yaml` in the memory store and use the test command `counted`
(`register_counted` in `tests/common/manager_scenarios.rs`), whose call counter shows a
recomputation.

- `sync01_outside_byte_edit_is_applied` (AC-1): overwrite `data/a.txt`'s bytes; call; `versions.changed`
  names it, `lookup_key_asset` is `None`, the dependent `data/b.txt` is `Expired`.
- `sync02_deleted_stored_copy_unmaps_registration` (AC-2): evaluate `data/a.txt` (persisted); `store.remove`;
  call; `unmapped == [data/a.txt]`; the next `get` recomputes (counter +1); `other/c.txt` is still mapped.
- `sync03_metadata_version_change_unmaps_registration` (AC-2): rewrite only the stored metadata with
  another version; call; unmapped.
- `sync04_stale_stored_dependent_is_expired` (AC-3): store `data/b.txt` recording a version of `data/a.txt`
  the store no longer holds; call; `stored_audit.expired` contains `data/b.txt`.
- `sync05_report_only_changes_nothing` (AC-5): the sync01-sync04 changes together; `ReportOnly`; the
  findings are non-empty, and the store bytes, the map and the statuses are unchanged.
- `sync06_consistent_store_is_a_noop` (AC-6).
- `sync07_unwritten_value_is_not_unmapped` (AC-2).
- `invalidate_caches_clears_manifests` (AC-4), in `liquers-records/src/provider.rs` tests: edit a
  manifest in the store; the provider still returns the old recipe; after `invalidate_caches` it
  returns the new one.
- `chain_forwards_invalidate_caches` (AC-4), in `liquers-core/src/recipes.rs` tests: a recording
  provider in a chain sees the call.

Commands: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test store_sync`,
`cargo test -p liquers-core --lib chain_forwards_invalidate_caches`,
`cargo test -p liquers-records --lib invalidate_caches`.

## Learning Log

- The composed operations already accept `AuditMode`, so a dry run of the whole sequence comes
  almost free. Only the store repair and the command refresh have no report-only form.
