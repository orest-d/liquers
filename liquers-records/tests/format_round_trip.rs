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
    assert_every_cell_equal(&read_back, &batch);
    Ok(())
}

/// Same row count, same schema width, names and types, and every cell equal — not a sample.
fn assert_every_cell_equal(actual: &RecordBatch, expected: &RecordBatch) {
    assert_eq!(actual.len, expected.len, "row count");
    assert_eq!(actual.schema.fields.len(), expected.schema.fields.len(), "schema width");
    for (a, e) in actual.schema.fields.iter().zip(expected.schema.fields.iter()) {
        assert_eq!(a.name, e.name, "field name");
        assert_eq!(a.data_type, e.data_type, "type of field '{}'", e.name);
    }
    for row in 0..expected.len {
        for col in 0..expected.schema.fields.len() {
            assert_eq!(
                actual.value(row, col).expect("cell"),
                expected.value(row, col).expect("cell"),
                "row {row}, column {col}"
            );
        }
    }
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
fn markdown_is_write_only_in_practice_but_parses_its_own_output(
) -> Result<(), Box<dyn std::error::Error>> {
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
    let bytes = write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?;
    let read_back = read_table(&bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.schema, batch.schema);
    assert_every_cell_equal(&read_back, &batch);
    Ok(())
}

/// A file written by pyarrow with several record batches, each a *slice* of one table: pyarrow
/// writes a sliced batch's buffers without truncating them to the slice, so a buffer is longer
/// than its `FieldNode.length` needs and a string batch's offsets do not start at zero. The row
/// count must come from the node, not from the buffer's byte length.
///
/// `tests/fixtures/pyarrow_multi_batch.arrow` was generated once, with pyarrow 25.0.1:
///
/// ```python
/// import datetime, pyarrow as pa
/// t = pa.table({
///     "i": pa.array([1, 2, 3, 4, 5], pa.int64()),
///     "u": pa.array([10, None, 30, 40, 18446744073709551615], pa.uint64()),
///     "f": pa.array([1.5, None, 3.5, 4.5, -0.25], pa.float64()),
///     "b": pa.array([True, False, None, True, False], pa.bool_()),
///     "s": pa.array(["a", "bb", None, "dddd", "e"], pa.string()),
///     "bin": pa.array([b"\x00", b"", b"xyz", None, b"\xff\xfe"], pa.binary()),
///     "d": pa.array([datetime.date(2020, 1, 1), None, datetime.date(1969, 12, 31),
///                    datetime.date(2026, 9, 27), datetime.date(2000, 2, 29)], pa.date32()),
///     "t": pa.array([0, 1_600_000_000_123_456, None, -1, 42], pa.timestamp("us")),
///     "v": pa.array([[1, 2], [3, 4], None, [7, 8], [9, 10]], pa.list_(pa.float32(), 2)),
/// })
/// sink = pa.BufferOutputStream()
/// with pa.ipc.new_file(sink, t.schema) as w:
///     w.write_table(t, max_chunksize=2)          # three record batches: 2 + 2 + 1 rows
/// open("pyarrow_multi_batch.arrow", "wb").write(sink.getvalue().to_pybytes())
/// ```
#[test]
#[cfg(feature = "ipc")]
fn feather_reads_a_multi_batch_pyarrow_file_row_for_row() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = include_bytes!("fixtures/pyarrow_multi_batch.arrow");
    let batch = read_table(bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(batch.len, 5, "five rows in three batches, not one row per buffer element");
    let names: Vec<&str> = batch.schema.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["i", "u", "f", "b", "s", "bin", "d", "t", "v"]);

    let text = |s: &str| FieldValue::Text(Arc::from(s));
    let bytes_of = |b: &[u8]| FieldValue::Bytes(Arc::from(b));
    let vector = |a: f32, b: f32| FieldValue::Vector(Arc::from(vec![a, b]));
    use FieldValue::{Bool, Date, Float, Int, Null, Timestamp, UInt};
    let expected: Vec<Vec<FieldValue>> = vec![
        vec![Int(1), UInt(10), Float(1.5), Bool(true), text("a"), bytes_of(b"\x00"), Date(18262), Timestamp(0), vector(1.0, 2.0)],
        vec![Int(2), Null, Null, Bool(false), text("bb"), bytes_of(b""), Null, Timestamp(1_600_000_000_123_456), vector(3.0, 4.0)],
        vec![Int(3), UInt(30), Float(3.5), Null, Null, bytes_of(b"xyz"), Date(-1), Null, Null],
        vec![Int(4), UInt(40), Float(4.5), Bool(true), text("dddd"), Null, Date(20723), Timestamp(-1), vector(7.0, 8.0)],
        vec![Int(5), UInt(u64::MAX), Float(-0.25), Bool(false), text("e"), bytes_of(b"\xff\xfe"), Date(11016), Timestamp(42), vector(9.0, 10.0)],
    ];
    for (row, cells) in expected.iter().enumerate() {
        for (col, cell) in cells.iter().enumerate() {
            assert_eq!(&batch.value(row, col)?, cell, "row {row}, column '{}'", names[col]);
        }
    }
    Ok(())
}

#[test]
#[cfg(feature = "parquet")]
fn parquet_write_succeeds_but_reading_here_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    // Writing works — a minimal, real Parquet file (§"Tier 3 — Parquet: writing is cheap, reading
    // is not"). Reading does not: `liquers-records` never reads Parquet, in any feature
    // configuration, naming `liquers-lib`'s polars bridge instead — this is not a round trip.
    let batch = sample()?;
    let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
    assert!(!bytes.is_empty());
    assert_eq!(&bytes[0..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");

    let error = read_table(&bytes, TableFormat::Parquet, ReadSchema::Infer, &ReadOptions::default())
        .expect_err("liquers-records never reads Parquet; read it through liquers-lib's polars bridge");
    let message = format!("{error}").to_lowercase();
    assert!(message.contains("polars"));
    Ok(())
}
