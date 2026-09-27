---
title: Record Stream Guide
kind: guide
audience: internal
area: [records, lib/value]
reviewed: 2026-09-27
---

# Record Stream Guide

How to produce records from a new source: a command that returns a table, a manifest that stitches
many queries into one stream, and a view of your own. This guide covers the workflow. The contracts
(every trait method, the format tables, the Arrow layout, the browser hazards) are in
[`reference/RECORD_STREAMS.md`](../reference/RECORD_STREAMS.md), and the reasons behind them are in
[`design/record-streams/`](../design/record-streams/).

Every code snippet below is copied from a passing test, and the test's path and name appear under
the snippet. If a snippet stops matching its test, trust the test and correct the guide.

**Where the code lives.** The data model, views, formats, manifests, sources and
`ManifestRecipeProvider` are in `liquers-records/src/`, a crate that depends only on
`liquers-core`. The glue that needs `liquers-lib`'s `Value` is in `liquers-lib/src/records/`:
`ExtValue::RecordView` / `RecordSource`, `to_record` / `to_record_source`, the `ns-rec` commands
and the polars bridge. `liquers_lib::records` re-exports all of `liquers-records`, so a crate above
`liquers-lib` does not need its own dependency on it.

---

## 1. Choose your shape first

Choosing the wrong shape is the expensive mistake. Everything after it (storage, refresh, memory,
retrieval) follows from this choice, so make it before you write any code.

| You have… | Produce | What exists at HEAD |
|---|---|---|
| A small or medium table, computed in one go | A **view**, usually a `RecordBatch` built with `RecordBatchMut`, returned as `ExtValue::RecordView` | §2. `ns-rec/file_records` is the in-tree model |
| A table that is a cheap function of another | A **view over a view**: `select_columns`, `filter`, `slice`, `with_column` | §5. Nothing is copied until `materialize` |
| Many files, or many queries whose results form one table | A **manifest**: a `<name>.manifest.yaml` in the store, read as a `ManifestSource` | §3 |
| A result too large to hold at once, produced in pages (offset/limit) | A manifest **template**: chunk `i` is `<query>-<offset>-<batch_size>` | §3, §4 |
| A single huge file | There is no streaming file reader. `read_table` reads a whole buffer. Split the file into chunks behind a manifest, or implement `RecordSource` yourself (§6.3) | Custom `RecordSource` |
| Rows that each need async work (a lookup, an embedding call) | A wrapping source. None is built (`RECORD-SOURCE-WRAPPERS-UNSPECIFIED`), so implement `RecordSource` and do the async work inside `stream` | Custom `RecordSource` |

Three rules follow from the model:

- A **view** is synchronous and finite. Any asynchronous work belongs in a **source**.
- A **source** can be asked for a stream any number of times, and it is never used up. A **stream**
  is one traversal and is never stored as a value.
- A **chunk** is the unit of retrieval and refresh. A stored chunk is read whole, so at HEAD a chunk
  is also a batch (§4).

---

## 2. Walkthrough: a command producing records

This walkthrough starts from an empty function and ends with a registered, tested command. The
in-tree model is `file_records` in `liquers-lib/src/records/commands.rs`, which lists a store
directory as a table.

**Where it goes.** A general-purpose command goes in `liquers-lib/src/records/commands.rs` and is
registered inside `register_records_commands!` in the same file. A command owned by another
library goes in that library's module. Either way it sits behind `#[cfg(feature = "records")]`: the
feature is on by default, but `liquers-lib` must still build without it.

### 2.1 Define the schema

Give every field a type. When rows have a natural identity, declare exactly one `Id` field.
`RecordSchema::new` validates the schema and refuses, among other things, a second `Id` field and
duplicate field names.

```rust
fn schema_with_id_and_size() -> Arc<RecordSchema> {
    Arc::new(
        RecordSchema::new(vec![
            FieldSchema::new("file_id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("size_bytes", FieldType::Int),
        ])
        .expect("schema valid"),
    )
}

// …
let schema = schema_with_id_and_size();
assert_eq!(schema.id_field(), Some(0));
assert_eq!(schema.payload_fields(), vec![1]);
```
<sub>`liquers-lib/tests/records_scenario_files_to_csv.rs`, `schema_with_id_and_roles_roundtrips`</sub>

A field carries two kinds of information, and they are independent of each other:

- **What the field is**, set with `.with_key(KeyRole::Id | KeyRole::Source)`. The `Id` field is how
  `ns-rec/rec_id` finds a row. The `Source` field indexes the batch's `sources` dictionary (§2.3).
- **How search indexes it**, set with `.with_role(FieldRole::text() | keyword() | numeric() |
  vector(metric) | stored_only() | ignored())`.

Presentation formats print `.with_label(…)` instead of the name, and `.not_null()` declares that
the column holds no nulls (checked against a manifest's `uniform_schema`, §3.3). `file_records`
uses all four.

### 2.2 Build the batch

`RecordBatchMut` builds a batch row by row. The capacity you pass is allocated once in every column.

```rust
let schema = schema_with_id_and_size();
let mut batch = RecordBatchMut::with_capacity(schema, 2);
batch.append_row(&[FieldValue::Text(Arc::from("file1")), FieldValue::Int(42)])?;
batch.append_row(&[FieldValue::Text(Arc::from("file2")), FieldValue::Int(100)])?;
let frozen = batch.freeze()?;
assert_eq!(frozen.len, 2);
```
<sub>`liquers-lib/tests/records_scenario_files_to_csv.rs`, `record_batch_mut_builds_and_freezes`</sub>

`append_row` is atomic. A row with the wrong number of values, or a value of the wrong type, is
refused and leaves the batch unchanged. To fill column by column instead, use `column_mut(i)` and
`push`. `freeze` refuses columns of uneven length (§6.2). `append_row` and `set_value` come from
the `RecordViewMut` trait, so import it.

### 2.3 Make the rows retrievable

A row can always be found again through its implicit `RowId { chunk, row }`. Beyond that, do the
following:

- **Declare an `Id` field** when rows have an identity (§2.1). This gives the guaranteed retrieval
  path, `…/ns-rec/rec_id-<id>`.
- **Set the chunk identity** when you know which chunk the batch is. When a batch reaches a
  consumer through a manifest traversal, the source sets it for you (`place_chunk`). A standalone
  batch can set the public field itself:

  ```rust
  batch.chunk_id = Some(ChunkId::Key(liquers_core::parse::parse_key("data/sales/daily_0000.csv")?));
  ```
  <sub>`liquers-records/tests/records_guide_counterparts.rs`,
  `records01_serde_round_trip_preserves_schema_roles_and_chunk_id`</sub>

- **Fill `ChunkOrigin`** when the rows of one batch come from different assets. Add a `Source`-role
  integer column, and put one `ChunkOrigin` per origin in `sources`. `RecordBatchMut` has no
  `sources`, so build the batch with `RecordBatch::new`:

  ```rust
  RecordBatch::new(
      schema, // fields: x: Int, src: UInt with_key(KeyRole::Source)
      vec![
          Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) },
          Column::UInt { validity: None, values: Buffer::from_slice(&[source_index]) },
      ],
      None,
      None,
      vec![origin(origin_name)],
  )
  ```
  <sub>`liquers-records/src/batch.rs`, `sourced_batch` (used by
  `concat_rebases_the_source_column_onto_the_joined_dictionary`)</sub>

  `ChunkOrigin.chunk` is the query that re-produces the rows. `info` is `None` when the origin is
  not an asset: a CSV row has no `AssetInfo`, though the file it came from does. `locator` is an
  optional shortcut from an `Id` value to a query:

  ```rust
  let origin = ChunkOrigin {
      asset: asset.clone(),
      chunk: asset,
      info: None,
      locator: Some(LocatorRule {
          namespace: "csv".to_string(),
          command: "row".to_string(),
          leading_parameters: Vec::new(),
      }),
  };
  let query = origin.locator_query(&FieldValue::Int(42)).expect("locator");
  assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-csv/row-42");
  ```
  <sub>`liquers-records/src/sources.rs`,
  `locator_query_appends_namespace_and_command_after_the_asset`. `ns-csv/row` is the test's
  illustration, not a registered command.</sub>

`RecordBatch`'s fields are public, but only `RecordBatch::new` validates them. Setting `chunk_id`
is harmless. Editing `columns` or `len` directly bypasses validation
(`RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION`), so build a new batch instead.

### 2.4 Return the value

Wrap the frozen batch as a view and return it with `Value::from_record_view`, which comes from
`ExtValueInterface`:

```rust
fn fixture_rows(tag: String, offset: i64, batch: i64) -> Result<Value, Error> {
    // … (counts the call under `tag`)
    let rows = usize::try_from(batch)
        .map_err(|_| Error::general_error(format!("batch must not be negative, got {batch}")))?;
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("offset", FieldType::Int)])?);
    let mut builder = RecordBatchMut::with_capacity(schema, rows);
    for _ in 0..rows {
        builder.append_row(&[FieldValue::Int(offset)])?;
    }
    let view: Arc<dyn RecordView> = Arc::new(builder.freeze()?);
    Ok(Value::from_record_view(view))
}
```
<sub>`liquers-lib/tests/records_end_to_end.rs`, `fixture_rows` (used by every test in the file)</sub>

An integer argument arrives as `i64`, because the DSL has no unsigned type. Convert it with
`try_from` and refuse negative values. Never cast with `as`.

### 2.5 Register it

`register_command!` needs a `type CommandEnvironment = …` alias in scope. A producer with no input
and no I/O can be a plain `fn`:

```rust
type CommandEnvironment = DefaultEnvironment<Value>;

let mut env = DefaultEnvironment::<Value>::new();
{
    let cr = env.get_mut_command_registry();
    register_records_commands!(cr)?;
    register_command!(cr,
        fn fixture_rows(tag: String, offset: i64, batch: i64) -> result
        namespace: "fixture"
    )?;
}
```
<sub>`liquers-lib/tests/records_end_to_end.rs`, `build_env`</sub>

**A command that takes an input state is an `async fn` with `context` as its last parameter.** It
converts its input through `to_record` or `to_record_source`, both of which are async and need the
context, because the input may be a key the context has to fetch and record as a dependency. The
function takes an owned `State`:

```rust
async fn probe_source(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    match to_record_source(state.data_unchecked(), &state.metadata, &options, &context).await {
        // …
    }
}

register_command!(cr, async fn probe_source(state, context) -> result)?;
```
<sub>`liquers-lib/tests/record_manifest_keyed.rs`, `probe_source` / `probe` (used by
`to_record_source_recognizes_manifest_discriminator`)</sub>

In `liquers-lib/src/records/commands.rs` the same shape is generic
(`pub async fn …<E: Environment<Value = Value>>(state: State<Value>, …, context: Context<E>)`), and
the commands call the helpers by full path (`super::convert::to_record`). They do this because
`register_command!` cannot rename a command, and two commands share a name with their helper.

Two argument-type limits apply. An optional schema cannot be `Option<Value>` (§7.6), so it is
spelled `schema: String = ""`, where empty means "infer". And a variadic argument is
`Vec<T> multiple`, as in `select_columns`.

### 2.6 Test it through a query

Evaluate through the environment's own asset manager, not the free `interpreter::evaluate`, when
the result will be serialized (§7.8):

```rust
async fn eval(
    envref: EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<liquers_core::state::State<Value>, Error> {
    envref.evaluate(query).await?.get().await
}
```
<sub>`liquers-lib/tests/records_end_to_end.rs`, `eval`</sub>

For a result you only inspect as a value, the free function is fine:

```rust
let state = evaluate(env.to_ref(), "fixture_view/ns-rec/row-1", None).await?;
let view = state.value()?.as_record_view()?;
assert_eq!(view.len(), 1);
assert_eq!(view.value(0, 1)?, FieldValue::Int(20));
```
<sub>`liquers-lib/src/records/commands.rs`, `row_returns_a_one_row_materialized_view`</sub>

Gate an integration test file with `#![cfg(feature = "records")]`, as every file in
`liquers-lib/tests/record*.rs` is.

### 2.7 Regenerate the registry

A new or changed `register_command!` signature makes `specs/command_registry.yaml` stale, and
`cargo test -p liquers-lib --test registry_export` fails until you regenerate it:

```bash
cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  --format yaml -o specs/command_registry.yaml
```

Then add a dated line between `# CHANGELOG-BEGIN` and `# CHANGELOG-END`. The exporter's
`Group::Records` includes `register_records_commands!` only when `records` is enabled. Once the
command is exported, check your example queries with `liquers-validate` (CLAUDE.md, "Validating
queries").

### 2.8 Check the feature matrix

Run `bash scripts/check-build-matrix.sh` whenever you touch a `#[cfg(feature = "records")]`, a
`match` over `ExtValue`, or an optional dependency (§8).

---

## 3. Second walkthrough: a manifest source

A manifest is a YAML document listing the queries whose results, in order, make up one stream. Use
one for "every CSV file in a folder", "every month of a report", or "every page of a paged
query". Prefer it to a hand-written generator for three reasons:

- **Rewindable.** The source is a value, and `stream()` can be called again at any time.
- **Cacheable.** Every chunk is an asset with its own metadata and refresh.
- **Checkpointable.** Keyed chunks are stored under their keys, and an existing stored copy is
  preferred over re-evaluation, so an interrupted walk resumes where it stopped.

### 3.1 Name and place it

Store it as **`<folder>/<name>.manifest.yaml`**. Only that suffix gives the manifest a key, and
only a keyed manifest has keyed chunks. `ManifestRecipeProvider` reads only `*.manifest.yaml`, so a
manifest stored under any other name is keyless. A keyless manifest still materializes, but its
chunks are never stored, and any per-chunk or shared `arguments` / `links` are refused when the
stream opens (§7.5).

### 3.2 Write the chunks

```yaml
manifest: record-stream
chunks:
  - query: ns-fixture/fixed_rows-10-1/first.csv
    arguments:
      offset: 55
template:
  query: ns-fixture/limited_rows
  first_offset: 0
  step: 1
  batch_size: 1
```
<sub>`liquers-lib/tests/record_manifest_resource_key.rs`, `MANIFEST` (used by
`materialize_keys_the_chunks_of_a_manifest_fetched_as_a_resource`). The test writes it as a Rust
string constant; this is the same document.</sub>

- **`manifest: record-stream`** is the discriminator. `to_record_source` recognizes a document only
  when this field is present, and never guesses from the content.
- **`chunks`** (explicit) are `recipes.yaml` entries, so each has `query`, `arguments`, `links` and
  `title`. A chunk whose query ends in a filename is **keyed** by that filename in the manifest's
  folder. Above, that is `data/first.csv`, and its `arguments` override the query's `10` with `55`.
  A chunk with no filename is unkeyed and identified by its query.
- **`template`** generates every chunk after the explicit ones. Global chunk `i` runs
  `<query>-<offset>-<batch_size>`, where `offset = first_offset + step × i`, and is keyed
  **`<name>_{i:04}.<extension>`** in the manifest's folder (`data/x_0001.csv` above).
  `extension` defaults to `csv`, and it is also the format the chunk is stored in. A template walk
  ends at the first chunk shorter than `batch_size`. `batch_size: 0` and `step: 0` are refused at
  load, because either would make the walk endless.
- **Collisions are refused at load**: two explicit chunks keyed the same, or an explicit name that
  matches the template's pattern.
- **`stored`** (default `true`) writes keyed chunks to the store. **`cached`** (default `true`)
  keeps them in the asset manager. Setting both to `false` means "not kept", which is different
  from volatile. `volatile` and `expires` are copied onto every chunk recipe.

A **directory of CSV files** is the same shape: one explicit, unkeyed chunk per file, each chunk
query reading the file and converting it, for example `-R/data/raw/jan.csv/-/ns-rec/to_record-csv`.
The chunk has no filename, so it is identified by its query and served from the file itself. No
test covers this shape end to end yet. The file read is covered by `to_record_reads_labelled_csv_bytes`
and the unkeyed chunk by `manifest_source_reads_an_unkeyed_chunk_by_evaluating_its_query`.

### 3.3 Declare `uniform_schema` when you can

`uniform_schema` is a `RecordSchema` under the manifest's `uniform_schema:` key. When it is
declared:

- stored chunks are parsed against it instead of by inference, so nothing is guessed;
- every chunk view is checked against it, where a field declared not null must hold no nulls;
- `source.schema()` answers without opening any chunk.

Without it, chunks may legitimately disagree. Streaming and NDJSON still work, but
`ns-rec/materialize` (and therefore writing one CSV file) fails at `RecordBatch::concat`, naming the
first differing field. To obtain a schema to paste in, run
`…/ns-rec/records_schema` on one chunk.

### 3.4 Read it

With `records` on, `LibKind`'s default recipe provider is a chain:
`[DefaultRecipeProvider, ManifestRecipeProvider]`. A `recipes.yaml` entry always wins, and every
chunk key becomes addressable from anywhere `-R/` is. All of these queries validate:

| Query | Result |
|---|---|
| `-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` | Every chunk, concatenated, as CSV |
| `-R/data/x.manifest.yaml/-/ns-rec/materialize-5000000` | The same, with the default `max_rows` of 1 000 000 raised |
| `-R/data/sales/daily_0010.csv` | One template chunk, evaluated through its keyed recipe |
| `-R/data/sales/three.manifest.yaml/-/ns-rec/to_record_source/-/ns-rec/rowid-2-0` | Row 0 of chunk 2. Only that chunk runs |

```rust
let state = eval(envref.clone(), "-R/data/x.manifest.yaml/-/ns-rec/materialize").await?;
let view = state.value()?.as_record_view()?;
assert_eq!(
    offsets(&view)?,
    vec![FieldValue::Int(55), FieldValue::Int(1), FieldValue::Int(2)],
    "the explicit chunk's arguments must apply, which only its keyed recipe can do"
);

tokio::time::sleep(Duration::from_millis(300)).await;
let store = envref.get_async_store();
for chunk in ["data/first.csv", "data/x_0001.csv", "data/x_0002.csv"] {
    assert!(
        store.contains(&parse_key(chunk)?).await?,
        "chunk {chunk} must be a keyed asset, stored under its key"
    );
}
```
<sub>`liquers-lib/tests/record_manifest_resource_key.rs`,
`materialize_keys_the_chunks_of_a_manifest_fetched_as_a_resource`</sub>

If you build an environment whose **base** provider is replaced, for example
`with_recipe_provider_choice(...)` from a configuration document, the manifest provider is dropped.
Call `.with_records_recipe_provider()` (trait `liquers_lib::environment::RecordsRecipeProvider`) to
add it back. See `with_records_recipe_provider_serves_a_manifest_chunk_over_a_trivial_base` in
`liquers-lib/src/environment.rs`.

---

## 4. Choosing a batch size

At HEAD, **a chunk is a batch**: a stored chunk is read whole, and a stream holds one chunk at a
time (`records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time`). So the batch size
you choose is the rows per chunk, which is the template's `batch_size` or how you split the explicit
chunks. It sets three things:

- **Memory while streaming** is about one chunk.
- **Memory while materializing** is about the whole stream twice, because `materialize` collects
  every chunk and then concatenates them. `max_rows` (default 1 000 000) is the guard.
- **Refresh granularity**: a stale chunk is re-evaluated whole.

Estimate one row's bytes from the column layout. Each buffer is 64-byte aligned, so the padding is
negligible at any useful size.

| Type | Bytes per row |
|---|---|
| `Int`, `UInt`, `Float`, `Timestamp` | 8 |
| `Date` | 4 |
| `Bool` | 1/8 |
| `Text`, `Binary` | 4 (offset) + the value's bytes |
| `Vector` | 4 × `dim` |
| any nullable column | + 1/8 for validity |

For example, 10 `Float` columns plus a 40-byte text column come to about 124 bytes per row. A
100 000-row chunk is then about 12 MB, and a 1 000 000-row materialize peaks near 250 MB. Size
chunks by bytes rather than by a round row count: aim for a few MB to a few tens of MB per chunk,
and fewer rows when rows are wide or carry vectors. A byte-budget batch size is an open question in
the design and is not implemented.

---

## 5. Using views as a DataFrame

Views are the small DataFrame that works where polars is unavailable, including in wasm. Every
constructor is an inherent method on `dyn RecordView` taking `self: &Arc<Self>`, and it returns a
new `Arc<dyn RecordView>`. No data is copied until you `materialize`.

**Filter with a mask:**

```rust
let batch: Arc<dyn RecordView> = Arc::new(batch.freeze()?);

let size_col = batch.column(1)?;
let mask = size_col.compare(CompareOp::Eq, &FieldValue::Int(5))?;
let filtered = batch.filter(&mask)?;
assert_eq!(filtered.len(), 2); // rows 0 and 2

let mat = filtered.materialize()?;
assert_eq!(mat.column(0)?.get(0)?, FieldValue::Text(Arc::from("a")));
```
<sub>`liquers-lib/tests/records_scenario_files_to_csv.rs`, `filter_through_mask_gathers_selected_rows`</sub>

Combine masks with `Bitmap::and`, `or` and `not`. Use `Column::null_mask` to select nulls:

```rust
let a = Bitmap::from_bools(&[true, true, true, true, false, false, false, false]);
let b = Bitmap::from_bools(&[true, false, true, false, true, false, true, false]);
let result = a.and(&b).expect("and");
```
<sub>`liquers-records/src/buffer.rs`, `bitmap_and_combines_masks`</sub>

**Derive a column** lazily, computed from the same row range of the source columns:

```rust
let batch_arc: Arc<dyn RecordView> = make_simple_batch();
let view = batch_arc.with_column(FieldSchema::new("double_id", FieldType::Int), &[0], |cols| {
    let id_col = &cols[0];
    let mut doubled = Vec::with_capacity(id_col.len());
    for i in 0..id_col.len() {
        match id_col.get(i)? {
            FieldValue::Int(n) => doubled.push(n * 2),
            other => return Err(Error::general_error(format!("expected Int, got {other:?}"))),
        }
    }
    Ok(Column::Int { validity: None, values: Buffer::from_slice(&doubled) })
})?;
assert_eq!(view.value(4, 3)?, FieldValue::Int(10));
```
<sub>`liquers-records/src/views.rs`, `derived_column_view_computes_from_source_columns`</sub>

The full set of operations:

| Operation | Call | Query-level command |
|---|---|---|
| Projection | `select_columns(&[..])`, which keeps `Id` / `Source` and appends them after the named columns | `ns-rec/select_columns-a-b` |
| Row window | `slice(offset, len)`, `row(n)` | `ns-rec/slice-1-2`, `ns-rec/row-1`, `ns-rec/head-5` |
| Filter / gather | `filter(&mask)`, `take(&[u32])` | none (use Rust) |
| Derived / precomputed column | `with_column`, `with_columns` | none |
| One cell | `cell(row, "col")`, then `single_cell()` | `…/-/ns-rec/rec_id-42/select_columns-price` |
| Concatenate | `RecordBatch::concat(&[..])` (batches, not views; the schemas must match) | `ns-rec/materialize` on a source |

**When to `materialize`.** Views do not cache, so a chain of filters re-reads its base on every
access. Materialize when you will read the result many times, before handing it to JavaScript
(the web binding materializes non-batch views itself), and before exporting it. `materialize` on a
`RecordBatch` is a shallow clone.

**What is deliberately absent:** group-by, join and sort. They belong to a query engine. On
native builds, convert to a polars `DataFrame` (§6.4) and use `ns-pl`.

---

## 6. Writing a view, a source, or an export

### 6.1 Implementing `RecordView`

Only `schema`, `len` and `column_range` are required. `column`, `value`, `materialize`, `row_id`
and the rest have defaults built on `column_range`. Check both bounds and return an error. Never
panic.

```rust
impl RecordView for ConstantView {
    fn schema(&self) -> &Arc<RecordSchema> { &self.schema }
    fn len(&self) -> usize { self.len }
    fn column_range(&self, col: usize, rows: std::ops::Range<usize>) -> Result<Column, Error> {
        if col != 0 {
            return Err(Error::general_error(format!("ConstantView has one column, got {col}")));
        }
        if rows.end > self.len {
            return Err(Error::general_error(format!("range {rows:?} out of bounds for len {}", self.len)));
        }
        let values = vec![self.value; rows.len()];
        Ok(Column::Int { validity: None, values: Buffer::from_slice(&values) })
    }
}
```
<sub>`liquers-records/tests/records_guide_counterparts.rs`,
`records11_a_user_defined_view_agrees_across_column_value_and_materialize`</sub>

**The equivalence test.** Every view must give the same cells through `column_range(full)`,
`column_range(row..row+1)`, `value(row, col)` and `materialize()`. Copy `assert_reads_agree` from
`liquers-records/src/views.rs`, or the loop in `records11`, and run it on every column.

**A generator** that computes cells from a closure is `RowFnView`. It calls `f(row, col)` only for
the rows and column requested:

```rust
let view: Arc<dyn RecordView> = Arc::new(
    RowFnView::new(schema, batch_arc.len(), move |_row, _col| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        Ok(FieldValue::Int(0))
    })
    .expect("RowFnView::new"),
);
let _col = view.column_range(0, 1..3).expect("column_range");
assert_eq!(call_count.load(Ordering::SeqCst), 2); // exactly rows 1 and 2, column 0
```
<sub>`liquers-records/src/views.rs`, `row_fn_view_calls_closure_only_for_the_requested_range_and_column`</sub>

### 6.2 Building columns: `ColumnMut`

Inside `column_range` or `with_column`, build the output with `ColumnMut` rather than laying out
buffers by hand. It computes offsets and validity for you:

```rust
let mut col = ColumnMut::new(FieldType::Vector);
col.push(&FieldValue::Vector(Arc::from(vec![1.0f32, 2.0]))).expect("push");
col.push(&FieldValue::Vector(Arc::from(vec![3.0f32, 4.0]))).expect("push");
col.set(0, &FieldValue::Vector(Arc::from(vec![9.0f32, 9.5]))).expect("set");
let frozen = col.freeze();
```
<sub>`liquers-records/src/mutable.rs`, `column_mut_round_trips_vector_values`</sub>

To edit an existing batch, call `RecordBatch::into_mut()`. It takes over an unshared buffer and
copies a shared one, and the original is never touched
(`records04_editing_a_shared_batch_copy_leaves_the_original_untouched`). `set` on a text, binary or
validity column rebuilds the column (`COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY`).

### 6.3 Implementing `RecordSource`

Implement `stream`, `chunks` and `describe_chunk`. Override `schema` when you can promise one, and
`materialize` when you can do better than draining the stream. Build the stream with
`futures::stream::unfold`, resolving one chunk per poll, and wrap it with `record_stream`:

```rust
let inner = futures::stream::unfold((ids.into_iter(), resolver), move |(mut ids, resolver)| async move {
    let id = ids.next()?;
    // … map `id` to a query …
    let result = resolver.evaluate(query).await.map(|cv| match cv {
        ChunkValue::View(v) => v,
        // …
    });
    Some((result, (ids, resolver)))
});
Ok(record_stream(Box::pin(inner), None))
```
<sub>`liquers-records/tests/records_guide_counterparts.rs`, `SequentialSource` (used by
`records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time`)</sub>

Pass every `ChunkValue` through `view_from_chunk_value` and every view through `place_chunk`, both
exported. The first applies a declared schema. The second stamps the chunk index and id, so that
`row_id`, `rowid` and `rec_id` work. `InMemorySource::new(views)` is the ready-made source over
views already in memory.

### 6.4 Handing a batch to polars or pandas

There is **no zero-copy export and no Python binding** at HEAD. `liquers-py` has no record types,
and there is no `polars-arrow` hand-off. Three routes exist, and each one copies:

| Route | How | Limits |
|---|---|---|
| polars, in process | `liquers_lib::records::polars::{record_batch_to_dataframe, dataframe_to_record_batch}` (feature `polars`) | Copies per column. Roles are lost, and no `Id` is guessed on the way back. `Vector` is refused (`POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`) |
| Parquet bytes | Write with a `.parquet` filename, e.g. `…/ns-rec/materialize/daily.parquet`, or call `write_table(…, TableFormat::Parquet, …)`. Read in pandas or pyarrow | `Vector` is refused. Reading in Liquers goes through polars, and a declared schema is ignored (`RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA`) |
| Arrow IPC / Feather bytes | `TableFormat::Ipc` (feature `records-ipc`). The filename aliases are `ipc`, `feather`, `arrow`, `arrow_ipc` | This crate's reader cannot read any polars **string** column (§7.11) |

```rust
let df = record_batch_to_dataframe(&batch)?;
assert_eq!(df.height(), 3);
let back = dataframe_to_record_batch(&df)?;
assert_eq!(back.value(1, 0)?, FieldValue::Null);
// Every field lands as ordinary data — no `Id` is guessed, matching CSV's schema-less reader.
assert_eq!(back.schema.id_field(), None);
```
<sub>`liquers-lib/tests/records_parquet_polars.rs`, `bridge_round_trips_record_batch_through_dataframe`</sub>

The IPC and Parquet writers were checked against pyarrow 25 and pandas 3 during review. No
in-repo test runs Python.

### 6.5 Reading a chunk from JavaScript

`liquers-web` hands a `RecordView` to JavaScript as a `RecordBatch` handle
(`liquers-web/src/records.rs`). The handle offers `numRows`, `numColumns`, `schemaJson`,
`column(i)` (a descriptor `{kind, ptr, len, validity, …}` into wasm memory, with no data copied),
`columnView(i)` (a `RecordColumn` companion from `records_column.js`) and `columnCopy(i)` (an owned
copy).

The rule for a view built from a descriptor: **after any call into wasm, check
`view.buffer !== memory.buffer`. If it differs, re-create the view at the same pointer and
length.** `memory.grow` detaches the old `ArrayBuffer` but never moves the data. `RecordColumn`
does this check for you:

```rust
let before = memory_buffer();
let view = Float64Array::new_with_byte_offset_and_length(&before, ptr, len);
// Grow linear memory by one page: the old ArrayBuffer is detached, the data does not move.
let previous_pages = core::arch::wasm32::memory_grow::<0>(1);
let after = memory_buffer();
assert_eq!(view.length(), 0, "a view over a detached buffer reads nothing, not garbage");
let refreshed = Float64Array::new_with_byte_offset_and_length(&after, ptr, len);
assert_eq!(refreshed.to_vec(), values, "same pointer, same length, same values");
```
<sub>`liquers-web/tests/records_RECORDS.rs`,
`records05_a_column_view_is_detected_stale_after_memory_grow_and_refreshes_in_place`</sub>

Use **`columnCopy`** when the data must outlive the handle, cross a `postMessage`, or be held
through code you do not control. Also use it for `Text` and `Binary` columns, which have no single
typed view. After `free()`, any view into the handle's memory is dangling, and nothing detects it.
Views are read-only, so never write through one.

---

## 7. Pitfalls

Each of these was hit during implementation or review.

1. **Offsets are `len + 1` long.** A `Text` or `Binary` column of *n* rows has *n + 1* `i32`
   offsets, starting at 0, never decreasing, and ending within `data`. Text must be valid UTF-8 at
   every boundary. `RecordBatch::new` refuses anything else
   (`record_batch_new_refuses_text_offsets_past_the_data`,
   `…_refuses_decreasing_or_negative_offsets`). `ColumnMut` builds offsets correctly for you.
2. **`Vector` needs its width.** `Column::Vector { dim, data }` holds exactly `dim × rows` `f32`
   values, null rows included (`record_batch_new_refuses_vector_data_not_a_multiple_of_dim`).
   `ColumnMut` fixes `dim` at the first non-null push, backfills earlier null rows, and refuses a
   different width afterwards (`column_mut_vector_dim_mismatch_is_an_error`). Polars and Parquet
   refuse `Vector` columns.
3. **`Arc<RecordBatch>` is not `Arc<dyn RecordView>`.** The view constructors take
   `self: &Arc<dyn RecordView>`, so calling `batch.filter(…)` on an `Arc<RecordBatch>` is E0599.
   Bind the value as `let v: Arc<dyn RecordView> = …` or write `batch as Arc<dyn RecordView>`. This
   error recurred throughout the Phase 3 tests.
4. **`select_columns` puts the key columns last.** Named columns come first, in the order given,
   and `Id` / `Source` are appended after them. Look up a column by name, not by position.
5. **Store a manifest as `<name>.manifest.yaml`.** Under any other name it is keyless: its chunks
   are never stored, and `arguments` / `links` are refused when the stream opens
   (`a_keyless_manifest_with_chunk_arguments_is_refused`). A hand-placed manifest in a memory store
   also needs `Status::Source` metadata, or it will not fast-track (`set_manifest` in the tests).
6. **`Option<Value>` arguments do not bind** (`REGISTER-COMMAND-OPTION-VALUE-CANNOT-BIND`). Use
   `String = ""` and parse the value yourself, as `to_record`'s `schema` does.
7. **List a directory with the `sdir` header: `-R-sdir/<dir>/-/ns-rec/file_records`.** A plain
   `-R/<dir>/-/…` validates but fails at run time ("Key not found"), because it asks for the value
   *at* the key and a directory holds none; the `sdir` header yields the store's listing and carries
   the directory's key into the command
   (`records_end_to_end.rs`, `file_records_lists_a_store_directory_through_a_query`).
8. **The free `interpreter::evaluate` sets a `bin` data format.** Asking for `as_bytes()` on its
   result then fails even with a `.csv` filename (`FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`).
   Test serialization through `EnvRef::evaluate` (§2.6).
9. **A new value type with no `TypeInfo` cannot be stored.** The write path refuses an identifier
   the registry lacks. `RecordView` and `RecordSource` are declared in
   `ExtValue::type_descriptions()`. A source declares only `yaml` and `json`, because its only byte
   form is its manifest (`record_typeinfo.rs`). A new `TableFormat` alias needs declaring there too
   (`record_view_type_info_declares_every_table_format_alias`).
10. **A `#[cfg(feature = "records")]` variant needs a gated arm in every exhaustive `match`.** The
    `ExtValue` variants are gated, and matches must not use `_ =>`. A missing arm compiles with
    default features and breaks the `--no-default-features` build. `value/mod.rs`,
    `ui/web/html.rs`, `egui/mod.rs` and `liquers-web/src/default_value.rs` all carry these arms.
    Run the build matrix.
11. **The IPC reader cannot read a polars string column.** Polars writes text as `Utf8View` or
    `LargeUtf8`, never as plain `Utf8` (`IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN`). IPC from
    Liquers to polars works. For polars to Liquers, use Parquet or avoid text columns.
12. **Do not keep a JS view across a wasm call.** Any call can grow memory, which leaves your view
    empty (Hazard A). Use a view after `free()` and it reads freed memory, with no detection
    (Hazard B). Refresh or copy (§6.5).
13. **`RecordView::column` returns a copy.** `Column::slice` copies
    (`COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE`). A pointer taken into that copy
    dangles as soon as the copy drops, which is why the web descriptor reads `inner.columns[i]`
    directly.
14. **A replaced recipe provider drops manifests** (§3.4). In addition, the provider's folder
    listing is cached and never refreshed (`MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES`): a
    manifest added to a running server's folder does not serve its **explicit** chunk keys.
15. **Stored chunks are written in the background.** The asset manager's metadata saver debounces
    writes, so a test that checks the store sleeps first (300 ms in the tests).
16. **Nulls are not always distinguishable from empty values.** CSV writes a null as an unquoted
    empty field and `""` as a quoted one (`csv_serialization_distinguishes_null_from_empty_string`).
    Markdown cannot tell them apart (`MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT`), and
    HTML cannot be read back at all.

---

## 8. Testing

Put unit tests beside the code (`#[cfg(test)] mod tests` at the end of the file). Put integration
tests in `liquers-records/tests/` (data model only) or `liquers-lib/tests/record*.rs` (anything
needing `Value` or an environment), gated `#![cfg(feature = "records")]`.

**The loops:**

```bash
cargo test -p liquers-records --all-features --lib --tests    # records work: builds core only
cargo test -p liquers-records --lib --tests                   # built-in formats only
cargo test -p liquers-lib --lib --tests                       # the normal loop
cargo test -p liquers-lib --no-default-features --features records --lib --tests
bash scripts/check-build-matrix.sh                            # every feature row, incl. records ones
```

`liquers-web` builds only for wasm32. Run its loops separately, after `cargo clean`:

```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
./liquers-web/examples-web/quickstart/build.sh && ./liquers-web/scripts/check-stubs.sh
cd liquers-web/tests/e2e && npm install && npx playwright test
```

`--features debug-handles` exposes `live_batch_handle_count()`, so
`records06_freeing_the_handle_releases_the_batch` can assert that `free()` really drops the
`Arc<RecordBatch>`. Use the same pattern for any new handle type. If you change the TypeScript
surface, update `liquers-web/tests/stubs/valid_usage.ts` so `check-stubs.sh` exercises it.

**What each new piece owes:**

| You added | Its test |
|---|---|
| A view | The equivalence check (§6.1) on every column, plus bounds errors for column and row |
| A command | Through a query with `evaluate`, including one refusal. Regenerate the registry and run `registry_export` |
| A manifest feature | End to end in a memory store (`records_end_to_end.rs` pattern), with a counting fixture command to prove which chunks ran |
| A table format or a type in one | A round trip: `format_round_trip.rs` (`assert_values_round_trip`, `assert_every_cell_equal`), and the TypeInfo-driven `every_declared_record_view_format_round_trips_or_is_recorded_as_write_only` in `record_typeinfo.rs` |
| Interop with polars, pyarrow or pandas | Both directions where both are possible (`records_ipc_polars.rs`, `records_parquet_polars.rs`), with fixtures committed under `liquers-records/tests/fixtures/` and the Python that generated them in a doc comment |
| A wasm-visible handle | A Node-loop test like `records_RECORDS.rs`, and a `debug-handles` release assertion |

**Round trips at HEAD:** CSV, TSV, NDJSON and JSON round-trip values. Roles and chunk identity do
not survive CSV (`records01_csv_documents_which_metadata_it_loses`). IPC round-trips losslessly,
schema included (`feather_round_trips_lossless`). Markdown reads back its own output. HTML is
write-only. Parquet is written here and read back only through polars
(`parquet_write_succeeds_but_reading_here_is_refused`, `parquet_round_trip_through_polars_preserves_data`).

---

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-27 | PR #72 review: pitfall 7 now gives the working directory query, `-R-sdir/<dir>/-/ns-rec/file_records`, with its end-to-end test. | PR #72 review |
| 2026-09-27 | Created: the shape decision, the record-producing command walkthrough, the manifest walkthrough, batch sizing, views as a DataFrame, writing views and sources, the polars/Parquet/IPC and JavaScript hand-offs, pitfalls from implementation and review, and testing. Every snippet is taken from a passing test. | `design/record-streams/` phase-5 |
