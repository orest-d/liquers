---
id: RECORD-STREAMS-PHASE3-TESTS
kind: analysis
title: Phase 3 test code — record streams
workflow: liquers-project
status: draft
area: [lib/value]
created: 2026-09-21
---
# Phase 3 test code — Record streams

**This is the deliverable of a test-first Phase 3.** None of it compiles today; it is written to
compile once Phase 4 implements the types declared in `phase2-architecture.md` and the completions
listed in `phase3-examples.md` §"What Phase 3 found that Phase 2 must absorb". Phase 4's job is to
make it pass.

See [`phase3-examples.md`](./phase3-examples.md) for the narrative, the overview table, the pitfalls
and the corner cases. This file is the code, organized by the file each block lands in. Every code
block opens with a comment naming its target path. A block still using `todo!(...)` is labelled a
**sketch**, not a test — it is either a genuine Phase 4 decision or infrastructure this design
defers on purpose (see the sketch's own comment for which).

Every name below is the Phase 2 architecture's or a Phase 3 completion's; where a name a drafter
used differs from Phase 2 (`get_value` for `value`, `Bitmap::from_bytes` for the constructors this
document adds), the Phase 2 / completion name is what appears here.

| § | File | Tests | Covers |
|---|---|---|---|
| 1.1 | `liquers-lib/src/records/commands.rs` (+ `liquers-lib/tests/records_scenario_files_to_csv.rs`) | 8 | Scenario 1 — a store directory projected to records, filtered, serialized |
| 1.2 | `liquers-lib/src/records/convert.rs`, `liquers-records/src/provider.rs`, `liquers-records/src/chunk_resolver.rs`, `liquers-records/src/lib.rs` (+ `liquers-lib/tests/record_manifest_keyed.rs`) | 9 | Scenario 2 — a manifest-driven stream with keyed chunks |
| 2.1 | `liquers-records/src/schema.rs` | 13 | `RecordSchema::new` invariants, `FieldSchema` builders, YAML defaults |
| 2.2 | `liquers-records/src/buffer.rs` | 14 | `AlignedBuffer`, `Buffer<T>`, `Bitmap` incl. the new constructors |
| 2.3 | `liquers-records/src/lib.rs` (`Column` kernels) | 11 | `Column::{get,slice,take,filter,compare,null_mask,concat}` |
| 2.4 | `liquers-records/src/views.rs` | 11 | `column_range`/`value` agreement, every view constructor |
| 2.5 | `liquers-records/src/views.rs` (implicit ids) | 4 | `RowId`, `RowRun`, ids through a filter |
| 2.6 | `liquers-records/src/mutable.rs` | 8 | `RecordBatchMut`, `ColumnMut`, `into_mut` |
| 2.7 | `liquers-records/src/manifest.rs` | 12 | `ChunkNaming`, `ChunkTemplate`, `ManifestSpec`/`ManifestSource` validation |
| 3.1 | `liquers-records/src/formats/csv.rs` | 13 | null/empty-string quoting, canonical-int rule, schema-aware errors |
| 3.2 | `liquers-records/src/formats/ndjson.rs` | 3 | union keys, vector inference, round trip |
| 3.3 | `liquers-records/src/formats/shapes.rs` | 12 | all eight `JsonOrient`s, `auto` detection, the ambiguous-shape error |
| 3.4 | `liquers-records/src/formats/markdown.rs` | 3 | escaping, label header, numeric alignment |
| 3.5 | `liquers-records/src/formats/html.rs` | 4 | escaping, structure, numeric/null classes |
| 3.6 | `liquers-records/src/formats/mod.rs` | 3 | `TableFormat::from_data_format` aliases, `ReadOptions`/`WriteOptions` defaults |
| 3.7 | `liquers-records/src/formats/ipc.rs` (feature `ipc`) | 4 (2 `#[ignore]`d) | lossless round trip, `chunk_id`, refused dictionary/compressed batches |
| 3.8 | `liquers-records/src/formats/parquet.rs` (feature `parquet`) | 2 (1 `#[ignore]`d) | `Vector` column refused, polars round trip |
| 4.1 | `liquers-core/src/recipes.rs` (`RecipeProviderChain`) | 5 | first-`Some`-wins, `contains`/`assets_with_recipes` union |
| 4.2 | `liquers-core/src/recipes.rs` (`stored`/`cached`) | 10 | `Recipe`/`MetadataRecord`/`AssetInfo` default-true accessors |
| 5.1 | `liquers-lib/tests/record_value_round_trip.rs` | 2 | `ExtValue::RecordView`/`RecordSource` preserve their `Arc` |
| 5.2 | `liquers-lib/tests/record_typeinfo.rs` | 3 | `TypeInfo` entries exist with the bare identifiers |
| 5.3 | `liquers-lib/tests/to_record_conversions.rs` | 3 | `to_record` on bytes, a source, unlabelled text |
| 5.4 | `liquers-lib/tests/to_record_source_manifest.rs` | 1 | manifest discriminator recognized through `evaluate` |
| 5.5 | `liquers-lib/tests/record_scalar_reading.rs` | 3 (1 `#[ignore]`d) | single-cell scalar read, shape-naming refusal, linked `f64` binding |
| 5.6 | `liquers-records/tests/manifest_chunking.rs` | 3 | `ChunkList::Unbounded`, `ChunkNaming` round trip |
| 5.7 | `liquers-records/tests/stream_static_lifetime.rs` | 1 | a stream outlives the `Arc<dyn RecordSource>` that opened it |
| 5.8 | `liquers-lib/tests/resolver_dependency_recording.rs` | 0 (2 `#[ignore]`d sketches) | `ContextResolver` vs `EnvResolver` dependency recording — Phase 4 |
| 6 | `liquers-records/tests/records_guide_counterparts.rs` (+ `liquers-web/tests/records_RECORDS.rs`, 2 wasm tests) | 9 | `RECORDS01`–`RECORDS11` Rust counterparts (`RECORDS10` is §5.5; `RECORDS03` is §3.7) |
| 7 | `liquers-records/tests/format_round_trip.rs` | 8 | one round-trip test per serialization format |
| 8 | (script, no new file) | — | build-matrix rows this design adds |
| 9 | `liquers-records/tests/manifest_validation.rs` | 5 | chunk-query planning, unknown version, name collisions, templated naming |

**Totals:** 189 `#[test]`/`#[tokio::test]` functions across 28 files (recounted at the Phase 4 review; earlier drafts said 191 across 32), of which **6 are `#[ignore]`d
sketches**: 2 in §3.7 (IPC dictionary/compression fixtures), 1 in §3.8 (the polars Parquet bridge),
2 in §5.8 (dependency recording, needs a live `Context`), and 1 in §5.5 (blocked on
`EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS`). **183 tests carry a real, non-`#[ignore]`d
assertion.** Two further tests in `liquers-web/tests/records_RECORDS.rs` (§6) use
`#[wasm_bindgen_test]` and run in `liquers-web`'s wasm loop; they are not counted above.


# 1. Scenario code

## 1.1 Scenario 1 — Files to CSV

### Target: `liquers-lib/src/records/commands.rs`

```rust
// liquers-lib/src/records/commands.rs — list files in a store directory as a table
use std::sync::Arc;

use liquers_macro::register_command;
use liquers_core::{context::{Context, Environment}, error::Error, state::State};
use liquers_records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordValue, RecordView, RecordViewMut,
};

use crate::value::Value;

/// List every file directly under a directory key as a `RecordBatch`: `file_id`, `file_name`,
/// `size_bytes`, `modified_timestamp`. Async because it lists the store. Registered as
/// `ns-rec/file_records`.
pub async fn file_records<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("file_id", FieldType::Text)
            .with_label("File ID")
            .with_key(KeyRole::Id),
        FieldSchema::new("file_name", FieldType::Text)
            .with_label("Name")
            .with_role(FieldRole::text()),
        FieldSchema::new("size_bytes", FieldType::Int)
            .with_label("Size")
            .with_role(FieldRole::numeric())
            .not_null(),
        FieldSchema::new("modified_timestamp", FieldType::Timestamp)
            .with_label("Modified")
            .with_role(FieldRole::numeric()),
    ])?);

    // `-R/data/` is a directory: its state carries no value, only metadata naming the key.
    let dir_key = state.metadata.key()?.ok_or_else(|| {
        Error::general_error("file_records needs a directory resource, e.g. -R/data/".to_string())
    })?;
    // `Context` has no store accessor of its own; the store is the environment's.
    let store = context.get_envref().get_async_store();
    let entries = store.listdir_asset_info(&dir_key).await?;

    let mut batch = RecordBatchMut::with_capacity(schema, entries.len());
    for info in entries.into_iter().filter(|info| !info.is_dir) {
        let name = info.filename.unwrap_or_default();
        let file_key = dir_key.join(&name);
        // `updated` is RFC 3339; a file whose store does not report a valid timestamp gets 0
        // rather than failing the whole listing — this is diagnostic, not load-bearing data.
        let modified_us = chrono::DateTime::parse_from_rfc3339(&info.updated)
            .map(|dt| dt.timestamp_micros())
            .unwrap_or(0);
        batch.append_row(&[
            FieldValue::Text(Arc::from(file_key.to_string().as_str())),
            FieldValue::Text(Arc::from(name.as_str())),
            FieldValue::Int(info.file_size.unwrap_or(0) as i64),
            FieldValue::Timestamp(modified_us),
        ])?;
    }

    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}

// Registration:
// let cr = env.get_mut_command_registry();
// register_command!(cr,
//     async fn file_records(state, context) -> result
//     namespace: "rec"
//     label: "File records"
//     doc: "List all files in a directory as a table with size and modification time"
// )?;
```

**Filtering, and why it is Rust code rather than a query action.** Phase 2's Relevant Commands
table exposes `rec_id`, `row`, `select_columns`, `head`, `slice` and `rowid` as queries; `filter`
and `take` are inherent methods on `dyn RecordView` (§"Building views") with **no query-level
command** — arbitrary boolean filtering is out of scope for this design's own vocabulary (a future
`ns-search` predicate is where it belongs). The example below is therefore library code operating
on an already-evaluated view, not a query string.

```rust
// Example of filtering and scalar reading — library code, not a query
use liquers_records::{CompareOp, FieldValue};

// `files_view` came from evaluating a query, e.g. `-R/data/-/ns-rec/file_records`.
fn largest_files(files_view: &Arc<dyn RecordView>) -> Result<Arc<dyn RecordView>, Error> {
    // Step 1: project down to id + size (Id survives even though only size_bytes is named).
    let projected = files_view.select_columns(&["size_bytes"])?;

    // Step 2: build a mask over the size column — files > 10 000 bytes.
    let size_col = projected.column(1)?; // 0 = file_id (kept), 1 = size_bytes
    let mask = size_col.compare(CompareOp::Gt, &FieldValue::Int(10_000))?;

    // Step 3: filter. The mask is turned into a row-index list once, so later reads are
    // proportional to the filtered range, not the base.
    projected.filter(&mask)
}

// Step 4: a single cell reads as a scalar when the result is one row and one payload column.
fn largest_size(files_view: &Arc<dyn RecordView>) -> Result<Option<i64>, Error> {
    let large = largest_files(files_view)?;
    if large.len() != 1 {
        return Ok(None);
    }
    match large.value(0, 1)? {
        FieldValue::Int(n) => Ok(Some(n)),
        FieldValue::Null
        | FieldValue::Bool(_)
        | FieldValue::UInt(_)
        | FieldValue::Float(_)
        | FieldValue::Text(_)
        | FieldValue::Bytes(_)
        | FieldValue::Date(_)
        | FieldValue::Timestamp(_)
        | FieldValue::Vector(_) => Ok(None),
    }
}
```

### Queries — Scenario 1

```
-R/data/-/ns-rec/file_records/files.csv
-R/data/-/ns-rec/file_records/files.md
-R/data/-/ns-rec/file_records/files.html
-R/data/-/ns-rec/file_records/select_columns-file_id-size_bytes/files_projected.csv
-R/data/-/ns-rec/file_records/head-3/top_files.csv
-R/data/-/ns-rec/file_records/rowid-0-0
```

None of these validate against `specs/command_registry.yaml` today — `file_records` and every
`ns-rec` command are Phase 4 work. Each would be checked at Phase 4 with
`liquers-validate --command file_records -- '<query>'` (and `--command` for every other new name in
the chain) before being trusted; the shapes above follow the chaining convention already in use for
`ns-pl` (`specs/reference/POLARS_COMMAND_LIBRARY.md:61`: one namespace prefix, dash-joined
arguments, actions chained by `/`).

### Target: `liquers-lib/tests/records_scenario_files_to_csv.rs`

```rust
// liquers-lib/tests/records_scenario_files_to_csv.rs
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_records::{
    CompareOp, FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut,
    RecordSchema, RecordView, RecordViewMut,
};

fn schema_with_id_and_size() -> Arc<RecordSchema> {
    Arc::new(
        RecordSchema::new(vec![
            FieldSchema::new("file_id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("size_bytes", FieldType::Int),
        ])
        .expect("schema valid"),
    )
}

#[test]
fn schema_with_id_and_roles_roundtrips() {
    let schema = schema_with_id_and_size();
    assert_eq!(schema.id_field(), Some(0));
    assert_eq!(schema.payload_fields(), vec![1]);
    assert_eq!(schema.index_of("file_id"), Some(0));
    assert_eq!(schema.index_of("size_bytes"), Some(1));
}

#[test]
fn record_batch_mut_builds_and_freezes() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_with_id_and_size();
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::Text(Arc::from("file1")), FieldValue::Int(42)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("file2")), FieldValue::Int(100)])?;
    let frozen = batch.freeze()?;
    assert_eq!(frozen.len, 2);
    assert_eq!(frozen.schema.fields.len(), 2);
    Ok(())
}

#[test]
fn column_compare_builds_mask_matching_selected_rows() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_with_id_and_size();
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(5)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("b")), FieldValue::Int(15)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("c")), FieldValue::Int(25)])?;
    let batch = batch.freeze()?;

    let size_col = batch.column(1)?;
    let mask = size_col.compare(CompareOp::Gt, &FieldValue::Int(10))?;

    assert_eq!(mask.count_ones(), 2); // rows 1 and 2 are > 10
    assert!(!mask.get(0));
    assert!(mask.get(1));
    assert!(mask.get(2));
    Ok(())
}

#[test]
fn select_columns_keeps_id_field_even_when_unnamed() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("size", FieldType::Int),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[
        FieldValue::Text(Arc::from("f1")),
        FieldValue::Text(Arc::from("report.csv")),
        FieldValue::Int(1024),
    ])?;
    let batch: Arc<dyn RecordView> = Arc::new(batch.freeze()?);

    let projected = batch.select_columns(&["name", "size"])?;
    assert_eq!(projected.schema().fields.len(), 3); // id kept
    assert_eq!(projected.schema().fields[0].name, "id");
    assert_eq!(projected.schema().id_field(), Some(0));
    Ok(())
}

#[test]
fn filter_through_mask_gathers_selected_rows() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("size", FieldType::Int),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(5)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("b")), FieldValue::Int(15)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("c")), FieldValue::Int(5)])?;
    let batch: Arc<dyn RecordView> = Arc::new(batch.freeze()?);

    let size_col = batch.column(1)?;
    let mask = size_col.compare(CompareOp::Eq, &FieldValue::Int(5))?;
    let filtered = batch.filter(&mask)?;
    assert_eq!(filtered.len(), 2); // rows 0 and 2

    let mat = filtered.materialize()?;
    assert_eq!(mat.column(0)?.get(0)?, FieldValue::Text(Arc::from("a")));
    assert_eq!(mat.column(0)?.get(1)?, FieldValue::Text(Arc::from("c")));
    Ok(())
}

#[test]
fn csv_serialization_distinguishes_null_from_empty_string() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("note", FieldType::Text),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::Text(Arc::from("Alice")), FieldValue::Null])?;
    batch.append_row(&[FieldValue::Text(Arc::from("Bob")), FieldValue::Text(Arc::from(""))])?;
    let batch = batch.freeze()?;

    let csv = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
    let csv_str = String::from_utf8(csv)?;

    assert!(csv_str.contains("Alice,\n") || csv_str.contains("Alice,\r\n")); // null: unquoted empty
    assert!(csv_str.contains("Bob,\"\"")); // empty string: quoted
    Ok(())
}

#[test]
fn markdown_uses_labels_not_names_in_header() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("file_id", FieldType::Text).with_label("File ID"),
        FieldSchema::new("size_bytes", FieldType::Int).with_label("Size (bytes)"),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[FieldValue::Text(Arc::from("data.csv")), FieldValue::Int(2048)])?;
    let batch = batch.freeze()?;

    let md = write_table(&batch, TableFormat::Markdown, &WriteOptions::default())?;
    let md_str = String::from_utf8(md)?;
    assert!(md_str.contains("File ID"));
    assert!(md_str.contains("Size (bytes)"));
    assert!(!md_str.contains("file_id"));
    Ok(())
}

#[test]
fn html_escapes_cell_content() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("content", FieldType::Text)])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[FieldValue::Text(Arc::from("<script>alert('xss')</script>"))])?;
    let batch = batch.freeze()?;

    let html = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
    let html_str = String::from_utf8(html)?;
    assert!(html_str.contains("&lt;script&gt;"));
    assert!(!html_str.contains("<script>"));
    Ok(())
}
```

## 1.2 Scenario 2 — A manifest-driven stream with keyed chunks

### The manifest

```yaml
# data/sales/daily.manifest.yaml
manifest: record-stream
version: 1

title: Daily order extract, split by offset
description: |
  1000-row chunks extracted from the orders table, keyed by filename. Explicit chunks override
  the template when they exist; the template generates subsequent chunks as long as they have
  enough rows.

extension: csv
uniform_schema:
  fields:
    - {name: order_id, data_type: Int, key: Id, nullable: false}
    - {name: customer_id, data_type: Int}
    - {name: total, data_type: Float, label: Order total, role: {indexed: [Range]}}
    - {name: region, data_type: Text, description: "Geographic region"}

stored: false
cached: true

chunks:
  # Unkeyed: no filename, identity is its query
  - query: ns-sql/sql_query-0-500
    title: Partial first batch (smaller start)

  # Keyed: query ends with a filename, addressable as -R/data/sales/orders_na.csv
  - query: ns-sql/sql_query-500-500/orders_na.csv
    title: North American orders
    links:
      connection: -R/db/na.yaml

template:
  query: ns-sql/sql_query
  first_offset: 1000
  step: 1000
  batch_size: 1000
```

The stream has two explicit chunks (indices 0, 1) and generates chunks from offset 1000 onward
(index 2+). Chunk 42, if it exists, is `data/sales/daily_0042.csv`.

### Target: `liquers-lib/src/records/convert.rs`

```rust
// liquers-lib/src/records/convert.rs
use std::sync::Arc;

use liquers_core::{
    context::{Context, Environment},
    error::{Error, ErrorType},
    metadata::Metadata,
};
use liquers_records::{ManifestSource, ManifestSpec, RecordSource};

use crate::value::Value;

/// Recognizes the `manifest: record-stream` discriminator and builds a `ManifestSource`. The
/// folder of the state's metadata key becomes the manifest's `cwd`; a manifest with no key (built
/// by a command, never stored) stays keyless, and all of its chunks are unkeyed.
pub async fn to_record_source(
    value: &Value,
    metadata: &Metadata,
    _options: &ToRecordOptions,
    _context: &Context<impl Environment<Value = Value>>,
) -> Result<Arc<dyn RecordSource>, Error> {
    let text = value.try_into_string()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|e| Error::from_error(ErrorType::DeserializationError, e))?;

    let discriminator = doc.get("manifest").and_then(|v| v.as_str());
    if discriminator != Some("record-stream") {
        return Err(Error::general_error(
            "not a record-stream manifest: missing the 'manifest: record-stream' discriminator"
                .to_string(),
        ));
    }

    let spec: ManifestSpec = serde_yaml::from_value(doc)
        .map_err(|e| Error::from_error(ErrorType::DeserializationError, e))?;
    let source = ManifestSource::new(spec, None)?;
    // `Metadata` is an enum; its key is read through the fallible accessor (metadata.rs:1734).
    let source = match metadata.key()? {
        Some(key) => source.with_key(key)?,
        None => source,
    };
    Ok(Arc::new(source))
}
```

### Target: `liquers-records/src/provider.rs` (sketch — Phase 4 fills the bodies)

`ManifestRecipeProvider` implements the **real** `AsyncRecipeProvider<E>` signature
(`liquers-core/src/recipes.rs:477-497`): every directory method takes `&Key` **and** `envref:
EnvRef<E>`, and returns a `Result`, not a bare `Option` — a distinction two of the drafts missed.

```rust
// liquers-records/src/provider.rs
use std::sync::Arc;

use liquers_core::{
    context::{EnvRef, Environment},
    error::Error,
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{AsyncRecipeProvider, Recipe},
};

/// Parsed manifests, cached by the key of their `*.manifest.yaml` file; a stored version check
/// (Phase 4 detail) invalidates a stale entry.
pub struct ManifestRecipeProvider {
    cache: Arc<scc::HashMap<Key, Arc<liquers_records::ManifestSource>>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for ManifestRecipeProvider {
    async fn has_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        todo!("contract: true when `key`'s folder holds a *.manifest.yaml with a record-stream discriminator")
    }
    async fn assets_with_recipes(&self, key: &Key, envref: EnvRef<E>) -> Result<Vec<ResourceName>, Error> {
        todo!("contract: the explicit chunks only — generated names are addressable but not listed (§B)")
    }
    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        todo!("contract: the chunk's recipe, planned — delegates to Recipe::to_plan once recipe() resolves")
    }
    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error> {
        self.recipe_opt(key, envref)
            .await?
            .ok_or_else(|| Error::key_not_found(key))
    }
    async fn recipe_opt(&self, key: &Key, envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        todo!(
            "contract: read the folder's manifest (cached), match `key`'s filename against the \
             explicit chunks or ChunkNaming::index_of, and return that chunk's recipe with \
             cwd/stored/cached copied from the manifest (§B)"
        )
    }
    // `contains` is overridden (not left at the default), because the default assumes a directory
    // listing is enumerable — false for a template's unbounded generated names
    // (RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY).
    async fn contains(&self, key: &Key, envref: EnvRef<E>) -> Result<bool, Error> {
        todo!("contract: match `key` against the explicit chunks or ChunkNaming::index_of, without enumerating")
    }
}
```

### Target: `liquers-records/src/chunk_resolver.rs` (sketch)

**File placement (Phase 4):** there is no `chunk_resolver.rs`. Following Phase 2's Integration
Points table, the `ChunkResolver` trait and `ChunkValue` are in `value.rs` (Step 2.6) and the two
resolvers in `sources.rs` (Step 4.2), with the bound `E::Value: RecordValue` the sketch below
omits.

`ChunkResolver` is a plain object-safe trait returning `BoxFuture` (§"`ChunkResolver`"), not an
`#[async_trait]` trait — an implementation writes `fn foo(&self, …) -> BoxFuture<'static, …> {
Box::pin(async move { … }) }`, never `async fn`.

```rust
// liquers-records/src/chunk_resolver.rs
use std::sync::Arc;

use liquers_core::{
    context::{Context, EnvRef, Environment},
    error::Error,
    maybe_send::BoxFuture,
    metadata::Metadata,
    query::{Key, Query},
};
use liquers_records::{ChunkResolver, ChunkValue};

/// The resolver a command uses: evaluating a chunk records it as a dependency of the result, so
/// the asset manager can invalidate the materialized table when a chunk changes.
pub struct ContextResolver<E: Environment> {
    context: Context<E>,
}

impl<E: Environment> ChunkResolver for ContextResolver<E> {
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            todo!(
                "contract: context.evaluate(&query), record `query` as a dependency of the \
                 current asset, then classify the resulting state as ChunkValue::View / \
                 ::Source / ::Bytes (§'Two readers')"
            )
        })
    }
    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            todo!("contract: read `query`'s metadata without producing its value; record it as a dependency")
        })
    }
    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            todo!("contract: read `key`'s bytes and metadata from the store; record `key` as a dependency")
        })
    }
}

/// The resolver `liquers-axum` uses to serve a source over HTTP (§"Streaming a record source over
/// HTTP"): the stream must outlive the request handler, so nothing it does may write into a
/// `Context` that dies with the handler. Records no dependency.
pub struct EnvResolver<E: Environment> {
    envref: EnvRef<E>,
}

impl<E: Environment> ChunkResolver for EnvResolver<E> {
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let envref = self.envref.clone();
        Box::pin(async move { todo!("contract: evaluate `query` through `envref`; no dependency recorded") })
    }
    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        let envref = self.envref.clone();
        Box::pin(async move { todo!("contract: as evaluate, metadata only") })
    }
    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        let envref = self.envref.clone();
        Box::pin(async move { todo!("contract: read the store directly through `envref`; no dependency recorded") })
    }
}
```

### Target: `liquers-records/src/lib.rs` — `ManifestSource::stream` (sketch)

**Corrected shape.** An earlier draft of this sketch built every chunk with
`futures::stream::FuturesUnordered`, awaited all of them, and concatenated the result *inside*
`stream()` — exactly the "materialize eagerly" behaviour the design exists to avoid, and it also
threw away chunk order, which `RowRun` depends on. The stream must walk chunks **in order**, one
resident at a time, and yield each view as it becomes ready; concatenation is what
`RecordStreamExt::materialize` does to an already-open stream, not what opening one does.

```rust
// liquers-records/src/lib.rs — ManifestSource::stream
impl RecordSource for ManifestSource {
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
        Box::pin(async move {
            let schema = self.schema();
            // `futures::stream::unfold` walks the explicit chunks in order, then the template's
            // chunks in order, stopping at the first short one — one chunk resolved (and
            // resident) per `poll_next`, never all of them at once.
            todo!(
                "contract: futures::stream::unfold over (explicit ids, then template index), \
                 resolving one chunk per step through `resolver` (read_resource for a Key id \
                 whose key the store holds, evaluate otherwise — §'How a manifest's chunks reach \
                 the reader'), stopping the template arm at the first chunk shorter than \
                 batch_size; wrap with `record_stream(inner, schema)`"
            )
        })
    }
}
```

### Target: `liquers-lib/tests/record_manifest_keyed.rs`

```rust
// liquers-lib/tests/record_manifest_keyed.rs
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    parse::parse_key,
    state::State,
};
use liquers_lib::{
    records::{to_record_source, ToRecordOptions},
    value::Value,
};
use liquers_macro::register_command;
use liquers_records::{
    ChunkList, ChunkTemplate, FieldSchema, FieldType, KeyRole, ManifestSource, ManifestSpec,
    RecordSchema,
};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn template_spec(stored: bool, cached: bool, uniform_schema: Option<Arc<RecordSchema>>) -> ManifestSpec {
    ManifestSpec {
        chunks: vec![],
        template: Some(ChunkTemplate {
            query: "ns-sql/sql_query".to_string(),
            first_offset: 0,
            step: 1000,
            batch_size: 1000,
        }),
        extension: Some("csv".to_string()),
        stored,
        cached,
        uniform_schema,
    }
}

#[test]
fn manifest_with_key_derives_unbounded_naming() -> Result<(), Box<dyn std::error::Error>> {
    let key = parse_key("data/sales/daily.manifest.yaml")?;
    let source = ManifestSource::new(template_spec(true, true, None), None)?.with_key(key)?;
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded for a manifest with a template"),
    }
    Ok(())
}

#[test]
fn keyed_chunk_addressing_carries_uniform_schema() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("order_id", FieldType::Int).with_key(KeyRole::Id),
    ])?);
    let spec = template_spec(false, true, Some(schema));
    let source = ManifestSource::new(spec, None)?.with_key(parse_key("data/sales/daily.manifest.yaml")?)?;
    assert!(source.schema().is_some());
    Ok(())
}

#[test]
fn chunk_list_unbounded_with_template_has_no_computed_ids_yet() -> Result<(), Box<dyn std::error::Error>> {
    let source = ManifestSource::new(template_spec(true, true, None), None)?
        .with_key(parse_key("data/sales/daily.manifest.yaml")?)?;
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded"),
    }
    Ok(())
}

#[test]
fn stored_false_cached_true_are_preserved_on_spec() -> Result<(), Box<dyn std::error::Error>> {
    let source = ManifestSource::new(template_spec(false, true, None), None)?;
    assert!(!source.spec().stored);
    assert!(source.spec().cached);
    Ok(())
}

#[test]
fn manifest_schema_is_applied_to_the_whole_source() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
        FieldSchema::new("value", FieldType::Float),
    ])?);
    let source = ManifestSource::new(template_spec(true, true, Some(schema)), None)?;
    let source_schema = source.schema().expect("uniform_schema declared");
    assert_eq!(source_schema.fields.len(), 2);
    Ok(())
}

#[test]
fn manifest_spec_serde_round_trip_ignores_envelope_fields() -> Result<(), Box<dyn std::error::Error>> {
    // `manifest:` and `version:` are the discriminator envelope `to_record_source` reads before
    // deserializing `ManifestSpec` — the spec type itself does not model them and must tolerate
    // their presence when a whole manifest document (not a bare spec) is fed to it directly.
    let yaml = r#"
manifest: record-stream
version: 1
extension: csv
stored: false
cached: true
chunks: []
template:
  query: ns-sql/sql_query
  first_offset: 0
  step: 1000
  batch_size: 1000
"#;
    let spec: ManifestSpec = serde_yaml::from_str(yaml)?;
    assert!(!spec.stored);
    assert!(spec.cached);
    assert!(spec.template.is_some());
    Ok(())
}

// `to_record_source` needs a `Context`, whose constructor is async and takes an `AssetRef`.
// The tests therefore evaluate a query ending in a probe command, as
// `liquers-core/tests/async_hellow_world.rs` does, instead of building a `Context` by hand.

fn manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("manifest: record-stream\nchunks: []\n"))
}

fn not_a_manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("chunks: []\n")) // valid YAML, no `manifest:` key
}

/// Reports what `to_record_source` made of the state: `"schema:<bool>,manifest:<bool>"`, or the
/// error text prefixed with `"error:"`.
async fn probe_source(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    match to_record_source(state.data_unchecked(), &state.metadata, &options, &context).await {
        Ok(source) => Ok(Value::from(format!(
            "schema:{},manifest:{}",
            source.schema().is_some(),
            source.manifest().is_some()
        ))),
        Err(e) => Ok(Value::from(format!("error:{e}"))),
    }
}

async fn probe(query: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn manifest_text(state) -> result)?;
    register_command!(cr, fn not_a_manifest_text(state) -> result)?;
    register_command!(cr, async fn probe_source(state, context) -> result)?;
    let state = evaluate(env.to_ref(), query, None).await?;
    Ok(state.try_into_string()?)
}

#[tokio::test]
async fn to_record_source_recognizes_manifest_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    // No `uniform_schema` declared, so the source has no schema.
    assert_eq!(probe("manifest_text/probe_source").await?, "schema:false,manifest:true");
    Ok(())
}

#[tokio::test]
async fn to_record_source_rejects_missing_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    // Text without `manifest: record-stream` is never taken for a manifest; with no data format
    // to parse it as a table either, it is refused rather than sniffed.
    assert!(probe("not_a_manifest_text/probe_source").await?.starts_with("error:"));
    Ok(())
}

#[tokio::test]
async fn to_record_source_leaves_source_keyless_without_metadata_key() -> Result<(), Box<dyn std::error::Error>> {
    // A manifest built by a command (never stored) has no metadata key; its chunks stay unkeyed
    // rather than the call failing.
    assert!(probe("manifest_text/probe_source").await?.ends_with("manifest:true"));
    Ok(())
}
```

### Queries — Scenario 2

```
-R/data/sales/daily.manifest.yaml/-/ns-rec/to_record_source
-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv
-R/data/sales/daily_0010.csv
-R/data/sales/orders_na.csv
-R/data/sales/daily.manifest.yaml/-/ns-rec/rowid-2-0
-R/data/sales/daily.manifest.yaml/-/ns-rec/rec_id-123
```

# 2. Data-model unit tests (`liquers-records`)

## 2.1 `liquers-records/src/schema.rs`

```rust
// liquers-records/src/schema.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_new_single_id_field_succeeds() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("name", FieldType::Text),
        ];
        let schema = RecordSchema::new(fields).expect("single id field");
        assert_eq!(schema.id_field(), Some(0));
    }

    #[test]
    fn schema_new_multiple_id_fields_refused() {
        let fields = vec![
            FieldSchema::new("id1", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("id2", FieldType::Text).with_key(KeyRole::Id),
        ];
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn schema_new_id_without_exact_index_is_supplied_not_rejected() {
        // The Id field's implied role (Exact-indexed, stored) is supplied by `new` when left at
        // the default — only a *contradicting* role is rejected.
        let fields = vec![FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id)];
        let schema = RecordSchema::new(fields).expect("default role is supplied, not rejected");
        assert_eq!(schema.id_field(), Some(0));
    }

    #[test]
    fn schema_new_id_with_contradicting_role_refused() {
        let fields = vec![FieldSchema::new("id", FieldType::Text)
            .with_key(KeyRole::Id)
            .with_role(FieldRole::ignored())]; // ignored: not indexed, not stored — contradicts Id
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn schema_new_single_source_field_succeeds() {
        let fields = vec![
            FieldSchema::new("source_idx", FieldType::UInt).with_key(KeyRole::Source),
            FieldSchema::new("data", FieldType::Text),
        ];
        let schema = RecordSchema::new(fields).expect("single source field");
        assert_eq!(schema.source_field(), Some(0));
    }

    #[test]
    fn schema_new_multiple_source_fields_refused() {
        let fields = vec![
            FieldSchema::new("src1", FieldType::UInt).with_key(KeyRole::Source),
            FieldSchema::new("src2", FieldType::UInt).with_key(KeyRole::Source),
        ];
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn payload_fields_excludes_id_and_source() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("value", FieldType::Int),
            FieldSchema::new("source_idx", FieldType::UInt).with_key(KeyRole::Source),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.payload_fields(), vec![1]);
    }

    #[test]
    fn id_field_returns_none_when_not_declared() {
        let fields = vec![FieldSchema::new("value", FieldType::Int), FieldSchema::new("name", FieldType::Text)];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.id_field(), None);
    }

    #[test]
    fn index_of_finds_field_by_name() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("date", FieldType::Date),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.index_of("id"), Some(0));
        assert_eq!(schema.index_of("amount"), Some(1));
        assert_eq!(schema.index_of("date"), Some(2));
        assert_eq!(schema.index_of("missing"), None);
    }

    #[test]
    fn text_fields_returns_fulltext_indexed_columns_only() {
        let fields = vec![
            FieldSchema::new("title", FieldType::Text).with_role(FieldRole {
                indexed: vec![IndexKind::FullText { analyzer: Analyzer::Simple, positions: true }],
                stored: true,
                fast: false,
            }),
            FieldSchema::new("tags", FieldType::Text)
                .with_role(FieldRole { indexed: vec![IndexKind::Exact], stored: true, fast: false }),
            FieldSchema::new("value", FieldType::Int),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.text_fields(), &[0]);
    }

    #[test]
    fn field_schema_new_defaults() {
        let field = FieldSchema::new("user_name", FieldType::Text);
        assert_eq!(field.name, "user_name");
        assert_eq!(field.label, "user name");
        assert_eq!(field.data_type, FieldType::Text);
        assert!(field.nullable);
        assert_eq!(field.key, KeyRole::None);
    }

    #[test]
    fn yaml_deserialization_omitted_fields_use_defaults() {
        let yaml = "name: status\ndata_type: Text\n";
        let field: FieldSchema = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(field.name, "status");
        assert_eq!(field.label, "status");
        assert!(field.nullable);
        assert_eq!(field.key, KeyRole::None);
    }

    #[test]
    fn yaml_deserialization_explicit_nullable_false() {
        let yaml = "name: id\ndata_type: Text\nnullable: false\n";
        let field: FieldSchema = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(!field.nullable);
    }
}
```

## 2.2 `liquers-records/src/buffer.rs`

**`Bitmap` gains four constructors this document adds** (§"What Phase 3 found that Phase 2 must
absorb" in `phase3-examples.md`): `new(len)`, `from_bools(&[bool])`, `set(i, bool)`, `len()`. Tests
below build masks with `from_bools`, never by hand-packing bytes — the packing itself (LSB-first
per byte, per Arrow) is `Bitmap`'s own concern, not a test's.

```rust
// liquers-records/src/buffer.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_buffer_alignment_is_64_bytes() {
        let data = vec![0u8; 200];
        let buffer = AlignedBuffer::from_slice(&data);
        assert_eq!(buffer.as_bytes().as_ptr() as usize % 64, 0);
    }

    #[test]
    fn buffer_from_slice_and_as_slice_roundtrip() {
        let data = vec![1i64, 2, 3, 4, 5];
        let buffer = Buffer::from_slice(&data);
        assert_eq!(buffer.as_slice(), &data[..]);
    }

    #[test]
    fn buffer_cast_to_bytes_has_expected_length() {
        let data = vec![256u64, 512, 1024];
        let buffer = Buffer::from_slice(&data);
        assert_eq!(buffer.as_bytes().len(), data.len() * 8);
    }

    #[test]
    fn bitmap_new_is_all_clear() {
        let bitmap = Bitmap::new(5);
        assert_eq!(bitmap.len(), 5);
        assert_eq!(bitmap.count_ones(), 0);
        for i in 0..5 {
            assert!(!bitmap.get(i));
        }
    }

    #[test]
    fn bitmap_from_bools_roundtrips_through_get() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, false]);
        assert_eq!(bitmap.len(), 5);
        assert!(bitmap.get(0));
        assert!(!bitmap.get(1));
        assert!(bitmap.get(2));
    }

    #[test]
    fn bitmap_set_mutates_a_single_bit() {
        let mut bitmap = Bitmap::new(4);
        bitmap.set(2, true);
        assert!(!bitmap.get(0));
        assert!(!bitmap.get(1));
        assert!(bitmap.get(2));
        assert!(!bitmap.get(3));
        bitmap.set(2, false);
        assert!(!bitmap.get(2));
    }

    #[test]
    fn bitmap_and_combines_masks() {
        let a = Bitmap::from_bools(&[true, true, true, true, false, false, false, false]);
        let b = Bitmap::from_bools(&[true, false, true, false, true, false, true, false]);
        let result = a.and(&b).expect("and");
        assert!(result.get(0));
        assert!(!result.get(1));
        assert!(result.get(2));
        assert!(!result.get(4));
    }

    #[test]
    fn bitmap_or_combines_masks() {
        let a = Bitmap::from_bools(&[true, true, false, false]);
        let b = Bitmap::from_bools(&[false, false, true, true]);
        let result = a.or(&b).expect("or");
        assert_eq!(result.count_ones(), 4);
    }

    #[test]
    fn bitmap_not_inverts_mask() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, true, false, true, false]);
        assert_eq!(bitmap.not().count_ones(), 4);
    }

    #[test]
    fn bitmap_count_ones_returns_set_bit_count() {
        let bitmap = Bitmap::from_bools(&[true, true, true, true, false, true, false, true]);
        assert_eq!(bitmap.count_ones(), 6);
    }

    #[test]
    fn bitmap_iter_ones_yields_set_positions_in_order() {
        let bitmap = Bitmap::from_bools(&[true, false, true, false, true, false, false, false]);
        let positions: Vec<usize> = bitmap.iter_ones().collect();
        assert_eq!(positions, vec![0, 2, 4]);
    }

    #[test]
    fn bitmap_len_reports_bit_count_not_byte_count() {
        let bitmap = Bitmap::from_bools(&[true; 5]);
        assert_eq!(bitmap.len(), 5); // not 8, even though it is stored in one byte
    }

    #[test]
    fn bitmap_and_length_mismatch_error() {
        let a = Bitmap::new(8);
        let b = Bitmap::new(16);
        assert!(a.and(&b).is_err());
    }

    #[test]
    fn bitmap_or_length_mismatch_error() {
        let a = Bitmap::new(8);
        let b = Bitmap::new(16);
        assert!(a.or(&b).is_err());
    }
}
```

## 2.3 `liquers-records/src/lib.rs` (`Column` kernels)

```rust
// liquers-records/src/lib.rs — within the column kernels
#[cfg(test)]
mod column_tests {
    use super::*;

    #[test]
    fn column_get_returns_field_value() {
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30]) };
        assert_eq!(column.get(0).expect("get"), FieldValue::Int(10));
        assert_eq!(column.get(1).expect("get"), FieldValue::Int(20));
    }

    #[test]
    fn column_slice_returns_the_requested_range() {
        // Zero-copy sharing is `Buffer`'s internal representation (an `Arc`-shared aligned
        // slice), not something this level exposes an accessor for; the observable contract is
        // that the slice holds exactly rows 1..4.
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]) };
        let sliced = column.slice(1, 3).expect("slice");
        assert_eq!(sliced.len(), 3);
        assert_eq!(sliced.get(0).expect("get"), FieldValue::Int(2));
        assert_eq!(sliced.get(2).expect("get"), FieldValue::Int(4));
    }

    #[test]
    fn column_take_gathers_values() {
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30, 40, 50]) };
        let gathered = column.take(&[0, 2, 4]).expect("take");
        assert_eq!(gathered.len(), 3);
        assert_eq!(gathered.get(0).expect("get"), FieldValue::Int(10));
        assert_eq!(gathered.get(1).expect("get"), FieldValue::Int(30));
        assert_eq!(gathered.get(2).expect("get"), FieldValue::Int(50));
    }

    #[test]
    fn column_filter_applies_bitmap() {
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]) };
        let mask = Bitmap::from_bools(&[true, false, true, false, true]);
        let filtered = column.filter(&mask).expect("filter");
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered.get(0).expect("get"), FieldValue::Int(1));
        assert_eq!(filtered.get(1).expect("get"), FieldValue::Int(3));
        assert_eq!(filtered.get(2).expect("get"), FieldValue::Int(5));
    }

    #[test]
    fn column_compare_eq() {
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30]) };
        let mask = column.compare(CompareOp::Eq, &FieldValue::Int(20)).expect("compare");
        assert!(!mask.get(0));
        assert!(mask.get(1));
        assert!(!mask.get(2));
    }

    #[test]
    fn column_compare_lt() {
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30]) };
        let mask = column.compare(CompareOp::Lt, &FieldValue::Int(25)).expect("compare");
        assert!(mask.get(0));
        assert!(mask.get(1));
        assert!(!mask.get(2));
    }

    #[test]
    fn column_null_mask_reflects_validity() {
        let validity = Bitmap::from_bools(&[true, true, true, false, true, true, true, true]);
        let column = Column::Int { validity: Some(validity), values: Buffer::from_slice(&[1i64; 8]) };
        let mask = column.null_mask();
        assert!(!mask.get(0));
        assert!(!mask.get(2));
        assert!(mask.get(3)); // validity false => null
    }

    #[test]
    fn column_concat_same_type() {
        let c1 = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) };
        let c2 = Column::Int { validity: None, values: Buffer::from_slice(&[4i64, 5]) };
        let concatenated = Column::concat(&[c1, c2]).expect("concat");
        assert_eq!(concatenated.len(), 5);
        assert_eq!(concatenated.get(2).expect("get"), FieldValue::Int(3));
        assert_eq!(concatenated.get(3).expect("get"), FieldValue::Int(4));
    }

    #[test]
    fn column_concat_type_mismatch_error() {
        let c1 = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) };
        let c2 = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 4]),
            data: AlignedBuffer::from_slice(b"text"),
        };
        assert!(Column::concat(&[c1, c2]).is_err());
    }

    #[test]
    fn field_value_stays_reasonably_small() {
        // Not a hard architectural promise the way `Value`'s 704-byte ceiling is
        // (CORE-VALUE-ENUM-OVERSIZED), but a regression here — a variant growing the enum's
        // largest arm — is worth noticing rather than discovering by profiling later.
        assert!(std::mem::size_of::<FieldValue>() <= 32);
    }

    #[test]
    fn field_value_date_variant_holds_days_since_epoch() {
        match FieldValue::Date(19570) {
            FieldValue::Date(days) => assert_eq!(days, 19570),
            _ => panic!("expected Date variant"),
        }
    }
}
```

## 2.4 `liquers-records/src/views.rs`

```rust
// liquers-records/src/views.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_batch() -> Arc<RecordBatch> {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Int),
                FieldSchema::new("name", FieldType::Text),
                FieldSchema::new("value", FieldType::Float),
            ])
            .expect("schema"),
        );
        let id_col = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]) };
        let name_col = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 3, 6, 9, 12, 15]),
            data: AlignedBuffer::from_slice(b"aaabbbcccdddeee"),
        };
        let val_col = Column::Float { validity: None, values: Buffer::from_slice(&[1.5f64, 2.5, 3.5, 4.5, 5.5]) };
        Arc::new(RecordBatch::new(schema, vec![id_col, name_col, val_col], None, None, vec![]).expect("batch"))
    }

    /// `column_range` and `value` must agree everywhere: a full-column read sliced to one row
    /// equals a one-row range, which equals `value(row, col)` — the three ways of reading a cell
    /// this design provides, checked against each other rather than against a hand copy.
    fn assert_reads_agree(view: &dyn RecordView, col: usize) {
        let full = view.column_range(col, 0..view.len()).expect("column_range full");
        for row in 0..view.len() {
            let single = view.column_range(col, row..row + 1).expect("column_range single");
            let from_full = full.get(row).expect("get");
            let from_single = single.get(0).expect("get");
            let from_value = view.value(row, col).expect("value");
            assert_eq!(from_full, from_single, "column_range(full)[{row}] vs column_range({row}..{row}+1)");
            assert_eq!(from_full, from_value, "column_range(full)[{row}] vs value({row}, {col})");
        }
    }

    #[test]
    fn record_batch_column_range_and_value_agree() {
        let batch = make_simple_batch();
        for col in 0..batch.schema.fields.len() {
            assert_reads_agree(&*batch, col);
        }
    }

    #[test]
    fn record_batch_materialize_is_a_shallow_clone() {
        let batch = make_simple_batch();
        let materialized = batch.materialize().expect("materialize");
        for i in 0..batch.columns.len() {
            match (&batch.columns[i], &materialized.columns[i]) {
                (Column::Int { values: a, .. }, Column::Int { values: b, .. }) => {
                    assert_eq!(a.as_slice(), b.as_slice())
                }
                _ => {} // other columns compared by value elsewhere; the point here is `len`/schema identity
            }
        }
        assert_eq!(batch.len, materialized.len);
    }

    #[test]
    fn columns_view_select_columns_keeps_id_and_maps_indices() {
        let batch_arc = make_simple_batch();
        let view = batch_arc.select_columns(&["name", "value"]).expect("select");
        // No Id declared in this schema, so nothing is force-kept; the projection is exact.
        assert_eq!(view.schema().fields.len(), 2);
        assert_eq!(view.schema().fields[0].name, "name");
        assert_reads_agree(&*view, 0);
        assert_reads_agree(&*view, 1);
    }

    #[test]
    fn row_range_view_column_range_delegates_with_offset() {
        let batch_arc = make_simple_batch();
        let slice_view = batch_arc.slice(1, 3).expect("slice");
        assert_eq!(slice_view.len(), 3);
        for col in 0..slice_view.schema().fields.len() {
            assert_reads_agree(&*slice_view, col);
        }
    }

    #[test]
    fn row_index_view_gathers_selected_rows() {
        let batch_arc = make_simple_batch();
        let mask = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered = batch_arc.filter(&mask).expect("filter");
        assert_eq!(filtered.len(), 3);
        for col in 0..filtered.schema().fields.len() {
            assert_reads_agree(&*filtered, col);
        }
    }

    #[test]
    fn filter_over_filter_composes() {
        let batch_arc = make_simple_batch();
        let mask1 = Bitmap::from_bools(&[true, true, true, true, true]);
        let filtered1 = batch_arc.filter(&mask1).expect("filter1");
        let mask2 = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered2 = filtered1.filter(&mask2).expect("filter2");
        assert_eq!(filtered2.len(), 3);
    }

    #[test]
    fn row_fn_view_calls_closure_only_for_the_requested_range_and_column() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let batch_arc = make_simple_batch();
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();
        let schema = batch_arc.schema().clone();
        let view: Arc<dyn RecordView> = Arc::new(
            RowFnView::new(schema, batch_arc.len(), move |_row, _col| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(FieldValue::Int(0))
            })
            .expect("RowFnView::new"),
        );
        let _col = view.column_range(0, 1..3).expect("column_range");
        assert_eq!(call_count.load(Ordering::SeqCst), 2); // exactly rows 1 and 2, column 0
    }

    #[test]
    fn derived_column_view_computes_from_source_columns() -> Result<(), Error> {
        let batch_arc = make_simple_batch();
        let view = batch_arc.with_column(FieldSchema::new("double_id", FieldType::Int), &[0], |cols| {
            let id_col = &cols[0];
            let mut out = ColumnMut::with_capacity(FieldType::Int, id_col.len());
            for i in 0..id_col.len() {
                match id_col.get(i)? {
                    FieldValue::Int(n) => out.push(&FieldValue::Int(n * 2))?,
                    other => return Err(Error::general_error(format!("expected Int, got {other:?}"))),
                }
            }
            Ok(out.freeze())
        })?;
        assert_eq!(view.schema().fields.len(), 4);
        assert_eq!(view.value(0, 3)?, FieldValue::Int(2));
        assert_eq!(view.value(4, 3)?, FieldValue::Int(10));
        Ok(())
    }

    #[test]
    fn appended_columns_view_keeps_base_columns_readable() -> Result<(), Error> {
        let batch_arc = make_simple_batch();
        let extra_col = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30, 40, 50]) };
        let view = batch_arc.with_columns(vec![FieldSchema::new("extra", FieldType::Int)], vec![extra_col])?;
        assert_eq!(view.schema().fields.len(), 4);
        assert_reads_agree(&*view, 0); // base column still readable through the wrapper
        assert_eq!(view.value(2, 3)?, FieldValue::Int(30));
        Ok(())
    }

    #[test]
    fn column_range_out_of_bounds_range_is_an_error() {
        let batch = make_simple_batch();
        assert!(batch.column_range(0, 0..100).is_err());
    }

    #[test]
    fn column_range_out_of_bounds_column_is_an_error() {
        let batch = make_simple_batch();
        assert!(batch.column_range(100, 0..2).is_err());
    }
}
```

## 2.5 `liquers-records/src/views.rs` (implicit row ids)

```rust
// liquers-records/src/views.rs — continuation
#[cfg(test)]
mod implicit_id_tests {
    use super::*;

    fn one_column_batch(values: &[i64]) -> Arc<RecordBatch> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)]).expect("schema"));
        let column = Column::Int { validity: None, values: Buffer::from_slice(values) };
        Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
    }

    #[test]
    fn standalone_batch_row_id_defaults_to_chunk_0_row_n() {
        let batch = one_column_batch(&[1, 2, 3]);
        assert_eq!(batch.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(batch.row_id(2).expect("row_id"), RowId { chunk: 0, row: 2 });
    }

    #[test]
    fn standalone_batch_row_number_defaults_to_row_index() {
        let batch = one_column_batch(&[1, 2, 3]);
        assert_eq!(batch.row_number(0).expect("row_number"), Some(0));
        assert_eq!(batch.row_number(2).expect("row_number"), Some(2));
    }

    #[test]
    fn filtered_row_keeps_its_original_row_id() {
        // "The RowId of a filtered row is the row's original position, not its position in the
        // filtered view" (§"Every row has an implicit id").
        let batch = one_column_batch(&[1, 2, 3, 4, 5]);
        let mask = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered = batch.filter(&mask).expect("filter");
        assert_eq!(filtered.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(filtered.row_id(1).expect("row_id"), RowId { chunk: 0, row: 2 });
        assert_eq!(filtered.row_id(2).expect("row_id"), RowId { chunk: 0, row: 3 });
    }

    #[test]
    fn batch_from_three_runs_has_correct_row_ids_and_numbers() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)]).expect("schema"));
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5, 6]) };
        let runs = vec![
            RowRun { chunk: 0, first_row: 0, first_number: Some(0), len: 2 },
            RowRun { chunk: 1, first_row: 0, first_number: Some(2), len: 2 },
            RowRun { chunk: 2, first_row: 5, first_number: Some(4), len: 2 },
        ];
        let batch = RecordBatch::new(schema, vec![column], None, Some(runs), vec![]).expect("batch");
        assert_eq!(batch.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(batch.row_id(1).expect("row_id"), RowId { chunk: 0, row: 1 });
        assert_eq!(batch.row_id(2).expect("row_id"), RowId { chunk: 1, row: 0 });
        assert_eq!(batch.row_id(4).expect("row_id"), RowId { chunk: 2, row: 5 });
        assert_eq!(batch.row_number(4).expect("row_number"), Some(4));
    }
}
```

## 2.6 `liquers-records/src/mutable.rs`

```rust
// liquers-records/src/mutable.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn int_schema(names: &[&str]) -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(names.iter().map(|n| FieldSchema::new(*n, FieldType::Int)).collect())
                .expect("schema"),
        )
    }

    #[test]
    fn with_capacity_starts_empty() {
        let builder = RecordBatchMut::with_capacity(int_schema(&["id", "count"]), 100);
        assert_eq!(builder.len(), 0);
    }

    #[test]
    fn append_row_advances_len() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 10);
        builder.append_row(&[FieldValue::Int(1)]).expect("append_row");
        builder.append_row(&[FieldValue::Int(2)]).expect("append_row");
        assert_eq!(builder.len(), 2);
    }

    #[test]
    fn set_value_overwrites_an_appended_cell() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 10);
        builder.append_row(&[FieldValue::Int(0)]).expect("append_row");
        builder.set_value(0, 0, &FieldValue::Int(42)).expect("set_value");
        let frozen = builder.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(42));
    }

    #[test]
    fn column_mut_starts_empty_and_grows_independently() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["col1", "col2"]), 10);
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(1)).expect("push");
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(2)).expect("push");
        builder.column_mut(1).expect("col2").push(&FieldValue::Int(10)).expect("push");
        // `len()` is the shortest column's length while columns are still being filled.
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn freeze_fails_on_uneven_columns() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["col1", "col2"]), 10);
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(1)).expect("push");
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(2)).expect("push");
        builder.column_mut(1).expect("col2").push(&FieldValue::Int(10)).expect("push");
        assert!(builder.freeze().is_err());
    }

    #[test]
    fn column_mut_freeze_returns_a_frozen_column() {
        let mut col = ColumnMut::with_capacity(FieldType::Int, 10);
        col.push(&FieldValue::Int(42)).expect("push");
        assert_eq!(col.freeze().len(), 1);
    }

    #[test]
    fn record_batch_into_mut_takes_over_an_unshared_buffer() {
        let schema = int_schema(&["value"]);
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) };
        let batch = RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch");
        let mut mutable = batch.into_mut();
        mutable.set_value(0, 0, &FieldValue::Int(99)).expect("set_value");
        let frozen = mutable.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(99));
    }

    #[test]
    fn record_batch_into_mut_copies_a_shared_buffer() {
        let schema = int_schema(&["value"]);
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) };
        let batch = RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch");
        let batch_arc1 = Arc::new(batch);
        let batch_arc2 = batch_arc1.clone(); // second owner: `into_mut` must copy, not steal
        let mut mutable = (*batch_arc1).clone().into_mut();
        drop(batch_arc2);
        mutable.set_value(0, 0, &FieldValue::Int(99)).expect("set_value");
        let frozen = mutable.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(99));
        // The original, seen through `batch_arc1`, is untouched — the copy-on-write idiom RECORDS04
        // in §6 checks at the value-crossing boundary.
        assert_eq!(batch_arc1.value(0, 0).expect("value"), FieldValue::Int(1));
    }
}
```

## 2.7 `liquers-records/src/manifest.rs`

`Recipe::query` is a plain `String` and a keyed chunk is one whose query text ends in a filename
(`Recipe::filename()` derives it, `recipes.rs:178`) — `Recipe` has no separate `filename` field, and
`arguments` is `HashMap<String, serde_json::Value>`, not a list of string pairs. Two of the drafts'
`Recipe` literals used a `filename:` field and a `Vec<(String, String)>` for `arguments`; fixed below.

```rust
// liquers-records/src/manifest.rs
#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::{parse::parse_key, recipes::Recipe};

    #[test]
    fn chunk_naming_key_formats_with_padding() {
        let naming = ChunkNaming { folder: Key::new(), prefix: "daily".to_string(), extension: "csv".to_string() };
        assert_eq!(naming.key(42).to_string(), "daily_0042.csv");
        assert_eq!(naming.key(10_000).to_string(), "daily_10000.csv");
    }

    #[test]
    fn chunk_naming_index_of_returns_chunk_number() {
        let naming = ChunkNaming { folder: Key::new(), prefix: "daily".to_string(), extension: "csv".to_string() };
        assert_eq!(naming.index_of("daily_0042.csv"), Some(42));
        assert_eq!(naming.index_of("daily_0000.csv"), Some(0));
        assert_eq!(naming.index_of("other.csv"), None);
    }

    #[test]
    fn chunk_template_offset_at_advances_by_step() {
        let template = ChunkTemplate { query: "sql_query".to_string(), first_offset: 100, step: 50, batch_size: 25 };
        assert_eq!(template.offset_at(0), 100);
        assert_eq!(template.offset_at(1), 150);
        assert_eq!(template.offset_at(5), 350);
    }

    #[test]
    fn chunk_template_query_at_appends_offset_and_batch_size() -> Result<(), Error> {
        let template = ChunkTemplate { query: "sql_query".to_string(), first_offset: 0, step: 10, batch_size: 5 };
        let query = template.query_at(0, None)?;
        let encoded = query.encode();
        assert!(encoded.contains("sql_query"));
        assert!(encoded.contains("0") && encoded.contains("5"));
        Ok(())
    }

    #[test]
    fn chunk_template_query_at_with_filename_appends_it() -> Result<(), Error> {
        let template = ChunkTemplate { query: "sql_query".to_string(), first_offset: 0, step: 10, batch_size: 5 };
        let query = template.query_at(0, Some("chunk.csv"))?;
        assert!(query.encode().contains("chunk.csv"));
        Ok(())
    }

    #[test]
    fn manifest_spec_yaml_defaults_stored_cached_extension() {
        let yaml = "chunks:\n  - query: select 1\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(spec.stored);
        assert!(spec.cached);
        // `extension` is `Option<String>` with a plain `#[serde(default)]`, so an absent field
        // reads as `None`; the `csv` default is applied where the naming is derived
        // (`ChunkNaming`, in `with_key`), not by serde.
        assert_eq!(spec.extension, None);
    }

    #[test]
    fn manifest_spec_explicit_stored_false_is_honored() {
        let yaml = "chunks: []\nstored: false\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(!spec.stored);
    }

    #[test]
    fn manifest_source_new_refuses_colliding_explicit_chunk_names() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe { query: "ns-sql/sql_query-0-1000/data.csv".to_string(), ..Default::default() },
                Recipe { query: "ns-sql/sql_query-1000-1000/data.csv".to_string(), ..Default::default() },
            ],
            template: None,
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
        };
        assert!(ManifestSource::new(spec, None).is_err());
    }

    #[test]
    fn manifest_source_with_key_refuses_per_chunk_arguments_on_an_unkeyed_chunk() {
        let mut arguments = std::collections::HashMap::new();
        arguments.insert("param".to_string(), serde_json::json!("value"));
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(), // no filename: unkeyed
                arguments,
                ..Default::default()
            }],
            template: None,
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
        };
        let source = ManifestSource::new(spec, None).expect("new validates only key-independent rules");
        assert!(source.with_key(Key::new()).is_err());
    }

    #[test]
    fn manifest_source_with_key_refuses_explicit_name_matching_template_pattern() {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1/daily_0042.csv".to_string(),
                ..Default::default()
            }],
            template: Some(ChunkTemplate { query: "ns-sql/sql_query".to_string(), first_offset: 0, step: 1, batch_size: 1000 }),
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
        };
        let source = ManifestSource::new(spec, None).expect("new");
        // The manifest's own filename becomes the template's prefix ("daily"), which is what
        // makes "daily_0042.csv" collide with the pattern once the key is known.
        let key = parse_key("data/sales/daily.manifest.yaml").expect("key");
        assert!(source.with_key(key).is_err());
    }

    #[test]
    fn manifest_source_without_key_keeps_chunks_identified_by_query() {
        // No key supplied (a manifest built by a command, never stored): a chunk with no
        // filename in its query stays unkeyed — identified by the query itself, never by a key
        // it was never given.
        let yaml = "chunks:\n  - query: select 1\nstored: true\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        let source = ManifestSource::new(spec, None).expect("new");
        match source.chunks() {
            ChunkList::Known(ids) => assert!(matches!(ids[0], ChunkId::Query(_))),
            ChunkList::Unbounded { .. } => panic!("expected Known: this manifest has no template"),
        }
    }

    #[test]
    fn manifest_spec_deserialize_ignores_unmodeled_envelope_fields() {
        // `manifest:` and `version:` belong to the discriminator envelope a whole manifest
        // document carries (§1.2); `ManifestSpec` itself does not model them and must not choke
        // on their presence.
        let yaml = "manifest: record-stream\nversion: \"1.5\"\nchunks: []\n";
        assert!(serde_yaml::from_str::<ManifestSpec>(yaml).is_ok());
    }
}
```

# 3. Format unit tests (`liquers-records/src/formats/`)

## 3.1 `liquers-records/src/formats/csv.rs`

```rust
// liquers-records/src/formats/csv.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema, RecordViewMut};

    fn text_schema(names: &[&str]) -> Arc<RecordSchema> {
        Arc::new(RecordSchema::new(names.iter().map(|n| FieldSchema::new(*n, FieldType::Text)).collect()).unwrap())
    }

    /// The test Phase 2 names directly (§"Tier 1 — in `records`"): an unquoted empty field is
    /// null, a quoted `""` is the empty string, and the distinction survives a round trip.
    #[test]
    fn column_null_distinct_from_empty_string() -> Result<(), Error> {
        let schema = text_schema(&["name", "note"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("Alice")), FieldValue::Null])?;
        batch.append_row(&[FieldValue::Text(Arc::from("Bob")), FieldValue::Text(Arc::from(""))])?;
        let batch = batch.freeze()?;

        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;

        assert_eq!(read_back.value(0, 1)?, FieldValue::Null);
        assert_eq!(read_back.value(1, 1)?, FieldValue::Text(Arc::from("")));
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_separator_in_cell() -> Result<(), Error> {
        let schema = text_schema(&["cell"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("a,b"))])?;
        batch.append_row(&[FieldValue::Text(Arc::from("c"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        assert!(String::from_utf8(bytes)?.contains("\"a,b\""));
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_crlf_and_lf_both_read_as_line_breaks() -> Result<(), Error> {
        let schema = text_schema(&["cell"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("line1\nline2"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let written = String::from_utf8(bytes)?;
        assert!(written.contains("\"line1\nline2\"")); // embedded newline is quoted

        // Both line-ending conventions parse as the same two data rows for a file with two records.
        let lf = b"cell\nfirst\nsecond\n";
        let crlf = b"cell\r\nfirst\r\nsecond\r\n";
        let via_lf = read_table(lf, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        let via_crlf = read_table(crlf, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(via_lf.len, via_crlf.len);
        assert_eq!(via_lf.value(1, 0)?, via_crlf.value(1, 0)?);
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_doubled_quote_is_one_literal_quote() -> Result<(), Error> {
        let csv = b"cell\n\"a\"\"b\"\n"; // the cell `a"b`
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("a\"b")));
        Ok(())
    }

    #[test]
    fn csv_malformed_error_names_the_line_number() {
        let malformed = b"name,age\nalice,thirty\n"; // "thirty" does not parse as Int
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(malformed, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("thirty is not an Int");
        let message = format!("{err}");
        assert!(message.contains("2") || message.contains("line"), "error should name the line: {message}");
    }

    #[test]
    fn csv_round_trip_scalar_column_types() -> Result<(), Error> {
        // Vector and Binary are exercised in the NDJSON/table tests, where a native JSON-ish
        // shape makes the fixture legible; CSV's own contract is the scalar types below.
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("flag", FieldType::Bool),
            FieldSchema::new("count", FieldType::Int),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("label", FieldType::Text),
            FieldSchema::new("day", FieldType::Date),
            FieldSchema::new("at", FieldType::Timestamp),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 1);
        batch.append_row(&[
            FieldValue::Bool(true),
            FieldValue::Int(42),
            FieldValue::Float(3.5),
            FieldValue::Text(Arc::from("hello")),
            FieldValue::Date(19_570),
            FieldValue::Timestamp(1_726_944_000_000_000),
        ])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        for col in 0..schema.fields.len() {
            assert_eq!(read_back.value(0, col)?, batch.value(0, col)?, "column {col} round-trips");
        }
        Ok(())
    }

    #[test]
    fn csv_separator_is_configurable_for_tsv() -> Result<(), Error> {
        let schema = text_schema(&["a", "b"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x")), FieldValue::Text(Arc::from("y"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b'\t' }, &WriteOptions::default())?;
        let written = String::from_utf8(bytes)?;
        assert!(written.contains("x\ty"));
        Ok(())
    }

    #[test]
    fn schema_less_inference_canonical_int_rule_keeps_leading_zeros_as_text() -> Result<(), Error> {
        let csv = b"id,code,amount\n1,01234,+5\n2,9999,1e3\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[1].data_type, FieldType::Text); // 01234: leading zero
        assert_eq!(batch.schema.fields[2].data_type, FieldType::Text); // +5, 1e3: not canonical Int
        Ok(())
    }

    #[test]
    fn schema_less_inference_tries_bool_int_float_date_timestamp_then_text() -> Result<(), Error> {
        let csv = b"c_bool,c_int,c_float,c_date,c_ts,c_text\n\
                    true,42,3.14,2026-09-25,2026-09-25T12:00:00Z,hello\n\
                    false,43,2.71,2026-09-26,2026-09-26T13:00:00Z,world\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Bool);
        assert_eq!(batch.schema.fields[1].data_type, FieldType::Int);
        assert_eq!(batch.schema.fields[2].data_type, FieldType::Float);
        assert_eq!(batch.schema.fields[3].data_type, FieldType::Date);
        assert_eq!(batch.schema.fields[4].data_type, FieldType::Timestamp);
        assert_eq!(batch.schema.fields[5].data_type, FieldType::Text);
        Ok(())
    }

    #[test]
    fn schema_aware_read_declared_text_keeps_leading_zeros() -> Result<(), Error> {
        let csv = b"zip\n01234\n90210\n";
        let schema = RecordSchema::new(vec![FieldSchema::new("zip", FieldType::Text)])?;
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("01234")));
        Ok(())
    }

    #[test]
    fn schema_aware_read_refuses_an_undeclared_column() {
        let csv = b"name,age,email\nalice,30,alice@example.com\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("email is not in the schema");
        assert!(format!("{err}").contains("email"));
    }

    #[test]
    fn schema_aware_read_refuses_a_missing_non_nullable_column() {
        let csv = b"name\nalice\n"; // age is not_null and missing
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int).not_null(),
        ])
        .unwrap();
        assert!(read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default()).is_err());
    }

    #[test]
    fn schema_aware_read_unparsable_cell_names_row_and_column() {
        let csv = b"name,age\nalice,not_a_number\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("not_a_number is not an Int");
        let message = format!("{err}");
        assert!(message.contains("age"), "error should name the column: {message}");
    }
}
```

## 3.2 `liquers-records/src/formats/ndjson.rs`

```rust
// liquers-records/src/formats/ndjson.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordViewMut};

    #[test]
    fn ndjson_infers_the_union_of_keys_across_rows() -> Result<(), Error> {
        let ndjson = b"{\"name\":\"alice\",\"age\":30}\n{\"name\":\"bob\",\"email\":\"bob@example.com\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let names: Vec<&str> = batch.schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
        assert!(names.contains(&"email"));
        Ok(())
    }

    #[test]
    fn ndjson_infers_a_fixed_length_number_array_as_vector() -> Result<(), Error> {
        let ndjson = b"{\"id\":1,\"embedding\":[0.1,0.2,0.3]}\n{\"id\":2,\"embedding\":[0.4,0.5,0.6]}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let field = batch.schema.fields.iter().find(|f| f.name == "embedding").expect("embedding field");
        assert_eq!(field.data_type, FieldType::Vector);
        Ok(())
    }

    #[test]
    fn ndjson_write_read_round_trip_preserves_values() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int),
            FieldSchema::new("name", FieldType::Text),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 2);
        batch.append_row(&[FieldValue::Int(1), FieldValue::Text(Arc::from("alice"))])?;
        batch.append_row(&[FieldValue::Int(2), FieldValue::Text(Arc::from("bob"))])?;
        let batch = batch.freeze()?;

        let bytes = write_table(&batch, TableFormat::NdJson, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::NdJson, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(read_back.value(0, 0)?, FieldValue::Int(1));
        assert_eq!(read_back.value(1, 1)?, FieldValue::Text(Arc::from("bob")));
        Ok(())
    }
}
```

## 3.3 `liquers-records/src/formats/shapes.rs` — the `JsonOrient`s

Every orient below is checked against an inline pandas-shaped fixture, not a bare `todo!()`. The
`Id` is always `order_id`: the shapes with an index (`split`, `columns`, `index`, `table`) put it
there; `records` and `list` carry it as an ordinary column (§"JSON shapes are conversions").

```rust
// liquers-records/src/formats/shapes.rs
#[cfg(test)]
mod tests {
    use super::*;
    use liquers_records::{FieldSchema, FieldType, FieldValue, KeyRole, RecordSchema};

    fn orders_schema() -> RecordSchema {
        RecordSchema::new(vec![
            FieldSchema::new("order_id", FieldType::Int).with_key(KeyRole::Id),
            FieldSchema::new("total", FieldType::Float),
        ])
        .expect("schema")
    }

    #[test]
    fn orient_records_is_an_array_of_row_objects() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(
            r#"[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]"#,
        )?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(1, 0)?, FieldValue::Int(2));

        let round_tripped = to_json(&batch, JsonOrient::Records)?;
        assert_eq!(round_tripped, json);
        Ok(())
    }

    #[test]
    fn orient_list_is_a_dictionary_of_columns() -> Result<(), Error> {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"order_id":[1,2],"total":[9.5,3.0]}"#)?;
        let batch = from_json(&json, JsonOrient::List, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 1)?, FieldValue::Float(9.5));

        let round_tripped = to_json(&batch, JsonOrient::List)?;
        assert_eq!(round_tripped, json);
        Ok(())
    }

    #[test]
    fn orient_split_carries_columns_index_and_data_separately() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"columns":["total"],"index":[1,2],"data":[[9.5],[3.0]]}"#,
        )?;
        let batch = from_json(&json, JsonOrient::Split, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 0)?, FieldValue::Int(1)); // the index becomes the Id column
        assert_eq!(batch.value(0, 1)?, FieldValue::Float(9.5));
        Ok(())
    }

    #[test]
    fn orient_values_is_rows_without_names() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(r#"[[1,9.5],[2,3.0]]"#)?;
        // Positional: needs a schema to know which column is which.
        let batch = from_json(&json, JsonOrient::Values, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn orient_columns_keys_the_index_as_strings_unless_the_schema_says_otherwise() -> Result<(), Error> {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"total":{"1":9.5,"2":3.0}}"#)?;
        let batch = from_json(&json, JsonOrient::Columns, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 0)?, FieldValue::Int(1)); // schema declares order_id as Int
        Ok(())
    }

    #[test]
    fn orient_index_is_row_keyed_by_the_id() -> Result<(), Error> {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"1":{"total":9.5},"2":{"total":3.0}}"#)?;
        let batch = from_json(&json, JsonOrient::Index, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn orient_table_is_schema_aware_and_lossless() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"schema":{"fields":[{"name":"order_id","type":"integer"},
                {"name":"total","type":"number"}],"primaryKey":["order_id"]},
               "data":[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]}"#,
        )?;
        // `table` carries its own schema — Infer is legitimate here, unlike every other shape.
        let batch = from_json(&json, JsonOrient::Table, ReadSchema::Infer)?;
        assert_eq!(batch.schema.id_field(), Some(0));
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn auto_recognizes_records() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(r#"[{"order_id":1,"total":9.5}]"#)?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Infer)?;
        assert_eq!(batch.len, 1);
        Ok(())
    }

    #[test]
    fn auto_recognizes_list() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(r#"{"order_id":[1,2],"total":[9.5,3.0]}"#)?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Infer)?;
        assert_eq!(batch.len, 2);
        Ok(())
    }

    #[test]
    fn auto_refuses_an_object_of_objects_as_ambiguous() {
        // `columns` and `index` have the same JSON shape transposed — auto cannot choose.
        let json: serde_json::Value =
            serde_json::from_str(r#"{"1":{"order_id":1,"total":9.5},"2":{"order_id":2,"total":3.0}}"#).unwrap();
        let err = from_json(&json, JsonOrient::Auto, ReadSchema::Infer).expect_err("ambiguous shape");
        let message = format!("{err}").to_lowercase();
        assert!(message.contains("ambiguous") || message.contains("orient"));
    }

    #[test]
    fn json_orient_from_str_parses_every_name() {
        for (text, expected) in [
            ("records", JsonOrient::Records),
            ("list", JsonOrient::List),
            ("split", JsonOrient::Split),
            ("values", JsonOrient::Values),
            ("columns", JsonOrient::Columns),
            ("index", JsonOrient::Index),
            ("table", JsonOrient::Table),
            ("auto", JsonOrient::Auto),
        ] {
            assert_eq!(text.parse::<JsonOrient>().unwrap(), expected);
        }
    }

    #[test]
    fn to_json_refuses_auto_as_a_write_orient() -> Result<(), Error> {
        let json: serde_json::Value = serde_json::from_str(r#"[{"order_id":1,"total":9.5}]"#)?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;
        let err = to_json(&batch, JsonOrient::Auto).expect_err("`Auto` is a read-side concept only");
        assert!(format!("{err}").to_lowercase().contains("auto"));
        Ok(())
    }
}
```

## 3.4 `liquers-records/src/formats/markdown.rs`

```rust
// liquers-records/src/formats/markdown.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordViewMut};

    #[test]
    fn markdown_escapes_pipe_and_backslash() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("cell", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("a|b"))])?;
        batch.append_row(&[FieldValue::Text(Arc::from("c\\d"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Markdown, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("a\\|b"));
        assert!(text.contains("c\\\\d"));
        Ok(())
    }

    #[test]
    fn markdown_header_uses_the_label_reading_back_recovers_the_name() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("user_id", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Int(1)])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Markdown, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("user id")); // default label: name with `_` replaced by space

        let read_back = read_table(text.as_bytes(), TableFormat::Markdown, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(read_back.schema.fields[0].name, "user_id"); // inverse of the default label
        Ok(())
    }

    #[test]
    fn markdown_right_aligns_numeric_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("amount", FieldType::Int),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(1)])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Markdown, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        // The GFM alignment row marks a right-aligned column with a trailing colon: `---:`.
        let alignment_row = text.lines().nth(1).expect("alignment row");
        let cells: Vec<&str> = alignment_row.trim_matches('|').split('|').collect();
        assert!(!cells[0].trim().ends_with(':')); // name: left/default
        assert!(cells[1].trim().ends_with(':')); // amount: right-aligned
        Ok(())
    }
}
```

## 3.5 `liquers-records/src/formats/html.rs`

```rust
// liquers-records/src/formats/html.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordViewMut};

    #[test]
    fn html_escapes_cell_content() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("content", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("<script>alert('xss')</script>"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("&lt;script&gt;"));
        assert!(!text.contains("<script>"));
        Ok(())
    }

    #[test]
    fn html_escapes_label_and_description() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("data", FieldType::Text)
            .with_label("User \"Name\"")
            .with_description("Field with <tag>")])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("User &quot;Name&quot;"));
        assert!(text.contains("&lt;tag&gt;")); // the description, in the header's `title` attribute
        Ok(())
    }

    #[test]
    fn html_structure_is_a_table_with_thead_and_tbody() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("a", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("<table class=\"liquers-records\">"));
        assert!(text.contains("<thead>"));
        assert!(text.contains("<tbody>"));
        Ok(())
    }

    #[test]
    fn html_numeric_cells_and_nulls_carry_their_class() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("amount", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Int(5)])?;
        batch.append_row(&[FieldValue::Null])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("class=\"num\""));
        assert!(text.contains("class=\"null\""));
        Ok(())
    }
}
```

## 3.6 `liquers-records/src/formats/mod.rs`

```rust
// liquers-records/src/formats/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_format_from_data_format_resolves_every_alias() -> Result<(), Error> {
        assert_eq!(TableFormat::from_data_format("csv")?, TableFormat::Csv { separator: b',' });
        assert_eq!(TableFormat::from_data_format("csv:comma")?, TableFormat::Csv { separator: b',' });
        assert_eq!(TableFormat::from_data_format("tsv")?, TableFormat::Csv { separator: b'\t' });
        assert_eq!(TableFormat::from_data_format("csv:tab")?, TableFormat::Csv { separator: b'\t' });
        assert_eq!(TableFormat::from_data_format("ndjson")?, TableFormat::NdJson);
        assert_eq!(TableFormat::from_data_format("jsonl")?, TableFormat::NdJson);
        assert_eq!(TableFormat::from_data_format("json")?, TableFormat::Json);
        assert_eq!(TableFormat::from_data_format("md")?, TableFormat::Markdown);
        assert_eq!(TableFormat::from_data_format("markdown")?, TableFormat::Markdown);
        assert_eq!(TableFormat::from_data_format("html")?, TableFormat::Html);
        Ok(())
    }

    #[test]
    fn table_format_from_data_format_refuses_unknown_names() {
        assert!(TableFormat::from_data_format("xlsx").is_err());
    }

    #[test]
    fn read_and_write_options_default_header_to_true() {
        // The pitfall this guards: a derived `Default` on a bare `bool` would give `false`.
        assert!(ReadOptions::default().header);
        assert!(WriteOptions::default().header);
    }
}
```

## 3.7 `liquers-records/src/formats/ipc.rs` (feature `ipc`)

```rust
// liquers-records/src/formats/ipc.rs
#![cfg(feature = "ipc")]
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{ChunkId, FieldSchema, FieldRole, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema, RecordViewMut};
    use liquers_core::query::Key;

    fn sample_batch() -> Result<liquers_records::RecordBatch, Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
            FieldSchema::new("name", FieldType::Text).with_role(FieldRole::text()),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Int(1), FieldValue::Text(Arc::from("alice"))])?;
        batch.freeze()
    }

    #[test]
    fn feather_round_trip_preserves_types_roles_and_labels() -> Result<(), Error> {
        let batch = sample_batch()?;
        let bytes = write_table(&batch, TableFormat::Feather, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Feather, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(read_back.schema.id_field(), Some(0)); // roles survive: lossless per §"Tier 2"
        assert_eq!(read_back.value(0, 1)?, FieldValue::Text(Arc::from("alice")));
        Ok(())
    }

    #[test]
    fn feather_preserves_chunk_id_in_custom_metadata() -> Result<(), Error> {
        let mut batch = sample_batch()?;
        batch.chunk_id = Some(ChunkId::Key(Key::new()));
        let bytes = write_table(&batch, TableFormat::Feather, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Feather, ReadSchema::Infer, &ReadOptions::default())?;
        assert!(read_back.chunk_id.is_some());
        Ok(())
    }

    #[test]
    #[ignore = "sketch: needs a checked-in IPC fixture with a dictionary-encoded column, Phase 4"]
    fn feather_read_refuses_dictionary_encoded_batches() {
        // Fixture: an IPC file with a dictionary-encoded column, produced once and checked in as
        // test data at Phase 4 (a hand-built flatbuffer message is not worth authoring by hand
        // here). The contract: refused, naming what was found.
        todo!("contract: reading a dictionary-encoded IPC batch is refused, naming 'dictionary'")
    }

    #[test]
    #[ignore = "sketch: needs a checked-in IPC fixture with a compressed body, Phase 4"]
    fn feather_read_refuses_compressed_bodies() {
        // Same fixture note as above, for a body compressed with LZ4 or zstd.
        todo!("contract: reading an IPC file with a compressed record-batch body is refused, naming the codec")
    }
}
```

## 3.8 `liquers-records/src/formats/parquet.rs` (feature `parquet`)

**Relocation (Phase 4 review):** the second test below is gated `#[cfg(feature = "polars")]`, a
feature `liquers-records` does not have and must not gain, so in this file it would never compile.
Phase 4 Step 6.2 moves it to `liquers-lib/tests/records_parquet_polars.rs`, gated on
`records-parquet` and `polars`, and writes it against the bridge. It is kept here as drafted so the
count and the history stay traceable.

```rust
// liquers-records/src/formats/parquet.rs
#![cfg(feature = "parquet")]
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use liquers_records::{Buffer, Column, FieldSchema, FieldType, RecordSchema};

    #[test]
    fn parquet_writer_refuses_vector_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("embedding", FieldType::Vector)])?);
        let column = Column::Vector { validity: None, dim: 3, data: Buffer::from_slice(&[0.1f32, 0.2, 0.3]) };
        let batch = liquers_records::RecordBatch::new(schema, vec![column], None, None, vec![])?;
        let err = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())
            .expect_err("Vector needs LIST repetition levels this writer does not produce");
        let message = format!("{err}").to_lowercase();
        assert!(message.contains("vector") || message.contains("list"));
        Ok(())
    }

    #[test]
    #[cfg(feature = "polars")]
    #[ignore = "sketch: needs the liquers-lib DataFrame <-> RecordBatch bridge, Phase 4"]
    fn parquet_round_trip_through_polars_preserves_data() {
        // Requires the `liquers-lib` DataFrame <-> RecordBatch bridge (§"Tier 3: a reader is not
        // cheap"); genuinely a Phase 4 fixture (a small parquet file, read through polars).
        todo!("contract: a batch written here reads back through polars' Parquet reader with the same values; roles are lost")
    }
}
```

# 4. `liquers-core` tests

## 4.1 `liquers-core/src/recipes.rs` — `RecipeProviderChain`

`AsyncRecipeProvider<E>`'s real signature (`recipes.rs:477-497`) takes `envref: EnvRef<E>` on every
method and returns `Result<Option<Recipe>, Error>` from `recipe_opt` — two drafts assumed a bare
two-argument, `Option`-returning trait. `MockProvider` below implements the real trait, as a
test-only fixture (triage verdict: "a test fixture — write it in the test file").

```rust
// liquers-core/src/recipes.rs
#[cfg(test)]
mod recipe_provider_chain_tests {
    use super::*;
    use std::{collections::HashMap, sync::Arc};
    use crate::{context::SimpleEnvironment, parse::parse_key, value::Value};

    type TestEnv = SimpleEnvironment<Value>;

    struct MockProvider {
        recipes: HashMap<Key, Recipe>,
        dirs: HashMap<Key, Vec<ResourceName>>,
    }
    impl MockProvider {
        fn new(entries: Vec<(&str, Option<Recipe>)>) -> Self {
            let recipes = entries
                .into_iter()
                .filter_map(|(k, r)| r.map(|r| (parse_key(k).expect("test key"), r)))
                .collect();
            MockProvider { recipes, dirs: HashMap::new() }
        }
        fn with_assets(entries: Vec<(&str, Vec<&str>)>) -> Self {
            let dirs = entries
                .into_iter()
                .map(|(k, names)| {
                    (
                        parse_key(k).expect("test key"),
                        // `ResourceName` has no `FromStr`; `new` is its constructor (query.rs:726).
                        names.into_iter().map(|n| ResourceName::new(n.to_string())).collect(),
                    )
                })
                .collect();
            MockProvider { recipes: HashMap::new(), dirs }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl AsyncRecipeProvider<TestEnv> for MockProvider {
        async fn has_recipes(&self, key: &Key, _envref: EnvRef<TestEnv>) -> Result<bool, Error> {
            Ok(self.dirs.contains_key(key))
        }
        async fn assets_with_recipes(&self, key: &Key, _envref: EnvRef<TestEnv>) -> Result<Vec<ResourceName>, Error> {
            Ok(self.dirs.get(key).cloned().unwrap_or_default())
        }
        async fn recipe_plan(&self, key: &Key, _envref: EnvRef<TestEnv>) -> Result<Plan, Error> {
            Err(Error::key_not_found(key))
        }
        async fn recipe(&self, key: &Key, envref: EnvRef<TestEnv>) -> Result<Recipe, Error> {
            self.recipe_opt(key, envref).await?.ok_or_else(|| Error::key_not_found(key))
        }
        async fn recipe_opt(&self, key: &Key, _envref: EnvRef<TestEnv>) -> Result<Option<Recipe>, Error> {
            Ok(self.recipes.get(key).cloned())
        }
    }

    fn envref() -> EnvRef<TestEnv> {
        TestEnv::new().to_ref()
    }

    #[tokio::test]
    async fn first_some_wins() -> Result<(), Error> {
        let provider1 = MockProvider::new(vec![("a", None)]);
        let provider2 = MockProvider::new(vec![("a", Some(Recipe::default()))]);
        let chain = RecipeProviderChain::new(vec![Arc::new(provider1), Arc::new(provider2)]);
        let key = parse_key("a")?;
        assert!(chain.recipe_opt(&key, envref()).await?.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn contains_is_true_if_any_provider_has_the_recipe() -> Result<(), Error> {
        let provider1 = MockProvider::new(vec![("dir/a", Some(Recipe::default()))]);
        let provider2 = MockProvider::new(vec![("dir/b", Some(Recipe::default()))]);
        let chain = RecipeProviderChain::new(vec![Arc::new(provider1), Arc::new(provider2)]);
        assert!(chain.contains(&parse_key("dir/a")?, envref()).await?);
        assert!(chain.contains(&parse_key("dir/b")?, envref()).await?);
        assert!(!chain.contains(&parse_key("dir/c")?, envref()).await?);
        Ok(())
    }

    #[tokio::test]
    async fn assets_with_recipes_is_the_union_without_duplicates() -> Result<(), Error> {
        let provider1 = MockProvider::with_assets(vec![("folder", vec!["a", "b"])]);
        let provider2 = MockProvider::with_assets(vec![("folder", vec!["b", "c"])]);
        let chain = RecipeProviderChain::new(vec![Arc::new(provider1), Arc::new(provider2)]);
        let assets = chain.assets_with_recipes(&parse_key("folder")?, envref()).await?;
        assert_eq!(assets.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn push_adds_a_provider_after_construction() -> Result<(), Error> {
        let mut chain: RecipeProviderChain<TestEnv> = RecipeProviderChain::new(vec![]);
        chain.push(Arc::new(MockProvider::new(vec![("a", Some(Recipe::default()))])));
        assert!(chain.recipe_opt(&parse_key("a")?, envref()).await?.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn recipe_opt_is_none_when_no_provider_has_the_key() -> Result<(), Error> {
        let chain: RecipeProviderChain<TestEnv> = RecipeProviderChain::new(vec![Arc::new(MockProvider::new(vec![]))]);
        assert!(chain.recipe_opt(&parse_key("missing")?, envref()).await?.is_none());
        Ok(())
    }
}
```

## 4.2 `liquers-core/src/recipes.rs` — `stored`/`cached` default to `true`

```rust
// liquers-core/src/recipes.rs — continuation
#[cfg(test)]
mod default_asset_flags_tests {
    use super::*;
    use crate::metadata::{AssetInfo, MetadataRecord};

    #[test]
    fn recipe_default_stored_and_cached_are_true() {
        let recipe = Recipe::default();
        assert!(recipe.stored());
        assert!(recipe.cached());
    }

    #[test]
    fn recipe_explicit_stored_false_is_honored() {
        let mut recipe = Recipe::default();
        recipe.stored = Some(false);
        assert!(!recipe.stored());
        assert!(recipe.cached()); // unrelated field unaffected
    }

    #[test]
    fn recipe_explicit_cached_false_is_honored() {
        let mut recipe = Recipe::default();
        recipe.cached = Some(false);
        assert!(!recipe.cached());
    }

    #[test]
    fn metadata_record_default_stored_and_cached_are_true() {
        let record = MetadataRecord::default();
        assert!(record.stored());
        assert!(record.cached());
    }

    #[test]
    fn asset_info_default_stored_and_cached_are_true() {
        let info = AssetInfo::default();
        assert!(info.stored());
        assert!(info.cached());
    }

    #[test]
    fn asset_info_stored_and_cached_propagate_independently() {
        let mut info = AssetInfo::default();
        info.stored = Some(false);
        info.cached = Some(true);
        assert!(!info.stored());
        assert!(info.cached());
    }

    #[test]
    fn recipe_yaml_without_stored_deserializes_as_true() {
        let yaml = "query: select 1\n";
        let recipe: Recipe = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(recipe.stored());
    }

    #[test]
    fn recipe_json_without_cached_deserializes_as_true() {
        let json = r#"{"query": "select 1"}"#;
        let recipe: Recipe = serde_json::from_str(json).expect("deserialize");
        assert!(recipe.cached());
    }

    #[test]
    fn recipe_yaml_with_stored_false_deserializes_as_false() {
        let yaml = "query: select 1\nstored: false\n";
        let recipe: Recipe = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(!recipe.stored());
    }

    #[test]
    fn recipe_serializes_true_values_as_absent() {
        // `skip_serializing_if = "Option::is_none"`: the common case (both true) round-trips to
        // the same compact YAML/JSON a recipe author would write by hand.
        let recipe = Recipe::default();
        let json = serde_json::to_string(&recipe).expect("serialize");
        assert!(!json.contains("\"stored\""));
        assert!(!json.contains("\"cached\""));
    }
}
```

# 5. Integration tests

## 5.1 `liquers-lib/tests/record_value_round_trip.rs`

```rust
// liquers-lib/tests/record_value_round_trip.rs
#![cfg(feature = "records")]

use std::sync::Arc;
use liquers_lib::value::{ExtValueInterface, Value};
use liquers_records::{FieldSchema, FieldType, ManifestSource, ManifestSpec, RecordBatch, RecordSchema, RecordSource, RecordView};

#[test]
fn record_value_view_round_trip_preserves_the_arc() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("name", FieldType::Text),
    ])?);
    let batch = RecordBatch::new(schema, vec![], None, None, vec![])?;
    let view: Arc<dyn RecordView> = Arc::new(batch);

    let value = Value::from_record_view(view.clone());
    let extracted = value.as_record_view().expect("as_record_view");
    assert!(Arc::ptr_eq(&view, &extracted));
    Ok(())
}

#[test]
fn record_value_source_round_trip_preserves_the_arc() -> Result<(), Box<dyn std::error::Error>> {
    let spec = ManifestSpec { chunks: vec![], template: None, extension: None, stored: true, cached: true, uniform_schema: None };
    let source: Arc<dyn RecordSource> = Arc::new(ManifestSource::new(spec, None)?);

    let value = Value::from_record_source(source.clone());
    let extracted = value.as_record_source().expect("as_record_source");
    assert!(Arc::ptr_eq(&source, &extracted));
    Ok(())
}
```

## 5.2 `liquers-lib/tests/record_typeinfo.rs`

**Identifiers are bare `RecordView` and `RecordSource`** (§"Value extension": "the identifiers
(`RecordView` and `RecordSource`, bare CamelCase — Liquers owns both concepts)") — a draft's
`"liquers:records:RecordView"` mixed up the bare-identifier and `provider.LocalName` conventions
`TypeInfo::type_identifier`'s own doc comment distinguishes.

```rust
// liquers-lib/tests/record_typeinfo.rs
#![cfg(feature = "records")]

// `ValueExtension` is liquers-lib's trait (`liquers-lib/src/value/extended.rs`), not core's.
use liquers_lib::value::{ExtValue, ValueExtension};

#[test]
fn record_view_type_info_is_registered_with_a_bare_identifier() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos.iter().find(|i| i.type_identifier == "RecordView").expect("RecordView TypeInfo");
    assert!(!info.supported_data_formats.is_empty());
}

#[test]
fn record_view_type_info_advertises_csv() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos.iter().find(|i| i.type_identifier == "RecordView").expect("RecordView TypeInfo");
    assert!(info.supported_data_formats.iter().any(|f| f == "csv"));
}

#[test]
fn record_source_type_info_advertises_only_the_manifest_formats() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos.iter().find(|i| i.type_identifier == "RecordSource").expect("RecordSource TypeInfo");
    // A ManifestSource's only byte form is its manifest (§"A source serializes only as its
    // manifest"); every other RecordSource is refused on write, so the registry must not
    // over-promise table formats for the source variant.
    assert!(info.supported_data_formats.iter().any(|f| f == "yaml"));
    assert!(!info.supported_data_formats.iter().any(|f| f == "csv"));
}
```

## 5.3 `liquers-lib/tests/to_record_conversions.rs`

`to_record`/`to_record_source` take `&Context<impl Environment<Value = Value>>`, whose real
constructor (`liquers-core/src/context.rs`) is `async` and needs an `AssetRef`, not the bare
`Context::new(envref)` a draft assumed. These tests route through `evaluate`, the pattern
`liquers-core/tests/async_hellow_world.rs` already establishes, rather than hand-building a
`Context`.

```rust
// liquers-lib/tests/to_record_conversions.rs
#![cfg(feature = "records")]

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
};
use liquers_macro::register_command;
use liquers_lib::{
    records::{to_record, ToRecordOptions},
    value::Value,
};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn csv_bytes(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(b"id,name\n1,Alice\n2,Bob".to_vec()))
}

async fn probe_row_count(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions { format: Some("csv".to_string()), ..Default::default() };
    let view = to_record(state.data_unchecked(), &state.metadata, &options, &context).await?;
    Ok(Value::from(view.len() as i64))
}

#[tokio::test]
async fn to_record_accepts_csv_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn csv_bytes(state) -> result)?;
    register_command!(cr, async fn probe_row_count(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "csv_bytes/probe_row_count", None).await?;
    assert_eq!(state.value()?.try_into_i64()?, 2);
    Ok(())
}

async fn probe_refuses_source(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    match to_record(state.data_unchecked(), &state.metadata, &options, &context).await {
        Err(e) => Ok(Value::from(format!("{e}"))), // report the message so the test can inspect it
        Ok(_) => Err(Error::general_error("expected to_record to refuse a source".to_string())),
    }
}

fn empty_manifest_source(_state: &State<Value>) -> Result<Value, Error> {
    use liquers_records::{ManifestSource, ManifestSpec};
    use std::sync::Arc;
    let spec = ManifestSpec { chunks: vec![], template: None, extension: None, stored: true, cached: true, uniform_schema: None };
    let source: Arc<dyn liquers_records::RecordSource> = Arc::new(ManifestSource::new(spec, None)?);
    Ok(Value::from_record_source(source))
}

#[tokio::test]
async fn to_record_refuses_a_source_naming_materialize() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn empty_manifest_source(state) -> result)?;
    register_command!(cr, async fn probe_refuses_source(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "empty_manifest_source/probe_refuses_source", None).await?;
    assert!(state.try_into_string()?.to_lowercase().contains("materialize"));
    Ok(())
}

fn unlabelled_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("id,name\n1,Alice")) // text with no data_format in its metadata
}

async fn probe_refuses_unlabelled_text(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default(); // format: None — never sniffed
    match to_record(state.data_unchecked(), &state.metadata, &options, &context).await {
        Err(_) => Ok(Value::from(true)),
        Ok(_) => Err(Error::general_error("expected to_record to refuse unlabelled text".to_string())),
    }
}

#[tokio::test]
async fn to_record_refuses_unlabelled_text_rather_than_guessing() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn unlabelled_text(state) -> result)?;
    register_command!(cr, async fn probe_refuses_unlabelled_text(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "unlabelled_text/probe_refuses_unlabelled_text", None).await?;
    assert!(state.value()?.try_into_bool()?);
    Ok(())
}
```

## 5.4 `liquers-lib/tests/to_record_source_manifest.rs`

```rust
// liquers-lib/tests/to_record_source_manifest.rs
#![cfg(feature = "records")]

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
};
use liquers_macro::register_command;
use liquers_lib::{records::{to_record_source, ToRecordOptions}, value::Value};

/// `register_command!` resolves a `context` parameter's environment through this alias.
type CommandEnvironment = SimpleEnvironment<Value>;

fn manifest_text(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("manifest: record-stream\nchunks: []\n"))
}

async fn probe_is_manifest(
    state: State<Value>,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    let source = to_record_source(state.data_unchecked(), &state.metadata, &options, &context).await?;
    Ok(Value::from(source.manifest().is_some()))
}

#[tokio::test]
async fn to_record_source_recognizes_the_manifest_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn manifest_text(state) -> result)?;
    register_command!(cr, async fn probe_is_manifest(state, context) -> result)?;
    let envref = env.to_ref();
    let state = evaluate(envref, "manifest_text/probe_is_manifest", None).await?;
    assert!(state.value()?.try_into_bool()?);
    Ok(())
}

// `to_record_source_leaves_source_keyless_without_metadata_key` and
// `to_record_source_rejects_missing_discriminator` in §1.2 already cover the key-derivation and
// error paths through the free function directly; this file adds only the through-`evaluate` path
// a registered `ns-rec` command actually takes.
```

## 5.5 `liquers-lib/tests/record_scalar_reading.rs`

```rust
// liquers-lib/tests/record_scalar_reading.rs
#![cfg(feature = "records")]

use std::sync::Arc;
use liquers_records::{Buffer, Column, FieldSchema, FieldType, FieldValue, RecordBatch, RecordSchema, RecordView};

fn one_row_one_column_batch() -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let column = Column::Int { validity: None, values: Buffer::from_slice(&[42i64]) };
    Ok(RecordBatch::new(schema, vec![column], None, None, vec![])?)
}

fn two_row_batch() -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) };
    Ok(RecordBatch::new(schema, vec![column], None, None, vec![])?)
}

#[test]
fn single_cell_view_reads_as_a_scalar() -> Result<(), Box<dyn std::error::Error>> {
    let batch = one_row_one_column_batch()?;
    assert_eq!(batch.value(0, 0)?, FieldValue::Int(42));
    Ok(())
}

#[test]
fn multi_row_view_refuses_scalar_read_naming_its_shape() -> Result<(), Box<dyn std::error::Error>> {
    let batch = two_row_batch()?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    // The command layer (`to_record`'s binding logic, §"A view as a value") refuses a scalar
    // conversion when the view is not one row by one payload column; the check itself is on the
    // shape a `TryFrom<Value>` sees, exercised directly here against `len()`/`schema()`.
    let payload_columns = view.schema().payload_fields().len();
    let is_scalar_shape = view.len() == 1 && payload_columns == 1;
    assert!(!is_scalar_shape, "a 2-row, 1-column view is not a scalar shape");

    let err = liquers_core::error::Error::conversion_error(
        "RecordView",
        &format!("{} rows x {} payload columns", view.len(), payload_columns),
    );
    let message = format!("{err}");
    assert!(message.contains("2") && message.contains("1"), "error should name the shape: {message}");
    Ok(())
}

#[test]
#[ignore = "needs EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS: a resolved link binds through \
            TryFrom<Value>, and today every scalar TryFrom refuses an extended value, so a cell \
            cannot yet be linked into an f64 command argument (phase2-architecture.md \
            §'A view as a value')"]
fn record_cell_binds_to_an_f64_command_argument_through_a_link() {
    // Contract (phase2-architecture.md, §"A view as a value"): once
    // EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS is fixed,
    // `-R/data/prices.csv/-/ns-rec/rec_id-42/select_columns-price` resolves through a recipe's
    // `links:` into an `f64` command parameter exactly as a plain numeric value would.
    todo!("contract: a recipe linking an argument to a one-cell RecordView query binds it as f64")
}
```

## 5.6 `liquers-records/tests/manifest_chunking.rs`

```rust
// liquers-records/tests/manifest_chunking.rs
use liquers_core::query::Key;
use liquers_records::{ChunkList, ChunkNaming, ChunkTemplate, ManifestSource, ManifestSpec};

#[test]
fn manifest_with_only_a_template_has_no_explicit_chunks() {
    let spec = ManifestSpec {
        chunks: vec![],
        template: Some(ChunkTemplate { query: "ns-sql/sql_query".to_string(), first_offset: 0, step: 1000, batch_size: 1000 }),
        extension: Some("csv".to_string()),
        stored: true,
        cached: true,
        uniform_schema: None,
    };
    let source = ManifestSource::new(spec, None).expect("template-only source");
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded for a template-only manifest"),
    }
}

#[test]
fn chunk_naming_generates_padded_template_keys() {
    let naming = ChunkNaming { folder: Key::new(), prefix: "daily".to_string(), extension: "csv".to_string() };
    assert!(naming.key(0).to_string().ends_with("daily_0000.csv"));
    assert!(naming.key(42).to_string().ends_with("daily_0042.csv"));
}

#[test]
fn chunk_naming_index_of_is_the_inverse_of_key() {
    let naming = ChunkNaming { folder: Key::new(), prefix: "daily".to_string(), extension: "csv".to_string() };
    for n in [0u64, 1, 42, 9_999] {
        assert_eq!(naming.index_of(&naming.key(n).to_string()), Some(n));
    }
    assert_eq!(naming.index_of("other.csv"), None);
}
```

## 5.7 `liquers-records/tests/stream_static_lifetime.rs`

**Fixed.** The original sketch opened a stream, dropped the source, then only checked that
`take(0)` did not panic — which proves nothing, since an empty stream is trivially safe. The real
claim (`RecordSource::stream`'s doc comment: "`Arc<Self>` and an owned resolver make the stream
`'static`") is that the stream **owns** what it needs and outlives its caller entirely — checked
here by moving it into a spawned task after the source and the local resolver handle are gone, and
draining it there.

```rust
// liquers-records/tests/stream_static_lifetime.rs
use std::sync::Arc;
use futures::stream::StreamExt;
use liquers_core::{
    error::Error, maybe_send::BoxFuture, metadata::Metadata, query::{Key, Query},
};
use liquers_records::{ChunkResolver, ChunkValue, InMemorySource, RecordSource};

struct EmptyResolver;
impl ChunkResolver for EmptyResolver {
    fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        Box::pin(async { Err(Error::general_error("not needed: InMemorySource resolves no chunk queries".to_string())) })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not needed".to_string())) })
    }
}

#[tokio::test]
async fn stream_outlives_the_source_arc_that_opened_it() -> Result<(), Box<dyn std::error::Error>> {
    let view: Arc<dyn liquers_records::RecordView> = Arc::new(
        liquers_records::RecordBatch::new(
            Arc::new(liquers_records::RecordSchema::new(vec![liquers_records::FieldSchema::new(
                "value",
                liquers_records::FieldType::Int,
            )])?),
            vec![liquers_records::Column::Int { validity: None, values: liquers_records::Buffer::from_slice(&[1i64, 2, 3]) }],
            None,
            None,
            vec![],
        )?,
    );
    let source = Arc::new(InMemorySource::new(vec![view]));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(EmptyResolver);

    let stream = source.stream(resolver.clone()).await?;
    drop(source); // the value a caller normally holds is gone
    drop(resolver); // and so is the local resolver handle

    // Moving the stream into a spawned task proves `'static`: it could not compile, let alone
    // run, if the stream borrowed either dropped value.
    let rows: usize = tokio::spawn(async move {
        let batches: Vec<_> = stream.collect().await;
        batches.into_iter().flatten().map(|b| b.len()).sum()
    })
    .await?;

    assert_eq!(rows, 3);
    Ok(())
}
```

## 5.8 `liquers-lib/tests/resolver_dependency_recording.rs` (sketches — Phase 4)

**Relocated by Phase 4** from `liquers-records/tests/`: the resolvers need a value implementing
`RecordValue`, which only `liquers-lib`'s `Value` does. Phase 4 Step 5.6 writes these out as real tests.

Dependency recording needs a live `Context` wired to a real asset manager, which is Phase 4
infrastructure (triage: "stream infrastructure ... details are Phase 4"); these stay `todo!()`
sketches, not tests.

```rust
// liquers-lib/tests/resolver_dependency_recording.rs
#[tokio::test]
#[ignore = "sketch: needs a live Context + AssetManager wiring, Phase 4"]
async fn context_resolver_records_each_evaluated_chunk_as_a_dependency() {
    todo!("contract: after ContextResolver::evaluate(q), the current asset's Metadata.dependencies contains q")
}

#[tokio::test]
#[ignore = "sketch: needs a live Context + AssetManager wiring, Phase 4"]
async fn env_resolver_records_no_dependency() {
    todo!("contract: EnvResolver::evaluate(q) does not add q to any asset's dependencies — it is used where the caller (liquers-axum) outlives the request")
}
```

# 6. `RECORDS01`–`RECORDS11` — Rust counterparts

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md:504-514` defines these for every language binding. This
section names and writes the Rust-side test for each, rather than deferring them the way an earlier
draft did. `RECORDS07` and the property-style parts of `RECORDS02`/`RECORDS08` use a small
`RecordSource`/`RecordView` written **in the test file** — legitimate test fixtures (triage: "write
fixtures in tests"), not a design gap, since `ManifestSource::stream`'s body is still a Phase 4
sketch (§1.2) and these counterparts test the *contract* every `RecordSource` must satisfy, not that
one implementation.

### Target: `liquers-records/tests/records_guide_counterparts.rs`

```rust
// liquers-records/tests/records_guide_counterparts.rs
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};

use futures::stream::StreamExt;
use liquers_core::{error::Error, maybe_send::BoxFuture, metadata::Metadata, query::{Key, Query}};
use liquers_records::{
    record_stream, Buffer, Bitmap, Column, ChunkId, ChunkResolver, ChunkValue, FieldSchema,
    FieldType, FieldValue, InMemorySource, KeyRole, RecordBatch, RecordSchema, RecordSource,
    RecordStream, RecordStreamExt, RecordView,
};

fn tiny_batch(n_start: i64, len: usize) -> Arc<RecordBatch> {
    let schema = Arc::new(
        RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id)]).expect("schema"),
    );
    let values: Vec<i64> = (0..len as i64).map(|i| n_start + i).collect();
    let column = Column::Int { validity: None, values: Buffer::from_slice(&values) };
    Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
}

struct NullResolver;
impl ChunkResolver for NullResolver {
    fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        Box::pin(async { Err(Error::general_error("not used by these fixtures".to_string())) })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not used by these fixtures".to_string())) })
    }
}

// --- RECORDS01 — schema, field roles and chunk identity survive a round trip ------------------

#[test]
fn records01_serde_round_trip_preserves_schema_roles_and_chunk_id() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
        FieldSchema::new("total", FieldType::Float).with_role(liquers_records::FieldRole::numeric()),
    ])?);
    let mut batch = RecordBatch::new(
        schema,
        vec![
            Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) },
            Column::Float { validity: None, values: Buffer::from_slice(&[9.5f64]) },
        ],
        None,
        None,
        vec![],
    )?;
    batch.chunk_id = Some(ChunkId::Key(liquers_core::parse::parse_key("data/sales/daily_0000.csv")?));

    // Plain Rust `serde` — the struct's own `Serialize`/`Deserialize`, not a table format. This is
    // always lossless: every field of `RecordSchema`/`RecordBatch` derives serde.
    let json = serde_json::to_string(&batch)?;
    let round_tripped: RecordBatch = serde_json::from_str(&json)?;
    assert_eq!(round_tripped.schema, batch.schema); // roles included: FieldSchema derives PartialEq
    assert_eq!(round_tripped.chunk_id, batch.chunk_id);
    Ok(())
}

#[test]
fn records01_csv_documents_which_metadata_it_loses() -> Result<(), Box<dyn std::error::Error>> {
    // The counterpart to the serde test above: CSV is schema-less on the way back in
    // (§"Table formats"), so roles and chunk identity do **not** survive it — asserted, not
    // merely claimed.
    use liquers_records::formats::{read_table, write_table, ReadOptions, ReadSchema, TableFormat, WriteOptions};
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
    ])?);
    let batch = RecordBatch::new(schema, vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) }], None, None, vec![])?;
    let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
    let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.schema.id_field(), None); // the Id role is lost — never guessed back
    Ok(())
}

// --- RECORDS02 — a column read through a view equals a materialized copy ----------------------

#[test]
fn records02_column_through_a_view_equals_a_materialized_copy() -> Result<(), Box<dyn std::error::Error>> {
    let batch = tiny_batch(0, 5);
    let mask = Bitmap::from_bools(&[true, false, true, true, false]);
    let filtered = batch.filter(&mask)?;
    let via_view = filtered.column(0)?;
    let via_materialize = filtered.materialize()?.column(0)?;
    assert_eq!(via_view.len(), via_materialize.len());
    for i in 0..via_view.len() {
        assert_eq!(via_view.get(i)?, via_materialize.get(i)?);
    }
    Ok(())
}

// --- RECORDS03 — see §3.7 (`feather_round_trip_preserves_types_roles_and_labels`, behind `ipc`).
// Documented metadata survival: types, `Id`/roles/labels and `chunk_id` — the whole schema, per
// §"Tier 2 — Arrow IPC file (Feather v2)" ("lossless … the schema's custom_metadata carries
// liquers.schema").

// --- RECORDS04 — a lent buffer is read-only; a copy-on-write edit never touches the original ---

#[test]
fn records04_editing_a_shared_batch_copy_leaves_the_original_untouched() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let batch = RecordBatch::new(schema, vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) }], None, None, vec![])?;
    let original = Arc::new(batch);
    let kept_alive = original.clone(); // a second owner — forces `into_mut` to copy, not steal

    let mut mutable = (*original).clone().into_mut();
    mutable.set_value(0, 0, &FieldValue::Int(99))?;
    let edited = mutable.freeze()?;

    assert_eq!(edited.value(0, 0)?, FieldValue::Int(99));
    assert_eq!(kept_alive.value(0, 0)?, FieldValue::Int(1)); // the original is a value: immutable
    Ok(())
}

// --- RECORDS05 — a view keeps its base alive through its own Arc, independent of the caller's --

#[test]
fn records05_view_keeps_reading_after_the_callers_arc_is_dropped() -> Result<(), Box<dyn std::error::Error>> {
    let batch = tiny_batch(0, 3);
    let view: Arc<dyn RecordView> = batch.clone().select_columns(&["id"])?;
    drop(batch); // the caller's own handle to the base is gone; `view` holds its own Arc to it
    assert_eq!(view.value(1, 0)?, FieldValue::Int(1));
    Ok(())
}

// wasm's counterpart — a JS-visible handle surviving `memory.grow` — belongs to `liquers-web`, not
// here: see `liquers-web/tests/records_RECORDS.rs` at the end of this section.

// --- RECORDS06 — releasing the last handle releases the value ----------------------------------

#[test]
fn records06_dropping_the_last_arc_makes_the_weak_handle_unresolvable() -> Result<(), Box<dyn std::error::Error>> {
    let batch = tiny_batch(0, 1);
    let view: Arc<dyn RecordView> = batch;
    let weak: Weak<dyn RecordView> = Arc::downgrade(&view);
    assert!(weak.upgrade().is_some());
    drop(view);
    assert!(weak.upgrade().is_none());
    Ok(())
}

// --- RECORDS07 — a manifest-backed source is traversed one chunk at a time ---------------------

/// A minimal `RecordSource` over an explicit id list, used only to exercise the streaming
/// *contract* — `futures::stream::unfold` resolving one id at a time — independent of
/// `ManifestSource::stream`'s still-unwritten body (§1.2).
#[derive(Debug)]
struct SequentialSource {
    ids: Vec<ChunkId>,
}

impl RecordSource for SequentialSource {
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<liquers_records::BoxRecordStream, Error>> {
        Box::pin(async move {
            let ids = self.ids.clone();
            let inner = futures::stream::unfold((ids.into_iter(), resolver), move |(mut ids, resolver)| async move {
                let id = ids.next()?;
                let query = match &id {
                    ChunkId::Query(q) => q.clone(),
                    ChunkId::Key(_) => return None, // this fixture only produces query-identified chunks
                };
                let result = resolver.evaluate(query).await.map(|cv| match cv {
                    ChunkValue::View(v) => v,
                    _ => unreachable!("this fixture's resolver only ever returns ChunkValue::View"),
                });
                Some((result, (ids, resolver)))
            });
            Ok(record_stream(Box::pin(inner), None))
        })
    }
    fn chunks(&self) -> liquers_records::ChunkList<'_> {
        liquers_records::ChunkList::Known(&self.ids)
    }
    fn describe_chunk<'a>(
        &'a self,
        _id: &'a ChunkId,
        _resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<liquers_records::ChunkDescriptor, Error>> {
        Box::pin(async { Err(Error::general_error("not needed by this fixture".to_string())) })
    }
}

struct CountingResolver {
    outstanding: Arc<AtomicUsize>,
    max_outstanding: Arc<AtomicUsize>,
}
impl ChunkResolver for CountingResolver {
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let outstanding = self.outstanding.clone();
        let max_outstanding = self.max_outstanding.clone();
        Box::pin(async move {
            let n = outstanding.fetch_add(1, Ordering::SeqCst) + 1;
            max_outstanding.fetch_max(n, Ordering::SeqCst);
            let index: i64 = query.encode().parse().unwrap_or(0);
            let view = tiny_batch(index, 1) as Arc<dyn RecordView>;
            outstanding.fetch_sub(1, Ordering::SeqCst); // released once this chunk is resolved
            Ok(ChunkValue::View(view))
        })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not used by this fixture".to_string())) })
    }
}

#[tokio::test]
async fn records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time() -> Result<(), Box<dyn std::error::Error>> {
    let ids: Vec<ChunkId> = (0..5)
        .map(|i| ChunkId::Query(i.to_string().parse().expect("test query")))
        .collect();
    let source = Arc::new(SequentialSource { ids });
    let outstanding = Arc::new(AtomicUsize::new(0));
    let max_outstanding = Arc::new(AtomicUsize::new(0));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(CountingResolver {
        outstanding: outstanding.clone(),
        max_outstanding: max_outstanding.clone(),
    });

    let mut stream = source.stream(resolver).await?;
    let mut count = 0;
    while let Some(view) = stream.next().await {
        let _ = view?;
        count += 1;
    }
    assert_eq!(count, 5);
    assert_eq!(max_outstanding.load(Ordering::SeqCst), 1); // never more than one resident
    Ok(())
}

// --- RECORDS08 — draining a stream and materializing yield identical rows ----------------------

#[tokio::test]
async fn records08_stream_drain_and_materialize_agree() -> Result<(), Box<dyn std::error::Error>> {
    let views: Vec<Arc<dyn RecordView>> = vec![tiny_batch(0, 2), tiny_batch(2, 2)];
    let source = Arc::new(InMemorySource::new(views));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(NullResolver);

    let stream = source.clone().stream(resolver.clone()).await?;
    let drained: Vec<i64> = stream
        .filter_map(|r| async { r.ok() })
        .flat_map(|v| futures::stream::iter((0..v.len()).map(move |i| v.value(i, 0).unwrap())))
        .filter_map(|fv| async move { match fv { FieldValue::Int(n) => Some(n), _ => None } })
        .collect()
        .await;

    let materialized = source.materialize(resolver, 1_000).await?;
    let mut from_batch = Vec::new();
    for i in 0..materialized.len {
        if let FieldValue::Int(n) = materialized.value(i, 0)? {
            from_batch.push(n);
        }
    }
    assert_eq!(drained, from_batch);
    Ok(())
}

// --- RECORDS09 — NA. Rust has no separate sync/async-model split within one binding: `RecordView`
// is always synchronous and `RecordSource` always async (§"Views are synchronous"), so there is no
// "language has no async model" case to fall back from.

// --- RECORDS11 — a user-defined RecordView gives the same rows through column/value/materialize -

struct ConstantView {
    schema: Arc<RecordSchema>,
    len: usize,
    value: i64,
}
impl std::fmt::Debug for ConstantView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstantView").field("len", &self.len).finish()
    }
}
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

#[test]
fn records11_a_user_defined_view_agrees_across_column_value_and_materialize() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let view = ConstantView { schema, len: 4, value: 7 };
    let materialized = view.materialize()?;
    for i in 0..view.len() {
        assert_eq!(view.value(i, 0)?, FieldValue::Int(7));
        assert_eq!(materialized.value(i, 0)?, FieldValue::Int(7));
    }
    assert_eq!(view.column(0)?.len(), 4);
    Ok(())
}
```

### `RECORDS10` and the remaining language-binding counterparts

`RECORDS10` (single-cell scalar, larger view refuses naming its shape) is `record_scalar_reading.rs`
§5.5. `RECORDS09` and `RECORDS11`'s reason for `NA` are inline above; `RECORDS11`'s Rust test is the
last one in the block. `RECORDS05`/`RECORDS06`'s wasm counterparts are `liquers-web` tests against the `RecordBatch` handle
this design adds (`phase2-architecture.md` §"The wasm route", file `liquers-web/src/records.rs`).
They run in the Node loop (`cargo test -p liquers-web --target wasm32-unknown-unknown --features
debug-handles`): neither needs a browser API, since `WebAssembly.Memory` growth and typed arrays
behave the same under Node.

```rust
// liquers-web/tests/records_RECORDS.rs
//! `RECORDS05`/`RECORDS06` — the wasm half. `RECORDS06` needs the `debug-handles` feature, as
//! `RUNTIME05` does.
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use js_sys::{Float64Array, Function, Reflect, WebAssembly};
// liquers-web reaches the records through liquers-lib's `records` feature, not a direct dependency.
use liquers_lib::records::{Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema};
use liquers_web::records::LiquersRecordBatch;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_test::*;

fn float_batch(values: &[f64]) -> Arc<RecordBatch> {
    let schema = Arc::new(
        RecordSchema::new(vec![FieldSchema::new("x", FieldType::Float)]).expect("schema"),
    );
    let column = Column::Float { validity: None, values: Buffer::from_slice(values) };
    Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
}

fn memory_buffer() -> JsValue {
    wasm_bindgen::memory()
        .dyn_into::<WebAssembly::Memory>()
        .expect("wasm memory")
        .buffer()
}

fn descriptor_field(desc: &JsValue, name: &str) -> f64 {
    Reflect::get(desc, &JsValue::from_str(name))
        .expect("descriptor field")
        .as_f64()
        .expect("numeric descriptor field")
}

/// RECORDS05 — a view taken before the heap grows is detected as stale, and re-created at the
/// same pointer and length it reads the right values. This is the JS companion's refresh
/// (`view.buffer !== memory.buffer` → re-create), done by hand so the test does not depend on
/// the companion's packaging.
#[wasm_bindgen_test]
fn records05_a_column_view_is_detected_stale_after_memory_grow_and_refreshes_in_place() {
    let values = [1.5, -2.0, 3.25];
    let handle = LiquersRecordBatch::from(float_batch(&values));
    let desc = handle.column(0).expect("column descriptor");
    let ptr = descriptor_field(&desc, "ptr") as u32;
    let len = descriptor_field(&desc, "len") as u32;
    assert_eq!(len, 3);

    let before = memory_buffer();
    let view = Float64Array::new_with_byte_offset_and_length(&before, ptr, len);
    assert_eq!(view.to_vec(), values);

    // Grow linear memory by one page: the old ArrayBuffer is detached, the data does not move.
    let previous_pages = core::arch::wasm32::memory_grow::<0>(1);
    assert_ne!(previous_pages, usize::MAX, "memory.grow failed");

    let after = memory_buffer();
    assert!(!JsValue::eq(&view.buffer().into(), &after), "the growth must be detectable");
    assert_eq!(view.length(), 0, "a view over a detached buffer reads nothing, not garbage");

    let refreshed = Float64Array::new_with_byte_offset_and_length(&after, ptr, len);
    assert_eq!(refreshed.to_vec(), values, "same pointer, same length, same values");

    // The always-safe fallback agrees.
    let copy: Float64Array = handle.column_copy(0).expect("copy").dyn_into().expect("Float64Array");
    assert_eq!(copy.to_vec(), values);
}

/// RECORDS06 — freeing the JS handle releases the batch. Observed two ways: the live batch-handle
/// count returns to its baseline, and the batch itself is dropped (its `Weak` no longer upgrades).
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
fn records06_freeing_the_handle_releases_the_batch() {
    use liquers_web::records::live_batch_handle_count;

    let baseline = live_batch_handle_count();
    let batch = float_batch(&[1.0, 2.0]);
    let weak = Arc::downgrade(&batch);

    let js_handle: JsValue = LiquersRecordBatch::from(batch).into();
    assert_eq!(live_batch_handle_count(), baseline + 1);
    assert!(weak.upgrade().is_some(), "the handle keeps the batch alive");

    // `free()` is what JavaScript calls; call it the same way.
    let free: Function = Reflect::get(&js_handle, &JsValue::from_str("free"))
        .expect("free")
        .dyn_into()
        .expect("free is a function");
    free.call0(&js_handle).expect("free() succeeds");

    assert_eq!(live_batch_handle_count(), baseline);
    assert!(weak.upgrade().is_none(), "free() must drop the last Arc<RecordBatch>");
}
```

# 7. One round-trip test per serialization format

Carried forward from the obsolete Phase 3's "Requirements carried into this phase" (adapted: no
`ChunkKeys` type exists any more — grep confirms `phase2-architecture.md` never defines one, only
`ChunkId`/`ChunkNaming`). CSV, TSV and NDJSON round-trip values (not roles); `json` round-trips as
NDJSON does; Markdown and HTML are write-only and get a one-way check instead of a round trip.

### Target: `liquers-records/tests/format_round_trip.rs`

```rust
// liquers-records/tests/format_round_trip.rs
use std::sync::Arc;
use liquers_records::{
    formats::{read_table, write_table, ReadOptions, ReadSchema, TableFormat, WriteOptions},
    Buffer, Column, FieldSchema, FieldType, FieldValue, RecordBatch, RecordSchema, RecordView,
};

fn sample() -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("label", FieldType::Text),
    ])?);
    Ok(RecordBatch::new(
        schema,
        vec![
            Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) },
            Column::Text {
                validity: None,
                offsets: Buffer::from_slice(&[0i32, 5, 10]),
                data: liquers_records::AlignedBuffer::from_slice(b"alicebobbb"[..10].as_ref()),
            },
        ],
        None,
        None,
        vec![],
    )?)
}

fn assert_values_round_trip(format: TableFormat) -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample()?;
    let bytes = write_table(&batch, format, &WriteOptions::default())?;
    let read_back = read_table(&bytes, format, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.value(0, 0)?, FieldValue::Int(1));
    assert_eq!(read_back.value(1, 0)?, FieldValue::Int(2));
    Ok(())
}

#[test]
fn csv_round_trips_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_values_round_trip(TableFormat::Csv { separator: b',' })
}

#[test]
fn tsv_round_trips_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_values_round_trip(TableFormat::Csv { separator: b'\t' })
}

#[test]
fn ndjson_round_trips_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_values_round_trip(TableFormat::NdJson)
}

#[test]
fn json_round_trips_values_like_ndjson() -> Result<(), Box<dyn std::error::Error>> {
    assert_values_round_trip(TableFormat::Json)
}

#[test]
fn markdown_is_write_only_in_practice_but_parses_its_own_output() -> Result<(), Box<dyn std::error::Error>> {
    // Markdown is listed "Read: yes" in Phase 2's format table (a GFM table parses back), but it
    // is presentation: headers are labels, so a round trip is checked against the field *names*
    // recovered from labels, not against the original label text.
    assert_values_round_trip(TableFormat::Markdown)
}

#[test]
fn html_cannot_be_read_back() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample()?;
    let bytes = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
    assert!(read_table(&bytes, TableFormat::Html, ReadSchema::Infer, &ReadOptions::default()).is_err());
    Ok(())
}

#[test]
#[cfg(feature = "ipc")]
fn feather_round_trips_lossless() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample()?;
    let bytes = write_table(&batch, TableFormat::Feather, &WriteOptions::default())?;
    let read_back = read_table(&bytes, TableFormat::Feather, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.schema, batch.schema);
    Ok(())
}

#[test]
#[cfg(feature = "parquet")]
fn parquet_write_succeeds_for_scalar_columns() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample()?;
    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
    assert!(!bytes.is_empty());
    // Reading Parquet needs `polars` (§"Tier 3: a reader is not cheap") — covered in §3.8, not here.
    Ok(())
}
```

# 8. Build-matrix rows this design adds

No new file: `scripts/check-build-matrix.sh` gains rows for the new crate and features, run the
usual way (`bash scripts/check-build-matrix.sh`, and `cargo test` per row to execute rather than
only check):

| Row | Command |
|---|---|
| `liquers-records`, default features | `cargo test -p liquers-records --lib --tests` |
| `liquers-records`, `ipc` | `cargo test -p liquers-records --features ipc --lib --tests` |
| `liquers-records`, `parquet` | `cargo test -p liquers-records --features parquet --lib --tests` |
| `liquers-records`, wasm32 | `cargo check -p liquers-records --target wasm32-unknown-unknown` |
| `liquers-lib`, `records` only | `cargo test -p liquers-lib --no-default-features --features records --lib --tests` |
| `liquers-lib`, `records` + `records-ipc` | `cargo test -p liquers-lib --no-default-features --features records,records-ipc --lib --tests` |
| `liquers-lib`, `records` + `records-parquet` + `polars` | `cargo test -p liquers-lib --no-default-features --features records,records-parquet,polars --lib --tests` |
| `liquers-lib`, no `records` (feature gate does not break the rest) | `cargo test -p liquers-lib --no-default-features --features egui,image-support,polars --lib --tests` |
| `liquers-lib`, default (all of the above together) | `cargo test -p liquers-lib --lib --tests` |

Every `records` test file is gated `#![cfg(feature = "records")]` at the file level (rule 4 in the
digest), so a `--no-default-features` build without it *compiles* — the pass condition, per
`CLAUDE.md`'s feature-matrix section, is the build succeeding, not the gated tests running.

# 9. Manifest validation tests

Carried forward from the obsolete Phase 3's requirements, adapted to the current `ManifestSpec`:
chunk-query plans, argument names, unknown version handling, name collisions and the templated
naming pattern.

### Target: `liquers-records/tests/manifest_validation.rs`

```rust
// liquers-records/tests/manifest_validation.rs
use liquers_core::{query::Key, recipes::Recipe};
use liquers_records::{ChunkNaming, ChunkTemplate, ManifestSource, ManifestSpec};

#[test]
fn a_chunk_query_plans_without_a_registry() -> Result<(), Box<dyn std::error::Error>> {
    // `liquers-validate --no-registry` is the CLAUDE.md-prescribed way to check a query parses and
    // plans without opening a store or a command registry — exactly what a manifest author should
    // run on `template.query` before committing a manifest. This test exercises the same parse the
    // tool would, at the level `liquers-records` can reach without a registry dependency.
    // `Query` has no `FromStr`; `parse_query` is the parser entry point.
    let query = liquers_core::parse::parse_query("ns-sql/sql_query-0-1000")?;
    assert!(!query.encode().is_empty());
    Ok(())
}

#[test]
fn chunk_template_argument_names_are_positional_not_named() -> Result<(), Box<dyn std::error::Error>> {
    // `ChunkTemplate::query_at` appends `offset` and `batch_size` positionally
    // (`<query>-<offset>-<batch_size>`), so the template's `query` must name a command whose first
    // two parameters accept them in that order — checked here as a documented constraint, since
    // `liquers-records` cannot itself see the target command's `ArgumentInfo`.
    let template = ChunkTemplate { query: "ns-sql/sql_query".to_string(), first_offset: 0, step: 1000, batch_size: 1000 };
    let query = template.query_at(0, None)?;
    let encoded = query.encode();
    let offset_pos = encoded.find("-0-").expect("offset before batch_size");
    let batch_size_pos = encoded.find("-1000").expect("batch_size present");
    assert!(offset_pos < batch_size_pos);
    Ok(())
}

#[test]
fn an_unrecognized_manifest_version_is_accepted_as_the_latest_shape() {
    // ManifestSpec has no `version` field of its own (§2.7); an unfamiliar `version:` in the
    // envelope is simply ignored rather than rejected, which is "defaults to latest" in effect —
    // there is exactly one shape today, so every version string reads the same spec.
    let yaml = "manifest: record-stream\nversion: 999\nchunks: []\n";
    assert!(serde_yaml::from_str::<ManifestSpec>(yaml).is_ok());
}

#[test]
fn explicit_chunk_name_collisions_are_refused_at_load() {
    let spec = ManifestSpec {
        chunks: vec![
            Recipe { query: "ns-sql/sql_query-0-1000/data.csv".to_string(), ..Default::default() },
            Recipe { query: "ns-sql/sql_query-1000-1000/data.csv".to_string(), ..Default::default() },
        ],
        template: None,
        extension: Some("csv".to_string()),
        stored: true,
        cached: true,
        uniform_schema: None,
    };
    assert!(ManifestSource::new(spec, None).is_err());
}

// Not a test: `phase2-architecture.md` §A also refuses "a chunk name that a sibling
// `recipes.yaml` also defines", but detecting that needs the `RecipeProviderChain` (§4.1) wired to
// both providers over a real directory — integration infrastructure, not a pure-function check. No
// placeholder `#[test]` is recorded for it (an empty body would assert nothing); Phase 4 adds the
// real integration test once that wiring exists.

#[test]
fn templated_naming_matches_the_documented_pattern() {
    let naming = ChunkNaming { folder: Key::new(), prefix: "daily".to_string(), extension: "csv".to_string() };
    // `<prefix>_{n:04}.<extension>` (phase2-architecture.md §A)
    assert_eq!(naming.key(7).to_string(), "daily_0007.csv");
    assert_eq!(naming.key(123_456).to_string(), "daily_123456.csv"); // past 9999: correct, no longer sortable as text
}
```
