# Phase 3: Examples & Use-cases — Bounded asset cache with reachability-based collection

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Internet-facing server bounds anonymous queries | configuration and effect | AC-1, AC-8 |
| 2 | Example | Explicit collection from an admin task | `collect_garbage` report | AC-7 |
| 3 | Test | `asset_gc::gc01_query_map_bounded_by_count` | count bound, LRU order | AC-1 |
| 4 | Test | `asset_gc::gc02_held_asset_survives_collection` | strong-count reachability | AC-2 |
| 5 | Test | `asset_gc::gc03_running_asset_survives_collection` | in-flight exclusion | AC-2 |
| 6 | Test | `asset_gc::gc04_idle_asset_collected` | idle bound | AC-3 |
| 7 | Test | `asset_gc::gc05_collected_key_reloads_without_recompute` | fast-track reload, no cascade | AC-4 |
| 8 | Test | `asset_gc::gc06_source_and_override_are_sticky` | sticky statuses | AC-5 |
| 9 | Test | `asset_gc::gc07_unbounded_collects_nothing` | default unchanged | AC-6 |
| 10 | Test | `asset_gc::gc08_explicit_collection_reports` | report contents | AC-7 |
| 11 | Test | `environment_builder::tests::cache_limits_round_trip_and_zero_rejected` | config | AC-8 |
| 12 | Test | `assets::tests::gc_removal_rechecks_reachability` | race: clone between snapshot and removal | AC-2 |

## Example 1: internet-facing server bounds anonymous queries

```yaml
assets:
  recipe_cache_strategy: all
  query_cache_strategy: all
  max_cached_queries: 1000     # anonymous queries: at most 1000 kept
  max_cached_keys: 10000       # recipes
  max_idle_seconds: 3600       # anything unused for an hour goes
```

After 100 000 distinct `hello-<i>` requests (the Problem Example) the query map holds at most
1000 entries. A query asked every minute stays, because each hit refreshes its stamp.

## Example 2: explicit collection

```rust
let report = envref.get_asset_manager().collect_garbage().await;
eprintln!("collected {} keys, {} queries", report.keys.len(), report.queries.len());
```

On the inline manager (wasm32) this is the only way, besides an over-limit insert, to apply
`max_idle_seconds`.

## Edge and Error Cases

- **Clone between snapshot and removal** → the entry is kept, because the count re-check fails →
  `gc_removal_rechecks_reachability` (unit test in `assets.rs`, which reaches the private map).
- **Hit between snapshot and removal** → the stamp changed, so the entry is kept → same test.
- **Expiration monitor upgrades a collected asset's weak ref** → the upgrade fails or the asset
  is no longer mapped; `remove_expired_from_maps` returns `false` → covered by gc04 running on the
  queued manager with a deadline-bearing asset.
- **Limit 0** → build error `ConfigurationError` → test 11.
- **wasm32** → no collector task; `scripts/check-build-matrix.sh` wasm rows build.

## Test Plan

New file `liquers-core/tests/asset_gc.rs`. Each test runs on `SimpleEnvironment` (queued) and
`ImmediateEnvironment` (inline), built through `EnvironmentBuilder` with the limits, an
`AsyncMemoryStore` and the counting test command `counted` (`register_counted` in
`tests/common/manager_scenarios.rs`; distinct queries
`-R/in/<i>.txt/-/counted` over stored inputs `in/<i>.txt`). Collection is forced with `collect_garbage()`, so no test
sleeps on the periodic sweep, except gc04. That test uses `max_idle_seconds: 1` and
`tokio::time::pause` / `advance` on the queued manager. On the inline manager it uses the explicit
call after a real one-second wait, because the stamp reads `chrono::Utc::now`.

- `gc01_query_map_bounded_by_count` (AC-1): limit 100; 1000 queries, then touch queries 0-9; collect;
  `lookup_query_asset` is `Some` for 0-9 and 990-999, and the map holds at most 100.
- `gc02_held_asset_survives_collection` (AC-2): hold query 0's `AssetRef`; exceed the limit; collect;
  `get_asset` returns the same id.
- `gc03_running_asset_survives_collection` (AC-2): a gated `slow` command is running; collect; same id
  after it finishes.
- `gc04_idle_asset_collected` (AC-3).
- `gc05_collected_key_reloads_without_recompute` (AC-4): key `data/a.txt` with a recipe running `counted`, and
  `data/b.txt` depending on it; evaluate both, drop handles, collect with `max_cached_keys: 1`; get
  `data/a.txt` again: the command counter is unchanged and `data/b.txt` is still `Ready`.
- `gc06_source_and_override_are_sticky` (AC-5): `set_state` a `Source`, `to_override` another; collect
  with limit 1; both are still registered.
- `gc07_unbounded_collects_nothing` (AC-6).
- `gc08_explicit_collection_reports` (AC-7): the report names exactly the removed keys and queries.
- `cache_limits_round_trip_and_zero_rejected` (AC-8), in `environment_builder.rs`'s test module:
  YAML → options → YAML omits unset fields; limit 0 is rejected.
- `gc_removal_rechecks_reachability` (AC-2), in `assets.rs`'s test module.

Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_gc`, then
`cargo test -p liquers-core --lib gc_ cache_limits`.

## Learning Log

- Reachability needs no graph walk: every non-owning reference in core is already weak, which is
  the outcome of `expiration-monitor-assetref` and the `WeakAssetRef` dependents list.
- "Reload, not recompute" (AC-4) depends on the version still being in the dependency manager. Do
  not prune graph versions on collection.
