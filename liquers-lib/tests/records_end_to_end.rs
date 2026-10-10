//! End-to-end tests for the manifest recipe provider inside the library environment
//! (`specs/design/record-streams/phase4-implementation.md`, Step 5.6, §"Tests this plan adds").
//!
//! Every test builds its environment with [`DefaultEnvironment`] (or
//! [`default_environment_builder`]) **without** configuring a recipe provider: only `LibKind`'s
//! own default carries `ManifestRecipeProvider` (Step 5.6's decision — `SimpleEnvironment`
//! resolves no recipes at all, so it cannot serve a manifest's chunk keys).
//!
//! **Counting commands without cross-talk.** Tests in this file run in parallel threads, so a
//! single counter shared by all of them would race. `fixture_rows` counts per test tag in a
//! `static Mutex<HashMap<String, usize>>`, and the tag travels as part of the manifest's own
//! query text (embedded ahead of the offset/batch parameters a template or explicit chunk
//! appends) — each test uses its own tag and reads only its own entry
//! (phase4-implementation.md §"Counting commands without cross-talk").
#![cfg(feature = "records")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{
    FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema, RecordView,
    RecordViewMut,
};
use liquers_lib::register_records_commands;
use liquers_core::value::ValueInterface;
use liquers_lib::value::{ExtValueInterface, Value};

// -------------------------------------------------------------------------------------------
// The counting fixture command: `ns-fixture/fixture_rows-<tag>-<offset>-<batch>`
// -------------------------------------------------------------------------------------------

fn fixture_counts() -> &'static Mutex<HashMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fixture_count_of(tag: &str) -> usize {
    fixture_counts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(tag)
        .copied()
        .unwrap_or(0)
}

/// A batch of `batch` rows, every row's single `offset` field equal to `offset` — enough to check
/// which offset a served chunk resolved to without needing the row content to vary. Counts once
/// per call in `tag`'s slot.
fn fixture_rows(tag: String, offset: i64, batch: i64) -> Result<Value, Error> {
    {
        let mut counts = fixture_counts()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *counts.entry(tag).or_insert(0) += 1;
    }
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

/// Two rows keyed by a `Date` `Id`: 2026-09-26 (`"before"`) and 2026-09-27 (`"target"`).
fn dated_rows() -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("day", FieldType::Date).with_key(KeyRole::Id),
        FieldSchema::new("name", FieldType::Text),
    ])?);
    let mut builder = RecordBatchMut::with_capacity(schema, 2);
    builder.append_row(&[
        FieldValue::Date(20722),
        FieldValue::Text(Arc::from("before")),
    ])?;
    builder.append_row(&[
        FieldValue::Date(20723),
        FieldValue::Text(Arc::from("target")),
    ])?;
    let view: Arc<dyn RecordView> = Arc::new(builder.freeze()?);
    Ok(Value::from_record_view(view))
}

// -------------------------------------------------------------------------------------------
// Environment construction
// -------------------------------------------------------------------------------------------

/// A `DefaultEnvironment<Value>` over `store`, with the `ns-rec` commands and `fixture_rows`
/// registered — built through `DefaultEnvironment::new()`/`get_mut_command_registry`, never
/// through a builder call that would configure a recipe provider of its own (Step 5.6's decision:
/// only `LibKind::default_recipe_provider` may supply one here).
fn build_env(store: AsyncMemoryStore) -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
    type CommandEnvironment = DefaultEnvironment<Value>;

    let mut env = DefaultEnvironment::<Value>::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        register_command!(cr,
            fn fixture_rows(tag: String, offset: i64, batch: i64) -> result
            namespace: "fixture"
        )?;
        register_command!(cr,
            fn dated_rows() -> result
            namespace: "fixture"
        )?;
        register_command!(cr,
            fn short_csv() -> result
            namespace: "fixture"
        )?;
    }
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

/// Evaluates `query` through the environment's own asset manager ([`EnvRef::evaluate`]) rather
/// than `liquers_core::interpreter::evaluate`, the free function its own doc comment marks for
/// decommissioning. (It used to matter more: the free function's ad-hoc asset declared
/// `data_format: bin`, which a trailing `daily.csv` could not override —
/// `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`, closed by `design/plan-step-state-metadata/`.)
async fn eval(
    envref: EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<liquers_core::state::State<Value>, Error> {
    envref.evaluate(query).await?.get().await
}

/// Stores `yaml` at `key` as plain text with a `yaml` data format and no declared type
/// identifier — the shape Step 0.2 made loadable as a structured value, which `to_record_source`
/// recognizes by its `manifest: record-stream` discriminator rather than by any stored type name.
async fn set_manifest(store: &AsyncMemoryStore, key: &Key, yaml: &str) -> Result<(), Error> {
    let mut metadata = Metadata::new();
    // `set_filename` seeds `data_format` from the extension when none is set yet — the last
    // dot-segment of `daily.manifest.yaml` is `yaml`, which is exactly the format
    // `ManifestRecipeProvider` (raw YAML) and `to_record_source`'s `SimpleValue::Object` path
    // (Step 0.2) both need.
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?;
    metadata.set_filename(filename.encode().as_ref())?;
    // A hand-placed file, fully present: `try_fast_track` refuses to load a stored resource
    // whose status is not `Ready`/`Source`/`Override`, and `Metadata::new()`'s default is `None`.
    metadata.set_status(Status::Source)?;
    store.set(key, yaml.as_bytes(), &metadata).await
}

// -------------------------------------------------------------------------------------------
// template_chunk_key_is_served_by_the_manifest
// -------------------------------------------------------------------------------------------

/// With `data/sales/daily.manifest.yaml` (a template over `ns-fixture/fixture_rows`) in a memory
/// store, `-R/data/sales/daily_0010.csv` evaluates to the rows of offset
/// `first_offset + 10 × step`.
#[tokio::test]
async fn template_chunk_key_is_served_by_the_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let tag = "template_chunk_key_is_served_by_the_manifest";
    let store = AsyncMemoryStore::new(&Key::new());
    let manifest_key = parse_key("data/sales/daily.manifest.yaml")?;
    set_manifest(
        &store,
        &manifest_key,
        &format!(
            "manifest: record-stream\n\
             template:\n\
             \x20\x20query: ns-fixture/fixture_rows-{tag}\n\
             \x20\x20first_offset: 1000\n\
             \x20\x20step: 10\n\
             \x20\x20batch_size: 5\n"
        ),
    )
    .await?;
    let envref = build_env(store)?;

    // Global chunk index 10 (no explicit chunks precede the template): offset =
    // first_offset + step * 10 = 1000 + 10*10 = 1100.
    let state = eval(envref, "-R/data/sales/daily_0010.csv").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(view.len(), 5, "the template's batch_size");
    for row in 0..view.len() {
        assert_eq!(view.value(row, 0)?, FieldValue::Int(1100));
    }
    assert_eq!(fixture_count_of(tag), 1);
    Ok(())
}

// -------------------------------------------------------------------------------------------
// stored_template_chunk_is_written_under_its_key
// -------------------------------------------------------------------------------------------

/// With `stored` absent (default `true`), the store holds `data/sales/daily_0010.csv` after
/// evaluation; with `stored: false`, it does not. Asserted only after the `MetadataSaver`
/// interval (100 ms, `assets.rs:946`) has passed, since its writes are debounced in a spawned
/// task.
#[tokio::test]
async fn stored_template_chunk_is_written_under_its_key() -> Result<(), Box<dyn std::error::Error>>
{
    async fn manifest_env(
        tag: &str,
        stored_line: &str,
    ) -> Result<(EnvRef<DefaultEnvironment<Value>>, Key), Box<dyn std::error::Error>> {
        let store = AsyncMemoryStore::new(&Key::new());
        let manifest_key = parse_key("data/sales/daily.manifest.yaml")?;
        set_manifest(
            &store,
            &manifest_key,
            &format!(
                "manifest: record-stream\n\
                 {stored_line}\
                 template:\n\
                 \x20\x20query: ns-fixture/fixture_rows-{tag}\n\
                 \x20\x20first_offset: 0\n\
                 \x20\x20step: 1\n\
                 \x20\x20batch_size: 3\n"
            ),
        )
        .await?;
        let chunk_key = parse_key("data/sales/daily_0000.csv")?;
        let envref = build_env(store)?;
        Ok((envref, chunk_key))
    }

    // `stored` absent: the default is `true`, and the chunk key ends up in the store.
    {
        let (envref, chunk_key) = manifest_env("stored_absent", "").await?;
        eval(envref.clone(), "-R/data/sales/daily_0000.csv").await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let store = envref.get_async_store();
        assert!(
            store.contains(&chunk_key).await?,
            "stored absent (default true) must leave the chunk in the store"
        );
    }

    // `stored: false`: the chunk is served, but never written.
    {
        let (envref, chunk_key) = manifest_env("stored_false", "stored: false\n").await?;
        eval(envref.clone(), "-R/data/sales/daily_0000.csv").await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let store = envref.get_async_store();
        assert!(
            !store.contains(&chunk_key).await?,
            "stored: false must leave no data and no metadata-only entry in the store"
        );
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// materialize_query_yields_csv_bytes
// -------------------------------------------------------------------------------------------

/// `-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv` (an explicit-chunk
/// manifest) returns CSV whose rows are every chunk's rows, in order.
#[tokio::test]
async fn materialize_query_yields_csv_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let tag = "materialize_query_yields_csv_bytes";
    let store = AsyncMemoryStore::new(&Key::new());
    let manifest_key = parse_key("data/sales/daily.manifest.yaml")?;
    set_manifest(
        &store,
        &manifest_key,
        &format!(
            "manifest: record-stream\n\
             chunks:\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-10-1\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-20-1\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-30-1\n"
        ),
    )
    .await?;
    let envref = build_env(store)?;

    let state = eval(
        envref,
        "-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv",
    )
    .await?;
    let bytes = state.as_bytes()?;
    let csv = String::from_utf8(bytes)?;
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "offset");
    assert_eq!(lines[1], "10");
    assert_eq!(lines[2], "20");
    assert_eq!(lines[3], "30");
    Ok(())
}

// -------------------------------------------------------------------------------------------
// rowid_evaluates_only_its_chunk
// -------------------------------------------------------------------------------------------

/// `…/ns-rec/rowid-2-0` runs the fixture command once, for chunk 2.
#[tokio::test]
async fn rowid_evaluates_only_its_chunk() -> Result<(), Box<dyn std::error::Error>> {
    let tag = "rowid_evaluates_only_its_chunk";
    let store = AsyncMemoryStore::new(&Key::new());
    let manifest_key = parse_key("data/sales/three.manifest.yaml")?;
    set_manifest(
        &store,
        &manifest_key,
        &format!(
            "manifest: record-stream\n\
             chunks:\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-100-1\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-200-1\n\
             \x20\x20- query: ns-fixture/fixture_rows-{tag}-300-1\n"
        ),
    )
    .await?;
    let envref = build_env(store)?;

    let state = eval(
        envref,
        "-R/data/sales/three.manifest.yaml/-/ns-rec/to_record_source/-/ns-rec/rowid-2-0",
    )
    .await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(view.len(), 1);
    assert_eq!(view.value(0, 0)?, FieldValue::Int(300), "chunk 2's own offset");
    assert_eq!(
        view.row_id(0)?,
        liquers_records::RowId { chunk: 2, row: 0 },
        "the row keeps the implicit id it was addressed by"
    );
    assert_eq!(
        view.row_number(0)?,
        None,
        "a chunk read on its own has no known row number"
    );
    assert_eq!(
        fixture_count_of(tag),
        1,
        "only the addressed chunk's command may run"
    );
    Ok(())
}

// -------------------------------------------------------------------------------------------
// file_records_lists_a_store_directory_through_a_query
// -------------------------------------------------------------------------------------------

/// `-R-sdir/<dir>/-/ns-rec/file_records` lists a store directory end to end: the `sdir` header
/// yields the directory's listing and carries its key into the command's state. A plain
/// `-R/<dir>` asks for the value *at* the key, which a directory does not hold, so it fails.
#[tokio::test]
async fn file_records_lists_a_store_directory_through_a_query(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    for (name, body) in [("a.csv", "x\n1\n"), ("b.csv", "x\n2\n3\n")] {
        set_manifest(&store, &parse_key(&format!("files/{name}"))?, body).await?;
    }
    let envref = build_env(store)?;

    let state = eval(envref.clone(), "-R-sdir/files/-/ns-rec/file_records").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(view.len(), 2);
    let mut names: Vec<FieldValue> = (0..view.len()).map(|row| view.value(row, 1)).collect::<Result<_, _>>()?;
    names.sort_by_key(|value| format!("{value:?}"));
    assert_eq!(names, vec![FieldValue::Text(Arc::from("a.csv")), FieldValue::Text(Arc::from("b.csv"))]);

    let failed = match eval(envref, "-R/files/-/ns-rec/file_records").await {
        Err(_) => true,
        Ok(state) => state.is_error()?,
    };
    assert!(failed, "a plain -R/<dir> holds no value to list");
    Ok(())
}

// -------------------------------------------------------------------------------------------
// rec_id_selects_by_date_query
// -------------------------------------------------------------------------------------------

/// A `Date` `Id` is addressed by its ISO spelling, basic or extended — the extended one written
/// with `~` for its hyphens, since `-` separates action parameters
/// (`specs/design/rec-id-iso-date-parsing/`).
#[tokio::test]
async fn rec_id_selects_by_date_query() -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(AsyncMemoryStore::new(&Key::new()))?;
    for query in [
        "ns-fixture/dated_rows/ns-rec/rec_id-20260927",
        "ns-fixture/dated_rows/ns-rec/rec_id-2026~09~27",
    ] {
        let state = eval(envref.clone(), query).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1, "{query}");
        assert_eq!(
            view.value(0, 1)?,
            FieldValue::Text(Arc::from("target")),
            "{query}"
        );
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// to_record_logs_padded_csv_rows
// -------------------------------------------------------------------------------------------

/// The bytes of a CSV with two short rows, as a command returns them.
fn short_csv() -> Result<Value, Error> {
    Ok(Value::from_bytes(
        b"a,b,c,d\n1,2,3,4\n5,6\n7,8,9\n10,11,12,13\n".to_vec(),
    ))
}

/// A CSV with short rows reads, padded, and says so once in the asset's log — the aggregate
/// warning decided for `specs/design/csv-physical-lines-short-rows/`.
///
/// The CSV comes from a command rather than from a stored `data/short.csv`, as Phase 3 planned:
/// a stored CSV without a `RecordView` type identifier cannot be loaded at all
/// (`STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ`), and one with it is deserialized
/// before any command runs, where there is no log.
#[tokio::test]
async fn to_record_logs_padded_csv_rows() -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(AsyncMemoryStore::new(&Key::new()))?;

    let state = eval(envref, "ns-fixture/short_csv/ns-rec/to_record-csv").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(view.len(), 4);
    let record = state
        .metadata
        .metadata_record()
        .ok_or("the evaluated asset has a metadata record")?;
    let padded: Vec<&str> = record
        .log
        .iter()
        .map(|entry| entry.message.as_str())
        .filter(|message| message.contains("less than number of columns in the header"))
        .collect();
    assert_eq!(
        padded,
        vec![
            "There has been 2 rows with number of cells between 2 and 3, which is less than \
             number of columns in the header (4)."
        ],
        "one aggregate warning per read"
    );
    Ok(())
}
