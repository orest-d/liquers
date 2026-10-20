//! Interop tests for `liquers-records`'s Parquet writer against polars' own, independent reader —
//! phase2-architecture.md §"Tier 3 — Parquet: writing is cheap, reading is not": a file written
//! here reads back through polars, and the `RecordBatch <-> DataFrame` bridge
//! (`liquers-lib/src/records/polars.rs`) is exercised directly, both ways.
//!
//! **Relocated from Phase 3 §3.8** (`liquers-records/src/formats/parquet.rs`, as drafted): that
//! draft gated the round-trip test on `#[cfg(feature = "polars")]` inside `liquers-records`, which
//! has no `polars` feature and must never depend on polars, so it would never have compiled.
//! `parquet_writer_refuses_vector_columns` stayed where Phase 3 put it
//! (`liquers-records/src/formats/parquet.rs`'s own test module); this file carries the interop
//! coverage instead, written out against the bridge with its `#[ignore]` removed, per
//! `specs/design/record-streams/phase4-implementation.md` Step 6.2.
//!
//! Gated on both features per Step 6.1's `records_ipc_polars.rs` precedent: a `polars`
//! `[dev-dependency]` would compile polars into *every* `--tests` build of `liquers-lib`, including
//! the reduced-feature matrix rows that exist to avoid it. Using the crate's existing optional
//! `polars` dependency instead keeps this file compiled only when both features are on.
#![cfg(all(feature = "records-parquet", feature = "polars"))]

use std::io::Cursor;
use std::sync::Arc;

use polars::prelude::{
    AnyValue, DataFrame, KeyValueMetadata, NamedFrom, ParquetReader, ParquetWriter, SerReader, Series,
    TimeUnit,
};

use liquers_core::error::Error;
use liquers_core::value::DefaultValueSerializer;
use liquers_lib::records::polars::{dataframe_to_record_batch, record_batch_to_dataframe};
use liquers_lib::records::{
    read_parquet_record_batch, write_table, FieldSchema, FieldType, FieldValue, KeyRole, ReadSchema,
    RecordBatchMut, RecordSchema, RecordView, RecordViewMut, TableFormat, WriteOptions,
};
use liquers_lib::value::{ExtValueInterface, Value};

/// The six scalar types the writer maps onto Parquet's `INT64`/`DOUBLE`/`BYTE_ARRAY(STRING)`/
/// `INT32(DATE)`/`INT64(TIMESTAMP MICROS)` (this module's own doc comment in
/// `liquers-records/src/formats/parquet.rs`), each with at least one null.
fn sample_record_schema() -> Result<Arc<RecordSchema>, Error> {
    Ok(Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("amount", FieldType::Float),
        FieldSchema::new("label", FieldType::Text),
        FieldSchema::new("active", FieldType::Bool),
        FieldSchema::new("joined", FieldType::Date),
        FieldSchema::new("created_at", FieldType::Timestamp),
    ])?))
}

fn sample_batch() -> Result<liquers_lib::records::RecordBatch, Error> {
    let schema = sample_record_schema()?;
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[
        FieldValue::Int(1),
        FieldValue::Float(1.5),
        FieldValue::Text(Arc::from("alice")),
        FieldValue::Bool(true),
        FieldValue::Date(19_700),
        FieldValue::Timestamp(123_456_789),
    ])?;
    batch.append_row(&[
        FieldValue::Null,
        FieldValue::Float(2.5),
        FieldValue::Null,
        FieldValue::Bool(false),
        FieldValue::Null,
        FieldValue::Timestamp(99),
    ])?;
    batch.append_row(&[
        FieldValue::Int(3),
        FieldValue::Null,
        FieldValue::Text(Arc::from("carol")),
        FieldValue::Null,
        FieldValue::Date(19_800),
        FieldValue::Null,
    ])?;
    batch.freeze()
}

#[test]
fn parquet_round_trip_through_polars_preserves_data() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample_batch()?;

    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
    assert_eq!(&bytes[0..4], b"PAR1");

    let df: DataFrame = ParquetReader::new(Cursor::new(bytes)).finish()?;
    assert_eq!(df.height(), 3);
    assert_eq!(df.width(), 6);

    assert_eq!(df.column("id")?.get(0)?, AnyValue::Int64(1));
    assert_eq!(df.column("id")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("id")?.get(2)?, AnyValue::Int64(3));

    assert_eq!(df.column("amount")?.get(0)?, AnyValue::Float64(1.5));
    assert_eq!(df.column("amount")?.get(1)?, AnyValue::Float64(2.5));
    assert_eq!(df.column("amount")?.get(2)?, AnyValue::Null);

    assert_eq!(df.column("label")?.get(0)?, AnyValue::String("alice"));
    assert_eq!(df.column("label")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("label")?.get(2)?, AnyValue::String("carol"));

    // `Bool` is written as Parquet's BOOLEAN, so it reads back as a boolean.
    assert_eq!(df.column("active")?.get(0)?, AnyValue::Boolean(true));
    assert_eq!(df.column("active")?.get(1)?, AnyValue::Boolean(false));
    assert_eq!(df.column("active")?.get(2)?, AnyValue::Null);

    assert_eq!(df.column("joined")?.get(0)?, AnyValue::Date(19_700));
    assert_eq!(df.column("joined")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("joined")?.get(2)?, AnyValue::Date(19_800));

    // `TIMESTAMP(isAdjustedToUTC: true, unit: MICROS)` (the writer's `LogicalType`, per its module
    // doc comment) makes polars report the column's timezone as UTC — matched by pattern rather
    // than an exact `AnyValue::Datetime(.., None)`, whose third field's `&TimeZone` this test does
    // not need to construct just to assert on the value and unit.
    match df.column("created_at")?.get(0)? {
        AnyValue::Datetime(value, TimeUnit::Microseconds, _) => assert_eq!(value, 123_456_789),
        other => panic!("expected a microsecond Datetime, got {other:?}"),
    }
    match df.column("created_at")?.get(1)? {
        AnyValue::Datetime(value, TimeUnit::Microseconds, _) => assert_eq!(value, 99),
        other => panic!("expected a microsecond Datetime, got {other:?}"),
    }
    assert_eq!(df.column("created_at")?.get(2)?, AnyValue::Null);

    Ok(())
}

#[test]
fn bridge_round_trips_record_batch_through_dataframe() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample_batch()?;

    let df = record_batch_to_dataframe(&batch)?;
    assert_eq!(df.height(), 3);
    assert_eq!(df.width(), 6);

    let back = dataframe_to_record_batch(&df)?;
    assert_eq!(back.len(), 3);

    assert_eq!(back.value(0, 0)?, FieldValue::Int(1));
    assert_eq!(back.value(1, 0)?, FieldValue::Null);
    assert_eq!(back.value(2, 0)?, FieldValue::Int(3));

    assert_eq!(back.value(0, 1)?, FieldValue::Float(1.5));
    assert_eq!(back.value(1, 1)?, FieldValue::Float(2.5));
    assert_eq!(back.value(2, 1)?, FieldValue::Null);

    assert_eq!(back.value(0, 2)?, FieldValue::Text(Arc::from("alice")));
    assert_eq!(back.value(1, 2)?, FieldValue::Null);
    assert_eq!(back.value(2, 2)?, FieldValue::Text(Arc::from("carol")));

    assert_eq!(back.value(0, 3)?, FieldValue::Bool(true));
    assert_eq!(back.value(1, 3)?, FieldValue::Bool(false));
    assert_eq!(back.value(2, 3)?, FieldValue::Null);

    assert_eq!(back.value(0, 4)?, FieldValue::Date(19_700));
    assert_eq!(back.value(1, 4)?, FieldValue::Null);
    assert_eq!(back.value(2, 4)?, FieldValue::Date(19_800));

    assert_eq!(back.value(0, 5)?, FieldValue::Timestamp(123_456_789));
    assert_eq!(back.value(1, 5)?, FieldValue::Timestamp(99));
    assert_eq!(back.value(2, 5)?, FieldValue::Null);

    // Every field lands as ordinary data — no `Id` is guessed, matching CSV's schema-less reader.
    assert_eq!(back.schema.id_field(), None);

    Ok(())
}

#[test]
fn parquet_bytes_deserialize_into_a_record_view_through_the_lib_path(
) -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample_batch()?;
    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;

    let value = Value::deserialize_from_bytes(&bytes, "RecordView", "parquet")?;
    let view = value.as_record_view()?;
    assert_eq!(view.len(), 3);
    assert_eq!(view.value(0, 0)?, FieldValue::Int(1));
    assert_eq!(view.value(1, 0)?, FieldValue::Null);
    assert_eq!(view.value(2, 2)?, FieldValue::Text(Arc::from("carol")));

    Ok(())
}

#[test]
fn read_parquet_record_batch_matches_the_lib_deserialize_path() -> Result<(), Box<dyn std::error::Error>> {
    let batch = sample_batch()?;
    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;

    let read_back = read_parquet_record_batch(&bytes, ReadSchema::Infer)?;
    assert_eq!(read_back.len(), 3);
    assert_eq!(read_back.value(0, 1)?, FieldValue::Float(1.5));
    assert_eq!(read_back.value(2, 1)?, FieldValue::Null);

    Ok(())
}

/// A `UInt` column is written as `INT64` annotated `INTEGER(64, unsigned)`, so a value above
/// `i64::MAX` reads back in polars as that `u64`, not as a negative number.
#[test]
fn parquet_uint_column_reads_back_unsigned_in_polars() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::UInt)])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::UInt(u64::MAX)])?;
    batch.append_row(&[FieldValue::UInt(7)])?;
    let batch = batch.freeze()?;

    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
    let df: DataFrame = ParquetReader::new(Cursor::new(bytes)).finish()?;
    assert_eq!(df.column("n")?.get(0)?, AnyValue::UInt64(u64::MAX));
    assert_eq!(df.column("n")?.get(1)?, AnyValue::UInt64(7));
    Ok(())
}

#[cfg(feature = "records-ipc")]
#[test]
fn timestamp_dtype_agrees_across_parquet_and_ipc() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::{DataType, IpcReader, TimeZone};

    let batch = sample_batch()?;
    let parquet = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
    let ipc = write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?;
    let from_parquet: DataFrame = ParquetReader::new(Cursor::new(parquet)).finish()?;
    let from_ipc: DataFrame = IpcReader::new(Cursor::new(ipc)).finish()?;
    let bridged = record_batch_to_dataframe(&batch)?;

    let utc = DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC));
    assert_eq!(from_parquet.column("created_at")?.dtype(), &utc);
    assert_eq!(from_ipc.column("created_at")?.dtype(), &utc);
    assert_eq!(bridged.column("created_at")?.dtype(), &utc);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The declared and the embedded schema on the Parquet read path
// (specs/design/parquet-read-declared-schema/).
// ---------------------------------------------------------------------------------------------

/// Bytes written by polars' own `ParquetWriter`, so they carry no `liquers.schema`.
fn polars_parquet(mut df: DataFrame) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    ParquetWriter::new(&mut bytes).finish(&mut df)?;
    Ok(bytes)
}

/// `id: [1, 2]`, `amount: [1.5, 2.5]`, `label: ["a", "b"]`, written by polars.
fn polars_written_sample() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    polars_parquet(DataFrame::new(
        2,
        vec![
            Series::new("id".into(), &[1i64, 2]).into(),
            Series::new("amount".into(), &[1.5f64, 2.5]).into(),
            Series::new("label".into(), &["a", "b"]).into(),
        ],
    )?)
}

/// `id` (the `Id`, not null) and `amount`, written by Liquers, so `liquers.schema` is embedded.
fn liquers_written_id_amount() -> Result<(RecordSchema, Vec<u8>), Box<dyn std::error::Error>> {
    let schema = RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id).not_null(),
        FieldSchema::new("amount", FieldType::Float),
    ])?;
    let mut batch = RecordBatchMut::with_capacity(Arc::new(schema.clone()), 2);
    batch.append_row(&[FieldValue::Int(1), FieldValue::Float(1.5)])?;
    batch.append_row(&[FieldValue::Int(2), FieldValue::Null])?;
    let bytes = write_table(&batch.freeze()?, TableFormat::Parquet, &WriteOptions::default())?;
    Ok((schema, bytes))
}

/// AC-1: the declared schema, in another order and with an `Id` and a label, is the result's
/// schema; the columns are matched by name.
#[test]
fn parquet_declared_schema_is_the_result_schema() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = polars_written_sample()?;
    let declared = RecordSchema::new(vec![
        FieldSchema::new("label", FieldType::Text).with_label("Label"),
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id).not_null(),
        FieldSchema::new("amount", FieldType::Float),
    ])?;
    let batch = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))?;
    assert_eq!(batch.schema().as_ref(), &declared);
    assert_eq!(batch.schema().id_field(), Some(1));
    assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("a")));
    assert_eq!(batch.value(1, 1)?, FieldValue::Int(2));
    assert_eq!(batch.value(1, 2)?, FieldValue::Float(2.5));
    Ok(())
}

/// AC-2: a declared type that differs from the file's is refused, naming both; nothing is cast.
#[test]
fn parquet_declared_type_mismatch_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let (_, bytes) = liquers_written_id_amount()?;
    let wrong = RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("amount", FieldType::Text),
    ])?;
    let error = read_parquet_record_batch(&bytes, ReadSchema::Declared(&wrong))
        .expect_err("amount is Float in the file");
    for part in ["amount", "Float", "Text"] {
        assert!(error.message.contains(part), "{part}: {}", error.message);
    }
    Ok(())
}

/// AC-3: a file column the declared schema does not name is refused, naming it.
#[test]
fn parquet_undeclared_column_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = polars_written_sample()?;
    let declared = RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("amount", FieldType::Float),
    ])?;
    let error = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))
        .expect_err("label is not declared");
    assert!(error.message.contains("label"), "{}", error.message);
    Ok(())
}

/// AC-4: a missing nullable field reads as nulls; a missing non-nullable or `Vector` field is
/// refused, naming it.
#[test]
fn parquet_declared_missing_nullable_field_reads_as_nulls() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = polars_written_sample()?;
    let base = || {
        vec![
            FieldSchema::new("id", FieldType::Int),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("label", FieldType::Text),
        ]
    };

    let mut fields = base();
    fields.push(FieldSchema::new("note", FieldType::Text));
    let declared = RecordSchema::new(fields)?;
    let batch = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))?;
    assert_eq!(batch.schema().as_ref(), &declared);
    assert_eq!(batch.value(0, 3)?, FieldValue::Null);
    assert_eq!(batch.value(1, 3)?, FieldValue::Null);

    let mut fields = base();
    fields.push(FieldSchema::new("note", FieldType::Text).not_null());
    let declared = RecordSchema::new(fields)?;
    let error = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))
        .expect_err("a non-nullable field cannot be missing");
    assert!(error.message.contains("note"), "{}", error.message);

    let mut fields = base();
    fields.push(FieldSchema::new("embedding", FieldType::Vector));
    let declared = RecordSchema::new(fields)?;
    let error = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))
        .expect_err("a missing Vector field has no width");
    assert!(error.message.contains("embedding"), "{}", error.message);
    Ok(())
}

/// AC-5: a field declared not null that holds a null in the file is refused.
#[test]
fn parquet_declared_not_null_with_nulls_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = polars_parquet(DataFrame::new(
        2,
        vec![Series::new("n".into(), &[Some(1i64), None]).into()],
    )?)?;
    let declared = RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int).not_null()])?;
    let error = read_parquet_record_batch(&bytes, ReadSchema::Declared(&declared))
        .expect_err("n holds a null");
    assert!(error.message.contains("'n'"), "{}", error.message);
    assert!(error.message.contains('1'), "{}", error.message);
    Ok(())
}

/// AC-6: `Infer` restores the embedded `liquers.schema` of a file Liquers wrote — every scalar
/// type, an `Id`, a label and a not-null field.
#[test]
fn parquet_infer_restores_embedded_liquers_schema() -> Result<(), Box<dyn std::error::Error>> {
    let schema = RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id).not_null(),
        FieldSchema::new("n", FieldType::UInt),
        FieldSchema::new("amount", FieldType::Float).with_label("Amount"),
        FieldSchema::new("label", FieldType::Text),
        FieldSchema::new("active", FieldType::Bool),
        FieldSchema::new("raw", FieldType::Binary),
        FieldSchema::new("joined", FieldType::Date),
        FieldSchema::new("created_at", FieldType::Timestamp),
    ])?;
    let mut batch = RecordBatchMut::with_capacity(Arc::new(schema.clone()), 1);
    batch.append_row(&[
        FieldValue::Int(1),
        FieldValue::UInt(7),
        FieldValue::Float(1.5),
        FieldValue::Text(Arc::from("a")),
        FieldValue::Bool(true),
        FieldValue::Bytes(Arc::from(&b"xy"[..])),
        FieldValue::Date(19_700),
        FieldValue::Timestamp(99),
    ])?;
    let bytes = write_table(&batch.freeze()?, TableFormat::Parquet, &WriteOptions::default())?;

    let read_back = read_parquet_record_batch(&bytes, ReadSchema::Infer)?;
    assert_eq!(read_back.schema().as_ref(), &schema);
    assert_eq!(read_back.schema().id_field(), Some(0));
    assert_eq!(read_back.value(0, 1)?, FieldValue::UInt(7));

    // The lib deserialization path reads with `Infer` too.
    let value = Value::deserialize_from_bytes(&bytes, "RecordView", "parquet")?;
    assert_eq!(value.as_record_view()?.schema().as_ref(), &schema);
    Ok(())
}

/// AC-7: a file with no `liquers.schema` gets the schema derived from the polars dtypes.
#[test]
fn parquet_infer_without_liquers_schema_derives_from_dtypes() -> Result<(), Box<dyn std::error::Error>> {
    let read_back = read_parquet_record_batch(&polars_written_sample()?, ReadSchema::Infer)?;
    let expected = RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int),
        FieldSchema::new("amount", FieldType::Float),
        FieldSchema::new("label", FieldType::Text),
    ])?;
    assert_eq!(read_back.schema().as_ref(), &expected);
    assert_eq!(read_back.schema().id_field(), None);
    Ok(())
}

/// AC-7: a `liquers.schema` that no longer describes the columns (one was renamed by another tool
/// that kept the metadata), or that does not parse, is ignored.
#[test]
fn parquet_infer_ignores_stale_liquers_schema() -> Result<(), Box<dyn std::error::Error>> {
    let (schema, bytes) = liquers_written_id_amount()?;
    let schema_json = serde_json::to_string(&schema)?;
    let mut df: DataFrame = ParquetReader::new(Cursor::new(bytes)).finish()?;
    df.rename("amount", "total".into())?;

    for metadata in [schema_json, "{not json".to_string()] {
        let mut bytes = Vec::new();
        ParquetWriter::new(&mut bytes)
            .with_key_value_metadata(Some(KeyValueMetadata::from_static(vec![(
                "liquers.schema".to_string(),
                metadata,
            )])))
            .finish(&mut df.clone())?;
        let read_back = read_parquet_record_batch(&bytes, ReadSchema::Infer)?;
        let names: Vec<&str> = read_back.schema().fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["id", "total"]);
        assert_eq!(read_back.schema().id_field(), None);
    }
    Ok(())
}

type CommandEnvironment = liquers_lib::environment::DefaultEnvironment<Value>;

/// AC-8: `ns-rec/to_record` reading Parquet bytes with a `schema` argument is strict.
#[tokio::test]
async fn to_record_parquet_with_schema_option_is_strict() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::context::Environment;
    use liquers_core::metadata::{Metadata, Status};
    use liquers_core::parse::parse_key;
    use liquers_core::query::Key;
    use liquers_core::store::{AsyncMemoryStore, AsyncStore};
    use liquers_lib::environment::CommandRegistryAccess;
    // What `register_records_commands!` expands to names these unqualified.
    #[allow(unused_imports)]
    use liquers_core::{
        command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
        commands::CommandRegistry,
        context::Context,
    };
    #[allow(unused_imports)]
    use liquers_lib::value::simple::SimpleValue;

    let (_, bytes) = liquers_written_id_amount()?;
    let store = AsyncMemoryStore::new(&Key::new());
    let put = |key: &str, data: Vec<u8>| {
        let store = &store;
        let key = parse_key(key);
        async move {
            let key = key?;
            let mut metadata = Metadata::new();
            metadata.set_filename(key.filename().expect("filename").encode().as_ref())?;
            metadata.set_status(Status::Source)?;
            store.set(&key, &data, &metadata).await
        }
    };
    put("data/sales.parquet", bytes).await?;
    // `.txt`, so each schema is linked into the argument as text (a plain `.yaml` file would be
    // read as an object).
    put(
        "data/good.schema.txt",
        b"fields:\n  - name: amount\n    data_type: Float\n  - name: id\n    data_type: Int\n".to_vec(),
    )
    .await?;
    put(
        "data/bad.schema.txt",
        b"fields:\n  - name: id\n    data_type: Int\n  - name: amount\n    data_type: Text\n".to_vec(),
    )
    .await?;

    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        liquers_lib::register_records_commands!(cr)?;
    }
    env.with_async_store(Box::new(store));
    let envref = env.to_ref();

    let state = envref
        .evaluate("-R/data/sales.parquet/-/ns-rec/to_record-parquet-t-~X~-R/data/good.schema.txt~E")
        .await?
        .get()
        .await?;
    let view = state.value()?.as_record_view()?;
    let names: Vec<&str> = view.schema().fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["amount", "id"]);
    assert_eq!(view.value(0, 1)?, FieldValue::Int(1));

    // A failed evaluation may surface as an `Err` or as an error state; either way it names the
    // field.
    let failed = match envref
        .evaluate("-R/data/sales.parquet/-/ns-rec/to_record-parquet-t-~X~-R/data/bad.schema.txt~E")
        .await
    {
        Ok(asset) => match asset.get().await {
            Ok(state) => {
                assert!(state.is_error()?, "amount is Float in the file");
                format!("{:?}", state.metadata)
            }
            Err(error) => error.message.clone(),
        },
        Err(error) => error.message.clone(),
    };
    assert!(failed.contains("declared field 'amount' is Text"), "{failed}");
    Ok(())
}
