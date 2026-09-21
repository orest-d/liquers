# Phase 3: Examples & Testing — Record streams

**Form: test-first.** The examples *are* the tests. They will not compile until Phase 4 implements
the types, which makes them the specification rather than an illustration of one, and makes Phase 4
a matter of making them pass.

The test code lives in [`phase3-tests.md`](./phase3-tests.md), organized by the file it will land
in. This document is the narrative: what the scenarios are, what they exercise, what goes wrong, and
what Phase 3 discovered that Phase 2 must absorb.

## High-Level Introduction

Three abstractions carry the design — a **source** that can be asked repeatedly for a stream, a
**stream** that is one traversal, and a **chunk** that is a materialized table. The examples walk
that spine in order: a single chunk built and filtered in memory, then a multi-chunk stream driven
by a manifest, then the places where each goes wrong.

Everything is `liquers-lib` behind the `records` feature; the values are `ExtValue::RecordChunk` and
`ExtValue::RecordSource`.

## Overview Table

| # | Scenario | Exercises | Where |
|---|---|---|---|
| 1 | **Files to CSV** — project a store directory into records, filter by size, serialize | `RecordSchema::new`, `RecordBatchBuilder`, `Bitmap` mask, `RecordBatch::filter`, `as_bytes("csv")` | §Example 1 |
| 2 | **A manifest-driven stream** — open `daily.manifest.yaml`, traverse chunk by chunk, traverse again | `RecordSource`, `ChunkList::Known`, `describe_chunk`, `stream()`, re-openability, `arguments`/`links` | §Example 2 |
| 3 | **Pitfalls** — the traps the design's own shape creates | offsets invariant, `Vector` child node, missing `TypeInfo`, missing cfg arm, wasm view detachment, chunk aliasing | §Example 3 |
| — | **Unit tests** (39) | schema validation, `Bitmap`, `RecordBatch` ops, nulls, `FieldValue` size, alignment, `ChunkId` | `phase3-tests.md` §1 |
| — | **Integration tests** (25) | end-to-end evaluation, re-openability, bounded memory, manifest round-trip, non-uniform chunks, feature matrix | `phase3-tests.md` §2 |
| — | **`RECORDS01`–`09`** | the language-binding reference implementations | `phase3-tests.md` §3 |

## Example Type

Conceptual code that is **intended to compile at Phase 4**, not runnable today. No `examples/`
binary: nothing can run until the types exist, and a demo would add a third thing to keep in sync
with the tests and the reference.

## Example 1: Files to CSV

### Connection to the High-Level Design

The shortest complete path through the design: build one chunk, filter it, serialize it. It touches
the identity rule (exactly one `Id` field, `Exact`-indexed and stored), the columnar layout, and the
mask-based filter that makes the layout worth having.

### Scenario

A caller wants the files under `data/reports/` as a table, keeping only those over 1 MB, as CSV.

### Sequence of Steps

1. Build a `RecordSchema`: `key.name` as the `Id` (Exact-indexed, stored), `meta.file_size` as
   `Numeric`, `meta.updated` as a `Timestamp`.
2. List the directory and append a row per entry through `RecordBatchBuilder`.
3. Wrap the batch as `ExtValue::RecordChunk`.
4. Evaluate a predicate over the size column into a `Bitmap`, and `RecordBatch::filter` by it.
5. `as_bytes("csv")`.

### Core Example Code

See [`phase3-tests.md`](./phase3-tests.md) §1.1. The command is `async fn` taking owned `State`
with `context` last, per the macro's rule, and uses `get_asset_info` — which never schedules, after
the repair the search design carries.

### The query

```
-R-dir/data/reports/-/ns-rec/file_records/ns-rec/records_to_csv/report.csv
```

## Example 2: A manifest-driven stream

### Scenario

`data/sales/daily.manifest.yaml` describes a stream of four chunks, each a SQL query over an offset
window. The statement itself is shared and lives in a linked `.sql` file; only the offset varies, in
the query, because **the query is the chunk's identity**.

### What it exercises

- `RecordSource::chunks()` — synchronous and cheap, returning `ChunkList::Known`.
- `describe_chunk()` — asynchronous, because a descriptor carries a `Metadata`.
- `stream()` called **twice**, yielding identical rows. This is the property the source/stream split
  exists for, and the test that would have failed under the old single-type model.
- `arguments` and `links` merging into each chunk's plan by parameter name, exactly as a recipe does.

### The manifest

See [`phase3-tests.md`](./phase3-tests.md) §2.4 for the complete file. The shape:

```yaml
manifest: record-stream
version: 1
number_format: "{:04}"
extension: csv
links:
  sql: -R/queries/daily_orders.sql     # a query, so the chunk DEPENDS on the statement
arguments:
  batch_size: 1000
chunks:
  - query: ns-sql/sql_query-0-1000
  - query: ns-sql/sql_query-1000-1000
```

## Example 3: Pitfalls and Edge Cases

Each of these is a real consequence of a decision the architecture made, not a hypothetical.

| Pitfall | What happens | Avoided by |
|---|---|---|
| `Text` offsets with `len` entries | Arrow needs **`len + 1`**, starting at 0; a consumer reads past the end or truncates the last value | The builder enforces it; a unit test asserts it |
| `Vector` exported as a sibling buffer | It is a `FixedSizeList` whose values live in a **child** node; a flat export produces a malformed array | The export emits the child; `RECORDS03` checks it |
| A new `ExtValue` variant without a `TypeInfo` | The type **cannot be stored** — the write path refuses an unregistered identifier | `type_descriptions` covers both variants; an integration test asserts it |
| A `match` on `ExtValue` without a `#[cfg(feature = "records")]` arm | `--no-default-features` fails to compile | The build-matrix rows |
| A JS typed-array view held across a call into wasm | Heap growth **detaches** it; reads throw or return garbage | Create views at point of use; the wrapper revalidates by buffer identity |
| A per-chunk value in `arguments` with no `ChunkKeys` | Two chunks alias to one asset — **silently**, returning the first's data twice | Rejected at manifest load |
| A command whose per-chunk parameters are not first | Chunk queries need an empty positional placeholder: `sql_query--1000-1000` | Signature ordering, documented in the guide |

## Corner Cases

### 1. Memory

One batch resident during traversal, never the whole stream — the property the whole chunked design
exists for, and the one most easily lost by a `collect()` slipped in for convenience. Tested by
asserting how many batches are alive at once, not by measuring bytes.

### 2. Concurrency

Buffers are `Arc`-shared and immutable once built, so `select`, `slice` and a column hand-off copy
nothing and are safe to share. No lock is held across an `.await`. Two traversals of one source are
independent — which is what makes an HTTP handler able to open its own stream per request.

### 3. Errors

Every rejection names what is wrong: `concat` names the first differing field, a bad schema names
which invariant failed, an ambiguous unqualified field name lists every candidate. Typed
constructors throughout; `Error::new` appears nowhere.

### 4. Serialization

`RecordChunk` writes json / ndjson / csv; `RecordSource` writes its manifest. A non-uniform stream
serializes as NDJSON and **fails** as a single CSV, naming the reason — there is no single header.

### 5. Feature gating

`--no-default-features` must compile with no records module, no variants reached, and no `bytemuck`
in the dependency graph. This is the failure mode a cfg-gated enum variant causes, and the reason
the matrix rows exist.

## Test Plan

| Group | Count | File |
|---|---|---|
| Unit — schema, bitmap, batch ops, nulls, sizes, alignment, `ChunkId` | 39 | `liquers-lib/src/records/{mod,buffer}.rs` |
| Integration — end-to-end, re-openability, memory, manifest, serialization, features | 25 | `liquers-lib/tests/record_streams_*.rs` |
| Language-binding reference — `RECORDS01`–`09` | 9 | native: `liquers-lib`; wasm: `liquers-web` (`RECORDS05`, `RECORDS06`) |
| **Total** | **73** | |

`RECORDS05` (a view surviving host-heap growth) and `RECORDS06` (handle release via
`debug-handles`, as `RUNTIME05` already does) are `liquers-web` tests and run in the browser loop,
not the native one.

## What Phase 3 found that Phase 2 must absorb

Writing tests against the architecture surfaced API the architecture does not declare. These are
**findings, not inventions to wave through**: a test-first phase cannot compile against a surface
that does not exist, so **Phase 2 takes an amendment before Phase 4 starts.**

### Decisions needed

**1. `RecordBatchBuilder`'s append surface — three drafters, three incompatible APIs.**

Phase 2 declares `pub struct RecordBatchBuilder { /* … */ }` with **no methods at all**, and three
agents working independently each invented a different one:

| Pattern | Shape | Trouble |
|---|---|---|
| Per-column typed | `append_text(..)`, `append_uint(..)`, then `finish_row()` | Order-dependent, and `finish_row()` is easy to forget — a silent row miscount |
| Row-at-a-time | `append_row(vec![FieldValue::Int(1), …])` | A `Vec` and a boxed value per row, which is what the columnar layout exists to avoid |
| No builder | construct `RecordBatch { columns: vec![…], .. }` directly | Fine in a test, not an API |

Three independent inventions is the signal that a decision was deferred rather than made.
**Recommended: `append_row(&[FieldValue])` as the primary** — a slice not a `Vec`, hard to misuse,
and the obvious thing for a test to call — **with typed per-column pushes available for bulk paths**
where the per-value enum actually costs something. That is two entry points to one builder, not two
builders.

**2. Is `RecordBatch::columns` public, or reached through `column(i)`?** §1.2 indexes the field
directly; the `RECORDS` tests call a method. Both can exist, but the tests should not disagree about
which is idiomatic.

**3. Does `Bitmap` implement `Iterator`, or only `get(i)`?** Phase 2 lists `get`, `and`, `or`, `not`,
`count_ones` — no iteration. One scenario iterates. Both can coexist; pick the one the docs teach.

### Straightforward additions

| Needed | Status in Phase 2 | Disposition |
|---|---|---|
| `Column::gather(&mask)` | `RecordBatch::filter` is described as "gather by mask"; the column-level primitive it is built on is not declared | Declare it |
| `FieldRole::and_stored()` / `.and_fast()` | Named in prose — "composing with `.and_stored()` and `.and_fast()`" — never declared | Declare them |

### Already corrected in the tests

- **`ChunkOrigin` in `RECORDS01` would not have compiled.** It used `asset_query: Option<Query>`,
  omitted the required `chunk: Query`, and named `info` as `asset_info`. Phase 2 is authoritative;
  the test now matches it.
- **`Column::get_value` → `value`**, matching Phase 2's declared `RecordBatch::value(row, column)`.

None of this changes the architecture. All of it is surface Phase 2 left as an ellipsis, which a
narrative phase could tolerate and a test-first phase cannot.

## Documentation and Learning Log

- **The re-openability test is the one that earns the three-way split.** Under the earlier
  single-`RecordStream` model it could not have been written: a second traversal either failed or
  silently returned nothing. It is worth keeping prominent in `RECORD_STREAMS.md` for that reason.
- **`RecordBatchBuilder` being an ellipsis was not obvious until code was written against it.** Two
  independent drafters invented compatible-but-different append surfaces, which is the signal that a
  decision was deferred rather than made.
- **The guide's `RECORDS01`–`09` now have Rust counterparts**, which was the point of doing Phase 3
  test-first — a binding author gets reference code rather than a one-line summary.
- For `RECORD_STREAM_GUIDE.md`: the pitfalls table above is the guide's pitfalls section, and the
  two scenarios are its two walkthroughs. Phase 5 should lift rather than rewrite them.

## Requirements carried into this phase

### Identify the tests that belong to the language integration guide

`guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE now prescribes `RECORDS01`–`RECORDS09` for any
*language* binding that exposes record values. **Phase 3 must decide which of its own tests are the
Rust-side counterparts of those**, so a binding author has a reference implementation rather than a
one-line summary — the guide's §3 explicitly says its appendix pseudocode "often fixes the contract
more narrowly than the one-line summary suggests".

At minimum, Phase 3 identifies the Rust test that establishes each of:

| Guide test | What Phase 3 must have a counterpart for |
|---|---|
| `RECORDS01` | a chunk round-trips with schema, roles and `ChunkOrigin` intact |
| `RECORDS02` | a column read matches a copy |
| `RECORDS03` | an Arrow export equals the source data; metadata survives or its loss is asserted |
| `RECORDS04` | buffers are read-only |
| `RECORDS05` | a view survives host-heap growth, or fails loudly |
| `RECORDS06` | releasing a handle releases the value |
| `RECORDS07` | a manifest-backed source is traversed one chunk at a time, nothing else resident |
| `RECORDS08` | async and sync traversal of one source yield identical rows |

`RECORDS09` is a binding-only disposition and needs no Rust counterpart.

### Other requirements gathered during Phase 2

- **The growth test named in Phase 2** — force `memory.grow` between creating a typed-array view and
  reading it, and assert the wrapper refreshed transparently. It is the test most likely to be
  skipped and the one that catches the browser hazard.
- **A round-trip per serialization format**, since `DefaultValueSerializer` is where a missing arm
  surfaces.
- **The build-matrix rows** for `records` on and off, including the wasm target.
- **Manifest validation**: each chunk query plans, `arguments` names exist in the last action, and no
  chunk name collides with a sibling `recipes.yaml`.
- **The identity regimes**: a manifest using per-chunk `arguments` without a `ChunkKeys` must be
  rejected, because the failure is otherwise silent aliasing.


