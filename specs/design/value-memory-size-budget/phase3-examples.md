# Phase 3: Examples & Use-cases — Memory size estimate and byte budget

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Internet-facing server with byte budgets | configuration | AC-4, AC-8 |
| 2 | Example | Reading an estimate | the method | AC-1, AC-2 |
| 3 | Test | `value::tests::core_value_estimates` | core variants | AC-1 |
| 4 | Test | `value::simple::tests::simple_value_estimates` | lib base variants | AC-1 |
| 5 | Test | `value::tests::ext_value_estimates` (lib) | frame, image, record view, `None` variants | AC-2 |
| 6 | Test | `value::extended::tests::combined_and_foreign_forward` | delegation, default | AC-3 |
| 7 | Test | `liquers-records` `batch::tests::record_batch_estimate_covers_buffers` | column buffers, views forward | AC-2 |
| 8 | Test | `asset_gc::gc09_query_bytes_budget` | byte budget, LRU | AC-4 |
| 9 | Test | `asset_gc::gc10_held_entries_over_budget_reported` | never collect held | AC-5 |
| 10 | Test | `asset_gc::gc11_cached_binary_counts` | binary length counted | AC-6 |
| 11 | Test | `asset_gc_bytes::gc12_unknown_sizes_count_zero` | `None` → 0 and counted in report | AC-7 |
| 12 | Test | `environment_builder::tests::byte_limits_round_trip_and_zero_rejected` | config | AC-8 |
| 13 | Test | `asset_gc_bytes::gc13_no_byte_limit_measures_nothing` | no estimate calls | AC-9 |

## Example 1: internet-facing server with byte budgets

```yaml
assets:
  max_cached_queries: 1000
  max_cached_query_bytes: 1073741824     # 1 GiB of ad-hoc results
  max_cached_key_bytes: 4294967296       # 4 GiB of recipe results
  max_idle_seconds: 3600
```

Ten `-R/data/big<i>.csv/-/ns-pl/from_csv` queries (each validates) of 400 MB each: after the
third, the next insert's pass collects the least recently used ones that nobody holds, keeping the
query map at most 1 GiB.

## Example 2: reading an estimate

```rust
let state = envref.evaluate(&parse_query("-R/data/big.csv/-/ns-pl/from_csv")?).await?.get().await?;
eprintln!("about {:?} bytes", state.data.estimated_memory_size());
```

## Edge and Error Cases

- **Value replaced in place** (`to_override` keeps the value; `set_value` replaces it) → the
  cached measurement is keyed by the `Arc` pointer, so a new value is re-measured → gc11 covers the
  binary-length key, `measure_cache_invalidates_on_new_value` (unit, `assets.rs`) the pointer.
- **Budget below the held working set** → `over_budget: true`, nothing held collected → gc10.
- **Feature-reduced builds** (`--no-default-features`, each feature alone) → every `ExtValue` arm
  compiles → `bash scripts/check-build-matrix.sh`.
- **wasm32** → inline manager runs the byte step on insert; no task → matrix wasm rows.
- **Zero budget** → `ConfigurationError` → test 12.

## Test Plan

Part A, unit tests beside each implementation:
- `liquers-core/src/value.rs` `core_value_estimates` (AC-1): text of 1000 bytes ≥ 1000; bytes of
  4096 ≥ 4096; an array of ten such texts ≥ 10 000; every variant returns `Some`.
- `liquers-lib/src/value/simple.rs` `simple_value_estimates` (AC-1).
- `liquers-lib/src/value/mod.rs` `ext_value_estimates` (AC-2): a 1000×10 `i64` frame ≥ 80 000
  (`#[cfg(feature = "polars")]`); a 100×100 RGBA image ≥ 40 000 (`image-support`); a
  `RecordView` over a batch of 1000 `Int` rows ≥ 8000 (`records`); `UIElement` → `None`.
- `liquers-lib/src/value/extended.rs` `combined_and_foreign_forward` (AC-3): a test `ForeignValue`
  returning `Some(123)`; a `CombinedValue` of each half; a test `ValueExtension` with no override
  → `None`.
- `liquers-records/src/batch.rs` `record_batch_estimate_covers_buffers` (AC-2): batch, a
  `ColumnsView` and a `RowRangeView` over it report the batch's buffer bytes.

Part B, `liquers-core/tests/asset_gc.rs` (the file `asset-garbage-collection` creates), on both
managers, with a test command `bytes_of(size: usize, tag: usize)` producing `Value::Bytes(vec![0;
size])` (queries `bytes_of-1048576-<i>`, validated with `liquers-validate --command bytes_of`):
- `gc09_query_bytes_budget` (AC-4): budget 10 MiB, 20 queries; after collection the report's
  `query_bytes` ≤ 10 MiB, queries 10..19 registered, 0..9 not (minus any touched).
- `gc10_held_entries_over_budget_reported` (AC-5): hold 12 `AssetRef`s; `over_budget`; all 12 kept.
- `gc11_cached_binary_counts` (AC-6): call `get_binary` on one entry; its counted size doubles.
- `byte_limits_round_trip_and_zero_rejected` (AC-8) in `environment_builder.rs`.

New `liquers-lib/tests/asset_gc_bytes.rs` (core values always answer `Some`, so the unknown-size
cases need a lib value):
- `gc12_unknown_sizes_count_zero` (AC-7): queries producing a `UIElement` (estimate `None`) and
  `Bytes` values; the `UIElement` entries count 0 and `unknown_size_entries` equals their number.
- `gc13_no_byte_limit_measures_nothing` (AC-9): a test `ForeignValue` whose estimate increments a
  static counter is never asked when no byte limit is set.

Commands: `cargo test -p liquers-core --lib estimates`, `cargo test -p liquers-records --lib
estimate`, `cargo test -p liquers-lib --lib estimates forward`, `CARGO_INCREMENTAL=0 cargo test -p
liquers-core --test asset_gc`, `cargo test -p liquers-lib --test asset_gc_bytes`.

## Learning Log

- Core tests cannot produce a `None` estimate (every core variant answers), so the unknown-size
  and counting cases run in `liquers-lib`, whose `UIElement` honestly has no size.
