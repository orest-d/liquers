//! The buffer protocol for record columns: `memoryview(column)` and `memoryview(column.data)`.
//!
//! A column's buffers are `Arc`-shared and immutable (`liquers-records`' `Buffer`, `Bitmap`,
//! `AlignedBuffer`), so a Python object that holds a clone of the `Column` keeps the exported
//! bytes alive and unchanged for as long as any `memoryview` of it exists: the `Py_buffer` holds a
//! reference to that object (`view.obj`), and the object holds the `Arc`. Every export is
//! read-only; a writable request raises `BufferError`. This module holds the only `unsafe` of the
//! records wrappers.

use std::ffi::{c_char, c_int, c_void, CStr};

use liquers_records::Column;
use pyo3::exceptions::PyBufferError;
use pyo3::ffi;
use pyo3::prelude::*;

/// Which bytes of a column an export lends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The values of a fixed-width column (`Int`, `UInt`, `Float`, `Date`, `Timestamp`,
    /// `Vector`), typed.
    Values,
    /// The validity bitmap, packed bytes, LSB first.
    Validity,
    /// The `i32` offsets of a `Text` or `Binary` column.
    Offsets,
    /// The bytes behind a column: packed bits for `Bool`, the UTF-8 or binary data of `Text` /
    /// `Binary`, the values of a fixed-width column as bytes.
    Data,
}

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Values => "values",
            Part::Validity => "validity",
            Part::Offsets => "offsets",
            Part::Data => "data",
        }
    }
}

/// The shape of one export: its `struct` format, item size, shape and strides.
#[derive(Debug, Clone)]
pub struct Layout {
    pub format: &'static CStr,
    pub itemsize: isize,
    pub shape: Vec<isize>,
    pub strides: Vec<isize>,
}

impl Layout {
    fn flat(format: &'static CStr, itemsize: isize, count: usize) -> Layout {
        Layout { format, itemsize, shape: vec![count as isize], strides: vec![itemsize] }
    }
}

/// The bytes of `part` of `column`, or `None` when the column has no such part.
pub fn part_bytes(column: &Column, part: Part) -> Option<&[u8]> {
    let len = column.len();
    fn bitmap_bytes(bitmap: &liquers_records::Bitmap, len: usize) -> &[u8] {
        let bytes = bitmap.as_bytes();
        &bytes[..len.div_ceil(8).min(bytes.len())]
    }
    let bitmap_bytes = |bitmap| bitmap_bytes(bitmap, len);
    match part {
        Part::Validity => validity(column).map(bitmap_bytes),
        Part::Offsets => match column {
            Column::Text { offsets, .. } | Column::Binary { offsets, .. } => Some(offsets.as_bytes()),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => None,
        },
        Part::Values => match column {
            Column::Int { values, .. } | Column::Timestamp { values, .. } => Some(values.as_bytes()),
            Column::UInt { values, .. } => Some(values.as_bytes()),
            Column::Float { values, .. } => Some(values.as_bytes()),
            Column::Date { values, .. } => Some(values.as_bytes()),
            Column::Vector { data, .. } => Some(data.as_bytes()),
            Column::Bool { .. } | Column::Text { .. } | Column::Binary { .. } => None,
        },
        Part::Data => match column {
            Column::Bool { values, .. } => Some(bitmap_bytes(values)),
            Column::Text { data, .. } | Column::Binary { data, .. } => Some(data.as_bytes()),
            Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => part_bytes(column, Part::Values),
        },
    }
}

/// The layout `part` of `column` exports with, or `None` when it has no such part.
pub fn layout(column: &Column, part: Part) -> Option<Layout> {
    let bytes = part_bytes(column, part)?.len();
    Some(match part {
        Part::Validity => Layout::flat(c"B", 1, bytes),
        Part::Offsets => Layout::flat(c"i", 4, bytes / 4),
        Part::Values => match column {
            Column::Int { .. } | Column::Timestamp { .. } => Layout::flat(c"q", 8, bytes / 8),
            Column::UInt { .. } => Layout::flat(c"Q", 8, bytes / 8),
            Column::Float { .. } => Layout::flat(c"d", 8, bytes / 8),
            Column::Date { .. } => Layout::flat(c"i", 4, bytes / 4),
            Column::Vector { dim, .. } => Layout {
                format: c"f",
                itemsize: 4,
                shape: vec![column.len() as isize, *dim as isize],
                strides: vec![(*dim as isize) * 4, 4],
            },
            Column::Bool { .. } | Column::Text { .. } | Column::Binary { .. } => return None,
        },
        Part::Data => match column {
            Column::Bool { .. } | Column::Text { .. } | Column::Binary { .. } => Layout::flat(c"B", 1, bytes),
            Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return layout(column, Part::Values),
        },
    })
}

pub fn validity(column: &Column) -> Option<&liquers_records::Bitmap> {
    match column {
        Column::Bool { validity, .. }
        | Column::Int { validity, .. }
        | Column::UInt { validity, .. }
        | Column::Float { validity, .. }
        | Column::Text { validity, .. }
        | Column::Binary { validity, .. }
        | Column::Date { validity, .. }
        | Column::Timestamp { validity, .. }
        | Column::Vector { validity, .. } => validity.as_ref(),
    }
}

/// Fills `view` with a read-only export of `bytes`, owned by `owner`.
///
/// # Safety
///
/// `view` must be the `Py_buffer` Python passed to `__getbuffer__`. `bytes` and `layout` must live
/// as long as `owner` does: they are borrowed from the object `owner` refers to, which holds them
/// immutably, and the `Py_buffer` keeps `owner` alive until it is released.
pub unsafe fn fill(
    view: *mut ffi::Py_buffer,
    owner: Bound<'_, PyAny>,
    bytes: &[u8],
    layout: &Layout,
    flags: c_int,
) -> PyResult<()> {
    if view.is_null() {
        return Err(PyBufferError::new_err("a null Py_buffer was passed"));
    }
    if (flags & ffi::PyBUF_WRITABLE) == ffi::PyBUF_WRITABLE {
        return Err(PyBufferError::new_err("record buffers are read-only"));
    }
    let multi = layout.shape.len() > 1;
    if multi && (flags & ffi::PyBUF_ND) != ffi::PyBUF_ND {
        return Err(PyBufferError::new_err("this buffer has two dimensions; request its shape"));
    }
    // SAFETY: `view` is non-null and points to the `Py_buffer` being filled (the caller's
    // contract). `owner` is moved into `obj`, which owns that strong reference until
    // `PyBuffer_Release`. `buf`, `format`, `shape` and `strides` point into data `owner` keeps
    // alive and never mutates, and the export is marked read-only.
    unsafe {
        (*view).obj = owner.into_ptr();
        (*view).buf = bytes.as_ptr() as *mut c_void;
        (*view).len = bytes.len() as isize;
        (*view).readonly = 1;
        (*view).itemsize = layout.itemsize;
        (*view).format = if (flags & ffi::PyBUF_FORMAT) == ffi::PyBUF_FORMAT {
            layout.format.as_ptr() as *mut c_char
        } else {
            std::ptr::null_mut()
        };
        (*view).ndim = layout.shape.len() as c_int;
        (*view).shape = if (flags & ffi::PyBUF_ND) == ffi::PyBUF_ND {
            layout.shape.as_ptr() as *mut isize
        } else {
            std::ptr::null_mut()
        };
        (*view).strides = if (flags & ffi::PyBUF_STRIDES) == ffi::PyBUF_STRIDES {
            layout.strides.as_ptr() as *mut isize
        } else {
            std::ptr::null_mut()
        };
        (*view).suboffsets = std::ptr::null_mut();
        (*view).internal = std::ptr::null_mut();
    }
    Ok(())
}
