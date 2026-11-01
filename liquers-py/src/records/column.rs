//! `lq.RecordColumn` and `lq.RecordBuffer`: one column of a view, and one part of a column.
//!
//! A column object holds a `Column`, whose buffers are `Arc`s. A column of a `RecordBatch` is the
//! batch's own column (`as_batch`), sharing its storage; any other view's column is read once
//! (`RecordView::column`, which copies) and then lent the same way. Fixed-width columns export
//! their values with the buffer protocol (`memoryview(column)`); `Bool`, `Text` and `Binary`
//! expose their parts instead (`data`, `offsets`), and every column exposes `validity`.
//! `to_list()` is the copy that is always right.

use liquers_records::{Column, FieldType as CoreFieldType};
use pyo3::exceptions::{PyBufferError, PyIndexError};
use pyo3::ffi;
use pyo3::prelude::*;

use crate::error::error_to_pyerr;
use crate::records::buffer::{self, Layout, Part};
use crate::records::convert::field_value_to_py;
use crate::records::schema::FieldType;
use crate::records::LiveRecord;

/// One column of a record view.
#[pyclass(frozen, module = "liquers")]
pub struct RecordColumn {
    name: String,
    column: Column,
    /// The values' export, for fixed-width columns.
    layout: Option<Layout>,
    _live: LiveRecord,
}

impl RecordColumn {
    pub fn new(name: String, column: Column) -> Self {
        let layout = buffer::layout(&column, Part::Values);
        RecordColumn { name, column, layout, _live: LiveRecord::new() }
    }

    fn part(&self, py: Python<'_>, part: Part) -> PyResult<Option<Py<RecordBuffer>>> {
        match buffer::layout(&self.column, part) {
            Some(layout) => Ok(Some(Py::new(
                py,
                RecordBuffer { column: self.column.clone(), part, layout, _live: LiveRecord::new() },
            )?)),
            None => Ok(None),
        }
    }
}

#[pymethods]
impl RecordColumn {
    /// The column's field name.
    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// The column's `FieldType`.
    #[getter]
    fn r#type(&self) -> FieldType {
        FieldType::from_core(self.column.data_type())
    }

    /// The number of nulls.
    #[getter]
    fn null_count(&self) -> usize {
        self.column.null_mask().count_ones()
    }

    /// The validity bitmap (packed bytes, LSB first; a set bit is a value), or `None` when the
    /// column has no nulls recorded. Bits past the last row may be unmasked.
    #[getter]
    fn validity(&self, py: Python<'_>) -> PyResult<Option<Py<RecordBuffer>>> {
        self.part(py, Part::Validity)
    }

    /// The `i32` offsets of a `Text` or `Binary` column (one more than the rows), else `None`.
    #[getter]
    fn offsets(&self, py: Python<'_>) -> PyResult<Option<Py<RecordBuffer>>> {
        self.part(py, Part::Offsets)
    }

    /// The bytes behind the column: packed bits for `Bool`, the UTF-8 or binary data for `Text` /
    /// `Binary`, and the values for a fixed-width column.
    #[getter]
    fn data(&self, py: Python<'_>) -> PyResult<Option<Py<RecordBuffer>>> {
        self.part(py, Part::Data)
    }

    /// The width of a `Vector` column, else `None`.
    #[getter]
    fn dim(&self) -> Option<usize> {
        match &self.column {
            Column::Vector { dim, .. } => Some(*dim),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. } => None,
        }
    }

    /// The cells as Python objects, nulls as `None`.
    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        (0..self.column.len())
            .map(|i| {
                self.column
                    .get(i)
                    .and_then(|value| field_value_to_py(py, &value))
                    .map_err(|e| error_to_pyerr(py, &e))
            })
            .collect()
    }

    fn __len__(&self) -> usize {
        self.column.len()
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<Py<PyAny>> {
        let len = self.column.len() as isize;
        let position = if index < 0 { index + len } else { index };
        if position < 0 || position >= len {
            return Err(PyIndexError::new_err(format!("row {index} out of range for {len} rows")));
        }
        self.column
            .get(position as usize)
            .and_then(|value| field_value_to_py(py, &value))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    fn __repr__(&self) -> String {
        format!(
            "RecordColumn(name='{}', type='{}', len={})",
            self.name,
            FieldType::from_core(self.column.data_type()).name(),
            self.column.len()
        )
    }

    /// The values of a fixed-width column, read-only. `Bool`, `Text` and `Binary` columns raise
    /// `BufferError`; use their `data`, `offsets` and `validity` parts.
    unsafe fn __getbuffer__(slf: Bound<'_, Self>, view: *mut ffi::Py_buffer, flags: std::ffi::c_int) -> PyResult<()> {
        let this = slf.get();
        let (Some(layout), Some(bytes)) = (this.layout.as_ref(), buffer::part_bytes(&this.column, Part::Values)) else {
            let name = match this.column.data_type() {
                CoreFieldType::Bool => "Bool",
                CoreFieldType::Text => "Text",
                CoreFieldType::Binary => "Binary",
                CoreFieldType::Int
                | CoreFieldType::UInt
                | CoreFieldType::Float
                | CoreFieldType::Date
                | CoreFieldType::Timestamp
                | CoreFieldType::Vector => "this",
            };
            return Err(PyBufferError::new_err(format!(
                "a {name} column has no single buffer; use its data, offsets and validity"
            )));
        };
        // SAFETY: `view` comes from Python's buffer request. `bytes` and `layout` belong to the
        // column this object holds immutably; the `Py_buffer` keeps the object alive.
        unsafe { buffer::fill(view, slf.clone().into_any(), bytes, layout, flags) }
    }

    unsafe fn __releasebuffer__(&self, _view: *mut ffi::Py_buffer) {}
}

/// One part of a record column (`validity`, `offsets` or `data`), lent read-only with the buffer
/// protocol. It holds the column, so its bytes outlive the view and the column object they came
/// from.
#[pyclass(frozen, module = "liquers")]
pub struct RecordBuffer {
    column: Column,
    part: Part,
    layout: Layout,
    _live: LiveRecord,
}

#[pymethods]
impl RecordBuffer {
    /// The `struct` format of an item: `B` (bytes), `i` (int32), `q` (int64), `Q` (uint64),
    /// `d` (float64) or `f` (float32).
    #[getter]
    fn format(&self) -> String {
        self.layout.format.to_string_lossy().into_owned()
    }

    /// The number of items.
    fn __len__(&self) -> usize {
        self.layout.shape.first().copied().unwrap_or(0) as usize
    }

    fn __repr__(&self) -> String {
        format!("RecordBuffer({}, format='{}', len={})", self.part.name(), self.format(), self.__len__())
    }

    unsafe fn __getbuffer__(slf: Bound<'_, Self>, view: *mut ffi::Py_buffer, flags: std::ffi::c_int) -> PyResult<()> {
        let this = slf.get();
        let Some(bytes) = buffer::part_bytes(&this.column, this.part) else {
            return Err(PyBufferError::new_err("this part has no bytes"));
        };
        // SAFETY: as for `RecordColumn::__getbuffer__`: the bytes and layout belong to this object.
        unsafe { buffer::fill(view, slf.clone().into_any(), bytes, &this.layout, flags) }
    }

    unsafe fn __releasebuffer__(&self, _view: *mut ffi::Py_buffer) {}
}
