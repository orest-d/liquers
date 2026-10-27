# Phase 1: High-Level Design — Memory size estimate and byte budget

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new provided `pub` trait methods in `liquers-core`,
  `liquers-records` and `liquers-lib`, new configuration fields (rules 3, 4), three crates
  (rule 6), `L`, open decisions (rule 5).
- **Leading issue:** **Open design - a value of unknown size.** Recommended: counts as zero, and the
  report says how many such entries the map holds.
- **Explanation:** every variant's payload was read at HEAD; each has a cheap size source (lengths,
  `DataFrame::estimated_size`, image buffer length, record column buffers) or honestly has none
  (UI elements, record sources). The budget slots into the collector of `asset-garbage-collection`,
  which must land first. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - unknown size:** count as 0 (recommended: never collects a value for a size it
     does not have; a UI element or a lazy source is small or unknowable, and the count limits still
     bound it), or as a configurable `unknown_value_size` (conservative, but a guess that evicts
     real data for phantom bytes).
  2. **Open design - shared payloads:** a payload shared by several assets (`Arc` clones, record
     views over one batch, a frame passed through unchanged) is counted once **per asset**
     (recommended: an over-estimate that is cheap and never under-bounds memory), or deduplicated by
     pointer (exact, but a per-pass set of payload addresses and a trait method returning
     identities).
  3. **Open design - names:** `ValueInterface::estimated_memory_size(&self) -> Option<usize>`
     (recommended: "memory" distinguishes it from the serialized `bytes` of `value-summary` and
     from `file_size`), and `max_cached_key_bytes` / `max_cached_query_bytes` beside
     `max_cached_keys` / `max_cached_queries`.
  4. **Proposed resolution - what an entry costs:** value estimate plus the cached binary's length
     (`AssetData::binary`), plus nothing for metadata (small next to a value that needs a budget).
  5. **Proposed resolution - cost rule:** an estimate may walk the value's own containers (core
     `Array` / `Object`) but never decodes, materializes or calls into a foreign runtime; it is
     computed in the collection pass, cached per registration while the value is unchanged, never
     on the evaluation path.

## Purpose

An internet-facing service bounds its in-memory cache in bytes, separately for recipes and for
ad-hoc queries (maintainer request in `CORE-ASSET-GC`, 2026-10-10). A count limit cannot do this:
a few large DataFrames exceed the memory a count was chosen for. Values need a size estimate, and
the collector needs a byte budget (VISION: server / data service).

## Problem Example

```yaml
assets: { max_cached_queries: 1000 }        # asset-garbage-collection, count only
```

A server answers ad-hoc queries; most produce a few KB, but `-R/data/big.csv/-/ns-pl/from_csv`
variants (validates) each hold a 400 MB frame. Ten of them fit the count limit and take 4 GB. Today
nothing can say how large a value is: `ValueInterface` and `ExtValueInterface` have no such method.
Should be:

```yaml
assets: { max_cached_queries: 1000, max_cached_query_bytes: 1073741824 }   # 1 GiB
```

and after a collection the query map's values (and cached binaries) total at most 1 GiB, keeping
the most recently used, never collecting one a caller holds.

## Scope and Acceptance Criteria

- **AC-1** Core values estimate their size
  WHEN `estimated_memory_size` is called on core `Value::Text(s)`, `Bytes(b)`, `Array`, `Object`
  THEN it returns `Some(n)` with `n >= s.len()`, `n >= b.len()`, and at least the sum over the
  elements; scalars return `Some(size_of::<Value>())`
- **AC-2** Extended values estimate their size
  WHEN it is called on a `polars.DataFrame`, an `Image`, a `RecordView` over a `RecordBatch`
  THEN it returns `Some(n)` with `n >= DataFrame::estimated_size()`, `n >= width × height × bytes
  per pixel`, `n >=` the batch's column buffer bytes; a `RecordSource`, `UIElement`, `UiCommand` and
  `Widget` return `None`
- **AC-3** Delegation and defaults
  WHEN a `CombinedValue` holds either half, or an `ExtValue::Foreign` wraps a payload overriding the
  method, or a value type does not override it
  THEN the half's / payload's answer is returned, and a non-overriding type returns `None` and
  compiles unchanged
- **AC-4** Byte budget per map
  WHEN `max_cached_query_bytes` is 10 MiB and 20 distinct queries each produce a 1 MiB value and
  their `AssetRef`s are dropped, then the collector runs
  THEN the query map's counted total is at most 10 MiB and the most recently used entries stay
- **AC-5** Held, running and sticky entries are never collected
  WHEN entries a caller holds, in-flight entries, or `Source` / `Override` entries alone exceed the
  budget
  THEN none of them is collected and the report says the map is over budget
- **AC-6** The cached binary counts
  WHEN an entry holds a 1 MiB value and a 1 MiB cached binary
  THEN it counts 2 MiB
- **AC-7** Unknown sizes (question 1)
  WHEN entries' values return `None`
  THEN they count as zero and the report gives their number
- **AC-8** Configuration
  WHEN an environment document sets `max_cached_key_bytes` / `max_cached_query_bytes`
  THEN the built manager enforces them; `0` is rejected at build time; unset fields are omitted
  when serialized
- **AC-9** No budget, no change
  WHEN neither byte field is set
  THEN no estimate is computed and collection behaves as `asset-garbage-collection` specifies

Non-goals: exact accounting (allocator-level); deduplicating shared payloads (question 2); a
global budget across both maps; a budget for the dependency graph; Python and JavaScript
overrides (their values return `None` until a binding adds one); putting the size in metadata
(`value-summary` carries serialized shape, not memory).

## Core Interactions

- **Value types:** a provided method on `ValueInterface`, `ValueExtension`, `ForeignValue` and
  `RecordView`; implementations for every core, simple and extended variant.
- **Assets:** the collector pass and its triggers; two options; `CacheLimits` and `GcReport` fields.
- **Configuration:** `AssetManagerOptions` (`ENVIRONMENT_CONFIG.md`).

## Crate Placement

`liquers-core` (trait method, core `Value`, the budget in both managers); `liquers-records`
(`RecordView` method, `Column` buffer size, `RecordBatch` override); `liquers-lib` (`SimpleValue`,
`ExtValue`, `CombinedValue`, `ValueExtension`, `ForeignValue`). Dependency flow unchanged.

## Documentation Intent

- Reference: `specs/reference/VALUE_TYPE_SYSTEM.md` (the estimate and its contract),
  `specs/reference/ASSETS.md` (§When an asset is kept for reuse: the byte budget),
  `specs/reference/ENVIRONMENT_CONFIG.md` (two fields).
- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md` (an optional step: estimate your size),
  `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (bounding the cache in bytes).
- Documents to update: the five above, History rows and `reviewed:` bumps.

## Design Dependencies

- `asset-garbage-collection` (needs-decision, not implemented) — **requires** for part B:
  `Registration<E>`, `CacheLimits`, `GcReport`, `collect_garbage`, the collector task. Part A
  (the estimate) does not need it and can land first.
- `value-interface-capability-split` (needs-decision) — **overlaps**, weak: it moves identity
  methods into a `ValueType` trait; the estimate stays on `ValueInterface` / `ValueExtension` /
  `ForeignValue` whichever lands first, and the second rebases.
- `value-summary` (needs-decision) — **overlaps**, weak: a sibling hook with the same delegation
  pattern; different contract (serialized shape in metadata vs runtime memory).
- `core-value-enum-oversized` (ready) — **overlaps**, weak: moves large core variants behind
  `Arc`; the core estimate then reads through the `Arc`. Either order.
- `serialized-binary-disposal-policy` — **overlaps**, weak: releasing cached binaries lowers the
  counted totals (AC-6).

## Consolidated Findings

- Part A is independent and useful alone (an admin view, logs); part B is a small extension of the
  collector once it exists: one more ordering pass over the same candidates.
- Sizes are measured in the collection pass, not on insert: a value does not exist yet when its
  registration is inserted. A budget is therefore enforced within one collector trigger (an insert
  or a sweep) of the value appearing, not at the instant of finalization; stated in the reference.
- Measurements are cached per registration and keyed by the value's `Arc` pointer and the binary's
  length, so a pass costs one estimate per new value, not per entry.
- Per-asset counting over-estimates shared payloads (question 2), which only ever collects earlier
  than an exact count would.
- `DataFrame::estimated_size` and `DynamicImage::as_bytes` are the size sources; their exact names
  are confirmed against the pinned crate versions in Phase 4 step 3.
