---
id: RECORD-STREAMS-PHASE3-EXAMPLES
kind: analysis
title: Phase 3 examples — record streams
workflow: liquers-project
status: draft
area: [lib/value]
created: 2026-09-15
---
# Phase 3: Examples & Testing — Record streams

**Form: test-first.** The examples *are* the tests. None of the code below compiles today; it is
written against the trait form of Phase 2 (`phase2-architecture.md`) so that Phase 4's job is to
make it pass, not to invent an API around it. The code itself lives in
[`phase3-tests.md`](./phase3-tests.md), organized by the file each block lands in; this document is
the narrative — what the scenarios are, what they exercise, what goes wrong, and what Phase 3 found
that Phase 2 must still absorb.

## High-Level Introduction

Phase 1 asked for a tabular value that is interoperable, lazy and chunked, and that carries
provenance per chunk at almost no cost (`phase1-high-level-design.md`). Phase 2 answered with three
abstractions that carry the whole design: a **source** that can be asked, repeatedly, for a stream;
a **stream**, one traversal of it; and a **view**, a finite table — of which a `RecordBatch` is the
materialized case. The examples below walk that spine in the order a reader needs it:

1. **Example 1** builds and reads a single `RecordBatch` in memory — schema, roles, a mask, a
   filtered view, three output formats — without a source or a stream in sight. It is the smallest
   complete slice of the design: everything a `RecordView` promises, exercised once each.
2. **Example 2** adds the piece Example 1 does not need: a **manifest-driven source** with keyed
   chunks, addressable from anywhere in the store, traversed lazily one chunk at a time. This is
   where `ChunkResolver`, `RecipeProviderChain` and the `stored`/`cached` flags earn their place.
3. **Example 3** collects the pitfalls the design's own shape invites — not bugs, but places where
   the *right* answer is easy to get backwards (a derived `Default` silently flipping a flag's
   meaning, a CSV losing a leading zero, a one-row view pinning a hundred-megabyte batch).

The **Corner Cases** section after them takes the same three abstractions through memory,
concurrency, error and cross-crate axes systematically; the **Test Plan** points into
`phase3-tests.md`'s 189 functions; the **RECORDS counterparts** table gives every
`LANGUAGE-INTEGRATION_GUIDE.md` test its Rust-side proof; and the closing section records the small
number of places Phase 3 needed something Phase 2 declared only as an ellipsis, or not at all.

Two Phase 1 properties are proven by tests rather than by the scenarios:

- **Lazy and chunked.** Example 2's manifest walk is narrative, and `ManifestSource::stream`'s body
  there is a sketch. What proves the laziness is `RECORDS07` — a counting resolver asserts that at most one
  chunk is resident while the stream drains — and `RECORDS08`, which asserts that a drained stream
  and `materialize` yield the same rows (§"RECORDS counterparts").
- **Works on wasm32.** Proven at two levels. That it *builds* is shown by the build-matrix rows for
  `wasm32-unknown-unknown` (`liquers-records` alone and `liquers-lib` with `records`). That it
  *behaves* on wasm — a column view detected as stale after `memory.grow` and refreshed at the same
  pointer, and `free()` on the JavaScript handle releasing the batch — is shown by
  `liquers-web/tests/records_RECORDS.rs`, against the `RecordBatch` handle this design adds to
  `liquers-web`.

The records live in the `liquers-records` crate; `liquers-lib` holds the glue behind its `records`
feature, with the values as `ExtValue::RecordView` and `ExtValue::RecordSource`.

## Example Type

**Conceptual code, intended to compile at Phase 4.** No `examples/` binary: nothing in this design
can run until the types exist, and a demo would be a third thing to keep in sync with the tests and
the eventual reference — `phase3-tests.md`'s scenario code (§1) is the canonical worked example, and
the planned guide (`specs/guides/RECORD_STREAM_GUIDE.md`, `phase2-architecture.md` §"Documentation
Architecture") links it directly rather than duplicating it.

## Overview Table

| # | Type | Name | Purpose | Where |
|---|---|---|---|---|
| 1 | Example | Files to CSV | A single `RecordBatch`: schema with roles, a mask, a filtered view, three output formats | §Example 1, `phase3-tests.md` §1.1 (8 tests) |
| 2 | Example | A manifest-driven stream | Keyed chunks, `ChunkResolver`, a `RecipeProviderChain`, `stored: false` / `cached: true` | §Example 2, `phase3-tests.md` §1.2 (9 tests) |
| 3 | Pitfalls | 11 traps the design's shape invites | CSV null/leading-zero rules, non-uniform schemas, a pinned base, HTML injection, write-only formats, `Arc`+`serde`, derived-`Default` booleans | §Example 3 |
| — | Data-model unit tests | `liquers-records` core types | Schema invariants, `Bitmap`/`Buffer`, `Column` kernels, every view constructor, implicit row ids, `RecordBatchMut`/`ColumnMut`, manifest validation | `phase3-tests.md` §2 (73 tests) |
| — | Format unit tests | `liquers-records/src/formats/` | CSV quoting and inference, NDJSON, all eight `JsonOrient`s, Markdown/HTML escaping, IPC, Parquet | `phase3-tests.md` §3 (44 tests, 3 `#[ignore]`d) |
| — | `liquers-core` tests | `RecipeProviderChain`, `stored`/`cached` | Chain semantics against the real `AsyncRecipeProvider<E>` signature; default-true accessors | `phase3-tests.md` §4 (15 tests) |
| — | Integration tests | `liquers-lib`/`liquers-records` `tests/` | `ExtValue` round trip, `TypeInfo`, `to_record`/`to_record_source`, scalar reading, a `'static` stream | `phase3-tests.md` §5 (18 tests, 3 `#[ignore]`d) |
| — | `RECORDS01`–`RECORDS11` | Rust language-binding counterparts | Every meaningful test `LANGUAGE-INTEGRATION_GUIDE.md` defines, proved in Rust | `phase3-tests.md` §6 (9 tests) |
| — | Format round trip | one test per serialization format | CLAUDE.md-style coverage: csv, tsv, ndjson, json, md, html (write-only), ipc, parquet | `phase3-tests.md` §7 (8 tests) |
| — | Manifest validation | chunk-query planning, versions, collisions, naming | `phase3-tests.md` §9 (5 tests) |

**189** `#[test]`/`#[tokio::test]` functions in total (183 with a live assertion, 6 labelled
`#[ignore]` sketches — see `phase3-tests.md`'s own totals paragraph for the breakdown).

## Example 1: Files to CSV

### Connection to the High-Level Design

This is Phase 1's requirement 2 (a `Value` variant, produced by an ordinary command) and requirement
5 (provenance per chunk, "flyweighted to the record") in their smallest form: one command builds a
`RecordBatch` with a declared `Id` and concrete `FieldRole`s, and every read of it — a projection, a
mask, a filtered view, a scalar cell, three serialized formats — goes through the single
`RecordView` trait Phase 2 designed around one required method, `column_range` (§"Why the required
method is a column *range*").

### Scenario

A user wants a directory's contents as a table: file name, size, modification time — the kind of
listing a report or an audit trips over constantly, and a natural first thing to try once records
exist. The example keeps every default except what the scenario itself needs (a declared `Id`, so
`file_id` — not the implicit `RowId` — identifies each row across refreshes) and reaches into
`Column`/`Bitmap` directly for the filtering step, because **filtering has no query-level command**
(§"Building views": `filter`/`take` are inherent Rust methods on `dyn RecordView`, deliberately out
of this design's own query vocabulary — a future `ns-search` predicate is where that belongs).

### Sequence of Steps

1. The query `-R/data/-/ns-rec/file_records/files.csv` is submitted; `-R/data` resolves the `data`
   directory as a key, then `ns-rec/file_records` runs as an action on it.
2. `file_records` (an async command, since it lists the store) calls `context.get_async_store()`,
   walks the directory with `listdir_asset_info`, and builds a `RecordBatchMut` with a declared
   `Id` (`file_id`) and typed, role-tagged columns for name, size and modification time.
3. `batch.freeze()` produces the `RecordBatch`; the command wraps it as `Value::from_record_view`
   (an `ExtValue::RecordView`) and returns it.
4. The trailing `.csv` in the query selects the serialization format; `write_table` runs the
   schema's labels and the CSV null/empty-string convention (§Example 3, pitfall 1) over the batch.
5. Separately — as library code, not a query — `select_columns`, `Column::compare` and `filter`
   narrow the batch to files over a size threshold, and a one-row, one-payload-column result of
   that filtering reads directly as a scalar (§"A view as a value").

### Core Example Code

See `phase3-tests.md` §1.1 for the full command (`file_records`) and the filtering/scalar-reading
code (`largest_files`, `largest_size`). The command's shape:

```rust
pub async fn file_records(
    state: &State<Value>,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("file_id", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("size_bytes", FieldType::Int).with_role(FieldRole::numeric()).not_null(),
        // … file_name, modified_timestamp …
    ])?);
    let entries = context.get_async_store().listdir_asset_info(&state.try_into_key()?).await?;
    let mut batch = RecordBatchMut::with_capacity(schema, entries.len());
    for info in entries.into_iter().filter(|i| !i.is_dir) {
        batch.append_row(&[/* … from `info` … */])?;
    }
    Ok(Value::from_record_view(Arc::new(batch.freeze()?)))
}
```

### Guide and Executable Example

No runnable `examples/` binary (§"Example Type"); the planned `RECORD_STREAM_GUIDE.md` links
`phase3-tests.md` §1.1 directly as its worked example, the way `POLARS_COMMAND_LIBRARY.md` links its
own test suite rather than duplicating code that would drift from it.

**Expected output:**
```
file_id,file_name,size_bytes,modified_timestamp
data/sales_2024.csv,sales_2024.csv,25600,2024-09-21T12:00:00Z
```

**Validation:**
- [x] Demonstrates core functionality (a store directory → a typed, rolled-up table)
- [x] Uses realistic parameters (a real `AsyncStore` listing, not a stub)
- [x] Shows expected output for all three serialization formats exercised
- [x] Every API name checked against `phase2-architecture.md` or a declared Phase 3 completion

## Example 2: A manifest-driven stream with keyed chunks

Builds on Example 1's schema and command shape, and goes into the mechanism Example 1 does not need:
a **source** that outlives any one call, chunks addressable from anywhere in the store, and the two
general `liquers-core` features — `RecipeProviderChain` and the `stored`/`cached` flags — that make
keyed chunks work (§"Keyed chunks: naming, a recipe provider, and `stored`/`cached`").

### Connection to the High-Level Design

Phase 1 requirement 4 ("lazy and chunked, so a multi-gigabyte table is processed with one chunk
resident at a time") and requirement 5 (per-chunk provenance) are both about a **source**, not a
view — a view is already finite and in memory. This scenario is the one place in Phase 3 that opens
a stream at all.

### Scenario

A sales-analytics folder holds `daily.manifest.yaml`: two explicit chunks (one unkeyed, one keyed —
`orders_na.csv`) and a template generating `daily_0002.csv`, `daily_0003.csv`, … for as long as each
chunk has 1000 rows. `uniform_schema` is declared, `stored: false` (chunks are re-read, not
duplicated on disk) and `cached: true` (a chunk stays in memory for the session once evaluated).
Traversal happens through a `ChunkResolver`; a `RecipeProviderChain` is what makes
`-R/data/sales/daily_0042.csv` evaluable directly, without going through the source at all.

### Sequence of Steps

1. `-R/data/sales/daily.manifest.yaml/-/ns-rec/to_record_source` is evaluated:
   `to_record_source` recognizes the `manifest: record-stream` discriminator, deserializes
   `ManifestSpec`, and calls `ManifestSource::with_key` with the state's metadata key — which is
   what derives the template's naming (`daily_{n:04}.csv`) and validates the per-chunk
   `arguments`/`links` that only a keyed chunk may carry.
2. `-R/data/sales/daily_0042.csv`, evaluated directly, never touches the source: the environment's
   `RecipeProviderChain` tries `recipes.yaml` first, then `ManifestRecipeProvider`, which matches
   the filename against `ChunkNaming::index_of`, renders the template's query at that index, and
   hands back a `Recipe` carrying the manifest's `stored`/`cached` flags.
3. `-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` opens a stream with a
   `ContextResolver` (so every chunk it touches becomes a dependency of the result), walks chunks
   **in order, one resident at a time** (`futures::stream::unfold`, never
   `FuturesUnordered` — §Example 3, pitfall implicit in the corrected sketch), and concatenates them
   into one `RecordBatch` once uniformity is confirmed.
4. `-R/data/sales/daily.manifest.yaml/-/ns-rec/rowid-2-0` addresses row 0 of chunk 2 without
   walking the stream at all — `chunks()` names chunk 2's query directly.

### Core Example Code

`phase3-tests.md` §1.2 has the manifest YAML, `to_record_source`, the `ManifestRecipeProvider` and
`ContextResolver`/`EnvResolver` sketches (Phase 4 fills their bodies — the *shape* is corrected
there: lazy, in order, one chunk at a time), and nine tests against the parts that do not need a
stream body to be written yet — `ManifestSpec`/`ManifestSource` construction, `ChunkList`, the
discriminator, and the metadata-key-to-`with_key` wiring.

### Guide and Executable Example

As Example 1: no standalone binary. The guide's "manifests and keyed chunks" section links
`phase3-tests.md` §1.2, plus the corrected `stream()` sketch as the canonical shape a real
`RecordSource` implementation should follow.

**Expected output:**
```
order_id,customer_id,total,region
1,4021,58.50,EU
```
(one row of the materialized, concatenated stream, typed per `uniform_schema` — not inferred, so a
leading zero or an intentionally-text-typed id column survives).

## Example 3: Pitfalls and edge cases

Each pitfall names its symptom, cause, and correction; the protecting test is in `phase3-tests.md`.

**1. CSV null vs. empty string — quoting is the only signal.** *Symptom:* a round trip changes an
empty field's meaning. *Cause:* CSV has no null literal; the `csv` crate does not report whether a
field was quoted. *Correction:* PostgreSQL's own convention — unquoted empty is null, `""` is the
empty string — hand-written on both ends. Test: `column_null_distinct_from_empty_string`
(`phase3-tests.md` §3.1, the exact name `phase2-architecture.md` §"Tier 1" gives it).

**2. Leading zeros and the canonical-int rule.** *Symptom:* a ZIP code `01234` is read back as
`1234`. *Cause:* schema-less inference tries `Int` first; the fix is that a cell counts as `Int`
only when re-formatting the parsed number gives the cell back verbatim. *Correction:* declare the
column `Text` in a schema — the schema-aware reader never guesses. Test:
`schema_less_inference_canonical_int_rule_keeps_leading_zeros_as_text`.

**3. Schema-less chunks inferring differently from each other.** *Symptom:* the same column is
`Int` in one chunk and `Float` in the next, because each chunk is inferred alone. *Correction:*
declare `uniform_schema` once, at the manifest level. Covered by Example 2's manifest and
`manifest_schema_is_applied_to_the_whole_source`.

**4. A small view pinning a large base.** *Symptom:* a one-row cached view keeps a 100 MB batch
alive indefinitely. *Cause:* every view holds its base as `Arc<dyn RecordView>`. *Correction:*
commands bounded by a caller-chosen count — `rec_id`, `row`, `head` — **materialize**; `select_columns`
and `slice` stay views, by design (§"A small view keeps its whole base alive"). Not independently
tested as a Rust unit (it is a command-layer policy, not a type invariant); named here so Phase 4's
command implementations do not skip it.

**5. HTML injection via an unescaped cell.** *Symptom:* `<script>` in a cell executes when served as
`text/html`. *Correction:* every cell, label and description is escaped in the HTML writer. Tests:
`html_escapes_cell_content`, `html_escapes_label_and_description` (§3.5, and §1.1's
`html_escapes_cell_content` at the scenario level).

**6. A stored `.html` table cannot be reloaded.** *Symptom:* a `.html` key fails to deserialize.
*Cause:* HTML is presentation markup with no machine-readable structure — `TypeInfo` cannot declare
"write-only" (filed as `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS`). *Correction:* this is correct
behavior — the value is re-derived from its recipe. Test: `html_cannot_be_read_back` (§7).

**7. A CSV keyed chunk re-read outside its manifest loses types.** *Symptom:* `data/sales/daily_0042.csv`
read directly infers types instead of using the manifest's schema. *Correction:* choose
`extension: arrow` (`records-ipc`) for a keyed chunk that must survive direct, out-of-manifest reads
losslessly; CSV stays smaller but schema-less outside the manifest. Documented, not independently
tested (it is a modeling choice for a manifest's author, not an assertion about the code).

**8. `materialize` of a non-uniform source fails.** *Symptom:* two chunks with columns in different
order refuse to concatenate. *Correction:* this is correct — declare `uniform_schema`, project with
`select_columns` first, or export chunk by chunk. Covered by `RecordBatch::concat`'s documented
error in Error Handling, below (`phase2-architecture.md` §"Error Handling"); the passing case is
`manifest_schema_is_applied_to_the_whole_source`.

**9. A value with no `data_format` is never sniffed.** *Symptom:* a stored value with a
`type_identifier` but no format cannot be reloaded. *Correction:* this is correct — a format is
never guessed; metadata must always carry one. Test: `to_record_refuses_unlabelled_text_rather_than_guessing`
(§5.3).

**10. `Arc` fields need serde's `rc` feature.** *Symptom:* `#[derive(Serialize)]` fails to compile
over an `Arc<T>` field. *Correction:* `serde = { features = ["rc"] }`, already in Phase 2's
`liquers-records/Cargo.toml` block. A compile-time fact, not a runtime test.

**11. A derived `Default` silently flips `stored`/`cached` to `false`.** *Symptom:* a struct literal
built with `..Default::default()` produces `stored: false` when the field means "true unless said
otherwise." *Cause:* a derived `Default` for `bool` is `false`. *Correction:* `Option<bool>` with a
`stored()`/`cached()` accessor defaulting to `true` — applied to `Recipe`, `MetadataRecord` and
`AssetInfo`. Tests: `phase3-tests.md` §4.2 (10 tests), all passing exactly this check.

## Corner Cases

### 1. Memory

- **A one-row view pinning a 100 MB base** — Example 3, pitfall 4; mitigated at the command layer,
  not the type layer, because `select_columns`/`slice` must stay lazy for streaming to work at all.
- **A stream never holds more than one chunk resident** — the corrected `ManifestSource::stream`
  shape (§1.2) and `records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time`
  (`phase3-tests.md` §6), which proves the *contract* against a reference `RecordSource` since
  `ManifestSource::stream`'s own body is still a Phase 4 sketch.
- **`materialize`'s `max_rows`** bounds the one place a source is deliberately collected whole;
  refused past the limit rather than truncated silently (Error Handling, `phase2-architecture.md`).
- **Buffers are 64-byte aligned and `Arc`-shared** — slicing, projecting and filtering never copy
  column data except where `RowIndexView`'s gather is inherently a copy (§"The implementations").

### 2. Concurrency

- **Two concurrent requests for the same manifest-backed source each get their own traversal** —
  the entire reason a source, not a stream, is the value (§"Why a *source* makes this work, and a
  stream would not"). No test needed beyond `stream_outlives_the_source_arc_that_opened_it`
  (`phase3-tests.md` §5.7): each `.stream()` call is independent by construction.
- **`RowFnView`'s closure runs only for the requested range and column** —
  `row_fn_view_calls_closure_only_for_the_requested_range_and_column` (§2.4) proves no
  over-computation happens under concurrent readers sharing one view.
- **`Send`/`Sync` on native, vacuous on wasm** — every trait's `MaybeSend + MaybeSync` supertrait
  bound, unchanged from `ForeignValue`'s precedent; checked by the build matrix (§8), not a runtime
  test.

### 3. Errors

- Every error path in `phase2-architecture.md`'s "Error Handling" table has a test: schema-aware
  read failures (§3.1), the ambiguous JSON shape (§3.3), a non-`Id` schema refusing `rec_id` (not
  independently tested — a command-layer check with no type to exercise until Phase 4 writes the
  command), `RecordBatch::new`/`concat` mismatches (§2.4, §2.6), scalar-read shape errors (§5.5).
- **A mid-HTTP-stream failure after headers are sent** is a documented limitation
  (`phase2-architecture.md` §"The hard part: an error after the first byte"), not something Phase 3
  can test without `liquers-axum` wiring — named here so Phase 4 does not treat it as solved.

### 4. Serialization

- **Round trip per format** — `phase3-tests.md` §7, one test per format, csv/tsv/ndjson/json
  round-tripping values, markdown/html one-way, ipc lossless behind its feature.
- **`RECORDS01`'s two halves** — a plain `serde` round trip of `RecordBatch` (always lossless: every
  field derives `Serialize`/`Deserialize`) versus a CSV round trip (loses the `Id` role, explicitly
  asserted, not merely claimed) — `phase3-tests.md` §6.
- **Compression, dictionary encoding** — IPC refuses both, naming what was found; the checked-in
  fixture the refusal tests need is a named Phase 4 sketch (§3.7), not fabricated here.

### 5. Integration (cross-crate)

- **`liquers-core`**: `RecipeProviderChain` (general — any generative provider plugs in the same
  way) and `stored`/`cached` on `Recipe`/`MetadataRecord`/`AssetInfo` — §4, 15 tests, none behind
  `records`, since both are general core features records merely motivated.
- **`liquers-lib`**: `ExtValue::RecordView`/`RecordSource`, `TypeInfo` with the bare identifiers,
  `to_record`/`to_record_source` through the ordinary `evaluate` path — §5.
- **`liquers-axum`**: streaming a source over HTTP is superseded as a mechanism
  (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, noted in `phase2-architecture.md`); until
  that lands, a source is served by materializing it, the ordinary `BinaryResponse` path with no
  `liquers-axum` change — nothing new to test here.
- **`liquers-web`**: the wasm counterparts to `RECORDS05`/`RECORDS06` — stale-view detection after
  `memory.grow`, and `free()` releasing the batch — are `liquers-web/tests/records_RECORDS.rs` (§6).

### 6. Feature gating

`records`, `records-ipc`, `records-parquet` on `liquers-lib`; `ipc`, `parquet` on `liquers-records`
itself. Every format test file that needs one is gated at the file or test level (rule 4, digest);
§8 lists the nine build-matrix rows this design adds to `scripts/check-build-matrix.sh`.

## Documentation and Learning Log

### Guide candidate workflows

- **"How do I get a directory listing as a table?"** → Example 1, `phase3-tests.md` §1.1.
- **"How do I set up a manifest-driven stream with keyed chunks?"** → Example 2, §1.2, plus the
  manifest YAML itself as a template to copy.
- **"How do I address one row without walking a whole stream?"** → `rowid`/`rec_id`,
  `phase2-architecture.md` §"`rec_id` — the guaranteed path, as a query"; demonstrated in Example
  2's step 4.
- **"How do I filter rows by a value?"** → **not a query action** — `Column::compare` + `filter` in
  library code (Example 1's `largest_files`); worth a guide callout precisely because it is the one
  place the `pl` namespace's habits (`ns-pl/gt-amount-1000`) do not carry over.
- **"How do I know which serialization format keeps my roles?"** → the format table in
  `phase2-architecture.md` §"Table formats", and pitfalls 1/2/6/7 above for the traps in each
  direction.
- **"How do I add a value type Phase 4 needs a fixture for?"** → the three `#[ignore]`d format
  sketches in `phase3-tests.md` §3.7/§3.8 name exactly what is missing (a dictionary-encoded IPC
  file, a compressed one, a polars-produced Parquet file) — a guide for *producing* those fixtures
  is worth writing once Phase 4 needs them, not before.

### Usage, meaning, and connections

`RecordView`/`RecordSource` connect to the existing command system exactly as any other `ExtValue`
does — through `register_command!` and ordinary query evaluation — and to the store and asset
manager through the two general features (`RecipeProviderChain`, `stored`/`cached`) rather than any
records-specific hook, which is what makes them reusable by whatever generative provider comes next.
The planned reference (`specs/reference/RECORD_STREAMS.md`, per Phase 1's "Documentation Intent")
should draw its column-layout and provenance sections directly from `phase2-architecture.md`
§"Data Structures" and §"Provenance and validity", with this document's Example 1/2 as its own
worked illustrations.

### Repeatable development guidance

- Validate every query against `specs/command_registry.yaml` once `ns-rec` commands exist
  (`liquers-validate --command <name>` per name until they are registered, per `CLAUDE.md`).
- When adding a new `Column` variant or `FieldType`, the checklist is the same as any `ExtValue`
  addition (`CLAUDE.md` "Adding a Value Type"): a `TypeInfo` entry, both `ExtValueInterface`
  directions, and — specific to records — a row in the Arrow-compatibility table
  (`phase2-architecture.md` §"Where our layout meets Arrow's").
- When writing a new `RecordSource`, run it against `RECORDS07`'s pattern
  (`phase3-tests.md` §6): a counting resolver, drained through the real `stream()`, asserting the
  peak concurrently-resolved chunk count is 1.

### Corrections and unexpected learning

- **An async command takes its `State` and `Context` by value**, and a `context` parameter needs a
  `type CommandEnvironment = …` alias in scope for `register_command!` to name the environment
  (`REGISTER_COMMAND_FSD.md` §"State Parameter"; `liquers-core/tests/injection.rs`). Several drafts
  wrote `&State<Value>` and `&Context<…>`; found while planning Phase 4 and corrected in every
  registered async command. `to_record`/`to_record_source` themselves still take `&Context`, so a
  command passes `&context`.
- **`ManifestSource::stream`'s reference sketch materialized eagerly inside `stream()`** in an
  earlier draft (`futures::stream::FuturesUnordered`, awaited and concatenated before returning) —
  exactly the behavior the chunked design exists to avoid, and it also discarded chunk order, which
  `RowRun` depends on. Corrected to a `futures::stream::unfold` shape that resolves one chunk per
  step, in order (§1.2). No Phase 2 change needed — the trait signatures were already right; only a
  draft's *implementation sketch* of them was wrong.
- **`AsyncRecipeProvider<E>`'s real signature** takes `envref: EnvRef<E>` on every method and
  returns `Result<Option<Recipe>, Error>` from `recipe_opt`, not the bare two-argument,
  `Option`-returning shape two drafts assumed. Fixed throughout `phase3-tests.md` §1.2 and §4.1.
- **`Recipe` has no `filename` field** — a keyed chunk's name comes from `Recipe::filename()`,
  derived from the query's own trailing segment. Fixed in every `Recipe` literal in
  `phase3-tests.md` §2.7 and §9.
- **`Context`'s real constructor is `async` and needs an `AssetRef`**, not a bare
  `Context::new(envref)`. Tests needing a `Context` now route through `evaluate()` against a tiny
  registered probe command, following `liquers-core/tests/async_hellow_world.rs` — the pattern the
  digest already pointed at (rule 7), which two drafts did not follow through on.
- **`TypeInfo::type_identifier` for the two new variants is bare (`RecordView`, `RecordSource`)**,
  not `provider.LocalName` — a draft's `"liquers:records:RecordView"` mixed the two conventions
  `TypeInfo`'s own doc comment keeps separate. Fixed in `phase3-tests.md` §5.2.
- **Phase 4 review (2026-09-25), compile fidelity against the real code:** `ResourceName` has no
  `FromStr` (§4.1 now uses `ResourceName::new`); `Query` has no `FromStr` (§9 uses `parse_query`);
  `ValueExtension` is `liquers-lib`'s trait, not `liquers_core::type_system`'s (§5.2); `Context`
  has no `get_async_store` (§1.1 goes through `get_envref()`); `Metadata` is an enum whose key is
  `metadata.key()?`, not a field (§1.2); an in-crate unit test naming `liquers_records::…` needs
  `extern crate self as liquers_records;` (Phase 4 Step 2.1); `ManifestSpec.extension` absent reads
  as `None`, the `csv` default being applied by the naming (§2.7); §3.8's polars test cannot live in
  `liquers-records` (moved by Phase 4 Step 6.2). The totals were recounted: 189 tests in 28 files,
  §2 has 73, §3 has 44 and §9 has 5.
- Nothing here reopened a Phase 1 `neither` decision; the corrections above are all implementation
  fidelity, not design questions.

## Test Plan

See `phase3-tests.md` for all 189 functions (183 with a live assertion, 6 `#[ignore]`d sketches),
organized:

- **§1** — the two scenarios' own code and tests (17 tests)
- **§2** — `liquers-records` data-model unit tests: schema, buffer/bitmap, column kernels, views
  (including implicit row ids), mutable builders, manifest validation (73 tests)
- **§3** — format unit tests: csv, ndjson, json shapes, markdown, html, mod-level, ipc, parquet
  (44 tests, 3 `#[ignore]`d)
- **§4** — `liquers-core`: `RecipeProviderChain`, `stored`/`cached` defaults (15 tests)
- **§5** — integration tests across `liquers-lib`/`liquers-records` (18 tests, 3 `#[ignore]`d)
- **§6** — `RECORDS01`–`RECORDS11` Rust counterparts (9 tests, plus 2 wasm-only sketches for
  `liquers-web`)
- **§7** — one round-trip test per serialization format (8 tests)
- **§8** — the nine build-matrix rows this design adds (script changes, no new tests)
- **§9** — manifest validation: chunk-query planning, versions, collisions, naming (5 tests)

Run once Phase 4 lands: `cargo test -p liquers-records --lib --tests`, then
`cargo test -p liquers-lib --lib --tests` (default features cover `records`/`records-ipc`/
`records-parquet`), then the reduced-feature and wasm32 rows of §8.

## RECORDS counterparts

| Test | Contract | Rust counterpart |
|---|---|---|
| `RECORDS01` | A view round-trips with schema, roles and chunk identity intact | `phase3-tests.md` §6: `records01_serde_round_trip_preserves_schema_roles_and_chunk_id` (lossless path) and `records01_csv_documents_which_metadata_it_loses` (the lossy path, explicitly asserted) |
| `RECORDS02` | A column read through a view equals a materialized copy | §6: `records02_column_through_a_view_equals_a_materialized_copy`; also every `assert_reads_agree` call in §2.4 |
| `RECORDS03` | An Arrow export equals the wrapper's data; documented metadata survives | §3.7 (feature `ipc`): `feather_round_trip_preserves_types_roles_and_labels`, `feather_preserves_chunk_id_in_custom_metadata` |
| `RECORDS04` | A lent buffer is read-only; an edit through a copy never touches the original | §6: `records04_editing_a_shared_batch_copy_leaves_the_original_untouched`; also §2.6's `record_batch_into_mut_copies_a_shared_buffer` |
| `RECORDS05` | A borrowed view survives an operation that could invalidate it | §6: `records05_view_keeps_reading_after_the_callers_arc_is_dropped` (Rust: `Arc` ownership). Wasm counterpart (heap growth): `liquers-web/tests/records_RECORDS.rs::records05_…`, end of §6 |
| `RECORDS06` | Releasing the last handle releases the value | §6: `records06_dropping_the_last_arc_makes_the_weak_handle_unresolvable` (`Weak::upgrade` after drop). Wasm counterpart (live handle count via `debug-handles`): `liquers-web/tests/records_RECORDS.rs::records06_…` |
| `RECORDS07` | A manifest-backed source is traversed one chunk at a time | §6: `records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time`, against a reference `RecordSource` (`ManifestSource::stream`'s own body is a Phase 4 sketch, §1.2) |
| `RECORDS08` | Draining a stream and materializing yield identical rows | §6: `records08_stream_drain_and_materialize_agree`, against the real `InMemorySource` |
| `RECORDS09` | NA unless the language has no async model | NA — Rust always has one; `RecordView` is synchronous and `RecordSource` always async, so there is no fallback route to document |
| `RECORDS10` | A single-cell view reads as a scalar; a larger view refuses, naming its shape | `phase3-tests.md` §5.5: `single_cell_view_reads_as_a_scalar`, `multi_row_view_refuses_scalar_read_naming_its_shape` |
| `RECORDS11` | NA unless the language may implement the traits | Rust can: §6, `records11_a_user_defined_view_agrees_across_column_value_and_materialize`, a `RecordView` defined entirely in the test file |

## Queries used

```
-R/data/-/ns-rec/file_records/files.csv
-R/data/-/ns-rec/file_records/files.md
-R/data/-/ns-rec/file_records/files.html
-R/data/-/ns-rec/file_records/select_columns-file_id-size_bytes/files_projected.csv
-R/data/-/ns-rec/file_records/head-3/top_files.csv
-R/data/-/ns-rec/file_records/rowid-0-0
-R/data/sales/daily.manifest.yaml/-/ns-rec/to_record_source
-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv
-R/data/sales/daily_0010.csv
-R/data/sales/orders_na.csv
-R/data/sales/daily.manifest.yaml/-/ns-rec/rowid-2-0
-R/data/sales/daily.manifest.yaml/-/ns-rec/rec_id-123
```

None validate against `specs/command_registry.yaml` today, because no `ns-rec` command is registered
yet — that is the point of a test-first Phase 3. Each should be checked at Phase 4 with
`liquers-validate --command <name> -- '<query>'` (one `--command` per new name in the chain) before
being trusted; the shapes follow the chaining convention already validated for `ns-pl`
(`specs/reference/POLARS_COMMAND_LIBRARY.md:61`: one namespace prefix per chain, dash-joined
arguments, actions chained by `/`).

## What Phase 3 found that Phase 2 must absorb

**Absorbed 2026-09-25**, on Phase 3's approval: (a), (b) and (b′) are now declared in
`phase2-architecture.md` §"Construction helpers, options and the provider chain" and §"Function
Signatures" (`Bitmap`).

### (a) The digest's own "Phase 3 completions" — already used throughout `phase3-tests.md`

These were declared in the drafting digest as Phase 3's to specify and Phase 2's to absorb
verbatim; nothing below changed during drafting, so this is a confirmation list, not a new proposal:

- **`ChunkTemplate`** — fields (`query`, `first_offset`, `step`, `batch_size`) and methods
  (`query_at`, `offset_at`). Used and tested in `phase3-tests.md` §2.7.
- **`JsonOrient`** — the eight-variant enum and its `FromStr` (`"records"`, …, `"auto"`), plus
  `to_json`/`from_json`. Used and tested in §3.3, with a real inline fixture per orient (§3.3's own
  note explains why the previous draft's skeletal `todo!()`s are now assertions).
- **`ReadOptions`/`WriteOptions`** with a hand-written `Default` (`header: true`) — the exact lesson
  pitfall 11 generalizes. Tested in §3.6.
- **`TableFormat::from_data_format`** and its aliases (`csv:comma`, `csv:tab`, `jsonl`, `arrow`/
  `arrow_ipc`/`feather`, …). Tested in §3.6.
- **`FieldSchema` builders** (`new`, `with_label`, `with_description`, `with_key`, `with_role`,
  `not_null`) and **`FieldRole` constructors** (`text`, `keyword`, `stored_only`, `numeric`,
  `vector`, `ignored`, `and_stored`, `and_fast`). Used throughout §2.1–§2.7 and every scenario.
- **`ToRecordOptions`** (`format`, `header`, `schema`, `max_rows`). Used in §5.3/§5.4.
- **`RecipeProviderChain::new`/`push`** — confirmed against the *real* `AsyncRecipeProvider<E>`
  signature (see §"Corrections and unexpected learning" above: `envref: EnvRef<E>` on every method,
  `Result<Option<Recipe>, Error>` from `recipe_opt`). Tested in §4.1.

### (b) The one real gap: `Bitmap` construction

Phase 2 declares only `Bitmap::{get, and, or, not, count_ones, iter_ones}` — every one of them reads
an *existing* bitmap; nothing builds one. Every test in this document that needs a mask (`filter`,
`Column::filter`, `select_columns` combined with a predicate) needs to construct a `Bitmap` from
scratch, and hand-packing bytes to do it (`Bitmap::from_bytes(vec![0b00010101], 5)`, an earlier
draft's approach) makes every such test depend on the LSB-first packing convention that is
`Bitmap`'s own implementation detail, not a caller's concern. **Phase 2 should add:**

```rust
impl Bitmap {
    /// A bitmap of `len` bits, all clear. The building block for `set` to fill in.
    pub fn new(len: usize) -> Self;
    /// Built directly from booleans — the normal way a test, or a predicate evaluator, builds a
    /// mask without knowing the byte-packing convention.
    pub fn from_bools(bits: &[bool]) -> Self;
    /// Mutates one bit. Building block for incremental mask construction (e.g. a search
    /// predicate setting bits as clauses match).
    pub fn set(&mut self, i: usize, value: bool);
    /// The bit count — distinct from the byte count of its backing `AlignedBuffer`.
    pub fn len(&self) -> usize;
}
```

Used throughout `phase3-tests.md` §2.2 (`bitmap_new_is_all_clear`, `bitmap_from_bools_roundtrips_through_get`,
`bitmap_set_mutates_a_single_bit`, `bitmap_len_reports_bit_count_not_byte_count`) and every mask
built anywhere else in this document. No other `Bitmap` gap was found: `and`/`or`/`not`/
`count_ones`/`iter_ones` cover everything a filtered view or a search predicate needs to *read* a
mask once built.

### (b′) The `liquers-web` handle's Rust-side surface

Phase 2 declares the `RecordBatch` handle's JavaScript methods, and says that `debug-handles` makes the
live batch-handle count assertable. It does not name the Rust surface a test needs to get there.
`liquers-web/tests/records_RECORDS.rs` uses three additions:

```rust
// liquers-web/src/records.rs
impl From<Arc<RecordBatch>> for LiquersRecordBatch { /* wraps; the handle count rises by one */ }
impl Drop for LiquersRecordBatch { /* the handle count falls by one */ }

/// Live `LiquersRecordBatch` handles — the `RUNTIME05` idiom, applied to batches.
#[cfg(feature = "debug-handles")]
pub fn live_batch_handle_count() -> usize;
```

and fixes the column descriptor's field names: `kind`, `ptr` (a byte offset into linear memory),
`len` (in elements), and `validity` (a descriptor of the same shape, or `null`).

### (c) Confirmed as already sufficient — no gap

**`RowFnView` and the other view constructors** (`ColumnsView`, `RowRangeView`, `RowIndexView`,
`DerivedColumnView`, `AppendedColumnsView`) are fully declared in `phase2-architecture.md`
§"Building views" and §"Writing a view", including `RowFnView::new`'s closure signature
(`Fn(usize, usize) -> Result<FieldValue, Error>`, confirmed against the trait bound in
§"Generic Parameters & Bounds") and `with_column`'s (`Fn(&[Column]) -> Result<Column, Error>`). An
earlier draft listed these as gaps because it had not located the bound; `phase3-tests.md` §2.4 uses
every one of them without needing a new name.

### (d) Deferred to Phase 4 (not gaps)

- **`ContextResolver`'s dependency-recording body** for `read_resource`/`evaluate` — the trait
  signature is fixed (§"`ChunkResolver`"), only the implementation (which needs a live `Context`
  wired to a real asset manager) is Phase 4 work. Sketched, not gapped, in `phase3-tests.md` §1.2.
- **A schema living in `MetadataRecord`** — Phase 2 explicitly leaves this open
  (§"Should the schema live in metadata?", open question 17); nothing in Phase 3 needed it, since
  every test's schema comes from a manifest, a linked argument, or the data itself.
- **Incremental reads of a stored chunk** (`CORE-STORE-OPENBIN-MISSING`) — Phase 2 already notes a
  stored chunk is read whole, as one batch, and that this is accepted for this version.
- **A real pandas-produced fixture for the `table` JSON orient** — `phase2-architecture.md` calls
  for interop tested against a fixture pandas actually writes, not assumed from the spec; §3.3's
  `orient_table_is_schema_aware_and_lossless` uses a hand-written fixture shaped like pandas' output
  as a placeholder, and Phase 4 should replace it with a checked-in file once one is on hand.
