//! Projection steps into record views: one row, one column or one cell of a table.
//!
//! These are the record counterparts of `liquers_core::accessor::steps`: lenses from an
//! `ExtValue::RecordView` to a part of it, for `ValueAccessor::project`. A part is plain data (a
//! row is an object of field values, a column a list, a cell a scalar). A `put` builds a new batch
//! that shares every column except the edited one, so a table edit never mutates the old value.
//!
//! # Main components
//!
//! - [`RowStep`]: row `i`, as an object `{field: value}`.
//! - [`ColumnStep`]: a column by name, as a list of its values.
//! - [`CellStep`]: one cell, as a scalar.
//!
//! # Where it is used
//!
//! - Rust code that edits a table through an accessor (no `acc` command covers rows, columns or
//!   cells yet).
//!
//! # Related concepts
//!
//! - `liquers_core::accessor::ProjectionStep`: the trait these implement.
//! - `liquers_records::RecordBatch`: the batch a `put` rebuilds; its schema is unchanged.
//!
//! # Examples
//!
//! ```
//! # use std::sync::Arc;
//! # use liquers_core::{accessor::ProjectionStep, value::ValueInterface};
//! # use liquers_lib::{records::accessor::CellStep, value::{ExtValueInterface, Value}};
//! # use liquers_records::{Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema};
//! # let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)]).unwrap());
//! # let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) };
//! # let table = Value::from_record_view(Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).unwrap()));
//! let cell = CellStep::new(1, "n");
//! assert_eq!(cell.get(&table).unwrap().unwrap().try_into_i64().unwrap(), 2);
//! let edited = cell.put(&table, Value::from(5i64)).unwrap();
//! assert_eq!(cell.get(&edited).unwrap().unwrap().try_into_i64().unwrap(), 5);
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-lib/tests/value_accessor_commands.rs):
//!   `record_row_column_and_cell_projections`.

use std::borrow::Cow;
use std::sync::Arc;

use liquers_core::accessor::{PartDescription, ProjectionStep};
use liquers_core::error::Error;
use liquers_core::value::ValueInterface;
use liquers_records::{Column, ColumnMut, FieldType, FieldValue, RecordBatch, RecordView};

use crate::value::{ExtValueInterface, Value};

fn view_of(value: &Value) -> Result<Arc<dyn RecordView>, Error> {
    value.as_record_view()
}

fn column_index(view: &dyn RecordView, name: &str) -> Option<usize> {
    view.schema().fields.iter().position(|f| f.name == name)
}

/// A cell as a value.
fn cell_value(cell: &FieldValue) -> Result<Value, Error> {
    match cell {
        FieldValue::Null => Ok(Value::none()),
        FieldValue::Bool(b) => Ok(Value::from_bool(*b)),
        FieldValue::Int(n) | FieldValue::Timestamp(n) => Ok(Value::from(*n)),
        FieldValue::UInt(n) => match i64::try_from(*n) {
            Ok(n) => Ok(Value::from(n)),
            Err(_) => Ok(Value::from(*n as f64)),
        },
        FieldValue::Float(x) => Ok(Value::from(*x)),
        FieldValue::Text(t) => Ok(Value::from(t.as_ref())),
        FieldValue::Bytes(b) => Ok(Value::from_bytes(b.to_vec())),
        FieldValue::Date(days) => Ok(Value::from(i64::from(*days))),
        FieldValue::Vector(v) => {
            let items = v.iter().map(|x| serde_json::json!(f64::from(*x))).collect();
            Value::try_from_json_value(&serde_json::Value::Array(items))
        }
    }
}

/// A cell as JSON, for a row object or a column list.
fn cell_json(cell: &FieldValue) -> Result<serde_json::Value, Error> {
    cell_value(cell)?.try_into_json_value()
}

/// A value as a cell of a column of `data_type`.
fn field_value(value: &Value, data_type: FieldType) -> Result<FieldValue, Error> {
    if value.is_none() {
        return Ok(FieldValue::Null);
    }
    let refused = || {
        Error::conversion_error_with_message(
            value.identifier(),
            "record cell",
            &format!("a {data_type:?} column cannot hold it"),
        )
    };
    match data_type {
        FieldType::Bool => value.try_into_bool().map(FieldValue::Bool),
        FieldType::Int => value.try_into_i64().map(FieldValue::Int),
        FieldType::Timestamp => value.try_into_i64().map(FieldValue::Timestamp),
        FieldType::UInt => {
            let n = value.try_into_i64()?;
            u64::try_from(n).map(FieldValue::UInt).map_err(|_| refused())
        }
        FieldType::Float => value
            .try_into_f64()
            .or_else(|_| value.try_into_i64().map(|n| n as f64))
            .map(FieldValue::Float),
        FieldType::Text => value.try_into_string().map(|t| FieldValue::Text(t.into())),
        FieldType::Binary => value.try_into_bytes().map(|b| FieldValue::Bytes(b.into())),
        FieldType::Date => {
            let n = value.try_into_i64()?;
            i32::try_from(n).map(FieldValue::Date).map_err(|_| refused())
        }
        FieldType::Vector => {
            let json = value.try_into_json_value()?;
            let items = json.as_array().ok_or_else(refused)?;
            let floats = items
                .iter()
                .map(|x| x.as_f64().map(|f| f as f32).ok_or_else(refused))
                .collect::<Result<Vec<f32>, Error>>()?;
            Ok(FieldValue::Vector(floats.into()))
        }
    }
}

/// `column` with the rows `edit` names replaced.
fn rebuild_column(
    column: &Column,
    edit: impl Fn(usize) -> Option<FieldValue>,
) -> Result<Column, Error> {
    let mut built = ColumnMut::with_capacity(column.data_type(), column.len());
    for row in 0..column.len() {
        match edit(row) {
            Some(value) => built.push(&value)?,
            None => built.push(&column.get(row)?)?,
        }
    }
    Ok(built.freeze())
}

/// A new batch of `view` with column `col` replaced by `edit`'s values.
fn with_column(
    view: &dyn RecordView,
    col: usize,
    edit: impl Fn(usize) -> Option<FieldValue>,
) -> Result<Value, Error> {
    let batch = view.materialize()?;
    let mut columns = Vec::with_capacity(batch.schema().fields.len());
    for c in 0..batch.schema().fields.len() {
        let column = batch.column(c)?;
        columns.push(if c == col { rebuild_column(&column, &edit)? } else { column });
    }
    let edited = RecordBatch::new(batch.schema().clone(), columns, None, None, Vec::new())?;
    Ok(Value::from_record_view(Arc::new(edited)))
}

fn describe(step: String) -> PartDescription {
    PartDescription {
        step,
        ..PartDescription::default()
    }
}

/// Row `i` of a record view, as an object of its field values.
#[derive(Debug, Clone)]
pub struct RowStep(pub usize);

impl ProjectionStep<Value> for RowStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("row-{}", self.0))
    }
    fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
        let view = view_of(parent)?;
        if self.0 >= view.len() {
            return Ok(None);
        }
        let mut row = serde_json::Map::new();
        for (c, field) in view.schema().fields.iter().enumerate() {
            row.insert(field.name.clone(), cell_json(&view.value(self.0, c)?)?);
        }
        Ok(Some(Value::try_from_json_value(&serde_json::Value::Object(row))?))
    }
    fn put(&self, parent: &Value, part: Value) -> Result<Value, Error> {
        let view = view_of(parent)?;
        if self.0 >= view.len() {
            return Err(Error::general_error(format!("there is no row {} to set", self.0)));
        }
        let json = part.try_into_json_value()?;
        let object = json
            .as_object()
            .ok_or_else(|| Error::conversion_error(part.identifier(), "record row"))?;
        let mut current: Value = parent.clone();
        for (name, cell) in object {
            let col = column_index(view_of(&current)?.as_ref(), name).ok_or_else(|| {
                Error::general_error(format!("the table has no column '{name}'"))
            })?;
            let data_type = view.schema().fields[col].data_type;
            let cell = field_value(&Value::try_from_json_value(cell)?, data_type)?;
            let row = self.0;
            current = with_column(view_of(&current)?.as_ref(), col, |r| {
                (r == row).then(|| cell.clone())
            })?;
        }
        Ok(current)
    }
    fn anchor(&self, parent: &Value) -> Result<Option<u64>, Error> {
        let len = view_of(parent)?.len();
        Ok((self.0 < len).then(|| ((len as u64) << 32) ^ self.0 as u64))
    }
    fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
        Ok(describe(format!("row-{}", self.0)))
    }
}

/// A column of a record view by name, as the list of its values.
#[derive(Debug, Clone)]
pub struct ColumnStep(pub String);

impl ColumnStep {
    pub fn new(name: impl Into<String>) -> Self {
        ColumnStep(name.into())
    }
}

impl ProjectionStep<Value> for ColumnStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("column-{}", self.0))
    }
    fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
        let view = view_of(parent)?;
        let Some(col) = column_index(view.as_ref(), &self.0) else {
            return Ok(None);
        };
        let column = view.column(col)?;
        let values = (0..column.len())
            .map(|row| cell_json(&column.get(row)?))
            .collect::<Result<Vec<serde_json::Value>, Error>>()?;
        Ok(Some(Value::try_from_json_value(&serde_json::Value::Array(values))?))
    }
    fn put(&self, parent: &Value, part: Value) -> Result<Value, Error> {
        let view = view_of(parent)?;
        let col = column_index(view.as_ref(), &self.0)
            .ok_or_else(|| Error::general_error(format!("the table has no column '{}'", self.0)))?;
        let data_type = view.schema().fields[col].data_type;
        let json = part.try_into_json_value()?;
        let items = json
            .as_array()
            .ok_or_else(|| Error::conversion_error(part.identifier(), "record column"))?;
        if items.len() != view.len() {
            return Err(Error::general_error(format!(
                "a column of {} rows cannot replace one of {}",
                items.len(),
                view.len()
            )));
        }
        let cells = items
            .iter()
            .map(|item| field_value(&Value::try_from_json_value(item)?, data_type))
            .collect::<Result<Vec<FieldValue>, Error>>()?;
        with_column(view.as_ref(), col, |row| cells.get(row).cloned())
    }
    fn anchor(&self, parent: &Value) -> Result<Option<u64>, Error> {
        let view = view_of(parent)?;
        Ok(column_index(view.as_ref(), &self.0).map(|col| col as u64))
    }
    fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
        Ok(describe(format!("column-{}", self.0)))
    }
}

/// One cell of a record view, by row and column name.
#[derive(Debug, Clone)]
pub struct CellStep {
    pub row: usize,
    pub column: String,
}

impl CellStep {
    pub fn new(row: usize, column: impl Into<String>) -> Self {
        CellStep {
            row,
            column: column.into(),
        }
    }
}

impl ProjectionStep<Value> for CellStep {
    fn name(&self) -> Cow<'_, str> {
        Cow::Owned(format!("cell-{}-{}", self.row, self.column))
    }
    fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
        let view = view_of(parent)?;
        match column_index(view.as_ref(), &self.column) {
            Some(col) if self.row < view.len() => Ok(Some(cell_value(&view.value(self.row, col)?)?)),
            Some(_) | None => Ok(None),
        }
    }
    fn put(&self, parent: &Value, part: Value) -> Result<Value, Error> {
        let view = view_of(parent)?;
        let col = column_index(view.as_ref(), &self.column).ok_or_else(|| {
            Error::general_error(format!("the table has no column '{}'", self.column))
        })?;
        if self.row >= view.len() {
            return Err(Error::general_error(format!("there is no row {} to set", self.row)));
        }
        let cell = field_value(&part, view.schema().fields[col].data_type)?;
        let row = self.row;
        with_column(view.as_ref(), col, |r| (r == row).then(|| cell.clone()))
    }
    fn anchor(&self, parent: &Value) -> Result<Option<u64>, Error> {
        let view = view_of(parent)?;
        let len = view.len();
        Ok(column_index(view.as_ref(), &self.column)
            .filter(|_| self.row < len)
            .map(|col| ((len as u64) << 40) ^ ((col as u64) << 32) ^ self.row as u64))
    }
    fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
        Ok(describe(format!("cell-{}-{}", self.row, self.column)))
    }
}
