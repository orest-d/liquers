# Phase 3: Examples & Use-cases — Prune stored derived values

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Monthly pruning of a report folder | the call and the report | AC-1, AC-2 |
| 2 | Example | Dry run before enabling orphan removal | `dry_run`, `orphans` | AC-5, AC-6 |
| 3 | Test | `prune01_old_computed_value_loses_data_keeps_record` | data removed, `Recipe` + version | AC-1 |
| 4 | Test | `prune02_user_and_recent_values_are_kept` | `Source`, `Override`, no-metadata, recent | AC-2 |
| 5 | Test | `prune03_registered_key_is_kept` | in-use skip | AC-3 |
| 6 | Test | `prune04_pruned_key_recomputes_and_dependents_stay_valid` | recompute, same version, no cascade | AC-4 |
| 7 | Test | `prune05_orphans_reported_then_removed_on_request` | orphan option | AC-5 |
| 8 | Test | `prune06_dry_run_changes_nothing` | dry run | AC-6 |
| 9 | Test | `prune07_failure_on_one_key_continues` | read-only mount, `failed` | AC-7 |
| 10 | Test | `prune08_override_written_during_pass_survives` | guarded removal | AC-8 |
| 11 | Test | `assets::tests::prune_classify_is_exhaustive` | every `Status` classified, unparsable `updated` | AC-2 |

## Example 1: monthly pruning of a report folder

```rust
use std::time::Duration;
let policy = PrunePolicy::older_than(Duration::from_secs(30 * 24 * 3600));
let report = envref.get_asset_manager().prune_store(&parse_key("reports")?, &policy).await?;
eprintln!("pruned {} values, kept {}, failed {}",
          report.pruned.len(), report.kept.len(), report.failed.len());
```

`reports/recipes.yaml` declares `counted/a.txt` and `counted/b.txt` (both validate with
`liquers-validate --command counted`). After the call, an old `reports/a.txt` has a metadata-only
record `status: Recipe` with its version; requesting `-R/reports/a.txt` recomputes it.

## Example 2: dry run before enabling orphan removal

```rust
let report = manager.prune_store(&root, &policy.clone().with_dry_run(true)).await?;
for key in &report.orphans { eprintln!("would keep orphan {key} (recipe removed)"); }
let report = manager.prune_store(&root, &policy.with_remove_orphans(true)).await?;
```

## Edge and Error Cases

- **`updated` unparsable or empty** (`LegacyMetadata`, hand-written record) → kept, `UnknownAge`
  → test 11.
- **Key became `Override` between classification and removal** → `remove_computed` refuses →
  `failed` with `StatusConflict`, value intact → test 10 (forced with a store wrapper that writes
  the override from `get_metadata`'s second call).
- **Read-only mount** → `failed`, pass continues → test 9 (`ReadOnlyStore` under a router prefix).
- **Directory keys and nested folders** → descended, never removed → test 4 (`reports/old/` folder).
- **Volatile record** (`is_volatile: true` stored by mistake) → kept, `Volatile` → test 11.
- **Root that does not exist** → `listdir_keys_deep` answers an empty list → empty report (test 8
  asserts on a missing root too).

## Test Plan

New file `liquers-core/tests/store_pruning.rs`, each test run on `SimpleEnvironment` (queued) and
the inline manager, with an `AsyncMemoryStore` holding `reports/recipes.yaml`
(`recipes:\n  - query: counted/a.txt\n  - query: counted/b.txt\n`), the `counted` command
(`register_counted` in `tests/common/manager_scenarios.rs`) and helper `age_record(store, key,
days)` that rewrites a stored record's `updated` to `now - days` with `store.set_metadata`.

- `prune01_old_computed_value_loses_data_keeps_record` (AC-1): evaluate `reports/a.txt`, wait for
  `Persisted`, then build a second environment over the same store, so nothing is registered;
  age the record 40 days; prune 30 days; `store.get_bytes` is `KeyNotFound`, the record has
  `status: Recipe` and the old version; `report.pruned == [reports/a.txt]`.
- `prune02_user_and_recent_values_are_kept` (AC-2): `reports/s.txt` set as `Source`,
  `reports/o.txt` via `to_override`, `reports/raw.bin` written with `set_data` only, `reports/b.txt`
  computed and aged 1 day; all unchanged and in `kept` with `UserValue`, `UserValue`, `NoMetadata`,
  `Recent`.
- `prune03_registered_key_is_kept` (AC-3): hold the `AssetRef` of an aged `reports/a.txt` in the
  same environment; `kept` with `Registered`; data still present.
- `prune04_pruned_key_recomputes_and_dependents_stay_valid` (AC-4): a second recipe folder
  `deps/` with `-R/reports/a.txt/-/counted` stored as `deps/x.txt` (Ready, version recorded);
  prune `reports`; request `reports/a.txt`: counter +1, same version; `deps/x.txt` still `Ready`
  and its fast-track audit passes.
- `prune05_orphans_reported_then_removed_on_request` (AC-5): store a `Ready` record and data at
  `reports/gone.txt` with no recipe; first pass lists it in `orphans` unchanged; second pass with
  `remove_orphans` removes it (`get_metadata` is `KeyNotFound`).
- `prune06_dry_run_changes_nothing` (AC-6): store snapshot (`keys()` + bytes) identical before and
  after; report equal to a real run's report.
- `prune07_failure_on_one_key_continues` (AC-7).
- `prune08_override_written_during_pass_survives` (AC-8).
- `prune_classify_is_exhaustive` (AC-2), unit test in `assets.rs` for the private classifier.

Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test store_pruning`, then
`cargo test -p liquers-core --lib prune_`.

## Learning Log

- AC-4 depends on `remove` keeping the version in the rewritten record; if question 2 is answered
  "delete the record", AC-4's dependent expires on its next audit and the test changes.
