//! Serialization round trips for the Polars DataFrame value type.
//!
//! Gated at file level: every test here constructs a `polars` DataFrame.

#![cfg(feature = "polars")]

use liquers_core::value::DefaultValueSerializer;
use liquers_lib::value::{ExtValueInterface, Value};
use polars::df;

#[test]
fn polars_value_csv_roundtrip() {
    let df = df!["a" => &[1i32, 2i32], "b" => &["x", "y"]].unwrap();
    let value = Value::from_polars_dataframe(df);

    let bytes = value.as_bytes("csv").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "polars.DataFrame", "csv").unwrap();
    let decoded_df = decoded.as_polars_dataframe().unwrap();

    assert_eq!(decoded_df.height(), 2);
    assert_eq!(decoded_df.width(), 2);
}

#[test]
fn polars_value_parquet_roundtrip() {
    let df = df!["id" => &[1i64, 2i64, 3i64]].unwrap();
    let value = Value::from_polars_dataframe(df);

    let bytes = value.as_bytes("parquet").unwrap();
    let decoded = Value::deserialize_from_bytes(&bytes, "polars.DataFrame", "parquet").unwrap();
    let decoded_df = decoded.as_polars_dataframe().unwrap();

    assert_eq!(decoded_df.height(), 3);
    assert_eq!(decoded_df.width(), 1);
}

#[test]
fn polars_value_xlsx_returns_error() {
    let df = df!["id" => &[1i32]].unwrap();
    let value = Value::from_polars_dataframe(df);
    let err = value.as_bytes("xlsx").unwrap_err();
    assert!(err.to_string().contains("XLSX serialization"));
}

/// The four Arrow IPC spellings, as `polars.DataFrame` declares them
/// (`design/polars-dataframe-ipc/`).
const IPC_SPELLINGS: [&str; 4] = ["ipc", "feather", "arrow_ipc", "arrow"];

fn typed_frame() -> polars::prelude::DataFrame {
    df![
        "id" => &[1i64, 2, 3, 4, 5, 6],
        "amount" => &[Some(1.5f64), Some(f64::NAN), None, Some(4.0), Some(5.0), Some(6.0)],
        "label" => &["a", "b", "c", "d", "e", "f"],
        "flag" => &[true, false, true, false, true, false],
    ]
    .unwrap()
}

#[test]
fn polars_value_ipc_roundtrip_every_alias() {
    let df = typed_frame();
    let value = Value::from_polars_dataframe(df.clone());
    for format in IPC_SPELLINGS {
        let bytes = value.as_bytes(format).unwrap();
        let decoded = Value::deserialize_from_bytes(&bytes, "polars.DataFrame", format).unwrap();
        let decoded_df = decoded.as_polars_dataframe().unwrap();
        assert_eq!(decoded_df.schema(), df.schema(), "{format}: dtypes kept");
        for name in ["id", "label", "flag"] {
            assert!(
                decoded_df.column(name).unwrap().equals_missing(df.column(name).unwrap()),
                "{format}: {name}"
            );
        }
        let amount: Vec<Option<f64>> =
            decoded_df.column("amount").unwrap().f64().unwrap().iter().collect();
        assert!(amount[1].is_some_and(f64::is_nan), "{format}: NaN kept");
        assert_eq!(amount[2], None, "{format}: null kept");
    }
}

#[test]
fn polars_type_info_declares_ipc_spellings() {
    use liquers_core::value::ValueInterface;
    let value = Value::from_polars_dataframe(typed_frame());
    let info = value.type_info();
    for format in IPC_SPELLINGS {
        assert!(info.supports_data_format(format), "{format} is declared");
        assert!(value.supports_data_format(format), "{format} is accepted");
    }
}

/// A hand-placed `.feather` file (IPC bytes, `data_format: feather`, no type identifier) is the
/// input of an `ns-pl` command.
#[tokio::test]
async fn stored_feather_file_feeds_ns_pl_head() -> Result<(), Box<dyn std::error::Error>> {
    stored_file_feeds_ns_pl_head("feather").await
}

/// The same for Parquet: a stored binary file reaches the command as bytes, not through the lossy
/// text conversion that corrupted it before.
#[tokio::test]
async fn stored_parquet_file_feeds_ns_pl_head() -> Result<(), Box<dyn std::error::Error>> {
    stored_file_feeds_ns_pl_head("parquet").await
}

async fn stored_file_feeds_ns_pl_head(format: &str) -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::{
        context::Environment,
        metadata::{Metadata, MetadataRecord},
        parse::parse_key,
        query::Key,
        store::{AsyncMemoryStore, AsyncStore},
    };
    use liquers_lib::environment::{DefaultEnvironment, PolarsCommandRegistration};

    let df = typed_frame();
    let bytes = Value::from_polars_dataframe(df.clone()).as_bytes(format)?;
    let store = AsyncMemoryStore::new(&Key::new());
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some(format.to_string());
    let name = format!("data/sales.{format}");
    store
        .set(&parse_key(&name)?, &bytes, &Metadata::MetadataRecord(metadata))
        .await?;

    let mut env = DefaultEnvironment::<Value>::new();
    env.with_async_store(Box::new(store));
    env.with_default_recipe_provider();
    env.register_polars_commands()?;
    let envref = env.to_ref();

    let asset = envref.evaluate(format!("-R/{name}/-/ns-pl/head-5")).await?;
    let head = asset.get().await?.value()?.as_polars_dataframe()?;
    assert_eq!(head.height(), 5);
    assert_eq!(head.get_column_names(), df.get_column_names());
    Ok(())
}
