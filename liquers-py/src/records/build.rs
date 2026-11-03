//! Building a record view from plain Python data: `RecordView.from_records` and `from_dict`.
//!
//! The data is read in Rust from the Python values, not through a text reader: columns keep the
//! first-seen key order, a missing key is a null, and each column's type is inferred from its
//! values (the inference table below). `fields=` then declares some or all fields, and each
//! declared column is cast to its declared type ([`cast`]). Dataframes reach the same two passes
//! through Arrow or the polars bridge (`arrow.rs`). See
//! `specs/design/liquers-py-records/phase1-high-level-design.md` question 9.
//!
//! | Python value | Inferred type |
//! |---|---|
//! | `bool` | `Bool` |
//! | `int` (64-bit) | `Int`; beyond 64 bits a `ConversionError` |
//! | `float`, or `int` mixed with `float` | `Float` |
//! | `str` | `Text` |
//! | `bytes` | `Binary` |
//! | `datetime.date` | `Date` |
//! | `datetime.datetime` | `Timestamp`, in UTC (a naive one is taken as UTC) |
//! | `list` of numbers | `Vector` |
//! | only `None` | `Text` |
//!
//! A column mixing any other two types is refused, naming the field and the first row that
//! disagrees. An `Id` field holding a null is refused, since `RecordSchema::new` checks only that
//! roles are unique.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_records::{
    format_field_value, ColumnMut, FieldSchema as CoreFieldSchema, FieldType as CoreFieldType, FieldValue,
    KeyRole as CoreKeyRole, RecordBatch, RecordSchema as CoreRecordSchema, RecordView as CoreRecordView,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::error::pyerr_to_error;
use crate::records::convert::py_to_field_value;
use crate::records::schema::{type_name, FieldSchema, FieldType, RecordSchema};

/// Columns read from Python: names in first-seen order, and each column's cells.
pub struct RawColumns {
    pub names: Vec<String>,
    pub cells: Vec<Vec<FieldValue>>,
    pub rows: usize,
}

/// `fields=`: nothing, some declared fields, or a whole schema.
pub enum Fields {
    Infer,
    Declared(Vec<(String, FieldSchema)>),
    Schema(Arc<CoreRecordSchema>),
}

impl Fields {
    pub fn extract(fields: Option<&Bound<'_, PyAny>>) -> Result<Fields, Error> {
        let Some(fields) = fields.filter(|f| !f.is_none()) else {
            return Ok(Fields::Infer);
        };
        if let Ok(schema) = fields.cast::<RecordSchema>() {
            return Ok(Fields::Schema(schema.get().inner.clone()));
        }
        let dict = fields.cast::<PyDict>().map_err(|_| {
            Error::conversion_error_with_message(
                type_name(fields),
                "fields",
                "fields= is a dict from field name to type or FieldSchema, or an lq.RecordSchema",
            )
        })?;
        let mut declared = Vec::with_capacity(dict.len());
        for (name, declaration) in dict.iter() {
            let name = name
                .cast::<PyString>()
                .map_err(|_| Error::conversion_error_with_message(type_name(&name), "a field name", "names are str"))?
                .to_string();
            declared.push((name, FieldSchema::extract_declaration(&declaration)?));
        }
        Ok(Fields::Declared(declared))
    }
}

fn py_error(py: Python<'_>) -> impl Fn(PyErr) -> Error + '_ {
    move |e| pyerr_to_error(py, &e)
}

/// Reads a list of dicts.
pub fn read_records(rows: &Bound<'_, PyAny>) -> Result<RawColumns, Error> {
    let py = rows.py();
    let mut names: Vec<String> = Vec::new();
    let mut cells: Vec<Vec<FieldValue>> = Vec::new();
    let mut count = 0usize;
    for (row, item) in rows.try_iter().map_err(py_error(py))?.enumerate() {
        let item = item.map_err(py_error(py))?;
        let dict = item.cast::<PyDict>().map_err(|_| {
            Error::conversion_error_with_message(
                type_name(&item),
                "a record",
                &format!("from_records takes a list of dicts; row {row} is not one"),
            )
        })?;
        for (key, value) in dict.iter() {
            let name = key
                .cast::<PyString>()
                .map_err(|_| {
                    Error::conversion_error_with_message(type_name(&key), "a field name", &format!("row {row}: keys are str"))
                })?
                .to_string();
            let index = match names.iter().position(|n| *n == name) {
                Some(index) => index,
                None => {
                    names.push(name.clone());
                    cells.push(vec![FieldValue::Null; count]);
                    names.len() - 1
                }
            };
            let cell = py_to_field_value(&value, &|| format!("field '{name}', row {row}"))?;
            cells[index].push(cell);
        }
        count += 1;
        for column in cells.iter_mut() {
            if column.len() < count {
                column.push(FieldValue::Null);
            }
        }
    }
    Ok(RawColumns { names, cells, rows: count })
}

/// Reads a dict of lists.
pub fn read_dict(columns: &Bound<'_, PyAny>) -> Result<RawColumns, Error> {
    let py = columns.py();
    let dict = columns.cast::<PyDict>().map_err(|_| {
        Error::conversion_error_with_message(type_name(columns), "columns", "from_dict takes a dict of lists")
    })?;
    let mut names = Vec::with_capacity(dict.len());
    let mut cells = Vec::with_capacity(dict.len());
    let mut rows: Option<usize> = None;
    for (key, values) in dict.iter() {
        let name = key
            .cast::<PyString>()
            .map_err(|_| Error::conversion_error_with_message(type_name(&key), "a field name", "keys are str"))?
            .to_string();
        let mut column = Vec::new();
        for (row, value) in values.try_iter().map_err(py_error(py))?.enumerate() {
            let value = value.map_err(py_error(py))?;
            column.push(py_to_field_value(&value, &|| format!("field '{name}', row {row}"))?);
        }
        match rows {
            Some(expected) if expected != column.len() => {
                return Err(Error::conversion_error_with_message(
                    format!("field '{name}' with {} values", column.len()),
                    "a column",
                    &format!("every column needs the same length; earlier columns have {expected}"),
                ))
            }
            Some(_) | None => rows = Some(column.len()),
        }
        names.push(name);
        cells.push(column);
    }
    Ok(RawColumns { names, cells, rows: rows.unwrap_or(0) })
}

fn kind(value: &FieldValue) -> Option<CoreFieldType> {
    match value {
        FieldValue::Null => None,
        FieldValue::Bool(_) => Some(CoreFieldType::Bool),
        FieldValue::Int(_) => Some(CoreFieldType::Int),
        FieldValue::UInt(_) => Some(CoreFieldType::UInt),
        FieldValue::Float(_) => Some(CoreFieldType::Float),
        FieldValue::Text(_) => Some(CoreFieldType::Text),
        FieldValue::Bytes(_) => Some(CoreFieldType::Binary),
        FieldValue::Date(_) => Some(CoreFieldType::Date),
        FieldValue::Timestamp(_) => Some(CoreFieldType::Timestamp),
        FieldValue::Vector(_) => Some(CoreFieldType::Vector),
    }
}

fn type_label(data_type: CoreFieldType) -> &'static str {
    FieldType::from_core(data_type).name()
}

/// A column's type from its cells: the inference table of the module docs.
pub fn infer(name: &str, cells: &[FieldValue]) -> Result<CoreFieldType, Error> {
    let mut inferred: Option<CoreFieldType> = None;
    for (row, cell) in cells.iter().enumerate() {
        let Some(this) = kind(cell) else { continue };
        inferred = Some(match inferred {
            None => this,
            Some(seen) if seen == this => seen,
            Some(CoreFieldType::Int | CoreFieldType::Float) if matches!(this, CoreFieldType::Int | CoreFieldType::Float) => {
                CoreFieldType::Float
            }
            Some(seen) => {
                return Err(Error::conversion_error_with_message(
                    format!("field '{name}', row {row}"),
                    type_label(seen),
                    &format!("the value is {} where earlier rows are {}", type_label(this), type_label(seen)),
                ))
            }
        });
    }
    Ok(inferred.unwrap_or(CoreFieldType::Text))
}

/// Converts `value` to `target` when no value is lost: numbers between the numeric types, text
/// parsed as the target type (`FieldValue::parse_text`), anything to text through the CSV
/// writer's cell formatter, a date to a timestamp at midnight UTC and a timestamp at midnight to a
/// date. Anything else is refused.
pub fn cast(value: &FieldValue, target: CoreFieldType) -> Result<FieldValue, Error> {
    const EXACT_FLOAT: i64 = 1 << 53;
    const DAY_MICROS: i64 = 86_400_000_000;
    let refuse = || {
        Error::conversion_error_with_message(
            describe(value),
            type_label(target),
            "the value does not fit the declared type",
        )
    };
    if matches!(value, FieldValue::Null) || kind(value) == Some(target) {
        return Ok(value.clone());
    }
    match (target, value) {
        (CoreFieldType::Text, _) => Ok(match format_field_value(value)? {
            Some(text) => FieldValue::Text(Arc::from(text)),
            None => FieldValue::Null,
        }),
        (_, FieldValue::Text(text)) => FieldValue::parse_text(target, text).map_err(|_| refuse()),
        (CoreFieldType::Int, FieldValue::UInt(u)) => i64::try_from(*u).map(FieldValue::Int).map_err(|_| refuse()),
        (CoreFieldType::Int, FieldValue::Float(f)) => {
            if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f < i64::MAX as f64 {
                Ok(FieldValue::Int(*f as i64))
            } else {
                Err(refuse())
            }
        }
        (CoreFieldType::UInt, FieldValue::Int(i)) => u64::try_from(*i).map(FieldValue::UInt).map_err(|_| refuse()),
        (CoreFieldType::UInt, FieldValue::Float(f)) => {
            if f.fract() == 0.0 && *f >= 0.0 && *f < u64::MAX as f64 {
                Ok(FieldValue::UInt(*f as u64))
            } else {
                Err(refuse())
            }
        }
        (CoreFieldType::Float, FieldValue::Int(i)) if i.abs() <= EXACT_FLOAT => Ok(FieldValue::Float(*i as f64)),
        (CoreFieldType::Float, FieldValue::UInt(u)) if *u <= EXACT_FLOAT as u64 => Ok(FieldValue::Float(*u as f64)),
        (CoreFieldType::Timestamp, FieldValue::Date(days)) => Ok(FieldValue::Timestamp(i64::from(*days) * DAY_MICROS)),
        (CoreFieldType::Date, FieldValue::Timestamp(micros)) if micros % DAY_MICROS == 0 => {
            i32::try_from(micros / DAY_MICROS).map(FieldValue::Date).map_err(|_| refuse())
        }
        (
            CoreFieldType::Bool
            | CoreFieldType::Int
            | CoreFieldType::UInt
            | CoreFieldType::Float
            | CoreFieldType::Binary
            | CoreFieldType::Date
            | CoreFieldType::Timestamp
            | CoreFieldType::Vector,
            _,
        ) => Err(refuse()),
    }
}

fn describe(value: &FieldValue) -> String {
    match format_field_value(value) {
        Ok(Some(text)) => {
            let shown: String = text.chars().take(40).collect();
            format!("{} '{shown}'", kind(value).map(type_label).unwrap_or("null"))
        }
        Ok(None) | Err(_) => "the value".to_string(),
    }
}

/// The schema and columns of a table: inferred, then declared, then cast.
pub fn build(raw: RawColumns, fields: Fields) -> Result<Arc<dyn CoreRecordView>, Error> {
    let types = raw
        .names
        .iter()
        .zip(raw.cells.iter())
        .map(|(name, column)| infer(name, column))
        .collect::<Result<Vec<_>, Error>>()?;
    build_typed(raw, types, fields)
}

/// [`build`] with each column's type already known (a dataframe's, read through Arrow).
pub fn build_typed(raw: RawColumns, types: Vec<CoreFieldType>, fields: Fields) -> Result<Arc<dyn CoreRecordView>, Error> {
    let RawColumns { names, mut cells, rows } = raw;
    let inferred: Vec<CoreFieldSchema> =
        names.iter().zip(types).map(|(name, data_type)| CoreFieldSchema::new(name.clone(), data_type)).collect();
    let (schema_fields, column_cells): (Vec<CoreFieldSchema>, Vec<Vec<FieldValue>>) = match fields {
        Fields::Infer => (inferred, cells),
        Fields::Declared(declared) => {
            let mut schema_fields = inferred;
            for (name, declaration) in declared {
                let field = declaration.to_core(Some(&name))?;
                match names.iter().position(|n| *n == name) {
                    Some(index) => schema_fields[index] = field,
                    None => {
                        schema_fields.push(field);
                        cells.push(vec![FieldValue::Null; rows]);
                    }
                }
            }
            (schema_fields, cells)
        }
        Fields::Schema(schema) => {
            if let Some(extra) = names.iter().find(|name| schema.index_of(name).is_none()) {
                return Err(Error::conversion_error_with_message(
                    format!("field '{extra}'"),
                    "the given RecordSchema",
                    "the schema has no such field",
                ));
            }
            let mut ordered = Vec::with_capacity(schema.fields.len());
            for field in &schema.fields {
                ordered.push(match names.iter().position(|n| *n == field.name) {
                    Some(index) => std::mem::take(&mut cells[index]),
                    None => vec![FieldValue::Null; rows],
                });
            }
            (schema.fields.clone(), ordered)
        }
    };
    let mut columns = Vec::with_capacity(schema_fields.len());
    for (field, values) in schema_fields.iter().zip(column_cells.iter()) {
        let mut column = ColumnMut::with_capacity(field.data_type, rows);
        for (row, value) in values.iter().enumerate() {
            let at = |e: Error| {
                Error::conversion_error_with_message(
                    format!("field '{}', row {row}", field.name),
                    type_label(field.data_type),
                    &e.message,
                )
            };
            let value = cast(value, field.data_type).map_err(at)?;
            if matches!(value, FieldValue::Null) {
                if field.key == CoreKeyRole::Id {
                    return Err(Error::conversion_error_with_message(
                        format!("field '{}', row {row}", field.name),
                        "an Id",
                        "an Id field cannot hold a null",
                    ));
                }
                if !field.nullable {
                    return Err(Error::conversion_error_with_message(
                        format!("field '{}', row {row}", field.name),
                        type_label(field.data_type),
                        "the field is declared not nullable",
                    ));
                }
            }
            column.push(&value).map_err(at)?;
        }
        columns.push(column.freeze());
    }
    let schema = CoreRecordSchema::new(schema_fields)?;
    let batch = RecordBatch::new(Arc::new(schema), columns, None, None, Vec::new())?;
    Ok(Arc::new(batch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> FieldValue {
        FieldValue::Text(Arc::from(s))
    }

    #[test]
    fn cast_keeps_null_and_same_type() -> Result<(), Error> {
        assert_eq!(cast(&FieldValue::Null, CoreFieldType::Int)?, FieldValue::Null);
        assert_eq!(cast(&FieldValue::Int(3), CoreFieldType::Int)?, FieldValue::Int(3));
        Ok(())
    }

    #[test]
    fn cast_to_text_uses_the_csv_formatter() -> Result<(), Error> {
        assert_eq!(cast(&FieldValue::Int(3), CoreFieldType::Text)?, text("3"));
        assert_eq!(cast(&FieldValue::Bool(true), CoreFieldType::Text)?, text("true"));
        assert_eq!(cast(&FieldValue::Date(0), CoreFieldType::Text)?, text("1970-01-01"));
        Ok(())
    }

    #[test]
    fn cast_from_text_parses() -> Result<(), Error> {
        assert_eq!(cast(&text("42"), CoreFieldType::Int)?, FieldValue::Int(42));
        assert_eq!(cast(&text("1.5"), CoreFieldType::Float)?, FieldValue::Float(1.5));
        assert_eq!(cast(&text("true"), CoreFieldType::Bool)?, FieldValue::Bool(true));
        assert_eq!(cast(&text("1970-01-02"), CoreFieldType::Date)?, FieldValue::Date(1));
        assert!(cast(&text("x"), CoreFieldType::Int).is_err());
        Ok(())
    }

    #[test]
    fn cast_between_integers_when_in_range() -> Result<(), Error> {
        assert_eq!(cast(&FieldValue::Int(5), CoreFieldType::UInt)?, FieldValue::UInt(5));
        assert!(cast(&FieldValue::Int(-1), CoreFieldType::UInt).is_err());
        assert_eq!(cast(&FieldValue::UInt(5), CoreFieldType::Int)?, FieldValue::Int(5));
        assert!(cast(&FieldValue::UInt(u64::MAX), CoreFieldType::Int).is_err());
        Ok(())
    }

    #[test]
    fn cast_between_integers_and_floats_without_loss() -> Result<(), Error> {
        assert_eq!(cast(&FieldValue::Int(2), CoreFieldType::Float)?, FieldValue::Float(2.0));
        assert!(cast(&FieldValue::Int((1 << 53) + 1), CoreFieldType::Float).is_err());
        assert_eq!(cast(&FieldValue::Float(2.0), CoreFieldType::Int)?, FieldValue::Int(2));
        assert!(cast(&FieldValue::Float(2.5), CoreFieldType::Int).is_err());
        assert_eq!(cast(&FieldValue::Float(2.0), CoreFieldType::UInt)?, FieldValue::UInt(2));
        Ok(())
    }

    #[test]
    fn cast_between_dates_and_timestamps() -> Result<(), Error> {
        assert_eq!(cast(&FieldValue::Date(1), CoreFieldType::Timestamp)?, FieldValue::Timestamp(86_400_000_000));
        assert_eq!(cast(&FieldValue::Timestamp(86_400_000_000), CoreFieldType::Date)?, FieldValue::Date(1));
        assert!(cast(&FieldValue::Timestamp(1), CoreFieldType::Date).is_err());
        Ok(())
    }

    #[test]
    fn cast_refuses_the_rest() {
        assert!(cast(&FieldValue::Bool(true), CoreFieldType::Int).is_err());
        assert!(cast(&FieldValue::Int(1), CoreFieldType::Bool).is_err());
        assert!(cast(&FieldValue::Bytes(Arc::from(&b"x"[..])), CoreFieldType::Vector).is_err());
    }

    #[test]
    fn inference_widens_int_and_float_and_refuses_other_mixes() -> Result<(), Error> {
        assert_eq!(infer("n", &[FieldValue::Int(1), FieldValue::Float(0.5)])?, CoreFieldType::Float);
        assert_eq!(infer("n", &[FieldValue::Null])?, CoreFieldType::Text);
        let error = infer("n", &[FieldValue::Int(1), FieldValue::Null, text("x")]).err().ok_or_else(|| {
            Error::general_error("expected a conflict".to_string())
        })?;
        assert!(error.message.contains("field 'n', row 2"), "{}", error.message);
        Ok(())
    }
}
