# Phase 2: Solution & Architecture — Memory size estimate and byte budget

## Overview

**Part A, the estimate.** A provided `estimated_memory_size(&self) -> Option<usize>` (default
`None`) on `ValueInterface` (core), `ValueExtension` and `ForeignValue` (lib) and `RecordView`
(records). Core `Value` and `SimpleValue` compute it from their own containers; `ExtValue` asks
each payload; `CombinedValue` forwards to the half it holds. **Part B, the budget.** Two
`AssetManagerOptions` fields become `CacheLimits::key_bytes` / `query_bytes`. After the count and
idle steps of `collect_garbage`, a byte step measures every entry (cached per registration), sums
the map, and removes the oldest collectable entries until the total is at most 90% of the budget.

Rejected alternatives:
- **Measure at finalization and store on `AssetData`** — the collector would need the asset's lock
  anyway to read the binary length, and finalization is the hot path.
- **`std::mem::size_of_val` / an allocator hook** — shallow, or global and not per asset.
- **A required trait method** — every external `ValueInterface` / `ForeignValue` would break; a
  default `None` is honest.
- **A size field in metadata** — `value-summary` owns metadata facts; memory is a runtime fact that
  varies with sharing.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-ASSET-GC` | accepted, designed | P3 | provides the collector part B extends | yes, for part B | no for part A | `requires` |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted, designed | P2 | moves sibling methods | no | no | rebase |
| `CORE-VALUE-ENUM-OVERSIZED` | draft, designed (ready) | P2 | core variants behind `Arc` | no | no | estimate reads through `Arc` |
| `VALUE-DESCRIPTION` | accepted, designed (`value-summary`) | — | sibling hook (serialized shape) | no | no | distinct contract |

## Interfaces

```rust
// liquers-core/src/value.rs — ValueInterface, provided
/// Approximate bytes of memory this value holds (heap payload plus the value itself), or `None`
/// when the type cannot tell. Must be cheap: may walk the value's own containers, must not decode,
/// materialize, block or call into a foreign runtime. Shared payloads are counted in full.
fn estimated_memory_size(&self) -> Option<usize> { None }

// liquers-lib/src/value/extended.rs — ValueExtension, provided (same doc and default)
fn estimated_memory_size(&self) -> Option<usize> { None }
// liquers-lib/src/value/foreign.rs — ForeignValue, provided (same)
fn estimated_memory_size(&self) -> Option<usize> { None }
// liquers-records/src/batch.rs — RecordView, provided (same)
fn estimated_memory_size(&self) -> Option<usize> { None }
// liquers-records/src/column.rs
impl Column { pub fn buffer_bytes(&self) -> usize; }   // values + offsets + validity buffers

// liquers-core/src/assets.rs (extends asset-garbage-collection's types)
pub struct CacheLimits { /* keys, queries, max_idle */ pub key_bytes: Option<u64>,
                         pub query_bytes: Option<u64> }
#[non_exhaustive] pub struct GcReport { /* keys, queries */
    pub key_bytes: Option<u64>, pub query_bytes: Option<u64>,      // totals after the pass
    pub over_budget: bool, pub unknown_size_entries: usize }
// AssetManagerOptions: max_cached_key_bytes: Option<u64>, max_cached_query_bytes: Option<u64>
```

All sync except `collect_garbage` (async, as specified there).

## Integration Points

Part A:
- `liquers-core/src/value.rs` `impl ValueInterface for Value`: explicit `match` (no `_`):
  `None | Bool | I32 | I64 | F64` → `size_of::<Value>()`; `Text(s)` → `+ s.capacity()`;
  `Bytes(b)` → `+ b.capacity()`; `Array(v)` → `+` sum of elements; `Object(m)` → `+` keys' and
  values' sums; `Metadata`, `AssetInfo`, `Recipe`, `CommandMetadata`, `Query`, `Key` →
  `size_of::<Value>()` plus their string and vector lengths where a field is a `Vec` or `String`
  (an approximation; documented). `Some` for every variant.
- `liquers-lib/src/value/simple.rs` `SimpleValue`: the same rules for its variants.
- `liquers-lib/src/value/mod.rs` `impl ValueExtension for ExtValue`: explicit `match` with every
  `#[cfg]` arm: `Image` → `value.as_bytes().len()`; `PolarsDataFrame` → `value.estimated_size()`;
  `RecordView` → `value.estimated_memory_size()`; `RecordSource`, `UIElement`, `UiCommand`,
  `Widget` → `None`; `Foreign` → `value.estimated_memory_size()`.
- `liquers-lib/src/value/extended.rs` `CombinedValue`: forward to `Base` or `Ext`.
- `liquers-records`: `Column::buffer_bytes`; `RecordBatch` sums its columns; `RecordBatchMut`
  likewise; `ColumnsView`, `RowRangeView`, `RowIndexView` forward to `base` (the retained buffer,
  question 2); `AppendedColumnsView` and `PlacedView` sum their parts; other views keep `None`.

Part B (`liquers-core/src/assets.rs`, on top of `asset-garbage-collection`):
- `Registration<E>` gains `measured: std::sync::Mutex<Option<Measured>>` with
  `Measured { value_ptr: usize, binary_len: usize, bytes: u64, known: bool }`; `measure(&self)`
  reads the asset under its read lock, reuses the cached figure when the value `Arc` pointer and
  binary length are unchanged, otherwise calls `estimated_memory_size` (None → 0, `known: false`).
- `collect_garbage`: when a byte limit is set for a map, after the count/idle removals, measure
  every remaining entry, total them, and remove collectable candidates oldest-first (same removal
  predicate, same re-check of strong count and stamp) until total ≤ `limit * 9 / 10`. Report totals,
  `over_budget` (still above the limit) and `unknown_size_entries`.
- Triggers: when a byte limit is set, every successful insert triggers a pass (the queued manager's
  collector drains and coalesces `OverLimit` messages; the inline manager runs it inline), and the
  periodic sweep runs every 10 s when no idle limit sets a shorter interval.
- `environment_builder.rs`: the two fields, `with_max_cached_key_bytes` /
  `with_max_cached_query_bytes`, zero rejected with `ErrorType::ConfigurationError` as the count
  fields are.

## Error Handling

The estimate is infallible by contract (`Option`, never `Result`). Collection never fails
(unchanged). Configuration: a zero budget is a build error. No new `ErrorType`.

## Relevant Commands

None; no registry change. (A command showing an asset's estimate is a possible follow-up.)

## Documentation Architecture

`VALUE_TYPE_SYSTEM.md` (the method's contract and per-type sources), `TYPE_SYSTEM_GUIDE.md` (an
optional step), `ASSETS.md` and `ENVIRONMENT_CONFIG.md` (the budget, its enforcement window, what
counts), `ENVIRONMENT_CONSTRUCTION_GUIDE.md` (the internet-facing example with both limits).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `value.rs`, `assets.rs`, `environment_builder.rs`; records `batch.rs`, `column.rs`, `views.rs`, `mutable.rs`; lib `value/mod.rs`, `simple.rs`, `extended.rs`, `foreign.rs` |
| Crates and workflows affected | three crates; behaviour changes only when a byte limit is set |
| Existing tests likely to change | none (provided methods); `asset_gc.rs` gains tests |
| New validation | per-variant unit tests; `asset_gc` byte tests on both managers; `scripts/check-build-matrix.sh` (every `#[cfg]` arm of `ExtValue`) |
| Compatibility / data / concurrency / performance / security | additive; estimates are approximate (documented); a pass measures each new value once; a budget set below the held working set reports `over_budget` rather than thrashing |
| Recovery | part B revertible alone; part A is inert |
| Certainty and open questions | high on structure; polars / image method names to confirm at the pinned versions; questions 1-3 are policy |
