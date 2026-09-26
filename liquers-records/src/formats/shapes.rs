//! `JsonOrient` and the `to_json`/`from_json` conversions between a [`RecordView`]/[`RecordBatch`]
//! and a plain [`serde_json::Value`] — every JSON table shape pandas and polars use, reached by
//! **explicit conversion**, never by a data-format name (`json` the [`super::TableFormat`] is only
//! the `records` shape; every other shape lives here).
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"JSON shapes are conversions, not
//! formats". Every orient but `records`/`list` transposes its JSON shape into the row-object form
//! [`super::ndjson::objects_to_batch`] already knows how to read, rather than re-implementing
//! declared/inferred reading a second time; `records`/`list` also funnel through it directly.

use std::str::FromStr;

use liquers_core::error::Error;
use serde_json::{Map, Value};

use crate::batch::{RecordBatch, RecordView};
use crate::formats::csv;
use crate::formats::ndjson;
use crate::formats::ReadSchema;
use crate::schema::{FieldType, KeyRole, RecordSchema};

/// Every JSON table shape §"JSON shapes are conversions" lists. `Auto` is a **read-side** concept
/// only — [`to_json`] refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonOrient {
    Records,
    List,
    Split,
    Values,
    Columns,
    Index,
    Table,
    Auto,
}

impl FromStr for JsonOrient {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        match text {
            "records" => Ok(JsonOrient::Records),
            "list" => Ok(JsonOrient::List),
            "split" => Ok(JsonOrient::Split),
            "values" => Ok(JsonOrient::Values),
            "columns" => Ok(JsonOrient::Columns),
            "index" => Ok(JsonOrient::Index),
            "table" => Ok(JsonOrient::Table),
            "auto" => Ok(JsonOrient::Auto),
            other => Err(Error::general_error(format!(
                "JsonOrient: unknown orient '{other}' (expected one of records, list, split, \
                 values, columns, index, table, auto)"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Small, exhaustive JSON-shape helpers (every arm names every `serde_json::Value` variant, per
// CLAUDE.md's "Match Statements" convention)
// ---------------------------------------------------------------------------------------------

fn require_object<'v>(value: &'v Value, context: &str) -> Result<&'v Map<String, Value>, Error> {
    match value {
        Value::Object(map) => Ok(map),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => Err(
            Error::conversion_error(format!("{value:?}"), format!("a JSON object ({context})")),
        ),
    }
}

fn require_array<'v>(value: &'v Value, context: &str) -> Result<&'v Vec<Value>, Error> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => Err(
            Error::conversion_error(format!("{value:?}"), format!("a JSON array ({context})")),
        ),
    }
}

fn require_string<'v>(value: &'v Value, context: &str) -> Result<&'v str, Error> {
    match value {
        Value::String(s) => Ok(s.as_str()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => Err(
            Error::conversion_error(format!("{value:?}"), format!("a JSON string ({context})")),
        ),
    }
}

fn required_field<'v>(obj: &'v Map<String, Value>, key: &str, context: &str) -> Result<&'v Value, Error> {
    obj.get(key)
        .ok_or_else(|| Error::general_error(format!("from_json: {context}: missing '{key}'")))
}

/// The declared `Id` field's name, when there is one — what the `split`/`columns`/`index` orients
/// key their index on. `None` for `ReadSchema::Infer` or a schema without an `Id`: the index is
/// then read back as a plain column named `index`, never guessed as `Id`
/// (phase2-architecture.md §"Schema-less inference rules").
fn declared_id_field<'s>(schema: ReadSchema<'s>) -> Option<(&'s str, FieldType)> {
    match schema {
        ReadSchema::Declared(schema) => schema
            .id_field()
            .map(|index| (schema.fields[index].name.as_str(), schema.fields[index].data_type)),
        ReadSchema::Infer => None,
    }
}

/// Converts a JSON object key (always a string) into the value that belongs in an `Id`/`index`
/// column: parsed as `field_type` when a schema says what that type is, kept as text otherwise —
/// "an `Id` read from `columns` or `index` keys is text unless a schema says otherwise."
fn index_key_to_json(key: &str, declared: Option<(&str, FieldType)>) -> Result<Value, Error> {
    match declared {
        Some((_, field_type)) => {
            let scalar = csv::parse_scalar(key, field_type)?;
            ndjson::scalar_to_json(&scalar)
        }
        None => Ok(Value::String(key.to_string())),
    }
}

/// The name the `Id`/`index` column takes in the row objects this module builds for
/// [`ndjson::objects_to_batch`]: the schema's own `Id` field name when declared, `"index"`
/// otherwise.
fn index_field_name(declared: Option<(&str, FieldType)>) -> &str {
    match declared {
        Some((name, _)) => name,
        None => "index",
    }
}

// ---------------------------------------------------------------------------------------------
// `records` — exactly the `json` TableFormat's shape
// ---------------------------------------------------------------------------------------------

fn from_json_records(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let items = require_array(value, "records")?;
    ndjson::objects_to_batch(items, schema)
}

fn to_json_records(view: &dyn RecordView) -> Result<Value, Error> {
    Ok(Value::Array(ndjson::rows_to_json_values(view)?))
}

// ---------------------------------------------------------------------------------------------
// `list` — a dictionary of columns; the `Id` is an ordinary column, same as `records`
// ---------------------------------------------------------------------------------------------

fn from_json_list(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let obj = require_object(value, "list")?;
    let names: Vec<String> = match schema {
        ReadSchema::Declared(schema) => schema.fields.iter().map(|field| field.name.clone()).collect(),
        ReadSchema::Infer => obj.keys().cloned().collect(),
    };

    let mut row_count: Option<usize> = None;
    let mut columns: Vec<&Vec<Value>> = Vec::with_capacity(names.len());
    for name in &names {
        let column_value = required_field(obj, name, "list")?;
        let array = require_array(column_value, &format!("list column '{name}'"))?;
        match row_count {
            None => row_count = Some(array.len()),
            Some(expected) if expected != array.len() => {
                return Err(Error::general_error(format!(
                    "from_json: list: column '{name}' has {} rows, expected {expected}",
                    array.len()
                )))
            }
            Some(_) => {}
        }
        columns.push(array);
    }
    let row_count = row_count.unwrap_or(0);

    let mut items = Vec::with_capacity(row_count);
    for row in 0..row_count {
        let mut map = Map::with_capacity(names.len());
        for (name, array) in names.iter().zip(columns.iter()) {
            map.insert(name.clone(), array[row].clone());
        }
        items.push(Value::Object(map));
    }
    ndjson::objects_to_batch(&items, schema)
}

fn to_json_list(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let mut obj = Map::with_capacity(schema.fields.len());
    for (col, field) in schema.fields.iter().enumerate() {
        let mut array = Vec::with_capacity(view.len());
        for row in 0..view.len() {
            array.push(ndjson::scalar_to_json(&view.value(row, col)?)?);
        }
        obj.insert(field.name.clone(), Value::Array(array));
    }
    Ok(Value::Object(obj))
}

// ---------------------------------------------------------------------------------------------
// `split` — `{"columns": […], "index": […], "data": [[…], …]}`
// ---------------------------------------------------------------------------------------------

fn from_json_split(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let obj = require_object(value, "split")?;
    let columns_value = required_field(obj, "columns", "split")?;
    let columns: Vec<&str> = require_array(columns_value, "split.columns")?
        .iter()
        .map(|item| require_string(item, "split.columns element"))
        .collect::<Result<_, _>>()?;
    let index = require_array(required_field(obj, "index", "split")?, "split.index")?;
    let data = require_array(required_field(obj, "data", "split")?, "split.data")?;
    if index.len() != data.len() {
        return Err(Error::general_error(format!(
            "from_json: split: index has {} entries, but data has {}",
            index.len(),
            data.len()
        )));
    }

    let declared = declared_id_field(schema);
    let id_name = index_field_name(declared);

    let mut items = Vec::with_capacity(data.len());
    for (row, row_value) in data.iter().enumerate() {
        let row_array = require_array(row_value, &format!("split.data row {row}"))?;
        if row_array.len() != columns.len() {
            return Err(Error::general_error(format!(
                "from_json: split: data row {row} has {} cells, but columns declares {}",
                row_array.len(),
                columns.len()
            )));
        }
        let mut map = Map::with_capacity(columns.len() + 1);
        map.insert(id_name.to_string(), index[row].clone());
        for (name, cell) in columns.iter().zip(row_array.iter()) {
            map.insert((*name).to_string(), cell.clone());
        }
        items.push(Value::Object(map));
    }
    ndjson::objects_to_batch(&items, schema)
}

fn to_json_split(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let id_col = schema.id_field();
    let payload_cols = schema.payload_fields();

    let mut index = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        let value = match id_col {
            Some(col) => ndjson::scalar_to_json(&view.value(row, col)?)?,
            None => Value::from(row as u64),
        };
        index.push(value);
    }
    let columns: Vec<Value> = payload_cols
        .iter()
        .map(|&col| Value::String(schema.fields[col].name.clone()))
        .collect();
    let mut data = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        let mut row_array = Vec::with_capacity(payload_cols.len());
        for &col in &payload_cols {
            row_array.push(ndjson::scalar_to_json(&view.value(row, col)?)?);
        }
        data.push(Value::Array(row_array));
    }

    let mut obj = Map::with_capacity(3);
    obj.insert("columns".to_string(), Value::Array(columns));
    obj.insert("index".to_string(), Value::Array(index));
    obj.insert("data".to_string(), Value::Array(data));
    Ok(Value::Object(obj))
}

// ---------------------------------------------------------------------------------------------
// `values` — rows without names, purely positional
// ---------------------------------------------------------------------------------------------

fn from_json_values(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let rows = require_array(value, "values")?;
    let names: Vec<String> = match schema {
        ReadSchema::Declared(schema) => schema.fields.iter().map(|field| field.name.clone()).collect(),
        ReadSchema::Infer => {
            let width = match rows.first() {
                Some(first) => require_array(first, "values row 0")?.len(),
                None => 0,
            };
            (0..width).map(|i| format!("c{i}")).collect()
        }
    };

    let mut items = Vec::with_capacity(rows.len());
    for (row, row_value) in rows.iter().enumerate() {
        let row_array = require_array(row_value, &format!("values row {row}"))?;
        if row_array.len() != names.len() {
            return Err(Error::general_error(format!(
                "from_json: values: row {row} has {} cells, expected {}",
                row_array.len(),
                names.len()
            )));
        }
        let mut map = Map::with_capacity(names.len());
        for (name, cell) in names.iter().zip(row_array.iter()) {
            map.insert(name.clone(), cell.clone());
        }
        items.push(Value::Object(map));
    }
    ndjson::objects_to_batch(&items, schema)
}

fn to_json_values(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let mut rows = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        let mut row_array = Vec::with_capacity(schema.fields.len());
        for col in 0..schema.fields.len() {
            row_array.push(ndjson::scalar_to_json(&view.value(row, col)?)?);
        }
        rows.push(Value::Array(row_array));
    }
    Ok(Value::Array(rows))
}

// ---------------------------------------------------------------------------------------------
// `columns` — `{"total": {"1": 9.5, …}}`, column-major, keyed by index as JSON object keys
// ---------------------------------------------------------------------------------------------

/// Row keys of the `columns` and `index` shapes, in row order. `serde_json`'s map sorts its keys
/// as strings, which puts `"10"` before `"2"`; when every key is a canonical integer — pandas'
/// default index — they are put back in numeric order. Other keys keep the map's order.
fn ordered_row_keys(keys: Vec<String>) -> Vec<String> {
    let numeric: Option<Vec<(i64, String)>> = keys
        .iter()
        .map(|key| match key.parse::<i64>() {
            Ok(n) if n.to_string() == *key => Some((n, key.clone())),
            Ok(_) | Err(_) => None,
        })
        .collect();
    match numeric {
        Some(mut pairs) => {
            pairs.sort_by_key(|(n, _)| *n);
            pairs.into_iter().map(|(_, key)| key).collect()
        }
        None => keys,
    }
}

fn from_json_columns(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let obj = require_object(value, "columns")?;
    let declared = declared_id_field(schema);
    let id_name = index_field_name(declared);

    let mut inner_columns: Vec<(&str, &Map<String, Value>)> = Vec::with_capacity(obj.len());
    let mut keys: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (column_name, column_value) in obj.iter() {
        let inner = require_object(column_value, &format!("columns.{column_name}"))?;
        for key in inner.keys() {
            if seen.insert(key.clone()) {
                keys.push(key.clone());
            }
        }
        inner_columns.push((column_name.as_str(), inner));
    }
    let keys = ordered_row_keys(keys);

    let mut items = Vec::with_capacity(keys.len());
    for key in &keys {
        let mut map = Map::with_capacity(inner_columns.len() + 1);
        map.insert(id_name.to_string(), index_key_to_json(key, declared)?);
        for (column_name, inner) in &inner_columns {
            let cell = inner.get(key).cloned().unwrap_or(Value::Null);
            map.insert((*column_name).to_string(), cell);
        }
        items.push(Value::Object(map));
    }
    ndjson::objects_to_batch(&items, schema)
}

fn to_json_columns(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let id_col = schema.id_field();
    let payload_cols = schema.payload_fields();

    let mut keys = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        let key = match id_col {
            Some(col) => csv::format_value(&view.value(row, col)?)?.unwrap_or_default(),
            None => row.to_string(),
        };
        keys.push(key);
    }

    let mut obj = Map::with_capacity(payload_cols.len());
    for &col in &payload_cols {
        let mut inner = Map::with_capacity(view.len());
        for row in 0..view.len() {
            inner.insert(keys[row].clone(), ndjson::scalar_to_json(&view.value(row, col)?)?);
        }
        obj.insert(schema.fields[col].name.clone(), Value::Object(inner));
    }
    Ok(Value::Object(obj))
}

// ---------------------------------------------------------------------------------------------
// `index` — `{"1": {"total": 9.5}, …}`, row-major, keyed by index as JSON object keys
// ---------------------------------------------------------------------------------------------

fn from_json_index(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let obj = require_object(value, "index")?;
    let declared = declared_id_field(schema);
    let id_name = index_field_name(declared);

    let keys = ordered_row_keys(obj.keys().cloned().collect());
    let mut items = Vec::with_capacity(obj.len());
    for key in &keys {
        let row_value = required_field(obj, key, "index")?;
        let inner = require_object(row_value, &format!("index.{key}"))?;
        let mut map = Map::with_capacity(inner.len() + 1);
        map.insert(id_name.to_string(), index_key_to_json(key, declared)?);
        for (column_name, cell) in inner.iter() {
            map.insert(column_name.clone(), cell.clone());
        }
        items.push(Value::Object(map));
    }
    ndjson::objects_to_batch(&items, schema)
}

fn to_json_index(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let id_col = schema.id_field();
    let payload_cols = schema.payload_fields();

    let mut obj = Map::with_capacity(view.len());
    for row in 0..view.len() {
        let key = match id_col {
            Some(col) => csv::format_value(&view.value(row, col)?)?.unwrap_or_default(),
            None => row.to_string(),
        };
        let mut inner = Map::with_capacity(payload_cols.len());
        for &col in &payload_cols {
            inner.insert(schema.fields[col].name.clone(), ndjson::scalar_to_json(&view.value(row, col)?)?);
        }
        obj.insert(key, Value::Object(inner));
    }
    Ok(Value::Object(obj))
}

// ---------------------------------------------------------------------------------------------
// `table` — the Frictionless Data "Table Schema", the lossless text shape
// ---------------------------------------------------------------------------------------------

fn table_type_to_field_type(type_name: &str, format: Option<&str>) -> Result<FieldType, Error> {
    match type_name {
        "integer" => Ok(FieldType::Int),
        "number" => Ok(FieldType::Float),
        "boolean" => Ok(FieldType::Bool),
        "string" => match format {
            Some("binary") => Ok(FieldType::Binary),
            Some(_) | None => Ok(FieldType::Text),
        },
        "date" => Ok(FieldType::Date),
        "datetime" => Ok(FieldType::Timestamp),
        "array" => Ok(FieldType::Vector),
        other => Err(Error::general_error(format!(
            "from_json: table: unsupported field type '{other}'"
        ))),
    }
}

fn field_type_to_table_type(field_type: FieldType) -> &'static str {
    match field_type {
        FieldType::Bool => "boolean",
        FieldType::Int => "integer",
        FieldType::UInt => "integer",
        FieldType::Float => "number",
        FieldType::Text => "string",
        FieldType::Binary => "string",
        FieldType::Date => "date",
        FieldType::Timestamp => "datetime",
        FieldType::Vector => "array",
    }
}

fn primary_key_names(value: &Value) -> Result<Vec<String>, Error> {
    match value {
        Value::String(name) => Ok(vec![name.clone()]),
        Value::Array(items) => items
            .iter()
            .map(|item| require_string(item, "table.schema.primaryKey element").map(|s| s.to_string()))
            .collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => Err(Error::general_error(
            "from_json: table: primaryKey must be a string or an array of strings".to_string(),
        )),
    }
}

/// Ignores the given `schema` argument entirely: a `table` document is schema-aware by
/// construction, and its own embedded Table Schema always wins (`ReadSchema::Infer` is what Phase
/// 3's test passes precisely because there is nothing else to infer *from*).
fn from_json_table(value: &Value) -> Result<RecordBatch, Error> {
    let obj = require_object(value, "table")?;
    let schema_value = required_field(obj, "schema", "table")?;
    let schema_obj = require_object(schema_value, "table.schema")?;
    let fields_value = required_field(schema_obj, "fields", "table.schema")?;
    let field_defs = require_array(fields_value, "table.schema.fields")?;

    let primary_key: Vec<String> = match schema_obj.get("primaryKey") {
        Some(value) => primary_key_names(value)?,
        None => Vec::new(),
    };

    let mut fields = Vec::with_capacity(field_defs.len());
    for field_def in field_defs {
        let field_obj = require_object(field_def, "table.schema.fields element")?;
        let name = require_string(required_field(field_obj, "name", "table.schema.fields element")?, "field name")?;
        let type_name = require_string(
            required_field(field_obj, "type", "table.schema.fields element")?,
            "field type",
        )?;
        let format = match field_obj.get("format") {
            Some(value) => Some(require_string(value, "field format")?),
            None => None,
        };
        let data_type = table_type_to_field_type(type_name, format)?;

        let mut field = crate::schema::FieldSchema::new(name, data_type);
        if let Some(title) = field_obj.get("title") {
            field = field.with_label(require_string(title, "field title")?.to_string());
        }
        if let Some(description) = field_obj.get("description") {
            field = field.with_description(require_string(description, "field description")?.to_string());
        }
        if primary_key.iter().any(|key| key == name) {
            field = field.with_key(KeyRole::Id).not_null();
        }
        // Frictionless `constraints.required` is our `nullable: false`.
        if let Some(constraints) = field_obj.get("constraints") {
            let constraints = require_object(constraints, "field constraints")?;
            if let Some(required) = constraints.get("required") {
                if required.as_bool() == Some(true) {
                    field = field.not_null();
                }
            }
        }
        fields.push(field);
    }
    let record_schema = RecordSchema::new(fields)?;

    let data_value = required_field(obj, "data", "table")?;
    let data = require_array(data_value, "table.data")?;
    ndjson::objects_to_batch(data, ReadSchema::Declared(&record_schema))
}

fn to_json_table(view: &dyn RecordView) -> Result<Value, Error> {
    let schema = view.schema();
    let field_defs: Vec<Value> = schema
        .fields
        .iter()
        .map(|field| {
            let mut obj = Map::with_capacity(4);
            obj.insert("name".to_string(), Value::String(field.name.clone()));
            obj.insert(
                "type".to_string(),
                Value::String(field_type_to_table_type(field.data_type).to_string()),
            );
            if field.data_type == FieldType::Binary {
                obj.insert("format".to_string(), Value::String("binary".to_string()));
            }
            obj.insert("title".to_string(), Value::String(field.label.clone()));
            if !field.description.is_empty() {
                obj.insert("description".to_string(), Value::String(field.description.clone()));
            }
            if !field.nullable {
                let mut constraints = Map::with_capacity(1);
                constraints.insert("required".to_string(), Value::Bool(true));
                obj.insert("constraints".to_string(), Value::Object(constraints));
            }
            Value::Object(obj)
        })
        .collect();

    let mut schema_obj = Map::with_capacity(2);
    schema_obj.insert("fields".to_string(), Value::Array(field_defs));
    if let Some(id_col) = schema.id_field() {
        schema_obj.insert(
            "primaryKey".to_string(),
            Value::Array(vec![Value::String(schema.fields[id_col].name.clone())]),
        );
    }

    let mut obj = Map::with_capacity(2);
    obj.insert("schema".to_string(), Value::Object(schema_obj));
    obj.insert("data".to_string(), Value::Array(ndjson::rows_to_json_values(view)?));
    Ok(Value::Object(obj))
}

// ---------------------------------------------------------------------------------------------
// `auto` — recognizes records, values, table, split and list; refuses the columns/index ambiguity
// ---------------------------------------------------------------------------------------------

fn array_value_len(value: &Value) -> Option<usize> {
    match value {
        Value::Array(items) => Some(items.len()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => None,
    }
}

fn from_json_auto(value: &Value, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    match value {
        Value::Array(items) => match items.first() {
            None => from_json_records(value, schema),
            Some(Value::Object(_)) => from_json_records(value, schema),
            Some(Value::Array(_)) => from_json_values(value, schema),
            Some(Value::Null) | Some(Value::Bool(_)) | Some(Value::Number(_)) | Some(Value::String(_)) => {
                Err(Error::general_error(
                    "from_json: auto: array elements are neither objects (records) nor arrays \
                     (values) — the shape is not recognized"
                        .to_string(),
                ))
            }
        },
        Value::Object(map) => {
            if map.contains_key("schema") && map.contains_key("data") {
                return from_json_table(value);
            }
            if map.contains_key("columns") && map.contains_key("data") {
                return from_json_split(value, schema);
            }
            let all_arrays = !map.is_empty() && map.values().all(|v| matches!(v, Value::Array(_)));
            if all_arrays {
                let mut lengths = map.values().filter_map(array_value_len);
                let first_len = lengths.next();
                if lengths.all(|len| Some(len) == first_len) {
                    return from_json_list(value, schema);
                }
            }
            let all_objects = !map.is_empty() && map.values().all(|v| matches!(v, Value::Object(_)));
            if all_objects {
                return Err(Error::general_error(
                    "from_json: auto: an object of objects is ambiguous between the `columns` \
                     and `index` orients (they are the same JSON shape transposed) — pick one \
                     explicitly"
                        .to_string(),
                ));
            }
            Err(Error::general_error(
                "from_json: auto: object shape not recognized (expected `table`, `split`, \
                 `list`, `columns` or `index`)"
                    .to_string(),
            ))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Err(Error::general_error(
            "from_json: auto: the top-level value must be a JSON array or object".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------------------------

/// Converts `view` to a plain JSON value in the given shape. `Auto` is a read-side concept only —
/// refused here, naming the orient to pick instead.
pub fn to_json(view: &dyn RecordView, orient: JsonOrient) -> Result<Value, Error> {
    match orient {
        JsonOrient::Auto => Err(Error::general_error(
            "to_json: `auto` detects a shape while reading; it cannot be used to write JSON — \
             pick a concrete orient (records, list, split, values, columns, index or table)"
                .to_string(),
        )),
        JsonOrient::Records => to_json_records(view),
        JsonOrient::List => to_json_list(view),
        JsonOrient::Split => to_json_split(view),
        JsonOrient::Values => to_json_values(view),
        JsonOrient::Columns => to_json_columns(view),
        JsonOrient::Index => to_json_index(view),
        JsonOrient::Table => to_json_table(view),
    }
}

/// Converts a plain JSON `value` in the given shape into a [`RecordBatch`]. `orient: Auto`
/// recognizes `records` (array of objects), `values` (array of arrays), `table` (an object with
/// `schema` and `data`), `split` (an object with `columns` and `data`) and `list` (an object whose
/// values are arrays of one length); an object of objects is ambiguous between `columns` and
/// `index` and is refused, asking for the orient explicitly.
pub fn from_json(value: &Value, orient: JsonOrient, schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    match orient {
        JsonOrient::Records => from_json_records(value, schema),
        JsonOrient::List => from_json_list(value, schema),
        JsonOrient::Split => from_json_split(value, schema),
        JsonOrient::Values => from_json_values(value, schema),
        JsonOrient::Columns => from_json_columns(value, schema),
        JsonOrient::Index => from_json_index(value, schema),
        JsonOrient::Table => from_json_table(value),
        JsonOrient::Auto => from_json_auto(value, schema),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_records::{Buffer, Column, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatch, RecordSchema};
    use std::sync::Arc;

    /// `Error` has no `From<serde_json::Error>` — CLAUDE.md's error-handling convention is typed
    /// constructors only, so Phase 3's `serde_json::from_str(...)?` inside a `-> Result<(), Error>`
    /// test does not compile as written (mirrors `csv.rs` tests' `utf8()` correction). Corrected
    /// with an explicit conversion here rather than adding a blanket `From` impl to
    /// `liquers_core::error`.
    fn parse_json(text: &str) -> Result<Value, Error> {
        serde_json::from_str(text).map_err(|e| Error::from_error(liquers_core::error::ErrorType::ConversionError, e))
    }

    fn orders_schema() -> RecordSchema {
        RecordSchema::new(vec![
            FieldSchema::new("order_id", FieldType::Int).with_key(KeyRole::Id),
            FieldSchema::new("total", FieldType::Float),
        ])
        .expect("schema")
    }

    #[test]
    fn orient_records_is_an_array_of_row_objects() -> Result<(), Error> {
        let json: Value = parse_json(r#"[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]"#)?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(1, 0)?, FieldValue::Int(2));

        let round_tripped = to_json(&batch, JsonOrient::Records)?;
        assert_eq!(round_tripped, json);
        Ok(())
    }

    #[test]
    fn orient_list_is_a_dictionary_of_columns() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"order_id":[1,2],"total":[9.5,3.0]}"#)?;
        let batch = from_json(&json, JsonOrient::List, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 1)?, FieldValue::Float(9.5));

        let round_tripped = to_json(&batch, JsonOrient::List)?;
        assert_eq!(round_tripped, json);
        Ok(())
    }

    #[test]
    fn orient_split_carries_columns_index_and_data_separately() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"columns":["total"],"index":[1,2],"data":[[9.5],[3.0]]}"#)?;
        let batch = from_json(&json, JsonOrient::Split, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 0)?, FieldValue::Int(1)); // the index becomes the Id column
        assert_eq!(batch.value(0, 1)?, FieldValue::Float(9.5));
        Ok(())
    }

    #[test]
    fn orient_values_is_rows_without_names() -> Result<(), Error> {
        let json: Value = parse_json(r#"[[1,9.5],[2,3.0]]"#)?;
        let batch = from_json(&json, JsonOrient::Values, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn orient_columns_keys_the_index_as_strings_unless_the_schema_says_otherwise() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"total":{"1":9.5,"2":3.0}}"#)?;
        let batch = from_json(&json, JsonOrient::Columns, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 0)?, FieldValue::Int(1)); // schema declares order_id as Int
        Ok(())
    }

    #[test]
    fn orient_index_is_row_keyed_by_the_id() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"1":{"total":9.5},"2":{"total":3.0}}"#)?;
        let batch = from_json(&json, JsonOrient::Index, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn orient_table_is_schema_aware_and_lossless() -> Result<(), Error> {
        let json: Value = parse_json(
            r#"{"schema":{"fields":[{"name":"order_id","type":"integer"},
                {"name":"total","type":"number"}],"primaryKey":["order_id"]},
               "data":[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]}"#,
        )?;
        // `table` carries its own schema — Infer is legitimate here, unlike every other shape.
        let batch = from_json(&json, JsonOrient::Table, ReadSchema::Infer)?;
        assert_eq!(batch.schema.id_field(), Some(0));
        assert_eq!(batch.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn auto_recognizes_records() -> Result<(), Error> {
        let json: Value = parse_json(r#"[{"order_id":1,"total":9.5}]"#)?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Infer)?;
        assert_eq!(batch.len, 1);
        Ok(())
    }

    #[test]
    fn auto_recognizes_list() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"order_id":[1,2],"total":[9.5,3.0]}"#)?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Infer)?;
        assert_eq!(batch.len, 2);
        Ok(())
    }

    #[test]
    fn auto_refuses_an_object_of_objects_as_ambiguous() {
        // `columns` and `index` have the same JSON shape transposed — auto cannot choose.
        let json: Value =
            parse_json(r#"{"1":{"order_id":1,"total":9.5},"2":{"order_id":2,"total":3.0}}"#).unwrap();
        let err = from_json(&json, JsonOrient::Auto, ReadSchema::Infer).expect_err("ambiguous shape");
        let message = format!("{err}").to_lowercase();
        assert!(message.contains("ambiguous") || message.contains("orient"));
    }

    #[test]
    fn json_orient_from_str_parses_every_name() {
        for (text, expected) in [
            ("records", JsonOrient::Records),
            ("list", JsonOrient::List),
            ("split", JsonOrient::Split),
            ("values", JsonOrient::Values),
            ("columns", JsonOrient::Columns),
            ("index", JsonOrient::Index),
            ("table", JsonOrient::Table),
            ("auto", JsonOrient::Auto),
        ] {
            assert_eq!(text.parse::<JsonOrient>().unwrap(), expected);
        }
    }

    #[test]
    fn to_json_refuses_auto_as_a_write_orient() -> Result<(), Error> {
        let json: Value = parse_json(r#"[{"order_id":1,"total":9.5}]"#)?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;
        let err = to_json(&batch, JsonOrient::Auto).expect_err("`Auto` is a read-side concept only");
        assert!(format!("{err}").to_lowercase().contains("auto"));
        Ok(())
    }

    // --- Additional coverage beyond Phase 3 §3.3 ---

    #[test]
    fn json_orient_from_str_refuses_unknown_names() {
        assert!("bogus".parse::<JsonOrient>().is_err());
    }

    #[test]
    fn orient_values_without_a_schema_infers_positional_names() -> Result<(), Error> {
        let json: Value = parse_json(r#"[[1,2],[3,4]]"#)?;
        let batch = from_json(&json, JsonOrient::Values, ReadSchema::Infer)?;
        assert_eq!(batch.schema.fields[0].name, "c0");
        assert_eq!(batch.schema.fields[1].name, "c1");
        Ok(())
    }

    #[test]
    fn orient_list_without_a_schema_infers_types() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"a":[1,2],"b":["x","y"]}"#)?;
        let batch = from_json(&json, JsonOrient::List, ReadSchema::Infer)?;
        let a = batch.schema.index_of("a").expect("a");
        let b = batch.schema.index_of("b").expect("b");
        assert_eq!(batch.schema.fields[a].data_type, FieldType::Int);
        assert_eq!(batch.schema.fields[b].data_type, FieldType::Text);
        Ok(())
    }

    #[test]
    fn orient_list_mismatched_column_lengths_is_an_error() {
        let json: Value = parse_json(r#"{"a":[1,2],"b":[1]}"#).unwrap();
        assert!(from_json(&json, JsonOrient::List, ReadSchema::Infer).is_err());
    }

    #[test]
    fn orient_values_wrong_row_width_is_an_error() {
        let json: Value = parse_json(r#"[[1,9.5],[2]]"#).unwrap();
        assert!(from_json(&json, JsonOrient::Values, ReadSchema::Declared(&orders_schema())).is_err());
    }

    #[test]
    fn orient_index_without_a_schema_keys_stay_text() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"1":{"total":9.5}}"#)?;
        let batch = from_json(&json, JsonOrient::Index, ReadSchema::Infer)?;
        let index_col = batch.schema.index_of("index").expect("index column");
        assert_eq!(batch.schema.fields[index_col].data_type, FieldType::Text);
        assert_eq!(batch.value(0, index_col)?, FieldValue::Text(std::sync::Arc::from("1")));
        Ok(())
    }

    #[test]
    fn to_json_split_round_trips_through_from_json() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"columns":["total"],"index":[1,2],"data":[[9.5],[3.0]]}"#)?;
        let batch = from_json(&json, JsonOrient::Split, ReadSchema::Declared(&orders_schema()))?;
        let rendered = to_json(&batch, JsonOrient::Split)?;
        let round_tripped = from_json(&rendered, JsonOrient::Split, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(round_tripped.value(0, 0)?, batch.value(0, 0)?);
        assert_eq!(round_tripped.value(1, 1)?, batch.value(1, 1)?);
        Ok(())
    }

    #[test]
    fn to_json_columns_and_index_round_trip_through_from_json() -> Result<(), Error> {
        let json: Value = parse_json(r#"[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]"#)?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;

        let columns_json = to_json(&batch, JsonOrient::Columns)?;
        let via_columns = from_json(&columns_json, JsonOrient::Columns, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(via_columns.value(1, 1)?, FieldValue::Float(3.0));

        let index_json = to_json(&batch, JsonOrient::Index)?;
        let via_index = from_json(&index_json, JsonOrient::Index, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(via_index.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn to_json_table_round_trips_through_from_json() -> Result<(), Error> {
        let json: Value = parse_json(r#"[{"order_id":1,"total":9.5},{"order_id":2,"total":3.0}]"#)?;
        let batch = from_json(&json, JsonOrient::Records, ReadSchema::Declared(&orders_schema()))?;
        let table_json = to_json(&batch, JsonOrient::Table)?;
        let round_tripped = from_json(&table_json, JsonOrient::Table, ReadSchema::Infer)?;
        assert_eq!(round_tripped.schema.id_field(), Some(0));
        assert_eq!(round_tripped.value(1, 1)?, FieldValue::Float(3.0));
        Ok(())
    }

    #[test]
    fn table_binary_field_uses_string_format_binary() -> Result<(), Error> {
        let json: Value = parse_json(
            r#"{"schema":{"fields":[{"name":"blob","type":"string","format":"binary"}]},
               "data":[{"blob":"AQID"}]}"#,
        )?;
        let batch = from_json(&json, JsonOrient::Table, ReadSchema::Infer)?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Binary);
        assert_eq!(batch.value(0, 0)?, FieldValue::Bytes(std::sync::Arc::from(vec![1u8, 2, 3])));
        Ok(())
    }

    #[test]
    fn table_unsupported_field_type_is_an_error() {
        let json: Value = parse_json(
            r#"{"schema":{"fields":[{"name":"x","type":"geopoint"}]},"data":[]}"#,
        )
        .unwrap();
        assert!(from_json(&json, JsonOrient::Table, ReadSchema::Infer).is_err());
    }

    #[test]
    fn auto_recognizes_table() -> Result<(), Error> {
        let json: Value = parse_json(
            r#"{"schema":{"fields":[{"name":"order_id","type":"integer"}],"primaryKey":["order_id"]},
               "data":[{"order_id":1}]}"#,
        )?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Infer)?;
        assert_eq!(batch.len, 1);
        Ok(())
    }

    #[test]
    fn auto_recognizes_split() -> Result<(), Error> {
        let json: Value = parse_json(r#"{"columns":["total"],"index":[1],"data":[[9.5]]}"#)?;
        let batch = from_json(&json, JsonOrient::Auto, ReadSchema::Declared(&orders_schema()))?;
        assert_eq!(batch.len, 1);
        Ok(())
    }

    #[test]
    fn auto_refuses_a_scalar_top_level_value() {
        let json: Value = parse_json("42").unwrap();
        assert!(from_json(&json, JsonOrient::Auto, ReadSchema::Infer).is_err());
    }

    #[test]
    fn index_and_columns_keep_numeric_row_order_past_nine() -> Result<(), Error> {
        let mut index = serde_json::Map::new();
        let mut column = serde_json::Map::new();
        for i in 0..12i64 {
            let mut row = serde_json::Map::new();
            row.insert("v".to_string(), Value::from(i));
            index.insert(i.to_string(), Value::Object(row));
            column.insert(i.to_string(), Value::from(i));
        }
        let mut columns = serde_json::Map::new();
        columns.insert("v".to_string(), Value::Object(column));
        for (orient, value) in [
            (JsonOrient::Index, Value::Object(index)),
            (JsonOrient::Columns, Value::Object(columns)),
        ] {
            let batch = from_json(&value, orient, ReadSchema::Infer)?;
            let v = batch.schema.index_of("v").ok_or_else(|| Error::general_error("v".to_string()))?;
            let read: Vec<FieldValue> = (0..batch.len).map(|row| batch.value(row, v)).collect::<Result<_, _>>()?;
            let expected: Vec<FieldValue> = (0..12i64).map(FieldValue::Int).collect();
            assert_eq!(read, expected, "{orient:?} rows in numeric order");
        }
        Ok(())
    }

    #[test]
    fn table_orient_round_trips_nullability() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("a", FieldType::Int).not_null(),
            FieldSchema::new("b", FieldType::Int),
        ])?);
        let batch = RecordBatch::new(
            schema,
            vec![
                Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) },
                Column::Int { validity: None, values: Buffer::from_slice(&[2i64]) },
            ],
            None,
            None,
            vec![],
        )?;
        let json = to_json(&batch, JsonOrient::Table)?;
        let back = from_json(&json, JsonOrient::Table, ReadSchema::Infer)?;
        assert!(!back.schema.fields[0].nullable);
        assert!(back.schema.fields[1].nullable);
        Ok(())
    }
}
