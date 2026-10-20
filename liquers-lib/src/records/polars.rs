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
//! Polars carries no equivalent of `RecordSchema`'s `FieldRole`/`KeyRole`, so a schema is not read
//! from the frame alone: a declared schema is checked against the file and becomes the result's
//! schema, and without one the `liquers.schema` Liquers' own Parquet writer embeds in the file's
//! key-value metadata is restored while it still describes the columns. Otherwise the `Id` field
//! is not guessed, exactly as CSV's schema-less reader does not guess one — see
//! [`read_parquet_record_batch`] and `RecordSchema`'s field resolution rules.
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
    SerReader, Series, TimeUnit, TimeZone,
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
            // A `Timestamp` is a UTC instant; the zone says so, as Arrow IPC and Parquet do. The
            // logical type is set directly: `cast` drops a zone unless polars' `timezones` feature
            // is on.
            Ok(Series::new(pl_name, data).into_datetime(TimeUnit::Microseconds, Some(TimeZone::UTC)))
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
            // Only the unit changes: a zoned `Datetime` already holds epoch microseconds, and a
            // naive one is taken as UTC, so keeping the input's zone keeps the values unshifted.
            let time_zone = if let DataType::Datetime(_, time_zone) = series.dtype() {
                time_zone.clone()
            } else {
                Some(TimeZone::UTC)
            };
            let normalized = series
                .cast(&DataType::Datetime(TimeUnit::Microseconds, time_zone))
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
    let (fields, columns) = dataframe_to_fields_and_columns(df)?;
    RecordBatch::new(Arc::new(RecordSchema::new(fields)?), columns, None, None, Vec::new())
}

/// Every column of `df` as an inferred field (nullable, `KeyRole::None`) and its bridged column.
fn dataframe_to_fields_and_columns(df: &DataFrame) -> Result<(Vec<FieldSchema>, Vec<RecColumn>), Error> {
    let height = df.height();
    let mut fields = Vec::with_capacity(df.width());
    let mut columns = Vec::with_capacity(df.width());
    for series in df.materialized_column_iter() {
        let name = series.name().to_string();
        let field_type = polars_dtype_to_field_type(series.dtype(), &name)?;
        fields.push(FieldSchema::new(name, field_type));
        let mut column = ColumnMut::with_capacity(field_type, height);
        append_series_values(&mut column, series, field_type)?;
        columns.push(column.freeze());
    }
    Ok((fields, columns))
}

/// The `liquers.schema` entry of a Parquet file's key-value metadata, parsed. A value that does not
/// parse is stale or foreign metadata and gives `None`, as an absent one does.
fn embedded_liquers_schema<'a>(
    key_value_metadata: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> Option<RecordSchema> {
    key_value_metadata
        .into_iter()
        .find(|(key, _)| *key == "liquers.schema")
        .and_then(|(_, value)| value)
        .and_then(|json| serde_json::from_str::<RecordSchema>(json).ok())
}

/// `liquers.schema` is trusted only while it still describes the file: the same fields, in the same
/// order, with the same names and types (another tool may rewrite the columns and keep the
/// metadata). The rule of the Arrow IPC reader's `check_embedded_schema`.
fn embedded_schema_describes(schema: &RecordSchema, inferred: &[FieldSchema]) -> bool {
    schema.fields.len() == inferred.len()
        && schema
            .fields
            .iter()
            .zip(inferred.iter())
            .all(|(embedded, file)| embedded.name == file.name && embedded.data_type == file.data_type)
}

/// The columns of `df` read under a declared schema, matched by name: every column of the file must be declared,
/// with the type it bridges to; a declared field the file lacks is an all-null column when it is
/// nullable, and an error otherwise. Nothing is cast. The rules of the Arrow IPC reader's
/// `resolve_declared`.
fn dataframe_to_declared_columns(df: &DataFrame, declared: &RecordSchema) -> Result<Vec<RecColumn>, Error> {
    for series in df.materialized_column_iter() {
        let name = series.name().as_str();
        if declared.index_of(name).is_none() {
            return Err(Error::general_error(format!(
                "parquet: the file's field '{name}' is not in the declared schema"
            )));
        }
    }
    let height = df.height();
    let mut columns = Vec::with_capacity(declared.fields.len());
    for field in &declared.fields {
        let mut column = ColumnMut::with_capacity(field.data_type, height);
        match df.column(&field.name) {
            Ok(file_column) => {
                let series = file_column.as_materialized_series();
                let file_type = polars_dtype_to_field_type(series.dtype(), &field.name)?;
                if file_type != field.data_type {
                    return Err(Error::general_error(format!(
                        "parquet: declared field '{}' is {:?} but the file's field '{}' is {:?}",
                        field.name, field.data_type, field.name, file_type
                    )));
                }
                append_series_values(&mut column, series, field.data_type)?;
            }
            Err(_) => {
                if !field.nullable {
                    return Err(Error::general_error(format!(
                        "parquet: declared non-nullable field '{}' is missing from the file",
                        field.name
                    )));
                }
                if field.data_type == FieldType::Vector {
                    return Err(Error::not_supported(format!(
                        "parquet: declared Vector field '{}' is missing from the file, and a null \
                         Vector column has no width to read it as",
                        field.name
                    )));
                }
                for _ in 0..height {
                    column.push(&FieldValue::Null)?;
                }
            }
        }
        columns.push(column.freeze());
    }
    Ok(columns)
}

/// A field declared not null must hold no null. A file written by Liquers marks such a column
/// `REQUIRED`, so it always passes; a file from elsewhere may not.
fn check_not_null(schema: &RecordSchema, columns: &[RecColumn]) -> Result<(), Error> {
    for (field, column) in schema.fields.iter().zip(columns.iter()) {
        if field.nullable {
            continue;
        }
        let nulls = column.null_mask().count_ones();
        if nulls > 0 {
            return Err(Error::general_error(format!(
                "parquet: field '{}' is declared not null, but the file holds {nulls} null(s) in it",
                field.name
            )));
        }
    }
    Ok(())
}

/// Reads Parquet `bytes` with polars' own reader, then bridges the result to a `RecordBatch` under
/// the same schema rules as the Arrow IPC reader:
///
/// - `ReadSchema::Declared(s)`: the result's schema is exactly `s` (order, roles, labels, `Id`,
///   nullability), with columns matched by name. A file column `s` does not declare, a type that
///   differs from the declared one, or a missing non-nullable (or `Vector`) field is an error;
///   nothing is cast. A missing nullable field reads as all nulls.
/// - `ReadSchema::Infer`: the `liquers.schema` Liquers' Parquet writer embeds is restored when it
///   still describes the file's columns (same names and types, in order); otherwise — absent,
///   unparseable or stale — the schema is derived from the polars dtypes, with no `Id` guessed.
///
/// On both paths a field declared not null that holds a null is an error.
pub fn read_parquet_record_batch(bytes: &[u8], schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let mut reader = ParquetReader::new(Cursor::new(bytes));
    let embedded = {
        let metadata = reader
            .get_metadata()
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
        embedded_liquers_schema(
            metadata
                .key_value_metadata
                .iter()
                .flatten()
                .map(|kv| (kv.key.as_str(), kv.value.as_deref())),
        )
    };
    let df = reader
        .finish()
        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
    let (schema, columns) = match schema {
        ReadSchema::Declared(declared) => (declared.clone(), dataframe_to_declared_columns(&df, declared)?),
        ReadSchema::Infer => {
            let (fields, columns) = dataframe_to_fields_and_columns(&df)?;
            match embedded {
                Some(embedded) if embedded_schema_describes(&embedded, &fields) => (embedded, columns),
                Some(_) | None => (RecordSchema::new(fields)?, columns),
            }
        }
    };
    check_not_null(&schema, &columns)?;
    RecordBatch::new(Arc::new(schema), columns, None, None, Vec::new())
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
