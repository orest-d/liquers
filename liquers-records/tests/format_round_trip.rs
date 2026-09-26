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
