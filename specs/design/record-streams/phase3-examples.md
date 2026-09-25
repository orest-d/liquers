# Phase 3: Examples & Testing — Record streams

**Form: test-first.** The examples *are* the tests. They will not compile until Phase 4 implements
the types, which makes them the specification rather than an illustration of one, and makes Phase 4
a matter of making them pass.

The test code lives in [`phase3-tests.md`](./phase3-tests.md), organized by the file it will land
in. This document is the narrative: what the scenarios are, what they exercise, what goes wrong, and
what Phase 3 discovered that Phase 2 must absorb.

## High-Level Introduction

Three abstractions carry the design — a **source** that can be asked repeatedly for a stream, a
**stream** that is one traversal, and a **view** that is a finite table, of which a `RecordBatch` is
the materialized kind. **This narrative predates the trait form of Phase 2** — see §"Reworked by the
Phase 2 trait revision". The examples walk
that spine in order: a single chunk built and filtered in memory, then a multi-chunk stream driven
by a manifest, then the places where each goes wrong.

The records are the `liquers-records` crate, and `liquers-lib` holds the glue behind its `records`
feature; the values are `ExtValue::RecordView` and
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
2. List the directory and append a row per entry through a `RecordBatchMut` sized `with_capacity`.
3. Wrap the batch as `ExtValue::RecordView`.
4. Evaluate a predicate over the size column into a `Bitmap`, and `RecordBatch::filter` by it.
5. `as_bytes("csv")`.

### Core Example Code

See [`phase3-tests.md`](./phase3-tests.md) §1.1. The command is `async fn` taking owned `State`
with `context` last, per the macro's rule, and uses `get_asset_info` — which never schedules, after
the repair the search design carries.

### The query

```
-R-dir/data/reports/-/ns-rec/file_records/report.csv
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

`RecordView` writes json / ndjson / csv; a source writes **only its manifest**, and its rows reach
bytes through `ns-rec/materialize`. A non-uniform source **cannot be materialized** — `materialize`
fails, naming the first differing field — so its rows are exported chunk by chunk.

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

## Reworked by the Phase 2 trait revision (2026-09-24)

**Phase 3 needs rework before it is approved.** Phase 2 changed from data structures to interfaces:
`RecordSource`, `RecordStream` and a new `RecordView` are traits; `RecordBatch` is the materialized
view; the value variants are `ExtValue::RecordView` and `ExtValue::RecordSource`, holding trait
objects. The narrative above and the code in `phase3-tests.md` were written against the earlier
form, and **40 functions** there touch something that changed. None of the changes alters what a
test *asserts*; each alters how it gets its value.

| Group | Change | Tests |
|---|---|---|
| **Renames** | `RecordChunk` → `RecordView` (variant, identifier, `TypeInfo`); the JS handle class `RecordChunk` → `RecordBatch` | `test_end_to_end_record_chunk_serialization`, `test_record_chunk_type_info_registered`, `test_type_descriptions_match_identifiers`, `test_record_chunk_{csv,json,ndjson}_serialization`, `test_records_feature_gated`, `test_per_chunk_arguments_without_cache_rejected`, `records04`, `records05`, `records06`, and scenario 1's helpers |
| **Batch operations become view constructors** | `RecordBatch::{select, filter, slice, value, with_columns}` → `select_columns`, `filter`, `slice`, `value`, `with_column`/`with_columns` on `Arc<dyn RecordView>`. Results are views, compared through `materialize()`. `select` by index becomes `select_columns` by name, **and must now assert that the `Id` column is kept** | `batch_select_zero_copy_projection`, `batch_filter_by_mask`, `batch_filter_length_mismatch_errors`, `batch_slice_preserves_arc_sharing`, `batch_value_reads_single_cell`, `column_null_distinct_from_empty_string`, `batch_with_columns_appends_derived_fields`, scenario 1's filtering and `records_to_csv` |
| **Source construction** | `RecordSource { backing: SourceBacking::… }` → `ManifestSource::new` / `InMemorySource::new`; the `uniform_schema` field → the `schema()` method | `test_record_source_reopenable`, `test_record_source_manifest_round_trip`, `test_non_uniform_chunks_ndjson_succeeds`, `test_non_uniform_chunks_single_csv_fails`, `test_manifest_template_unbounded`, `test_manifest_yaml_deserialization`, `test_chunk_descriptor_serialization`, `records07` |
| **Serializing a source** | A source serializes **only as a manifest**; its rows go through `materialize`. `records_to_csv` / `records_to_ndjson` are gone — the trailing filename chooses the format. `test_non_uniform_chunks_ndjson_succeeds` **inverts**: `materialize` of a non-uniform source must fail naming the first differing field, and per-chunk NDJSON succeeds | `test_non_uniform_chunks_ndjson_succeeds`, `test_non_uniform_chunks_single_csv_fails`, `test_end_to_end_record_chunk_serialization`, scenario 1's `records_to_csv` |
| **Test locations** | Records became their own crate. Tests of the data model, views, formats, readers, manifests and the provider move from `liquers-lib/src/records/…` to `liquers-records/src/…` and `liquers-records/tests/`, and lose their `#[cfg(feature = "records")]` gates. Tests of the `ExtValue` variants, `TypeInfo`, the scalar hooks, `to_record`, the `ns-rec` commands and the polars bridge stay in `liquers-lib`, gated. The `RECORDS` counterparts split the same way | every file header in `phase3-tests.md` naming `liquers-lib/src/records/` |
| **The builder** | `RecordBatchBuilder` / `ColumnBuilder` → `RecordBatchMut` / `ColumnMut` (`with_capacity`, `append_row`, `freeze`), behind the `RecordViewMut` trait. The explicit `Id` is optional, so schemas in fixtures need not declare one | every test building a batch, `test_record_batch_builder`, scenario 1 |
| **Opening a stream** | `source.stream(&context)` → `Arc::clone(&source).stream(resolver)`, with a `ContextResolver` inside a command and an `EnvResolver` outside one. Items are `Arc<dyn RecordView>`, so a test comparing rows materializes them | `test_record_source_reopenable`, `test_streaming_bounded_memory`, `records08`, scenario 2's consumption code |

Tests comparing two `RecordBatch`es directly — `batch_concat_same_schema`, `test_record_batch_builder`,
`test_empty_record_batch`, `records01` — are unaffected: `RecordBatch` keeps `PartialEq`.

**New tests the revision requires**, none of which the earlier form could have expressed:

| Test | Asserts |
|---|---|
| `view_reads_agree_with_column_range` | For every built-in view, `column`, `value` and `materialize` equal what `column_range` gives — the property test of Phase 2 open question 12 |
| `batch_materialize_is_shallow` | `materialize()` on a batch shares every buffer (`Arc::ptr_eq`) |
| `select_columns_keeps_key_columns` | `Id` and `Source` survive a projection that does not name them |
| `filter_over_filter_stacks` | Two filters compose to the intersection, with indices mapped through both layers |
| `row_fn_view_computes_only_the_range` | A `RowFnView`'s closure is called exactly for the requested column and rows |
| `single_cell_view_reads_as_scalar` | One row, one payload column: `try_into_i64`, `try_into_f64`, `try_into_string` agree with the equivalent base `Value`; `Null` gives `None` through the `_option` forms; a `select_columns-id` view reads as the id |
| `larger_view_refuses_scalar` | Two rows, or two payload columns, refuse with an error naming the shape |
| `single_cell_view_binds_to_linked_argument` | Through a recipe `links:` entry, into an `f64` argument. **Needs `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS` fixed** |
| `tiny_results_release_their_base` | `rec_id`, `row` and `head` return a batch (`as_batch().is_some()`), and dropping the base frees it |
| `materialize_refuses_past_max_rows` | The limit is an error naming how to raise it, not a silent truncation |
| `materialize_command_serializes_as_csv` | `…/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` evaluates to CSV bytes through the ordinary path; the chunks are dependencies of the result |
| `source_serializes_only_as_manifest` | A `ManifestSource` writes and reads back its manifest; an `InMemorySource` and a wrapping source refuse `as_bytes` |
| `csv_round_trip_values_and_types` | Every column type writes and reads back equal, through the inference rules |
| `csv_null_is_not_empty_string` | Unquoted empty reads as null, quoted `""` as the empty string (replaces the old `column_null_distinct_from_empty_string` expectation for the file form) |
| `csv_quoting_corpus` | Separator, quote, CR, LF and CRLF inside fields; doubled quotes; a malformed file fails with its line number |
| `inference_keeps_leading_zeros` | `01234`, `+5`, `1e3` stay text; an over-large integer is not turned into a float |
| `schema_less_read_has_no_id_and_implicit_row_ids` | A file read without a schema has no `Id` column; `row_id` gives `(chunk, row)` and `row_number` the position |
| `ndjson_reads_differing_keys_as_union` | Missing keys become nulls; nested arrays of numbers become vectors |
| `markdown_escapes_and_uses_labels` | `\|`, line breaks and `<` are escaped; headers are labels; a default label reads back as its name |
| `html_escapes_every_cell` | A cell, label or description holding `<script>` is escaped — the security test |
| `feather_round_trip_is_lossless` | Types, nulls, roles, labels and `chunk_id` survive (`records-ipc`) |
| `feather_interoperates_with_polars` | A file written here reads in polars, and one written by polars reads here (`records-ipc` + `polars`) |
| `parquet_written_here_reads_in_polars` | Including the null definition levels and the `liquers.schema` metadata (`records-parquet` + `polars`) |
| `parquet_read_without_polars_is_refused` | With an error naming the feature |
| `advertised_formats_match_features` | Every format in the `RecordView` `TypeInfo` writes, in each feature combination |
| `schema_aware_csv_keeps_declared_types` | With a declared schema, `01234` stays text, `1.50` stays `"1.50"` in a `Text` column, and the `Id` and roles come from the schema |
| `schema_aware_csv_rejects_what_does_not_fit` | An undeclared column, a missing non-nullable one, and an unparsable cell each fail with row and column |
| `manifest_parses_stored_chunks_with_its_schema` | A plain-resource chunk is read through `read_resource` and parsed, not deserialized; its key is a dependency |
| `manifest_checks_computed_chunks` | A command chunk whose view differs from `uniform_schema` is refused naming the field |
| `schema_less_chunks_may_disagree` | Two CSV chunks of one table, one with only integers in a column, infer different types without a declared schema — the documented reason to declare one |
| `json_orients_round_trip` | `records`, `list`, `split`, `values` (with a schema), `columns`, `index`, `table`: each written by `to_json` reads back through `from_json` |
| `json_table_matches_pandas` | A fixture written by pandas with `orient="table"` reads with its types and `primaryKey` as the `Id`; ours carries labels as `title` |
| `from_json_auto_refuses_ambiguous_shape` | An object of objects asks for `columns` or `index` |
| `mutable_table_builds_and_freezes` | `RecordBatchMut::with_capacity` allocates once; `append_row`, `set_value` and `column_mut` write; `freeze` moves rather than copies; `into_mut` takes over unshared buffers |
| `row_ids_survive_views_and_materialize` | A filtered row keeps its base `RowId`; a table materialized from three chunks has three `RowRun`s and correct row numbers |
| `rowid_reads_one_chunk` | `ns-rec/rowid-2-10` over a manifest evaluates only chunk 2 |
| `rec_id_without_declared_id_is_refused` | …naming `rowid` |
| `to_record_accepts_every_input` | A view, bytes and text in `csv`/`tsv`/`json`/`ndjson`/`jsonl` (format from metadata or argument), a JSON value, and a key; a source is refused naming `materialize`; a text value with no format is refused rather than sniffed |
| `to_record_source_recognizes_manifest_by_discriminator` | A document carrying `manifest: record-stream` under any key becomes a `ManifestSource` with its key's folder as `cwd` |
| `manifest_version_is_lenient` | No `version`, and an unknown one, read as the latest; an unknown field is a `Warning` log entry |
| `explicit_chunk_keyed_by_its_filename` | `ns-sql/sql_query-…/orders_eu.csv` in `data/sales/x.manifest.yaml` is the key `data/sales/orders_eu.csv`; one without a filename is unkeyed |
| `template_chunks_named_by_convention` | Chunk 42 of `daily.manifest.yaml` is `daily_0042.csv`; `extension: arrow` gives `daily_0042.arrow`; `index_of` inverts it |
| `per_chunk_arguments_need_a_key` | Per-chunk `arguments` on an unkeyed explicit chunk are refused at load, as are colliding names |
| `manifest_provider_serves_chunk_keys` | `-R/data/sales/daily_0042.csv` evaluates through `ManifestRecipeProvider` in the chain; `contains` is true without enumerating; listing shows explicit chunks only |
| `provider_chain_prefers_recipes_yaml` | A key both providers answer comes from `recipes.yaml`; and the collision is reported at manifest load |
| `stored_false_skips_the_write_but_reads_a_stored_copy` | A produced chunk is not written; a pre-existing stored copy, including an `Override`, is read in preference to recomputing |
| `cached_false_is_not_registered` | Two requests evaluate twice; `stored: false, cached: false` is evaluated each time and is **not volatile** — a dependent is not made volatile |
| `legacy_metadata_defaults_to_stored_and_cached` | A metadata record and a recipe written before the fields existed read as `true`/`true` |
| `record_value_adapter_round_trips` | `liquers-lib`'s `Value` implements `RecordValue`: a view and a source go in and come back out as the same `Arc` |
| `records_crate_builds_alone` | `cargo test -p liquers-records` and `--target wasm32-unknown-unknown -p liquers-records` build with nothing above `liquers-core` — the dependency boundary as a test |
| `manifest_document_converts_to_source` | A `*.manifest.yaml` loaded as YAML becomes a `ManifestSource` through `ns-rec/source`, and implicitly for `materialize`, with the key's folder as `cwd` |
| `context_resolver_records_dependencies` | Chunks read through a `ContextResolver` become dependencies of the asset; through an `EnvResolver` they do not |
| `stream_outlives_its_source_handle` | A stream stays valid after the caller's `Arc` of the source is dropped — the `'static` property axum needs |
| `wrapping_source_is_stored_as_metadata_only` | A filtering source refuses `serialize`, and the asset write path stores metadata without failing |
| `view_command_refuses_a_source` | `select_columns` given a source fails with a conversion error rather than collecting |
| `stored_view_reads_back_as_batch` | A view written as CSV reads back as a `RecordBatch` under the same `RecordView` identifier |

**Items of the list below that the revision settles:** `Column::gather` is declared, as
`Column::take` and `Column::filter`; `Bitmap` gains `iter_ones`; and `column(i)` — now a `RecordView`
method — is the idiomatic read, with `RecordBatch::columns` staying a public field for code that holds
a batch. **Settled 2026-09-25:** the builder is `RecordBatchMut` with `append_row` and `with_capacity`
(Phase 2 open question 10), and `FieldRole::and_stored()` / `.and_fast()` are declared in Phase 4.

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


