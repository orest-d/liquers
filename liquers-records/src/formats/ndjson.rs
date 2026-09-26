//! NDJSON (one JSON object per line) and the `json` [`super::TableFormat`] (one JSON array of the
//! same row objects) — Tier 1's other JSON-shaped formats, alongside CSV/TSV. Both readers this
//! crate's [`super::ReadSchema`] can select live here, exactly as in [`super::csv`]: schema-aware
//! and strict, or schema-less and inferring, per `formats::infer`'s JSON-adapted rules below.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Tier 1 — in `records`, no new
//! dependency" and §"JSON shapes are conversions, not formats" ("the `json` format has one fixed
//! shape, the array of row objects that NDJSON's lines also hold").
//!
//! The row-object <-> [`RecordBatch`] machinery here ([`objects_to_batch`], [`rows_to_json_values`],
//! [`row_to_json`], [`scalar_to_json`], [`json_to_scalar`]) is `pub(super)`: [`super::shapes`]'s
//! `records` orient is exactly this shape, and every other orient transposes into it rather than
//! reimplementing declared/inferred reading a second time.

use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use serde_json::{Map, Value};

use crate::batch::{RecordBatch, RecordView};
use crate::column::FieldValue;
use crate::formats::csv;
use crate::formats::infer;
use crate::formats::{ReadOptions, ReadSchema, WriteOptions};
use crate::mutable::{RecordBatchMut, RecordViewMut};
use crate::schema::{FieldSchema, FieldType, RecordSchema};

// ---------------------------------------------------------------------------------------------
// Scalar <-> JSON conversions (shared by the declared and the post-inference readers, and by the
// writer — the JSON analogue of csv.rs's `parse_scalar`/`format_value`)
// ---------------------------------------------------------------------------------------------

/// `true` only for `Value::Null` — named so a call site reads as a question, not a `matches!`.
fn is_json_null(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => false,
    }
}

/// Converts one JSON array of numbers into a `Vector`. Every element must be a JSON number —
/// `Number::as_f64` always succeeds for `i64`/`u64`/`f64`, so only non-numeric elements are
/// refused.
fn json_to_vector(items: &[Value]) -> Result<FieldValue, Error> {
    let mut floats = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Number(n) => {
                let value = n.as_f64().ok_or_else(|| {
                    Error::conversion_error(n.to_string(), "Vector element (finite number)")
                })?;
                floats.push(value as f32);
            }
            Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
                return Err(Error::conversion_error(format!("{item:?}"), "Vector element (number)"));
            }
        }
    }
    Ok(FieldValue::Vector(Arc::from(floats)))
}

/// Parses one non-null JSON `value` as `field_type` — strict, no guessing, mirroring
/// [`csv::parse_scalar`]'s contract but over JSON's own types instead of cell text. The lossless
/// coercions phase2-architecture.md's schema-aware table lists: an integral or fractional number
/// into `Float` (`Number::as_f64` covers both), a string into `Date`/`Timestamp` by parsing, a
/// base64 string into `Binary`, an array of numbers into `Vector`.
pub(super) fn json_to_scalar(value: &Value, field_type: FieldType) -> Result<FieldValue, Error> {
    match field_type {
        FieldType::Bool => match value {
            Value::Bool(v) => Ok(FieldValue::Bool(*v)),
            Value::Null | Value::Number(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Bool (JSON boolean)"))
            }
        },
        FieldType::Int => match value {
            Value::Number(n) => n
                .as_i64()
                .map(FieldValue::Int)
                .ok_or_else(|| Error::conversion_error(n.to_string(), "Int (JSON integer)")),
            Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Int (JSON integer)"))
            }
        },
        FieldType::UInt => match value {
            Value::Number(n) => n
                .as_u64()
                .map(FieldValue::UInt)
                .ok_or_else(|| Error::conversion_error(n.to_string(), "UInt (JSON unsigned integer)")),
            Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "UInt (JSON unsigned integer)"))
            }
        },
        FieldType::Float => match value {
            Value::Number(n) => n
                .as_f64()
                .map(FieldValue::Float)
                .ok_or_else(|| Error::conversion_error(n.to_string(), "Float (JSON number)")),
            Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Float (JSON number)"))
            }
        },
        FieldType::Text => match value {
            Value::String(v) => Ok(FieldValue::Text(Arc::from(v.as_str()))),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Text (JSON string)"))
            }
        },
        FieldType::Binary => match value {
            Value::String(v) => csv::base64_decode(v).map(|bytes| FieldValue::Bytes(Arc::from(bytes))),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Binary (base64 JSON string)"))
            }
        },
        FieldType::Date => match value {
            Value::String(v) => csv::parse_date(v).map(FieldValue::Date),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Date (YYYY-MM-DD JSON string)"))
            }
        },
        FieldType::Timestamp => match value {
            Value::String(v) => csv::parse_timestamp(v).map(FieldValue::Timestamp),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Timestamp (RFC 3339 JSON string)"))
            }
        },
        FieldType::Vector => match value {
            Value::Array(items) => json_to_vector(items),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
                Err(Error::conversion_error(format!("{value:?}"), "Vector (JSON array of numbers)"))
            }
        },
    }
}

/// The schema-less reader's `Text` fallback: a JSON string is kept verbatim (not requoted), and
/// anything else that could not be classified as `Bool`/`Int`/`Float`/`Date`/`Timestamp`/`Vector`
/// falls back to holding its own JSON serialization — phase2-architecture.md §"Schema-less
/// inference rules": "any other array or object is `Text` holding its JSON".
fn json_cell_as_text(value: &Value) -> Result<FieldValue, Error> {
    match value {
        Value::String(v) => Ok(FieldValue::Text(Arc::from(v.as_str()))),
        Value::Null => Ok(FieldValue::Text(Arc::from(""))),
        Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => serde_json::to_string(value)
            .map(|text| FieldValue::Text(Arc::from(text.as_str())))
            .map_err(|e| Error::from_error(ErrorType::ConversionError, e)),
    }
}

/// The inverse of [`json_to_scalar`]: renders one [`FieldValue`] the way [`super::write_table`]
/// writes JSON — dates/timestamps as ISO strings (matching `csv.rs`'s rendering), `Binary` as a
/// base64 string, `Vector` as an array of numbers.
pub(super) fn scalar_to_json(value: &FieldValue) -> Result<Value, Error> {
    match value {
        FieldValue::Null => Ok(Value::Null),
        FieldValue::Bool(v) => Ok(Value::Bool(*v)),
        FieldValue::Int(v) => Ok(Value::from(*v)),
        FieldValue::UInt(v) => Ok(Value::from(*v)),
        FieldValue::Float(v) => serde_json::Number::from_f64(*v).map(Value::Number).ok_or_else(|| {
            Error::general_error(format!(
                "write_table: JSON: float value {v} is not finite (NaN/Infinity has no JSON representation)"
            ))
        }),
        FieldValue::Text(v) => Ok(Value::String(v.to_string())),
        FieldValue::Bytes(v) => Ok(Value::String(csv::base64_encode(v))),
        FieldValue::Date(days) => csv::format_date(*days).map(Value::String),
        FieldValue::Timestamp(micros) => csv::format_timestamp(*micros).map(Value::String),
        FieldValue::Vector(v) => {
            let mut items = Vec::with_capacity(v.len());
            for element in v.iter() {
                let number = serde_json::Number::from_f64(*element as f64).ok_or_else(|| {
                    Error::general_error(format!(
                        "write_table: JSON: vector element {element} is not finite"
                    ))
                })?;
                items.push(Value::Number(number));
            }
            Ok(Value::Array(items))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reading: row objects -> RecordBatch (shared by NDJSON, `json`, and every `shapes::JsonOrient`)
// ---------------------------------------------------------------------------------------------

/// Applies `nullable`/missing handling once, then hands the non-null JSON value to `parse` — the
/// JSON analogue of `csv::cell_value`.
fn resolve_cell<F>(
    raw: Option<&Value>,
    field_name: &str,
    nullable: bool,
    row: usize,
    parse: F,
) -> Result<FieldValue, Error>
where
    F: FnOnce(&Value) -> Result<FieldValue, Error>,
{
    match raw {
        None => {
            if nullable {
                Ok(FieldValue::Null)
            } else {
                Err(Error::general_error(format!(
                    "read_table: record {row}: column '{field_name}' is missing and not nullable"
                )))
            }
        }
        Some(value) if is_json_null(value) => {
            if nullable {
                Ok(FieldValue::Null)
            } else {
                Err(Error::general_error(format!(
                    "read_table: record {row}, column '{field_name}': null is not allowed (not nullable)"
                )))
            }
        }
        Some(value) => parse(value).map_err(|e| {
            Error::general_error(format!("read_table: record {row}, column '{field_name}': {e}"))
        }),
    }
}

/// The schema-aware reader: strict, no guessing. Every key of `obj` must be declared in `schema`
/// (an undeclared key is an error naming it, matching CSV's rule for an undeclared column); a
/// declared field missing from `obj` is a null column when nullable, an error otherwise.
fn declared_row_values(
    obj: &Map<String, Value>,
    schema: &RecordSchema,
    row: usize,
) -> Result<Vec<FieldValue>, Error> {
    for key in obj.keys() {
        if schema.index_of(key).is_none() {
            return Err(Error::general_error(format!(
                "read_table: record {row}: column '{key}' is not declared in the schema"
            )));
        }
    }
    let mut values = Vec::with_capacity(schema.fields.len());
    for field in &schema.fields {
        let raw = obj.get(&field.name);
        values.push(resolve_cell(raw, &field.name, field.nullable, row, |value| {
            json_to_scalar(value, field.data_type)
        })?);
    }
    Ok(values)
}

fn read_declared_objects(objects: &[&Map<String, Value>], schema: &RecordSchema) -> Result<RecordBatch, Error> {
    let mut builder = RecordBatchMut::with_capacity(Arc::new(schema.clone()), objects.len());
    for (row, obj) in objects.iter().enumerate() {
        builder.append_row(&declared_row_values(obj, schema, row)?)?;
    }
    builder.freeze()
}

/// Infers one column's [`FieldType`] and nullability from its JSON cell values —
/// phase2-architecture.md §"Schema-less inference rules": JSON's own types decide (an integral,
/// no-exponent number that fits `i64` is `Int`, any other number is `Float`); only *strings* are
/// tried against the date/timestamp tests; an array of numbers of one length in every row is a
/// `Vector`; anything else falls back to `Text`. The `Id` role is never guessed.
fn infer_json_column(cells: &[Option<&Value>]) -> (FieldType, bool) {
    let nullable = cells.iter().any(|cell| match cell {
        None => true,
        Some(value) => is_json_null(value),
    });
    let non_null: Vec<&Value> = cells
        .iter()
        .filter_map(|cell| match cell {
            Some(value) if !is_json_null(value) => Some(*value),
            Some(_) | None => None,
        })
        .collect();

    if non_null.is_empty() {
        return (FieldType::Text, true);
    }
    if non_null.iter().all(|value| matches!(value, Value::Bool(_))) {
        return (FieldType::Bool, nullable);
    }
    let is_canonical_int = |value: &&Value| match value {
        Value::Number(n) => n.is_i64(),
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => false,
    };
    if non_null.iter().all(is_canonical_int) {
        return (FieldType::Int, nullable);
    }
    let is_number = |value: &&Value| match value {
        Value::Number(_) => true,
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => false,
    };
    if non_null.iter().all(is_number) {
        return (FieldType::Float, nullable);
    }
    fn as_str(value: &Value) -> Option<&str> {
        match value {
            Value::String(s) => Some(s.as_str()),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => None,
        }
    }
    if non_null.iter().all(|value| as_str(value).is_some_and(infer::is_iso_date)) {
        return (FieldType::Date, nullable);
    }
    if non_null.iter().all(|value| as_str(value).is_some_and(infer::is_iso_timestamp)) {
        return (FieldType::Timestamp, nullable);
    }
    let vector_len = |value: &Value| match value {
        Value::Array(items) if items.iter().all(|item| matches!(item, Value::Number(_))) => Some(items.len()),
        Value::Array(_) | Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            None
        }
    };
    if let Some(first_len) = vector_len(non_null[0]) {
        if non_null.iter().all(|value| vector_len(value) == Some(first_len)) {
            return (FieldType::Vector, nullable);
        }
    }
    (FieldType::Text, nullable)
}

/// The schema-less reader: one column at a time, [`infer_json_column`] guesses its type from every
/// non-null cell across every row (the **union** of keys, not just the first row's), then every
/// cell is parsed as that type — [`json_cell_as_text`] for the `Text` fallback, since a plain
/// string column must round-trip verbatim rather than come back quoted.
fn read_inferred_objects(objects: &[&Map<String, Value>]) -> Result<RecordBatch, Error> {
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for obj in objects {
        for key in obj.keys() {
            if seen.insert(key.clone()) {
                names.push(key.clone());
            }
        }
    }

    let mut fields = Vec::with_capacity(names.len());
    let mut field_types = Vec::with_capacity(names.len());
    for name in &names {
        let cells: Vec<Option<&Value>> = objects.iter().map(|obj| obj.get(name.as_str())).collect();
        let (data_type, nullable) = infer_json_column(&cells);
        let mut field = FieldSchema::new(name.clone(), data_type);
        if !nullable {
            field = field.not_null();
        }
        field_types.push((data_type, nullable));
        fields.push(field);
    }
    let schema = RecordSchema::new(fields)?;

    let mut builder = RecordBatchMut::with_capacity(Arc::new(schema), objects.len());
    for (row, obj) in objects.iter().enumerate() {
        let mut values = Vec::with_capacity(names.len());
        for (name, &(data_type, nullable)) in names.iter().zip(field_types.iter()) {
            let raw = obj.get(name.as_str());
            values.push(resolve_cell(raw, name, nullable, row, |value| {
                if data_type == FieldType::Text {
                    json_cell_as_text(value)
                } else {
                    json_to_scalar(value, data_type)
                }
            })?);
        }
        builder.append_row(&values)?;
    }
    builder.freeze()
}

/// Every `item` must be a JSON object — the row-object shape NDJSON's lines, the `json` format's
/// array, and `shapes::JsonOrient::Records` all share. `pub(super)`: [`super::shapes`]'s `records`
/// orient and every transposed orient (`list`, `split`, `values`, `columns`, `index`, `table`)
/// funnel through this one declared/inferred split rather than re-implementing it.
pub(super) fn objects_to_batch(items: &[Value], schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let mut objects: Vec<&Map<String, Value>> = Vec::with_capacity(items.len());
    for (row, item) in items.iter().enumerate() {
        match item {
            Value::Object(obj) => objects.push(obj),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
                return Err(Error::general_error(format!(
                    "read_table: record {row} is not a JSON object"
                )));
            }
        }
    }
    match schema {
        ReadSchema::Declared(schema) => read_declared_objects(&objects, schema),
        ReadSchema::Infer => read_inferred_objects(&objects),
    }
}

// ---------------------------------------------------------------------------------------------
// Writing: RecordBatch -> row objects (shared the same way)
// ---------------------------------------------------------------------------------------------

/// One row as a JSON object, in schema field order. `pub(super)`: shared with [`super::shapes`].
pub(super) fn row_to_json(view: &dyn RecordView, row: usize) -> Result<Value, Error> {
    let schema = view.schema();
    let mut obj = Map::with_capacity(schema.fields.len());
    for (col, field) in schema.fields.iter().enumerate() {
        let value = view.value(row, col)?;
        obj.insert(field.name.clone(), scalar_to_json(&value)?);
    }
    Ok(Value::Object(obj))
}

/// Every row of `view`, in order — the shared body of both the `json` array and NDJSON's lines,
/// and of `shapes::to_json`'s `records` orient.
pub(super) fn rows_to_json_values(view: &dyn RecordView) -> Result<Vec<Value>, Error> {
    let mut rows = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        rows.push(row_to_json(view, row)?);
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------------------------
// `super::read_table`/`super::write_table` entry points
// ---------------------------------------------------------------------------------------------

/// One JSON object per line. `options.header` does not apply — NDJSON has no header row — so it is
/// accepted only for signature parity with every other format's reader.
pub(crate) fn read_ndjson(
    bytes: &[u8],
    schema: ReadSchema<'_>,
    _options: &ReadOptions,
) -> Result<RecordBatch, Error> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let mut items = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|e| {
            Error::general_error(format!("read_table: NDJSON line {}: {e}", line_number + 1))
        })?;
        items.push(value);
    }
    objects_to_batch(&items, schema)
}

/// One line per row, each a JSON object — the inverse of [`read_ndjson`].
pub(crate) fn write_ndjson(view: &dyn RecordView, _options: &WriteOptions) -> Result<Vec<u8>, Error> {
    let mut out = String::new();
    for row in rows_to_json_values(view)? {
        out.push_str(&serde_json::to_string(&row).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?);
        out.push('\n');
    }
    Ok(out.into_bytes())
}

/// The `json` [`super::TableFormat`]: one JSON array holding the same row objects NDJSON's lines
/// hold (phase2-architecture.md §"JSON shapes are conversions, not formats").
pub(crate) fn read_json(bytes: &[u8], schema: ReadSchema<'_>, _options: &ReadOptions) -> Result<RecordBatch, Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let items = match value {
        Value::Array(items) => items,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            return Err(Error::general_error(
                "read_table: JSON: expected a top-level array of row objects".to_string(),
            ));
        }
    };
    objects_to_batch(&items, schema)
}

/// The inverse of [`read_json`]: one JSON array of row objects.
pub(crate) fn write_json(view: &dyn RecordView, _options: &WriteOptions) -> Result<Vec<u8>, Error> {
    let rows = rows_to_json_values(view)?;
    serde_json::to_vec(&Value::Array(rows)).map_err(|e| Error::from_error(ErrorType::ConversionError, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{read_table, write_table, TableFormat};
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordViewMut};

    #[test]
    fn ndjson_infers_the_union_of_keys_across_rows() -> Result<(), Error> {
        let ndjson = b"{\"name\":\"alice\",\"age\":30}\n{\"name\":\"bob\",\"email\":\"bob@example.com\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let names: Vec<&str> = batch.schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
        assert!(names.contains(&"email"));
        Ok(())
    }

    #[test]
    fn ndjson_infers_a_fixed_length_number_array_as_vector() -> Result<(), Error> {
        let ndjson = b"{\"id\":1,\"embedding\":[0.1,0.2,0.3]}\n{\"id\":2,\"embedding\":[0.4,0.5,0.6]}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let field = batch.schema.fields.iter().find(|f| f.name == "embedding").expect("embedding field");
        assert_eq!(field.data_type, FieldType::Vector);
        Ok(())
    }

    #[test]
    fn ndjson_write_read_round_trip_preserves_values() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int),
            FieldSchema::new("name", FieldType::Text),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 2);
        batch.append_row(&[FieldValue::Int(1), FieldValue::Text(Arc::from("alice"))])?;
        batch.append_row(&[FieldValue::Int(2), FieldValue::Text(Arc::from("bob"))])?;
        let batch = batch.freeze()?;

        let bytes = write_table(&batch, TableFormat::NdJson, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::NdJson, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(read_back.value(0, 0)?, FieldValue::Int(1));
        assert_eq!(read_back.value(1, 1)?, FieldValue::Text(Arc::from("bob")));
        Ok(())
    }

    // --- Additional coverage beyond Phase 3 §3.2 ---

    #[test]
    fn json_format_round_trips_a_declared_schema() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("flag", FieldType::Bool),
            FieldSchema::new("blob", FieldType::Binary),
            FieldSchema::new("day", FieldType::Date),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 1);
        batch.append_row(&[
            FieldValue::Bool(true),
            FieldValue::Bytes(Arc::from(vec![1u8, 2, 3])),
            FieldValue::Date(19_570),
        ])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Json, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Json, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        for col in 0..schema.fields.len() {
            assert_eq!(read_back.value(0, col)?, batch.value(0, col)?);
        }
        Ok(())
    }

    #[test]
    fn json_format_is_an_array_not_lines() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Int(1)])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Json, &WriteOptions::default())?;
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(text.trim_start().starts_with('['));
        Ok(())
    }

    #[test]
    fn declared_read_refuses_an_undeclared_key() {
        let schema = RecordSchema::new(vec![FieldSchema::new("name", FieldType::Text)]).expect("schema");
        let ndjson = b"{\"name\":\"a\",\"extra\":1}\n";
        let err = read_table(ndjson, TableFormat::NdJson, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("extra is not declared");
        assert!(format!("{err}").contains("extra"));
    }

    #[test]
    fn declared_read_refuses_a_missing_non_nullable_field() {
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int).not_null(),
        ])
        .expect("schema");
        let ndjson = b"{\"name\":\"a\"}\n";
        assert!(read_table(ndjson, TableFormat::NdJson, ReadSchema::Declared(&schema), &ReadOptions::default()).is_err());
    }

    #[test]
    fn inferred_null_field_is_nullable() -> Result<(), Error> {
        let ndjson = b"{\"v\":1}\n{\"v\":null}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert!(batch.schema.fields[0].nullable);
        assert_eq!(batch.value(1, 0)?, FieldValue::Null);
        Ok(())
    }

    #[test]
    fn inferred_bool_and_text_columns() -> Result<(), Error> {
        // `serde_json::Map` is a `BTreeMap` here (no `preserve_order` feature), so the union of
        // keys comes back sorted rather than in first-seen order — fields are looked up by name
        // rather than by position.
        let ndjson = b"{\"ok\":true,\"note\":\"hello\"}\n{\"ok\":false,\"note\":\"world\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let ok = batch.schema.index_of("ok").expect("ok field");
        let note = batch.schema.index_of("note").expect("note field");
        assert_eq!(batch.schema.fields[ok].data_type, FieldType::Bool);
        assert_eq!(batch.schema.fields[note].data_type, FieldType::Text);
        Ok(())
    }

    #[test]
    fn inferred_string_that_looks_numeric_stays_text() -> Result<(), Error> {
        // JSON's own types decide: a *string* "42" is never number-inferred.
        let ndjson = b"{\"code\":\"42\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Text);
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("42")));
        Ok(())
    }

    #[test]
    fn inferred_date_and_timestamp_strings() -> Result<(), Error> {
        let ndjson = b"{\"day\":\"2026-09-25\",\"at\":\"2026-09-25T12:00:00Z\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        let day = batch.schema.index_of("day").expect("day field");
        let at = batch.schema.index_of("at").expect("at field");
        assert_eq!(batch.schema.fields[day].data_type, FieldType::Date);
        assert_eq!(batch.schema.fields[at].data_type, FieldType::Timestamp);
        Ok(())
    }

    #[test]
    fn inferred_ragged_array_falls_back_to_text_holding_its_json() -> Result<(), Error> {
        let ndjson = b"{\"v\":[1,2]}\n{\"v\":[1,2,3]}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Text);
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("[1,2]")));
        Ok(())
    }

    #[test]
    fn inferred_object_value_falls_back_to_text_holding_its_json() -> Result<(), Error> {
        let ndjson = b"{\"v\":{\"a\":1}}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Text);
        Ok(())
    }

    #[test]
    fn empty_ndjson_is_an_empty_batch() -> Result<(), Error> {
        let batch = read_table(b"", TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.len, 0);
        Ok(())
    }

    #[test]
    fn ndjson_row_that_is_not_an_object_is_an_error_not_a_panic() {
        let ndjson = b"[1,2,3]\n";
        assert!(read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default()).is_err());
    }

    #[test]
    fn ndjson_id_is_never_guessed_when_inferred() -> Result<(), Error> {
        let ndjson = b"{\"id\":\"abc\"}\n";
        let batch = read_table(ndjson, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.id_field(), None);
        Ok(())
    }
}
