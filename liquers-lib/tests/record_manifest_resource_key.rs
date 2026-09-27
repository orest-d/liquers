//! A manifest reached as a resource — `-R/<folder>/<name>.manifest.yaml/-/ns-rec/...` — is keyed by
//! the key it was fetched from, so its chunks are keyed too (phase2-architecture.md §"Keyed
//! chunks"): template chunks are named `<name>_{n:04}.<extension>` in the manifest's folder, an
//! explicit chunk whose query ends in a filename is that filename in the folder, and per-chunk
//! `arguments` act through the chunk's recipe.
//!
//! Only a `*.manifest.yaml` key keys a manifest: `ManifestRecipeProvider` reads only those, so a
//! manifest stored under any other name could never have its chunk keys served, and is keyless.
#![cfg(feature = "records")]

use std::sync::Arc;
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
    FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView, RecordViewMut,
};
use liquers_lib::register_records_commands;
use liquers_lib::value::{ExtValueInterface, Value};

/// `rows` rows, each with the single field `offset` set to `offset`.
fn rows_of(offset: i64, rows: usize) -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("offset", FieldType::Int)])?);
    let mut builder = RecordBatchMut::with_capacity(schema, rows);
    for _ in 0..rows {
        builder.append_row(&[FieldValue::Int(offset)])?;
    }
    let view: Arc<dyn RecordView> = Arc::new(builder.freeze()?);
    Ok(Value::from_record_view(view))
}

/// Always `batch` rows — the explicit chunk's command.
fn fixed_rows(offset: i64, batch: i64) -> Result<Value, Error> {
    let rows = usize::try_from(batch)
        .map_err(|_| Error::general_error(format!("batch must not be negative, got {batch}")))?;
    rows_of(offset, rows)
}

/// `batch` rows below offset 3, none from there on — so a template walk over it ends (a template
/// stops at the first chunk shorter than `batch_size`).
fn limited_rows(offset: i64, batch: i64) -> Result<Value, Error> {
    if offset < 3 {
        fixed_rows(offset, batch)
    } else {
        rows_of(offset, 0)
    }
}

fn build_env(store: AsyncMemoryStore) -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
    type CommandEnvironment = DefaultEnvironment<Value>;

    let mut env = DefaultEnvironment::<Value>::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        register_command!(cr,
            fn fixed_rows(offset: i64, batch: i64) -> result
            namespace: "fixture"
        )?;
        register_command!(cr,
            fn limited_rows(offset: i64, batch: i64) -> result
            namespace: "fixture"
        )?;
    }
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

async fn eval(
    envref: EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<liquers_core::state::State<Value>, Error> {
    envref.evaluate(query).await?.get().await
}

/// Stores `yaml` at `key` as a hand-placed `Source` file whose data format comes from its
/// extension — as `records_end_to_end.rs` does.
async fn set_manifest(store: &AsyncMemoryStore, key: &Key, yaml: &str) -> Result<(), Error> {
    let mut metadata = Metadata::new();
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?;
    metadata.set_filename(filename.encode().as_ref())?;
    metadata.set_status(Status::Source)?;
    store.set(key, yaml.as_bytes(), &metadata).await
}

/// One keyed explicit chunk whose `arguments` replace the query's offset 10 with 55, then a
/// template that yields offsets 1 and 2 and ends at 3.
const MANIFEST: &str = "manifest: record-stream\n\
chunks:\n\
\x20\x20- query: ns-fixture/fixed_rows-10-1/first.csv\n\
\x20\x20\x20\x20arguments:\n\
\x20\x20\x20\x20\x20\x20offset: 55\n\
template:\n\
\x20\x20query: ns-fixture/limited_rows\n\
\x20\x20first_offset: 0\n\
\x20\x20step: 1\n\
\x20\x20batch_size: 1\n";

/// A template only: no per-chunk arguments, so it is valid keyed or keyless.
const TEMPLATE_ONLY: &str = "manifest: record-stream\n\
template:\n\
\x20\x20query: ns-fixture/limited_rows\n\
\x20\x20first_offset: 0\n\
\x20\x20step: 1\n\
\x20\x20batch_size: 1\n";

fn offsets(view: &Arc<dyn RecordView>) -> Result<Vec<FieldValue>, Error> {
    (0..view.len()).map(|row| view.value(row, 0)).collect()
}

/// `ns-rec/materialize` on a manifest fetched from `data/x.manifest.yaml`: the explicit chunk's
/// `arguments` take effect (offset 55, not the query's 10) and every chunk — explicit and template
/// — is evaluated as a keyed asset and stored under its key.
#[tokio::test]
async fn materialize_keys_the_chunks_of_a_manifest_fetched_as_a_resource(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    set_manifest(&store, &parse_key("data/x.manifest.yaml")?, MANIFEST).await?;
    let envref = build_env(store)?;

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
    Ok(())
}

/// The same through `ns-rec/to_record_source` and a second command: the source the first action
/// keyed must stay keyed when the next command receives it — that state carries the evaluating
/// asset's metadata, which names no key, and must not re-key the source to "no key".
#[tokio::test]
async fn to_record_source_keeps_the_manifest_key_into_the_next_command(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    set_manifest(&store, &parse_key("data/x.manifest.yaml")?, MANIFEST).await?;
    let envref = build_env(store)?;

    let state = eval(
        envref.clone(),
        "-R/data/x.manifest.yaml/-/ns-rec/to_record_source/-/ns-rec/materialize",
    )
    .await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(
        offsets(&view)?,
        vec![FieldValue::Int(55), FieldValue::Int(1), FieldValue::Int(2)]
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        envref
            .get_async_store()
            .contains(&parse_key("data/first.csv")?)
            .await?,
        "the explicit chunk must be evaluated as the keyed asset data/first.csv"
    );
    Ok(())
}

/// A manifest stored under a name that is not `*.manifest.yaml` is keyless: its template chunks
/// are unkeyed (a key such as `data/plain.yaml_0001.csv` could never be served, since
/// `ManifestRecipeProvider` reads only `*.manifest.yaml`), so it still materializes.
#[tokio::test]
async fn a_manifest_not_named_manifest_yaml_is_keyless() -> Result<(), Box<dyn std::error::Error>>
{
    let store = AsyncMemoryStore::new(&Key::new());
    set_manifest(&store, &parse_key("data/plain.yaml")?, TEMPLATE_ONLY).await?;
    let envref = build_env(store)?;

    let state = eval(envref.clone(), "-R/data/plain.yaml/-/ns-rec/materialize").await?;
    let view = state.value()?.as_record_view()?;
    assert_eq!(
        offsets(&view)?,
        vec![FieldValue::Int(0), FieldValue::Int(1), FieldValue::Int(2)]
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !envref
            .get_async_store()
            .contains(&parse_key("data/plain.yaml_0000.csv")?)
            .await?,
        "a keyless manifest's chunks are unkeyed, never stored"
    );
    Ok(())
}

/// A keyless manifest with per-chunk arguments is refused with an error saying why, rather than
/// running the chunk as a bare query with its arguments silently dropped.
#[tokio::test]
async fn a_keyless_manifest_with_chunk_arguments_is_refused(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    set_manifest(&store, &parse_key("data/plain.yaml")?, MANIFEST).await?;
    let envref = build_env(store)?;

    // A failed command surfaces either as `Err` or as an error state whose `value()` refuses.
    let outcome = eval(envref, "-R/data/plain.yaml/-/ns-rec/materialize")
        .await
        .and_then(|state| state.value());
    let error = match outcome {
        Ok(value) => panic!(
            "a keyless manifest with chunk arguments must be refused, got {:?}",
            value.as_record_view().map(|view| offsets(&view))
        ),
        Err(error) => error,
    };
    assert!(
        error.message.contains("arguments") && error.message.contains("no key"),
        "the error must say the arguments cannot apply to a keyless manifest: {error}"
    );
    Ok(())
}
