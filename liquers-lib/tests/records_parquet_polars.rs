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

use polars::prelude::{AnyValue, DataFrame, ParquetReader, SerReader, TimeUnit};

use liquers_core::error::Error;
use liquers_core::value::DefaultValueSerializer;
use liquers_lib::records::polars::{dataframe_to_record_batch, record_batch_to_dataframe};
use liquers_lib::records::{
    read_parquet_record_batch, write_table, FieldSchema, FieldType, FieldValue, ReadSchema,
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
