//! A manifest over a directory of stored CSV files, read as one table — record-streams' motivating
//! case, end to end (`specs/design/manifest-over-stored-csv-test/`).
//!
//! Each file is one explicit, unkeyed chunk: its query reads the file and converts it
//! (`-R/data/raw/jan.csv/-/ns-rec/to_record-csv`), so the chunk has no filename of its own and is
//! identified by its query.
//!
//! The CSV files carry the `RecordView` type identifier, as Liquers writes them. A hand-placed CSV
//! without it cannot be loaded at all (`STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ`);
//! [`manifest_over_hand_placed_csv_files_materializes`] is that case, ignored until it is fixed.
#![cfg(feature = "records")]

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::records::{FieldValue, RecordView};
use liquers_lib::register_records_commands;
use liquers_lib::value::{ExtValueInterface, Value};

fn build_env(store: AsyncMemoryStore) -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
    type CommandEnvironment = DefaultEnvironment<Value>;

    let mut env = DefaultEnvironment::<Value>::new();
    {
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
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

/// Stores `bytes` at `key` as a `Source` file whose data format comes from its extension, with
/// `type_identifier` when given.
async fn put(
    store: &AsyncMemoryStore,
    key: &str,
    bytes: &[u8],
    type_identifier: Option<&str>,
) -> Result<(), Error> {
    let key = parse_key(key)?;
    let mut metadata = Metadata::new();
    let filename = key
        .filename()
        .ok_or_else(|| Error::general_error(format!("key '{key}' has no filename")))?;
    metadata.set_filename(filename.encode().as_ref())?;
    metadata.set_status(Status::Source)?;
    if let Some(type_identifier) = type_identifier {
        metadata.with_type_identifier(type_identifier.to_string());
    }
    store.set(&key, bytes, &metadata).await
}

const JAN: &[u8] = b"month,amount\njan,10\njan,20\n";
const FEB: &[u8] = b"month,amount\nfeb,30\nfeb,40\nfeb,50\n";
/// One `amount` is empty, so null, in a column `uniform_schema` declares not null. The other is a
/// number, so the column still reads as `Int` and the null check is what refuses it.
const BAD: &[u8] = b"month,amount\nmar,\nmar,60\n";

fn manifest(files: &[&str]) -> String {
    let mut yaml = String::from(
        "manifest: record-stream\n\
         uniform_schema:\n\
         \x20\x20fields:\n\
         \x20\x20\x20\x20- name: month\n\
         \x20\x20\x20\x20\x20\x20data_type: Text\n\
         \x20\x20\x20\x20- name: amount\n\
         \x20\x20\x20\x20\x20\x20data_type: Int\n\
         \x20\x20\x20\x20\x20\x20nullable: false\n\
         chunks:\n",
    );
    for file in files {
        yaml.push_str(&format!(
            "\x20\x20- query: -R/data/raw/{file}/-/ns-rec/to_record-csv\n"
        ));
    }
    yaml
}

/// `jan.csv`, `feb.csv` and `bad.csv` under `data/raw/`, plus `all.manifest.yaml` over the first
/// two and `with_bad.manifest.yaml` over all three.
async fn stored_files(type_identifier: Option<&str>) -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "data/raw/jan.csv", JAN, type_identifier).await?;
    put(&store, "data/raw/feb.csv", FEB, type_identifier).await?;
    put(&store, "data/raw/bad.csv", BAD, type_identifier).await?;
    put(
        &store,
        "data/raw/all.manifest.yaml",
        manifest(&["jan.csv", "feb.csv"]).as_bytes(),
        None,
    )
    .await?;
    put(
        &store,
        "data/raw/with_bad.manifest.yaml",
        manifest(&["jan.csv", "feb.csv", "bad.csv"]).as_bytes(),
        None,
    )
    .await?;
    Ok(store)
}

const MATERIALIZE: &str = "-R/data/raw/all.manifest.yaml/-/ns-rec/materialize";

/// The five rows, jan's then feb's, in the `uniform_schema`'s column order.
fn assert_five_rows_in_file_order(view: &dyn RecordView) -> Result<(), Error> {
    let names: Vec<&str> = view
        .schema()
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(names, vec!["month", "amount"]);
    let rows: Vec<(FieldValue, FieldValue)> = (0..view.len())
        .map(|row| Ok((view.value(row, 0)?, view.value(row, 1)?)))
        .collect::<Result<_, Error>>()?;
    let expected: Vec<(FieldValue, FieldValue)> =
        [("jan", 10), ("jan", 20), ("feb", 30), ("feb", 40), ("feb", 50)]
            .iter()
            .map(|(month, amount)| (FieldValue::Text((*month).into()), FieldValue::Int(*amount)))
            .collect();
    assert_eq!(rows, expected);
    Ok(())
}

#[tokio::test]
async fn manifest_over_stored_csv_files_materializes_in_order(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(stored_files(Some("RecordView")).await?)?;

    let state = eval(envref.clone(), MATERIALIZE).await?;
    assert_five_rows_in_file_order(state.value()?.as_record_view()?.as_ref())?;

    // The source is rewindable: the same query again reads the same rows.
    let again = eval(envref, MATERIALIZE).await?;
    assert_five_rows_in_file_order(again.value()?.as_record_view()?.as_ref())?;
    Ok(())
}

/// The chunk queries end in an action, not a filename, so the chunks are unkeyed: nothing is
/// written to the store beside the files and manifests already there.
#[tokio::test]
async fn manifest_csv_chunks_are_unkeyed() -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(stored_files(Some("RecordView")).await?)?;
    eval(envref.clone(), MATERIALIZE).await?;

    let mut listed = envref
        .get_async_store()
        .listdir(&parse_key("data/raw")?)
        .await?;
    listed.sort();
    assert_eq!(
        listed,
        vec![
            "all.manifest.yaml",
            "bad.csv",
            "feb.csv",
            "jan.csv",
            "with_bad.manifest.yaml"
        ]
    );
    Ok(())
}

/// The error a failed evaluation reports: `AssetRef::get` exposes a failed asset as a no-value
/// state, so the error comes from reading its value.
async fn evaluation_error(
    envref: EnvRef<DefaultEnvironment<Value>>,
    query: &str,
) -> Result<Error, Box<dyn std::error::Error>> {
    match eval(envref, query).await {
        Ok(state) => match state.value() {
            Ok(_) => Err(format!("{query} must fail").into()),
            Err(error) => Ok(error),
        },
        Err(error) => Ok(error),
    }
}

const MATERIALIZE_WITH_BAD: &str = "-R/data/raw/with_bad.manifest.yaml/-/ns-rec/materialize";

#[tokio::test]
async fn manifest_csv_chunk_violating_uniform_schema_fails(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(stored_files(Some("RecordView")).await?)?;
    let error = evaluation_error(envref, MATERIALIZE_WITH_BAD).await?;
    let message = format!("{error}");
    assert!(
        message.contains("'amount'") && message.contains("not null"),
        "the error must name the field and the violated constraint, got: {message}"
    );
    Ok(())
}

/// Phase 1 asks for the error to name the chunk; today it names only the field.
#[tokio::test]
#[ignore = "MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK"]
async fn manifest_csv_chunk_schema_error_names_the_chunk() -> Result<(), Box<dyn std::error::Error>>
{
    let envref = build_env(stored_files(Some("RecordView")).await?)?;
    let message = format!("{}", evaluation_error(envref, MATERIALIZE_WITH_BAD).await?);
    assert!(
        message.contains("bad.csv") || message.contains("chunk 2"),
        "the error must name the chunk, got: {message}"
    );
    Ok(())
}

/// The same directory of CSV files placed by hand, with no type identifier.
#[tokio::test]
#[ignore = "STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ"]
async fn manifest_over_hand_placed_csv_files_materializes(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = build_env(stored_files(None).await?)?;
    let state = eval(envref, MATERIALIZE).await?;
    assert_five_rows_in_file_order(state.value()?.as_record_view()?.as_ref())?;
    Ok(())
}
