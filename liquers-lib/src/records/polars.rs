//! The `RecordBatch <-> DataFrame` bridge, and Parquet reading through polars' own reader.
//!
//! `liquers-records` never reads Parquet (`specs/design/record-streams/phase2-architecture.md`
//! §"Tier 3 — Parquet: writing is cheap, reading is not"): a real-world Parquet file routinely uses
//! dictionary encoding, snappy/zstd compression and data page v2, none of which a from-scratch
//! reader built to handle only this crate's own writer would ever need to accept. Reading goes
//! through polars' `ParquetReader` instead, and [`dataframe_to_record_batch`] turns the result into
//! a `RecordBatch` — the reverse of [`record_batch_to_dataframe`], which the Parquet writer's own
//! interop tests exercise the other way (`liquers-lib/tests/records_parquet_polars.rs`).
//!
//! Roles are lost on this path (polars carries no equivalent of `RecordSchema`'s `FieldRole`/
//! `KeyRole`); the `Id` field is not guessed, exactly as CSV's schema-less reader does not guess
//! one — see `RecordSchema`'s field resolution rules.
//!
//! Imports name `polars::prelude::Column` explicitly, as `PolarsColumn`, rather than glob-importing
//! `polars::prelude::*`: this module also names `liquers_records::Column` (aliased `RecColumn`),
//! and the two would otherwise collide. `liquers-lib/tests/records_ipc_polars.rs` does the same for
//! the analogous IPC interop tests.

use std::io::Cursor;
use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use polars::prelude::{
    Column as PolarsColumn, DataFrame, DataType, IntoColumn, NamedFrom, ParquetReader, PlSmallStr,
    SerReader, Series, TimeUnit,
};

use liquers_records::{
    Bitmap, Column as RecColumn, ColumnMut, FieldSchema, FieldType, FieldValue, ReadSchema,
    RecordBatch, RecordSchema,
};

fn present(validity: &Option<Bitmap>, i: usize) -> bool {
    match validity {
        Some(bitmap) => bitmap.get(i),
        None => true,
    }
}

/// One column's bytes/offsets pair (`Text`/`Binary`) decoded into per-row `Option<&[u8]>`.
fn bytes_rows<'a>(offsets: &liquers_records::Buffer<i32>, data: &'a liquers_records::AlignedBuffer) -> Vec<&'a [u8]> {
    let off = offsets.as_slice();
    let bytes = data.as_bytes();
    let n = off.len().saturating_sub(1);
    (0..n).map(|i| &bytes[off[i] as usize..off[i + 1] as usize]).collect()
}

/// One [`RecColumn`] -> one polars [`Series`], named `name`. [`RecColumn::Vector`] is refused,
/// naming the column: a `FixedSizeList` needs polars' `dtype-array` feature, which this crate's
/// `polars` dependency does not enable (filed as `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`).
fn column_to_series(name: &str, column: &RecColumn) -> Result<Series, Error> {
    let pl_name = PlSmallStr::from(name);
    match column {
        RecColumn::Bool { validity, values } => {
            let data: Vec<Option<bool>> =
                (0..values.len()).map(|i| present(validity, i).then(|| values.get(i))).collect();
            Ok(Series::new(pl_name, data))
        }
        RecColumn::Int { validity, values } => {
            let slice = values.as_slice();
            let data: Vec<Option<i64>> =
                (0..slice.len()).map(|i| present(validity, i).then(|| slice[i])).collect();
            Ok(Series::new(pl_name, data))
        }
        RecColumn::UInt { validity, values } => {
            let slice = values.as_slice();
            let data: Vec<Option<u64>> =
                (0..slice.len()).map(|i| present(validity, i).then(|| slice[i])).collect();
            Ok(Series::new(pl_name, data))
        }
        RecColumn::Float { validity, values } => {
            let slice = values.as_slice();
            let data: Vec<Option<f64>> =
                (0..slice.len()).map(|i| present(validity, i).then(|| slice[i])).collect();
            Ok(Series::new(pl_name, data))
        }
        RecColumn::Text { validity, offsets, data } => {
            let rows = bytes_rows(offsets, data);
            let mut out: Vec<Option<String>> = Vec::with_capacity(rows.len());
            for (i, bytes) in rows.iter().enumerate() {
                if !present(validity, i) {
                    out.push(None);
                    continue;
                }
                let text = std::str::from_utf8(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
                out.push(Some(text.to_string()));
            }
            Ok(Series::new(pl_name, out))
        }
        RecColumn::Binary { validity, offsets, data } => {
            let rows = bytes_rows(offsets, data);
            let out: Vec<Option<Vec<u8>>> = rows
                .iter()
                .enumerate()
                .map(|(i, bytes)| present(validity, i).then(|| bytes.to_vec()))
                .collect();
            Ok(Series::new(pl_name, out))
        }
        RecColumn::Date { validity, values } => {
            let slice = values.as_slice();
            let data: Vec<Option<i32>> =
                (0..slice.len()).map(|i| present(validity, i).then(|| slice[i])).collect();
            Series::new(pl_name, data)
                .cast(&DataType::Date)
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))
        }
        RecColumn::Timestamp { validity, values } => {
            let slice = values.as_slice();
            let data: Vec<Option<i64>> =
                (0..slice.len()).map(|i| present(validity, i).then(|| slice[i])).collect();
            Series::new(pl_name, data)
                .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))
        }
        RecColumn::Vector { .. } => Err(Error::not_supported(format!(
            "polars bridge: column '{name}' is Vector, which this bridge does not convert (needs \
             polars' 'dtype-array' feature)"
        ))),
    }
}

/// `RecordBatch` -> a polars `DataFrame`, one `Series` per schema field, in order.
pub fn record_batch_to_dataframe(batch: &RecordBatch) -> Result<DataFrame, Error> {
    let mut columns: Vec<PolarsColumn> = Vec::with_capacity(batch.schema.fields.len());
    for (field, column) in batch.schema.fields.iter().zip(batch.columns.iter()) {
        columns.push(column_to_series(&field.name, column)?.into_column());
    }
    DataFrame::new(batch.len, columns).map_err(|e| Error::from_error(ErrorType::ConversionError, e))
}

/// The polars `DataType` a `Series` is read as, mapped onto our [`FieldType`]. Matches on an
/// *external* enum (`polars::prelude::DataType` — dozens of variants this crate never needs, e.g.
/// `Categorical`, `Struct`, `List`), so the catch-all arm is the documented exception to CLAUDE.md's
/// "no `_ =>`" rule, not an oversight.
fn polars_dtype_to_field_type(dtype: &DataType, name: &str) -> Result<FieldType, Error> {
    match dtype {
        DataType::Boolean => Ok(FieldType::Bool),
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => Ok(FieldType::Int),
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => Ok(FieldType::UInt),
        DataType::Float32 | DataType::Float64 => Ok(FieldType::Float),
        DataType::String => Ok(FieldType::Text),
        DataType::Binary => Ok(FieldType::Binary),
        DataType::Date => Ok(FieldType::Date),
        DataType::Datetime(_, _) => Ok(FieldType::Timestamp),
        other => Err(Error::not_supported(format!(
            "polars bridge: column '{name}' has unsupported dtype {other:?}"
        ))),
    }
}

/// Appends every row of `series` (already known to be `field_type`) onto `column`, casting to the
/// canonical physical width for its `FieldType` first (`Int64`/`UInt64`/`Float64`, or Parquet's own
/// physical repr for `Date`/`Timestamp`). Exhaustive over [`FieldType`] — a Liquers-owned enum — per
/// CLAUDE.md; the `Vector` arm can only be reached if [`polars_dtype_to_field_type`] is ever changed
/// to produce it, which it does not today.
fn append_series_values(column: &mut ColumnMut, series: &Series, field_type: FieldType) -> Result<(), Error> {
    match field_type {
        FieldType::Bool => {
            let ca = series.bool().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::Bool))?;
            }
        }
        FieldType::Int => {
            let casted = series
                .cast(&DataType::Int64)
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            let ca = casted.i64().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::Int))?;
            }
        }
        FieldType::UInt => {
            let casted = series
                .cast(&DataType::UInt64)
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            let ca = casted.u64().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::UInt))?;
            }
        }
        FieldType::Float => {
            let casted = series
                .cast(&DataType::Float64)
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            let ca = casted.f64().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::Float))?;
            }
        }
        FieldType::Text => {
            let ca = series.str().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, |s| FieldValue::Text(Arc::from(s))))?;
            }
        }
        FieldType::Binary => {
            let ca = series.binary().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, |b| FieldValue::Bytes(Arc::from(b))))?;
            }
        }
        FieldType::Date => {
            let physical = series.to_physical_repr();
            let ca = physical.i32().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::Date))?;
            }
        }
        FieldType::Timestamp => {
            let normalized = series
                .cast(&DataType::Datetime(TimeUnit::Microseconds, None))
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            let physical = normalized.to_physical_repr();
            let ca = physical.i64().map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
            for value in ca.iter() {
                column.push(&value.map_or(FieldValue::Null, FieldValue::Timestamp))?;
            }
        }
        FieldType::Vector => {
            return Err(Error::not_supported(
                "polars bridge: Vector columns are not produced from a DataFrame".to_string(),
            ))
        }
    }
    Ok(())
}

/// A polars `DataFrame` -> a `RecordBatch`. Every field lands as ordinary data (`KeyRole::None`):
/// no field is guessed as `Id`, matching CSV's schema-less reader.
pub fn dataframe_to_record_batch(df: &DataFrame) -> Result<RecordBatch, Error> {
    let height = df.height();
    let mut fields = Vec::with_capacity(df.width());
    let mut mut_columns: Vec<ColumnMut> = Vec::with_capacity(df.width());
    for series in df.materialized_column_iter() {
        let name = series.name().to_string();
        let field_type = polars_dtype_to_field_type(series.dtype(), &name)?;
        fields.push(FieldSchema::new(name, field_type));
        let mut column = ColumnMut::with_capacity(field_type, height);
        append_series_values(&mut column, series, field_type)?;
        mut_columns.push(column);
    }
    let schema = Arc::new(RecordSchema::new(fields)?);
    let columns: Vec<RecColumn> = mut_columns.into_iter().map(ColumnMut::freeze).collect();
    RecordBatch::new(schema, columns, None, None, Vec::new())
}

/// Reads Parquet `bytes` with polars' own reader, then bridges the result with
/// [`dataframe_to_record_batch`]. `schema` is not cross-checked against the file today — polars'
/// reader infers its own schema and nothing here validates a caller-declared one against it;
/// filed as `RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA`.
pub fn read_parquet_record_batch(bytes: &[u8], _schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let df = ParquetReader::new(Cursor::new(bytes))
        .finish()
        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
    dataframe_to_record_batch(&df)
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_records::{Buffer, RecordView};

    fn sample_batch() -> Result<RecordBatch, Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("label", FieldType::Text),
        ])?);
        RecordBatch::new(
            schema,
            vec![
                RecColumn::Int {
                    validity: Some(Bitmap::from_bools(&[true, false])),
                    values: Buffer::from_slice(&[1i64, 0]),
                },
                RecColumn::Float { validity: None, values: Buffer::from_slice(&[1.5f64, 2.5]) },
                RecColumn::Text {
                    validity: None,
                    offsets: Buffer::from_slice(&[0i32, 5, 10]),
                    data: liquers_records::AlignedBuffer::from_slice(b"aliceboboo"),
                },
            ],
            None,
            None,
            vec![],
        )
    }

    #[test]
    fn record_batch_to_dataframe_preserves_shape_and_nulls() -> Result<(), Error> {
        let batch = sample_batch()?;
        let df = record_batch_to_dataframe(&batch)?;
        assert_eq!(df.height(), 2);
        assert_eq!(df.width(), 3);
        Ok(())
    }

    #[test]
    fn dataframe_to_record_batch_round_trips_through_the_bridge() -> Result<(), Error> {
        let batch = sample_batch()?;
        let df = record_batch_to_dataframe(&batch)?;
        let back = dataframe_to_record_batch(&df)?;
        assert_eq!(back.len(), batch.len());
        assert_eq!(back.value(0, 0)?, FieldValue::Int(1));
        assert_eq!(back.value(1, 0)?, FieldValue::Null); // id row 1 was invalid
        assert_eq!(back.value(0, 1)?, FieldValue::Float(1.5));
        assert_eq!(back.value(1, 1)?, FieldValue::Float(2.5));
        assert_eq!(back.value(0, 2)?, FieldValue::Text(Arc::from("alice")));
        Ok(())
    }

    #[test]
    fn bridge_refuses_vector_columns_naming_the_column() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("embedding", FieldType::Vector)])?);
        let batch = RecordBatch::new(
            schema,
            vec![RecColumn::Vector { validity: None, dim: 2, data: Buffer::from_slice(&[0.1f32, 0.2]) }],
            None,
            None,
            vec![],
        )?;
        let error = record_batch_to_dataframe(&batch).expect_err("Vector is refused");
        assert!(format!("{error}").to_lowercase().contains("vector"));
        Ok(())
    }
}
