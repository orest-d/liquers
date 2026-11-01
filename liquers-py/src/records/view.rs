//! `lq.RecordView`: a table, read through the `RecordView` trait without copying it.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_records::{ChunkId, RecordView as CoreRecordView};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyList, PyString};

use crate::error::error_to_pyerr;
use crate::records::column::RecordColumn;
use crate::records::convert::field_value_to_py;
use crate::records::schema::{origins_to_py, type_name, RecordSchema};
use crate::records::LiveRecord;

/// A table of records: a schema, and rows read through it.
///
/// A view reaches Python from a query (`env.evaluate("…/ns-rec/to_record-csv")`), a command
/// argument, or a constructor (`RecordView.from_records`, `from_dict`, `from_pandas`,
/// `from_polars`, `from_arrow`). Narrowing (`select_columns`, `slice`, `row`, `cell`) makes
/// another view and copies nothing; `to_list()` and `to_dict()` copy into Python objects.
#[pyclass(frozen, module = "liquers")]
pub struct RecordView {
    pub inner: Arc<dyn CoreRecordView>,
    _live: LiveRecord,
}

impl RecordView {
    pub fn new(inner: Arc<dyn CoreRecordView>) -> Self {
        RecordView { inner, _live: LiveRecord::new() }
    }

    fn column_index(&self, column: &Bound<'_, PyAny>) -> Result<usize, Error> {
        let schema = self.inner.schema();
        if let Ok(name) = column.cast::<PyString>() {
            let name = name.to_string();
            return schema.index_of(&name).ok_or_else(|| {
                Error::general_error(format!(
                    "no field named '{name}' (available: {})",
                    schema.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
                ))
            });
        }
        if column.cast::<PyInt>().is_ok() {
            let index: isize = column.extract().map_err(|e| crate::error::pyerr_to_error(column.py(), &e))?;
            let len = schema.fields.len() as isize;
            let position = if index < 0 { index + len } else { index };
            if position < 0 || position >= len {
                return Err(Error::general_error(format!("column {index} out of range for {len} fields")));
            }
            return Ok(position as usize);
        }
        Err(Error::general_error(format!(
            "a column is a field name or an index, not {}",
            type_name(column)
        )))
    }

    fn column_name(&self, column: &Bound<'_, PyAny>) -> Result<String, Error> {
        let index = self.column_index(column)?;
        Ok(self.inner.schema().fields[index].name.clone())
    }

    fn row_index(&self, row: isize) -> Result<usize, Error> {
        let len = self.inner.len() as isize;
        let position = if row < 0 { row + len } else { row };
        if position < 0 || position >= len {
            return Err(Error::general_error(format!("row {row} out of range for {len} rows")));
        }
        Ok(position as usize)
    }

    fn wrap(py: Python<'_>, view: Result<Arc<dyn CoreRecordView>, Error>) -> PyResult<RecordView> {
        view.map(RecordView::new).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The single-cell shape's scalar, or an error naming the view's shape.
    pub fn item_of(view: &Arc<dyn CoreRecordView>) -> Result<liquers_records::FieldValue, Error> {
        if liquers_lib::value::record_view_is_single_cell(view.as_ref()) {
            return view.single_cell();
        }
        Err(Error::conversion_error_with_message(
            format!("{} rows x {} fields", view.len(), view.schema().fields.len()),
            "a single value",
            "item() needs one row and one value field",
        ))
    }
}

#[pymethods]
impl RecordView {
    /// The view's `RecordSchema`.
    #[getter]
    fn schema(&self) -> RecordSchema {
        RecordSchema::from_core(self.inner.schema().clone())
    }

    /// The chunk the rows came from (`lq.Key` for a stored chunk, `lq.Query` for a computed one),
    /// or `None` for a table assembled from several chunks or built in memory.
    #[getter]
    fn chunk_id(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(match self.inner.chunk_id() {
            Some(ChunkId::Key(key)) => Py::new(py, crate::query::Key(key.clone()))?.into_any(),
            Some(ChunkId::Query(query)) => Py::new(py, crate::query::Query(query.clone()))?.into_any(),
            None => py.None(),
        })
    }

    /// The `ChunkOrigin`s a `Source` field indexes, in order.
    #[getter]
    fn origins(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        origins_to_py(py, self.inner.origins())
    }

    /// The number of rows.
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// The number of fields.
    #[getter]
    fn num_columns(&self) -> usize {
        self.inner.schema().fields.len()
    }

    /// One column, by name or index. A column of a stored batch shares its storage; any other is
    /// read once.
    fn column(&self, py: Python<'_>, column: &Bound<'_, PyAny>) -> PyResult<RecordColumn> {
        let built = (|| -> Result<RecordColumn, Error> {
            let index = self.column_index(column)?;
            let name = self.inner.schema().fields[index].name.clone();
            let data = match self.inner.as_batch() {
                Some(batch) => batch.columns[index].clone(),
                None => self.inner.column(index)?,
            };
            Ok(RecordColumn::new(name, data))
        })();
        built.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Every column, in field order.
    fn columns(&self, py: Python<'_>) -> PyResult<Vec<RecordColumn>> {
        (0..self.num_columns())
            .map(|index| self.column(py, &index.into_pyobject(py)?.into_any()))
            .collect()
    }

    /// The cell at `row` (a position) and `col` (a field name or index), as a Python scalar.
    fn value(&self, py: Python<'_>, row: isize, col: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let read = (|| -> Result<Py<PyAny>, Error> {
            let value = self.inner.value(self.row_index(row)?, self.column_index(col)?)?;
            field_value_to_py(py, &value)
        })();
        read.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The named columns, plus the `Id` and `Source` fields whether named or not, as
    /// `ns-rec/select_columns` keeps them. Always a view, even of one cell.
    #[pyo3(signature = (*columns))]
    fn select_columns(&self, py: Python<'_>, columns: Vec<Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let names: Result<Vec<String>, Error> = columns.iter().map(|c| self.column_name(c)).collect();
        let selected = names.and_then(|names| {
            if names.is_empty() {
                return Err(Error::general_error("select_columns needs at least one column".to_string()));
            }
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            self.inner.select_columns(&names)
        });
        RecordView::wrap(py, selected)
    }

    /// `length` rows from `offset`, both clamped to the view as `ns-rec/slice` clamps them.
    fn slice(&self, py: Python<'_>, offset: usize, length: usize) -> PyResult<RecordView> {
        let len = self.inner.len();
        let offset = offset.min(len);
        let length = length.min(len - offset);
        RecordView::wrap(py, self.inner.slice(offset, length))
    }

    /// One row, by position. Always a view.
    fn row(&self, py: Python<'_>, index: isize) -> PyResult<RecordView> {
        let row = self.row_index(index).and_then(|row| self.inner.row(row));
        RecordView::wrap(py, row)
    }

    /// One cell as a view: the row, then the column (with the key fields, as `select_columns`).
    fn cell(&self, py: Python<'_>, row: isize, col: &Bound<'_, PyAny>) -> PyResult<RecordView> {
        let cell = self
            .row_index(row)
            .and_then(|row| Ok((row, self.column_name(col)?)))
            .and_then(|(row, name)| self.inner.cell(row, &name));
        RecordView::wrap(py, cell)
    }

    /// The value of a one-row view with one value field (the shape a query returns as a scalar).
    /// Any other shape raises `ConversionError` naming it.
    fn item(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        RecordView::item_of(&self.inner)
            .and_then(|value| field_value_to_py(py, &value))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The rows, as a list of dicts from field name to value.
    fn to_list(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let names: Vec<Bound<'_, PyString>> =
            self.inner.schema().fields.iter().map(|f| PyString::new(py, &f.name)).collect();
        let rows = PyList::empty(py);
        for row in 0..self.inner.len() {
            let dict = PyDict::new(py);
            for (col, name) in names.iter().enumerate() {
                let value = self
                    .inner
                    .value(row, col)
                    .and_then(|value| field_value_to_py(py, &value))
                    .map_err(|e| error_to_pyerr(py, &e))?;
                dict.set_item(name, value)?;
            }
            rows.append(dict)?;
        }
        Ok(rows.unbind())
    }

    /// The columns, as a dict from field name to a list of values.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        for (col, field) in self.inner.schema().fields.iter().enumerate() {
            let values = PyList::empty(py);
            for row in 0..self.inner.len() {
                let value = self
                    .inner
                    .value(row, col)
                    .and_then(|value| field_value_to_py(py, &value))
                    .map_err(|e| error_to_pyerr(py, &e))?;
                values.append(value)?;
            }
            dict.set_item(&field.name, values)?;
        }
        Ok(dict.unbind())
    }

    /// A view from a list of dicts, one per row. Columns follow the first-seen key order; a
    /// missing key is a null. `fields=` declares some fields (a dict from name to a type, its
    /// lowercase name or an `lq.FieldSchema`) or all of them (an `lq.RecordSchema`); the rest are
    /// inferred.
    #[staticmethod]
    #[pyo3(signature = (rows, *, fields=None))]
    fn from_records(py: Python<'_>, rows: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let built = crate::records::build::read_records(rows)
            .and_then(|raw| Ok((raw, crate::records::build::Fields::extract(fields)?)))
            .and_then(|(raw, fields)| crate::records::build::build(raw, fields));
        RecordView::wrap(py, built)
    }

    /// A view from a dict of equally long lists, one per column. `fields=` as for `from_records`.
    #[staticmethod]
    #[pyo3(signature = (columns, *, fields=None))]
    fn from_dict(py: Python<'_>, columns: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let built = crate::records::build::read_dict(columns)
            .and_then(|raw| Ok((raw, crate::records::build::Fields::extract(fields)?)))
            .and_then(|(raw, fields)| crate::records::build::build(raw, fields));
        RecordView::wrap(py, built)
    }

    /// The view as a `pyarrow.Table` (a copy), with the roles (`liquers.schema`), the chunk id
    /// (`liquers.chunk_id`) and the origins (`liquers.origins`) in its schema metadata. Needs
    /// `liquers-py[arrow]`.
    fn to_arrow(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::records::arrow::to_arrow(py, &self.inner).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The view as a `pandas.DataFrame` (a copy, through Arrow; metadata is dropped). Needs
    /// `liquers-py[pandas]`.
    fn to_pandas(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::records::arrow::to_pandas(py, &self.inner).map_err(|e| error_to_pyerr(py, &e))
    }

    /// The view as a `polars.DataFrame` (a copy; metadata is dropped). Needs `liquers-py[polars]`.
    fn to_polars(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::records::arrow::to_polars(py, &self.inner).map_err(|e| error_to_pyerr(py, &e))
    }

    /// A view of a `pyarrow.Table` or `RecordBatch` (a copy). Narrow integers and floats, large
    /// and view strings, dictionaries, `date64` and timestamps of other units are normalized
    /// first when no value is lost; any other column raises `ConversionError` naming it.
    /// `fields=` as for `from_records`. Needs `liquers-py[arrow]`.
    #[staticmethod]
    #[pyo3(signature = (table, *, fields=None))]
    fn from_arrow(py: Python<'_>, table: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let built = crate::records::build::Fields::extract(fields)
            .and_then(|fields| crate::records::arrow::with_fields(crate::records::arrow::from_arrow(table)?, fields));
        RecordView::wrap(py, built)
    }

    /// A view of a `pandas.DataFrame` (a copy, through Arrow; the index is dropped). An integer
    /// column holding `None` is `float64` in pandas and arrives as `Float`, unless the frame uses
    /// `Int64` or `fields=` declares it. Needs `liquers-py[pandas]`.
    #[staticmethod]
    #[pyo3(signature = (df, *, fields=None))]
    fn from_pandas(py: Python<'_>, df: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let built = crate::records::build::Fields::extract(fields)
            .and_then(|fields| crate::records::arrow::with_fields(crate::records::arrow::from_pandas(df)?, fields));
        RecordView::wrap(py, built)
    }

    /// A view of a `polars.DataFrame` (a copy, through the polars bridge; no pyarrow needed). A
    /// column the bridge cannot carry (a list, a decimal, a duration) raises `ConversionError`
    /// naming it. Needs `liquers-py[polars]`.
    #[staticmethod]
    #[pyo3(signature = (df, *, fields=None))]
    fn from_polars(py: Python<'_>, df: &Bound<'_, PyAny>, fields: Option<&Bound<'_, PyAny>>) -> PyResult<RecordView> {
        let built = crate::records::build::Fields::extract(fields)
            .and_then(|fields| crate::records::arrow::with_fields(crate::records::arrow::from_polars(df)?, fields));
        RecordView::wrap(py, built)
    }

    fn __repr__(&self) -> String {
        let names: Vec<&str> = self.inner.schema().fields.iter().map(|f| f.name.as_str()).collect();
        format!("RecordView({} rows, fields=[{}])", self.inner.len(), names.join(", "))
    }
}
