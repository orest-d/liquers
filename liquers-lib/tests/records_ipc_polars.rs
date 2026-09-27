//! Interop tests for `liquers-records`'s Arrow IPC (Feather v2) writer/reader against polars' own,
//! independent implementation — phase2-architecture.md §"Tier 2 — Arrow IPC file (Feather v2)":
//! "Interop is tested, not assumed: with `records-ipc` and `polars` both on, a file written here
//! reads in polars and one written by polars reads here."
//!
//! Gated on both features per `specs/design/record-streams/phase4-implementation.md` Step 6.1: a
//! `polars` entry in `[dev-dependencies]` would compile polars into *every* `--tests` build of
//! `liquers-lib`, including the reduced-feature matrix rows that exist to avoid it. Using the
//! crate's existing optional `polars` dependency (with `ipc` added to its feature list) instead
//! keeps this file compiled only when both features are on.
#![cfg(all(feature = "records-ipc", feature = "polars"))]

use std::io::Cursor;
use std::sync::Arc;

use polars::prelude::{
    AnyValue, Column as PolarsColumn, DataFrame, DataType, IpcReader, IpcWriter, NamedFrom, SerReader, SerWriter,
    Series, TimeUnit,
};

use liquers_lib::records::{
    read_table, write_table, FieldSchema, FieldType, FieldValue, ReadOptions, ReadSchema, RecordBatchMut,
    RecordSchema, RecordView, RecordViewMut, TableFormat, WriteOptions,
};
use liquers_core::error::Error;

/// The six scalar types `write_ipc`/`read_ipc` map onto Arrow's `Int64`/`Float64`/`Utf8`/`Bool`/
/// `Date32`/`Timestamp(µs)` (phase2-architecture.md §"Where our layout meets Arrow's"), each with
/// at least one null, built with `liquers-records` directly.
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

#[test]
fn ipc_written_by_records_reads_in_polars() -> Result<(), Box<dyn std::error::Error>> {
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
    let batch = batch.freeze()?;

    let bytes = write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?;

    let df: DataFrame = IpcReader::new(Cursor::new(bytes)).finish()?;
    assert_eq!(df.height(), 3);

    assert_eq!(df.column("id")?.get(0)?, AnyValue::Int64(1));
    assert_eq!(df.column("id")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("id")?.get(2)?, AnyValue::Int64(3));

    assert_eq!(df.column("amount")?.get(0)?, AnyValue::Float64(1.5));
    assert_eq!(df.column("amount")?.get(1)?, AnyValue::Float64(2.5));
    assert_eq!(df.column("amount")?.get(2)?, AnyValue::Null);

    assert_eq!(df.column("label")?.get(0)?, AnyValue::String("alice"));
    assert_eq!(df.column("label")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("label")?.get(2)?, AnyValue::String("carol"));

    assert_eq!(df.column("active")?.get(0)?, AnyValue::Boolean(true));
    assert_eq!(df.column("active")?.get(1)?, AnyValue::Boolean(false));
    assert_eq!(df.column("active")?.get(2)?, AnyValue::Null);

    assert_eq!(df.column("joined")?.get(0)?, AnyValue::Date(19_700));
    assert_eq!(df.column("joined")?.get(1)?, AnyValue::Null);
    assert_eq!(df.column("joined")?.get(2)?, AnyValue::Date(19_800));

    assert_eq!(df.column("created_at")?.get(0)?, AnyValue::Datetime(123_456_789, TimeUnit::Microseconds, None));
    assert_eq!(df.column("created_at")?.get(1)?, AnyValue::Datetime(99, TimeUnit::Microseconds, None));
    assert_eq!(df.column("created_at")?.get(2)?, AnyValue::Null);

    Ok(())
}

#[test]
fn ipc_written_by_polars_reads_in_records() -> Result<(), Box<dyn std::error::Error>> {
    // No `label: Text` column here (unlike the other direction): `polars::prelude::DataType::
    // String` cannot be written as plain, 32-bit-offset `Utf8` by *any* `IpcWriter` setting —
    // `CompatLevel::newest()` (the default) writes `Utf8View`, `CompatLevel::oldest()` writes
    // `LargeUtf8` — and this reader accepts neither (`Utf8View` is outside the design's accepted
    // subset; `LargeUtf8` is explicitly excluded by it). Filed as
    // `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN`. Int/Float/Bool/Date/Timestamp are
    // unaffected and covered here; `ipc_written_by_records_reads_in_polars` above covers Text in
    // the other direction, since this crate's own writer emits plain `Utf8`.
    let id = Series::new("id".into(), [Some(1i64), None, Some(3i64)]);
    let amount = Series::new("amount".into(), [Some(1.5f64), Some(2.5), None]);
    let active = Series::new("active".into(), [Some(true), Some(false), None]);
    let joined = Series::new("joined".into(), [Some(19_700i32), None, Some(19_800i32)])
        .cast(&DataType::Date)?;
    let created_at = Series::new("created_at".into(), [Some(123_456_789i64), Some(99), None])
        .cast(&DataType::Datetime(TimeUnit::Microseconds, None))?;

    let columns: Vec<PolarsColumn> = vec![id.into(), amount.into(), active.into(), joined.into(), created_at.into()];
    let mut df = DataFrame::new(3, columns)?;

    // polars' default `IpcWriter` writes no compression — decision 2's "polars' default IpcWriter
    // is uncompressed" — so this file is one `read_ipc` must accept.
    let mut buf: Vec<u8> = Vec::new();
    IpcWriter::new(&mut buf).finish(&mut df)?;

    let read_back = read_table(&buf, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.len(), 3);

    assert_eq!(read_back.value(0, 0)?, FieldValue::Int(1));
    assert_eq!(read_back.value(1, 0)?, FieldValue::Null);
    assert_eq!(read_back.value(2, 0)?, FieldValue::Int(3));

    assert_eq!(read_back.value(0, 1)?, FieldValue::Float(1.5));
    assert_eq!(read_back.value(1, 1)?, FieldValue::Float(2.5));
    assert_eq!(read_back.value(2, 1)?, FieldValue::Null);

    assert_eq!(read_back.value(0, 2)?, FieldValue::Bool(true));
    assert_eq!(read_back.value(1, 2)?, FieldValue::Bool(false));
    assert_eq!(read_back.value(2, 2)?, FieldValue::Null);

    assert_eq!(read_back.value(0, 3)?, FieldValue::Date(19_700));
    assert_eq!(read_back.value(1, 3)?, FieldValue::Null);
    assert_eq!(read_back.value(2, 3)?, FieldValue::Date(19_800));

    assert_eq!(read_back.value(0, 4)?, FieldValue::Timestamp(123_456_789));
    assert_eq!(read_back.value(1, 4)?, FieldValue::Timestamp(99));
    assert_eq!(read_back.value(2, 4)?, FieldValue::Null);

    Ok(())
}

/// **Not run routinely** (`#[ignore]`): writes the two IPC fixtures Phase 3 §3.7's dictionary/
/// compression refusal tests need — a compressed body (`IpcWriter::with_compression`) and a
/// dictionary-encoded column (a `Categorical` cast) — into `liquers-records/tests/fixtures/`. A
/// routine test must not write into the source tree, so this runs once, by hand:
/// `cargo test -p liquers-lib --test records_ipc_polars -- --ignored`.
#[test]
#[ignore = "writes fixtures into the source tree; run once by hand with `-- --ignored`"]
fn generate_dictionary_and_compressed_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;

    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../liquers-records/tests/fixtures");
    std::fs::create_dir_all(&fixtures_dir)?;

    // ---- Compressed body ----
    let mut df = DataFrame::new(1, vec![Series::new("id".into(), [1i64]).into()])?;
    let mut buf = Vec::new();
    IpcWriter::new(&mut buf)
        .with_compression(Some(polars::prelude::IpcCompression::LZ4))
        .finish(&mut df)?;
    std::fs::write(fixtures_dir.join("compressed_body.ipc"), &buf)?;

    // ---- Dictionary-encoded column (a `Categorical` cast writes dictionary-encoded IPC) ----
    let categories = polars::prelude::Categories::global();
    let mapping = categories.mapping();
    let category =
        Series::new("category".into(), ["a", "b", "a"]).cast(&DataType::Categorical(categories, mapping))?;
    let mut df = DataFrame::new(3, vec![category.into()])?;
    let mut buf = Vec::new();
    IpcWriter::new(&mut buf).finish(&mut df)?;
    std::fs::write(fixtures_dir.join("dictionary_encoded.ipc"), &buf)?;

    Ok(())
}
