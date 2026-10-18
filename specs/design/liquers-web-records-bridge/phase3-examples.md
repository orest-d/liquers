# Phase 3: Examples & Use-cases — liquers-web-records-bridge

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Stream a source in a page | `for await` over `src.stream()` | AC-1, AC-2 |
| 2 | Example | Hand a batch back to a command | a JS command returns a received batch | AC-5 |
| 3 | Test | `records01_a_batch_handle_crosses_back_as_the_same_view_with_origin_intact` | same `Arc`, roles, origin | AC-5 |
| 4 | Test | `records02_a_column_reads_the_same_through_a_view_and_a_copy` | descriptor view = `columnCopy` | AC-8 |
| 5 | Test | `records04_a_column_copy_is_detached_from_the_batch` | writing the copy leaves the batch | AC-8 |
| 6 | Test | `records07_chunk_ids_are_data_and_evaluate_one_at_a_time` | route 1 | AC-3 |
| 7 | Test | `records08_the_async_iterator_yields_the_same_rows_as_materialize` | route via async iterator | AC-1, AC-2 |
| 8 | Test | `records_source_materialize_is_bounded_by_max_rows` | `max_rows` | AC-4 |
| 9 | Test | `records_a_freed_handle_is_refused_by_name` | freed batch/source | AC-6 |
| 10 | Test | `records_ns_rec_commands_run_in_the_web_environment` | `ns-rec` registered | AC-7 |
| 11 | Test | `records06_releasing_a_source_handle_releases_the_source` | registry and live count | AC-1 |
| 12 | Test | `records_a_command_returning_its_batch_hands_rust_the_same_view` | Example 2 | AC-5 |
| 13 | Check | README `## Conformance` RECORDS rows | dispositions | AC-9 |

## Example 1: Stream a source in a page

A page reads a stored manifest as a source and sums rows chunk by chunk, holding one batch at a time.

```js
const src = await liquers.evaluate("-R/data/sales.manifest/-/ns-rec/to_record_source");
let rows = 0;
for await (const batch of await src.stream()) {   // chunks evaluate in the web environment
  rows += batch.numRows;
  batch.free();
}
const all = await src.materialize(100000);        // same rows, one RecordBatch
console.assert(all.numRows === rows);
```

## Example 2: Hand a batch back to a command

```js
liquers.registerCommand({ name: "keep", state: "value",
  run: (batch) => batch });                       // receives a RecordBatch, returns it unchanged
await liquers.evaluate("rows/ns-rec/from_json-records/keep/ns-rec/nrows");   // 3
```

Rust receives `ExtValue::RecordView` holding the same `Arc<RecordBatch>`; a downstream
`ns-rec/nrows` reads it without a JSON round trip. A page that wants to build a table itself
returns plain row objects and appends `ns-rec/from_json-records`.

## Edge and Error Cases

- Freed handle passed back → `ConversionError` naming the handle kind (test 9).
- `ChunkList::Unbounded` → `bounded === false`, `chunks()` lists only the computed ids (test 6
  asserts `bounded` on a known list; unbounded is covered by `liquers-records`' own tests).
- Two `nextBatch()` calls without awaiting → the second rejects (unit test in `records.rs`
  `tests::next_batch_refuses_a_concurrent_call`, Node).
- Iterator abandoned with `break` → `return()` frees the stream; the source handle stays live.
- Environment rebuilt between `stream()` calls → each call resolves the environment anew.
- Single-cell view → a scalar, not a handle (owned by `web-records-single-cell-scalar`).

## Test Plan

All in `liquers-web/tests/records_RECORDS.rs` (Node loop) unless noted, using `InMemorySource` for
sources without chunk evaluation and a `ManifestSource::new(spec, None)` with explicit
`ChunkId::Query` chunks over a test-registered command for route 1:

- `records01_a_batch_handle_crosses_back_as_the_same_view_with_origin_intact` (AC-5): a batch with
  an `Id`-role field, `chunk_id` and one `ChunkOrigin` → `value_to_js` → `js_to_value` →
  `Arc::ptr_eq` with the original; `chunkIdJson()`/`sourcesJson()` equal the batch's.
- `records02_a_column_reads_the_same_through_a_view_and_a_copy` (AC-8): Int, Float, Text columns.
- `records04_a_column_copy_is_detached_from_the_batch` (AC-8): write into `columnCopy(0)`, re-read
  `column(0)`'s view unchanged.
- `records07_chunk_ids_are_data_and_evaluate_one_at_a_time` (AC-3): `chunks()` returns `n` entries
  with `kind`/`query` and evaluates nothing (the test command's call counter stays 0); evaluating
  each `query` yields the same rows as `materialize`.
- `records08_the_async_iterator_yields_the_same_rows_as_materialize` (AC-1, AC-2): drive
  `Symbol.asyncIterator` from Rust via `Reflect`; batch count and concatenated column values equal
  `materialize(1000)`.
- `records_source_materialize_is_bounded_by_max_rows` (AC-4): 3 views × 2 rows; `materialize(5)`
  rejects naming `max_rows`; `materialize(6)` resolves with 6 rows.
- `records_a_freed_handle_is_refused_by_name` (AC-6): `free()` then pass back → error message
  contains "freed RecordBatch"; same for a source.
- `records_ns_rec_commands_run_in_the_web_environment` (AC-7): register JS command `rows`
  returning three row objects; `rows/ns-rec/from_json-records/nrows` evaluates to 3 (query
  validated with `liquers-validate --command rows`).
- `records06_releasing_a_source_handle_releases_the_source` (AC-1; `debug-handles`): live handle
  count returns to baseline and the registry no longer holds the id after `free()`.
- `records_a_command_returning_its_batch_hands_rust_the_same_view` (AC-5, AC-7): Example 2's
  `keep` command; `rows/ns-rec/from_json-records/keep/ns-rec/nrows` evaluates to 3 (validated).
- Conformance record (AC-9): reviewed in Step 6 — every `RECORDS` ID has a row naming its test or
  its `NA` with the reversing condition.
- Stubs: `./liquers-web/scripts/check-stubs.sh` lists `RecordSource` and `RecordStream`.
- Command to run: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test records_RECORDS`.

## Learning Log

- A page query cannot produce records until `ns-rec` is registered in the browser; every earlier
  RECORDS test built its batch in Rust.
- Traversal resolves the web environment at each call (`with_global()`), so no new accessor is needed.
- `liquers-validate` accepts `rows/ns-rec/from_json-records/nrows` and
  `-R/data/sales.manifest/-/ns-rec/to_record_source` against the committed registry.
