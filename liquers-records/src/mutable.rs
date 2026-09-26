//! `RecordViewMut`, `RecordBatchMut` and `ColumnMut` — the mutable, growable counterparts of
//! `RecordView` and `RecordBatch`, plus `RecordBatch::into_mut`.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Writing a view". A `RecordBatchMut`
//! is filled row by row (`append_row`) or column by column (`column_mut`), read while it is being
//! built (it implements `RecordView`, copying the requested range since the columns are still
//! changing), and turned into an immutable `RecordBatch` with `freeze()`. Editing a stored table is
//! `into_mut()`, edit, `freeze()` — a new value, never the old one mutated in place.
//!
//! `ColumnMut`'s value buffer is aligned from the start (`AlignedBytesMut`, `buffer.rs`'s growable
//! counterpart of `AlignedBuffer`), so `freeze()` hands it to the new `Column` without copying, and
//! `RecordBatch::into_mut` takes over a column's storage the same way — without copying — when this
//! batch is its only owner (`Arc::try_unwrap` succeeds); otherwise it copies. No `unsafe`.

use std::ops::Range;
use std::sync::Arc;

use liquers_core::error::Error;

use crate::batch::{RecordBatch, RecordView};
use crate::buffer::{AlignedBytesMut, Bitmap, Buffer};
use crate::column::{Column, FieldValue};
use crate::schema::{FieldType, RecordSchema};

/// A table being built or edited, readable as a view while it is written. A trait rather than one
/// struct's methods, so code that fills a table — a reader, a command — is written once against
/// it. Supertrait `RecordView` so a `RecordViewMut` can be read mid-build the same way any other
/// view is read.
pub trait RecordViewMut: RecordView {
    /// Room for `additional` more rows in every column — the capacity call.
    fn reserve(&mut self, additional: usize);

    /// One row, in schema order. Type-checked; `FieldValue::Null` sets the validity bit.
    fn append_row(&mut self, values: &[FieldValue]) -> Result<(), Error>;

    fn set_value(&mut self, row: usize, col: usize, value: &FieldValue) -> Result<(), Error>;

    /// One column, for bulk and typed writes. Columns may then differ in length for a while:
    /// `len()` is the **shortest** column's length, and `freeze()` requires them equal.
    fn column_mut(&mut self, col: usize) -> Result<&mut ColumnMut, Error>;
}

/// Names the mismatch between a pushed/set `FieldValue` and the column's declared type.
fn type_mismatch(value: &FieldValue, expected: &str, who: &str) -> Error {
    Error::conversion_error(format!("{value:?}"), format!("{expected} ({who})"))
}

/// `None` when every row is valid — the same "validity omitted when there are no nulls"
/// convention `Column` follows.
fn validity_option(validity: &[bool]) -> Option<Bitmap> {
    if validity.iter().all(|&valid| valid) {
        None
    } else {
        Some(Bitmap::from_bools(validity))
    }
}

/// `Column`'s `validity: Option<Bitmap>` expanded to one bool per row — the growable form
/// `ColumnMut` keeps validity in. Copies unconditionally: validity is a bit per row, negligible
/// next to the value buffers `AlignedBuffer::into_mut` moves without copying.
fn validity_to_vec(validity: Option<Bitmap>, len: usize) -> Vec<bool> {
    match validity {
        Some(bitmap) => (0..len).map(|i| bitmap.get(i)).collect(),
        None => vec![true; len],
    }
}

/// Overwrites the `row`-th variable-length cell of a `Text`/`Binary` column. There is no cheaper
/// in-place update: replacing one cell can change every later offset, so the tail is rebuilt.
/// `is_text` picks which `FieldValue` variant is accepted and how the mismatch is named.
fn set_bytes_cell(
    validity: &mut [bool],
    offsets: &mut Vec<i32>,
    data: &mut AlignedBytesMut,
    row: usize,
    value: &FieldValue,
    is_text: bool,
) -> Result<(), Error> {
    let expected = if is_text { "Text" } else { "Bytes" };
    let new_cell: Option<Vec<u8>> = match value {
        FieldValue::Null => None,
        FieldValue::Text(v) if is_text => Some(v.as_bytes().to_vec()),
        FieldValue::Bytes(v) if !is_text => Some(v.to_vec()),
        other => return Err(type_mismatch(other, expected, "ColumnMut::set")),
    };
    let old_bytes = data.as_bytes().to_vec();
    let row_count = offsets.len() - 1;
    let mut new_data = Vec::with_capacity(old_bytes.len());
    let mut new_offsets = Vec::with_capacity(offsets.len());
    new_offsets.push(0i32);
    for i in 0..row_count {
        if i == row {
            new_data.extend_from_slice(new_cell.as_deref().unwrap_or(&[]));
        } else {
            let start = offsets[i] as usize;
            let end = offsets[i + 1] as usize;
            new_data.extend_from_slice(&old_bytes[start..end]);
        }
        new_offsets.push(new_data.len() as i32);
    }
    *data = AlignedBytesMut::new();
    data.extend_from_slice(&new_data);
    *offsets = new_offsets;
    validity[row] = new_cell.is_some();
    Ok(())
}

/// The growable storage behind [`ColumnMut`], one variant per [`FieldType`] — the mutable mirror
/// of [`Column`]. Private: `ColumnMut` is the public surface, so a private type here never leaks a
/// `pub(crate)` type (`AlignedBytesMut`) into a public field.
#[derive(Debug, Clone)]
enum ColumnMutInner {
    Bool { validity: Vec<bool>, values: Vec<bool> },
    Int { validity: Vec<bool>, values: AlignedBytesMut },
    UInt { validity: Vec<bool>, values: AlignedBytesMut },
    Float { validity: Vec<bool>, values: AlignedBytesMut },
    Text { validity: Vec<bool>, offsets: Vec<i32>, data: AlignedBytesMut },
    Binary { validity: Vec<bool>, offsets: Vec<i32>, data: AlignedBytesMut },
    Date { validity: Vec<bool>, values: AlignedBytesMut },
    Timestamp { validity: Vec<bool>, values: AlignedBytesMut },
    Vector { validity: Vec<bool>, dim: usize, data: AlignedBytesMut },
}

impl ColumnMutInner {
    fn with_capacity(data_type: FieldType, rows: usize) -> Self {
        let offsets_with_capacity = |rows: usize| {
            let mut offsets = Vec::with_capacity(rows + 1);
            offsets.push(0i32);
            offsets
        };
        match data_type {
            FieldType::Bool => ColumnMutInner::Bool {
                validity: Vec::with_capacity(rows),
                values: Vec::with_capacity(rows),
            },
            FieldType::Int => ColumnMutInner::Int {
                validity: Vec::with_capacity(rows),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i64>()),
            },
            FieldType::UInt => ColumnMutInner::UInt {
                validity: Vec::with_capacity(rows),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<u64>()),
            },
            FieldType::Float => ColumnMutInner::Float {
                validity: Vec::with_capacity(rows),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<f64>()),
            },
            FieldType::Text => ColumnMutInner::Text {
                validity: Vec::with_capacity(rows),
                offsets: offsets_with_capacity(rows),
                data: AlignedBytesMut::new(),
            },
            FieldType::Binary => ColumnMutInner::Binary {
                validity: Vec::with_capacity(rows),
                offsets: offsets_with_capacity(rows),
                data: AlignedBytesMut::new(),
            },
            FieldType::Date => ColumnMutInner::Date {
                validity: Vec::with_capacity(rows),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i32>()),
            },
            FieldType::Timestamp => ColumnMutInner::Timestamp {
                validity: Vec::with_capacity(rows),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i64>()),
            },
            FieldType::Vector => ColumnMutInner::Vector {
                validity: Vec::with_capacity(rows),
                dim: 0,
                data: AlignedBytesMut::new(),
            },
        }
    }

    fn data_type(&self) -> FieldType {
        match self {
            ColumnMutInner::Bool { .. } => FieldType::Bool,
            ColumnMutInner::Int { .. } => FieldType::Int,
            ColumnMutInner::UInt { .. } => FieldType::UInt,
            ColumnMutInner::Float { .. } => FieldType::Float,
            ColumnMutInner::Text { .. } => FieldType::Text,
            ColumnMutInner::Binary { .. } => FieldType::Binary,
            ColumnMutInner::Date { .. } => FieldType::Date,
            ColumnMutInner::Timestamp { .. } => FieldType::Timestamp,
            ColumnMutInner::Vector { .. } => FieldType::Vector,
        }
    }

    fn len(&self) -> usize {
        match self {
            ColumnMutInner::Bool { values, .. } => values.len(),
            ColumnMutInner::Int { values, .. } => values.len() / std::mem::size_of::<i64>(),
            ColumnMutInner::UInt { values, .. } => values.len() / std::mem::size_of::<u64>(),
            ColumnMutInner::Float { values, .. } => values.len() / std::mem::size_of::<f64>(),
            ColumnMutInner::Text { offsets, .. } => offsets.len().saturating_sub(1),
            ColumnMutInner::Binary { offsets, .. } => offsets.len().saturating_sub(1),
            ColumnMutInner::Date { values, .. } => values.len() / std::mem::size_of::<i32>(),
            ColumnMutInner::Timestamp { values, .. } => values.len() / std::mem::size_of::<i64>(),
            ColumnMutInner::Vector { dim, data, .. } => {
                if *dim == 0 {
                    0
                } else {
                    data.len() / (std::mem::size_of::<f32>() * dim)
                }
            }
        }
    }

    fn reserve(&mut self, additional: usize) {
        match self {
            ColumnMutInner::Bool { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional);
            }
            ColumnMutInner::Int { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<i64>());
            }
            ColumnMutInner::UInt { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<u64>());
            }
            ColumnMutInner::Float { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<f64>());
            }
            ColumnMutInner::Text { validity, offsets, .. } => {
                validity.reserve(additional);
                offsets.reserve(additional);
            }
            ColumnMutInner::Binary { validity, offsets, .. } => {
                validity.reserve(additional);
                offsets.reserve(additional);
            }
            ColumnMutInner::Date { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<i32>());
            }
            ColumnMutInner::Timestamp { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<i64>());
            }
            ColumnMutInner::Vector { validity, dim, data } => {
                validity.reserve(additional);
                data.reserve(additional * std::mem::size_of::<f32>() * (*dim).max(1));
            }
        }
    }

    fn push(&mut self, value: &FieldValue) -> Result<(), Error> {
        match self {
            ColumnMutInner::Bool { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.push(false);
                }
                FieldValue::Bool(v) => {
                    validity.push(true);
                    values.push(*v);
                }
                other => return Err(type_mismatch(other, "Bool", "ColumnMut::push")),
            },
            ColumnMutInner::Int { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.extend_from_slice(bytemuck::bytes_of(&0i64));
                }
                FieldValue::Int(v) => {
                    validity.push(true);
                    values.extend_from_slice(bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Int", "ColumnMut::push")),
            },
            ColumnMutInner::UInt { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.extend_from_slice(bytemuck::bytes_of(&0u64));
                }
                FieldValue::UInt(v) => {
                    validity.push(true);
                    values.extend_from_slice(bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "UInt", "ColumnMut::push")),
            },
            ColumnMutInner::Float { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.extend_from_slice(bytemuck::bytes_of(&0f64));
                }
                FieldValue::Float(v) => {
                    validity.push(true);
                    values.extend_from_slice(bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Float", "ColumnMut::push")),
            },
            ColumnMutInner::Text { validity, offsets, data } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    offsets.push(data.len() as i32);
                }
                FieldValue::Text(v) => {
                    validity.push(true);
                    data.extend_from_slice(v.as_bytes());
                    offsets.push(data.len() as i32);
                }
                other => return Err(type_mismatch(other, "Text", "ColumnMut::push")),
            },
            ColumnMutInner::Binary { validity, offsets, data } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    offsets.push(data.len() as i32);
                }
                FieldValue::Bytes(v) => {
                    validity.push(true);
                    data.extend_from_slice(v);
                    offsets.push(data.len() as i32);
                }
                other => return Err(type_mismatch(other, "Bytes", "ColumnMut::push")),
            },
            ColumnMutInner::Date { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.extend_from_slice(bytemuck::bytes_of(&0i32));
                }
                FieldValue::Date(v) => {
                    validity.push(true);
                    values.extend_from_slice(bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Date", "ColumnMut::push")),
            },
            ColumnMutInner::Timestamp { validity, values } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    values.extend_from_slice(bytemuck::bytes_of(&0i64));
                }
                FieldValue::Timestamp(v) => {
                    validity.push(true);
                    values.extend_from_slice(bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Timestamp", "ColumnMut::push")),
            },
            ColumnMutInner::Vector { validity, dim, data } => match value {
                FieldValue::Null => {
                    validity.push(false);
                    let width = *dim * std::mem::size_of::<f32>();
                    data.extend_from_slice(&vec![0u8; width]);
                }
                FieldValue::Vector(v) => {
                    if *dim == 0 {
                        *dim = v.len();
                    }
                    if v.len() != *dim {
                        return Err(Error::general_error(format!(
                            "ColumnMut::push: Vector dim mismatch ({} vs {dim})",
                            v.len()
                        )));
                    }
                    validity.push(true);
                    data.extend_from_slice(bytemuck::cast_slice(v.as_ref()));
                }
                other => return Err(type_mismatch(other, "Vector", "ColumnMut::push")),
            },
        }
        Ok(())
    }

    fn set(&mut self, row: usize, value: &FieldValue) -> Result<(), Error> {
        let len = self.len();
        if row >= len {
            return Err(Error::general_error(format!(
                "ColumnMut::set: row {row} out of range (0..{len})"
            )));
        }
        match self {
            ColumnMutInner::Bool { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values[row] = false;
                }
                FieldValue::Bool(v) => {
                    validity[row] = true;
                    values[row] = *v;
                }
                other => return Err(type_mismatch(other, "Bool", "ColumnMut::set")),
            },
            ColumnMutInner::Int { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(&0i64));
                }
                FieldValue::Int(v) => {
                    validity[row] = true;
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Int", "ColumnMut::set")),
            },
            ColumnMutInner::UInt { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values.set(row * std::mem::size_of::<u64>(), bytemuck::bytes_of(&0u64));
                }
                FieldValue::UInt(v) => {
                    validity[row] = true;
                    values.set(row * std::mem::size_of::<u64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "UInt", "ColumnMut::set")),
            },
            ColumnMutInner::Float { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values.set(row * std::mem::size_of::<f64>(), bytemuck::bytes_of(&0f64));
                }
                FieldValue::Float(v) => {
                    validity[row] = true;
                    values.set(row * std::mem::size_of::<f64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Float", "ColumnMut::set")),
            },
            ColumnMutInner::Text { validity, offsets, data } => {
                set_bytes_cell(validity, offsets, data, row, value, true)?;
            }
            ColumnMutInner::Binary { validity, offsets, data } => {
                set_bytes_cell(validity, offsets, data, row, value, false)?;
            }
            ColumnMutInner::Date { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values.set(row * std::mem::size_of::<i32>(), bytemuck::bytes_of(&0i32));
                }
                FieldValue::Date(v) => {
                    validity[row] = true;
                    values.set(row * std::mem::size_of::<i32>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Date", "ColumnMut::set")),
            },
            ColumnMutInner::Timestamp { validity, values } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(&0i64));
                }
                FieldValue::Timestamp(v) => {
                    validity[row] = true;
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Timestamp", "ColumnMut::set")),
            },
            ColumnMutInner::Vector { validity, dim, data } => match value {
                FieldValue::Null => {
                    validity[row] = false;
                    let width = *dim * std::mem::size_of::<f32>();
                    data.set(row * width, &vec![0u8; width]);
                }
                FieldValue::Vector(v) => {
                    if v.len() != *dim {
                        return Err(Error::general_error(format!(
                            "ColumnMut::set: Vector dim mismatch ({} vs {dim})",
                            v.len()
                        )));
                    }
                    validity[row] = true;
                    let width = *dim * std::mem::size_of::<f32>();
                    data.set(row * width, bytemuck::cast_slice(v.as_ref()));
                }
                other => return Err(type_mismatch(other, "Vector", "ColumnMut::set")),
            },
        }
        Ok(())
    }

    /// Infallible: every value that reached this column was already type-checked by `push`/`set`.
    fn freeze(self) -> Column {
        match self {
            ColumnMutInner::Bool { validity, values } => Column::Bool {
                validity: validity_option(&validity),
                values: Bitmap::from_bools(&values),
            },
            ColumnMutInner::Int { validity, values } => Column::Int {
                validity: validity_option(&validity),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::UInt { validity, values } => Column::UInt {
                validity: validity_option(&validity),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Float { validity, values } => Column::Float {
                validity: validity_option(&validity),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Text { validity, offsets, data } => Column::Text {
                validity: validity_option(&validity),
                offsets: Buffer::from_slice(&offsets),
                data: data.freeze(),
            },
            ColumnMutInner::Binary { validity, offsets, data } => Column::Binary {
                validity: validity_option(&validity),
                offsets: Buffer::from_slice(&offsets),
                data: data.freeze(),
            },
            ColumnMutInner::Date { validity, values } => Column::Date {
                validity: validity_option(&validity),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Timestamp { validity, values } => Column::Timestamp {
                validity: validity_option(&validity),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Vector { validity, dim, data } => Column::Vector {
                validity: validity_option(&validity),
                dim,
                data: Buffer::from_aligned(data.freeze()),
            },
        }
    }
}

/// The mutable column: a growable typed buffer plus validity. Its value buffer is an aligned
/// buffer from the start (`AlignedBytesMut`), so [`ColumnMut::freeze`] is a move for the bulk of
/// the data, not a copy. See phase2-architecture.md §"Writing a view".
///
/// An opaque struct wrapping a private enum, rather than a public enum itself, so its growable
/// storage type (`pub(crate) AlignedBytesMut`) never has to be exposed as a public field.
#[derive(Debug, Clone)]
pub struct ColumnMut {
    inner: ColumnMutInner,
}

impl ColumnMut {
    pub fn new(data_type: FieldType) -> Self {
        ColumnMut::with_capacity(data_type, 0)
    }

    /// The expected number of rows, allocated once.
    pub fn with_capacity(data_type: FieldType, rows: usize) -> Self {
        ColumnMut { inner: ColumnMutInner::with_capacity(data_type, rows) }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Type-checked against this column's declared type; `FieldValue::Null` sets the validity bit
    /// rather than being written as data.
    pub fn push(&mut self, value: &FieldValue) -> Result<(), Error> {
        self.inner.push(value)
    }

    /// Overwrites an already-pushed row. Out of range (`row >= len()`) is an error.
    pub fn set(&mut self, row: usize, value: &FieldValue) -> Result<(), Error> {
        self.inner.set(row, value)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The logical type this column was built with — the inverse of [`Column::data_type`].
    pub fn data_type(&self) -> FieldType {
        self.inner.data_type()
    }

    /// Consumes this column into its frozen, immutable [`Column`].
    pub fn freeze(self) -> Column {
        self.inner.freeze()
    }
}

/// Converts an existing, immutable [`Column`] into a growable [`ColumnMut`] — the per-column
/// half of [`RecordBatch::into_mut`]. Takes over the value buffer without copying when this
/// `Column` is its only owner (`Buffer::into_aligned`/[`crate::buffer::AlignedBuffer::into_mut`]
/// succeed via `Arc::try_unwrap`); copies it otherwise. Validity always copies — a bit per row is
/// negligible next to the value buffer this optimizes.
fn column_into_mut(column: Column) -> ColumnMut {
    let inner = match column {
        Column::Bool { validity, values } => {
            let len = values.len();
            let bool_values: Vec<bool> = (0..len).map(|i| values.get(i)).collect();
            ColumnMutInner::Bool { validity: validity_to_vec(validity, len), values: bool_values }
        }
        Column::Int { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Int {
                validity: validity_to_vec(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::UInt { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::UInt {
                validity: validity_to_vec(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Float { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Float {
                validity: validity_to_vec(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Text { validity, offsets, data } => {
            let len = offsets.as_slice().len().saturating_sub(1);
            ColumnMutInner::Text {
                validity: validity_to_vec(validity, len),
                offsets: offsets.as_slice().to_vec(),
                data: data.into_mut(),
            }
        }
        Column::Binary { validity, offsets, data } => {
            let len = offsets.as_slice().len().saturating_sub(1);
            ColumnMutInner::Binary {
                validity: validity_to_vec(validity, len),
                offsets: offsets.as_slice().to_vec(),
                data: data.into_mut(),
            }
        }
        Column::Date { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Date {
                validity: validity_to_vec(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Timestamp { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Timestamp {
                validity: validity_to_vec(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Vector { validity, dim, data } => {
            let len = if dim == 0 { 0 } else { data.as_slice().len() / dim };
            ColumnMutInner::Vector {
                validity: validity_to_vec(validity, len),
                dim,
                data: data.into_aligned().into_mut(),
            }
        }
    };
    ColumnMut { inner }
}

/// The mutable table: owned, unshared, growable. `freeze()` makes it a `RecordBatch`.
#[derive(Debug)]
pub struct RecordBatchMut {
    schema: Arc<RecordSchema>,
    columns: Vec<ColumnMut>,
}

impl RecordBatchMut {
    pub fn new(schema: Arc<RecordSchema>) -> Self {
        RecordBatchMut::with_capacity(schema, 0)
    }

    /// The expected number of rows, allocated once in every column.
    pub fn with_capacity(schema: Arc<RecordSchema>, rows: usize) -> Self {
        let columns = schema
            .fields
            .iter()
            .map(|field| ColumnMut::with_capacity(field.data_type, rows))
            .collect();
        RecordBatchMut { schema, columns }
    }

    /// The shortest column's length — columns may differ in length while `column_mut` is filling
    /// them independently; `freeze()` is what requires them equal.
    fn computed_len(&self) -> usize {
        self.columns.iter().map(ColumnMut::len).min().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.computed_len()
    }

    pub fn is_empty(&self) -> bool {
        self.computed_len() == 0
    }

    /// Fails unless every column holds the same number of rows — `RecordBatch::new` already
    /// performs that check against the schema, so this just hands it the frozen columns.
    pub fn freeze(self) -> Result<RecordBatch, Error> {
        let columns: Vec<Column> = self.columns.into_iter().map(ColumnMut::freeze).collect();
        RecordBatch::new(self.schema, columns, None, None, Vec::new())
    }
}

impl RecordViewMut for RecordBatchMut {
    fn reserve(&mut self, additional: usize) {
        for column in &mut self.columns {
            column.reserve(additional);
        }
    }

    fn append_row(&mut self, values: &[FieldValue]) -> Result<(), Error> {
        if values.len() != self.columns.len() {
            return Err(Error::general_error(format!(
                "RecordBatchMut::append_row: {} values given, but the schema declares {} fields",
                values.len(),
                self.columns.len()
            )));
        }
        for (column, value) in self.columns.iter_mut().zip(values.iter()) {
            column.push(value)?;
        }
        Ok(())
    }

    fn set_value(&mut self, row: usize, col: usize, value: &FieldValue) -> Result<(), Error> {
        self.column_mut(col)?.set(row, value)
    }

    fn column_mut(&mut self, col: usize) -> Result<&mut ColumnMut, Error> {
        let len = self.columns.len();
        self.columns.get_mut(col).ok_or_else(|| {
            Error::general_error(format!(
                "RecordBatchMut::column_mut: column index {col} out of range (0..{len})"
            ))
        })
    }
}

/// `column_range` copies the requested range: the columns are still changing, so nothing here can
/// be a shared view into storage a later `push` might reallocate out from under it.
impl RecordView for RecordBatchMut {
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.computed_len()
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let column = self.columns.get(col).ok_or_else(|| {
            Error::general_error(format!(
                "RecordBatchMut::column_range: column index {col} out of range (0..{})",
                self.columns.len()
            ))
        })?;
        column.clone().freeze().slice(rows.start, rows.len())
    }
}

impl RecordBatch {
    /// A mutable table with these rows. Buffers this batch holds alone are taken over
    /// (`Arc::try_unwrap`, via [`crate::buffer::AlignedBuffer::into_mut`]); buffers shared with
    /// another `RecordBatch` — typically because this one was `.clone()`d from an `Arc` a second
    /// owner still holds — are copied instead.
    pub fn into_mut(self) -> RecordBatchMut {
        let schema = self.schema;
        let columns = self.columns.into_iter().map(column_into_mut).collect();
        RecordBatchMut { schema, columns }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldSchema;

    fn int_schema(names: &[&str]) -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(names.iter().map(|n| FieldSchema::new(*n, FieldType::Int)).collect())
                .expect("schema"),
        )
    }

    #[test]
    fn with_capacity_starts_empty() {
        let builder = RecordBatchMut::with_capacity(int_schema(&["id", "count"]), 100);
        assert_eq!(builder.len(), 0);
    }

    #[test]
    fn append_row_advances_len() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 10);
        builder.append_row(&[FieldValue::Int(1)]).expect("append_row");
        builder.append_row(&[FieldValue::Int(2)]).expect("append_row");
        assert_eq!(builder.len(), 2);
    }

    #[test]
    fn set_value_overwrites_an_appended_cell() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 10);
        builder.append_row(&[FieldValue::Int(0)]).expect("append_row");
        builder.set_value(0, 0, &FieldValue::Int(42)).expect("set_value");
        let frozen = builder.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(42));
    }

    #[test]
    fn column_mut_starts_empty_and_grows_independently() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["col1", "col2"]), 10);
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(1)).expect("push");
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(2)).expect("push");
        builder.column_mut(1).expect("col2").push(&FieldValue::Int(10)).expect("push");
        // `len()` is the shortest column's length while columns are still being filled.
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn freeze_fails_on_uneven_columns() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["col1", "col2"]), 10);
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(1)).expect("push");
        builder.column_mut(0).expect("col1").push(&FieldValue::Int(2)).expect("push");
        builder.column_mut(1).expect("col2").push(&FieldValue::Int(10)).expect("push");
        assert!(builder.freeze().is_err());
    }

    #[test]
    fn column_mut_freeze_returns_a_frozen_column() {
        let mut col = ColumnMut::with_capacity(FieldType::Int, 10);
        col.push(&FieldValue::Int(42)).expect("push");
        assert_eq!(col.freeze().len(), 1);
    }

    #[test]
    fn record_batch_into_mut_takes_over_an_unshared_buffer() {
        let schema = int_schema(&["value"]);
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) };
        let batch = RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch");
        let mut mutable = batch.into_mut();
        mutable.set_value(0, 0, &FieldValue::Int(99)).expect("set_value");
        let frozen = mutable.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(99));
    }

    #[test]
    fn record_batch_into_mut_copies_a_shared_buffer() {
        let schema = int_schema(&["value"]);
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) };
        let batch = RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch");
        let batch_arc1 = Arc::new(batch);
        let batch_arc2 = batch_arc1.clone(); // second owner: `into_mut` must copy, not steal
        let mut mutable = (*batch_arc1).clone().into_mut();
        drop(batch_arc2);
        mutable.set_value(0, 0, &FieldValue::Int(99)).expect("set_value");
        let frozen = mutable.freeze().expect("freeze");
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(99));
        // The original, seen through `batch_arc1`, is untouched — the copy-on-write idiom RECORDS04
        // in §6 checks at the value-crossing boundary.
        assert_eq!(batch_arc1.value(0, 0).expect("value"), FieldValue::Int(1));
    }

    // --- Additional coverage beyond Phase 3 §2.6 ---

    #[test]
    fn append_row_rejects_wrong_value_count() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["a", "b"]), 4);
        assert!(builder.append_row(&[FieldValue::Int(1)]).is_err());
    }

    #[test]
    fn push_type_mismatch_is_an_error() {
        let mut col = ColumnMut::new(FieldType::Int);
        assert!(col.push(&FieldValue::Text(Arc::from("x"))).is_err());
    }

    #[test]
    fn push_null_sets_the_validity_bit() {
        let mut col = ColumnMut::new(FieldType::Int);
        col.push(&FieldValue::Int(1)).expect("push");
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Int(3)).expect("push");
        let frozen = col.freeze();
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Int(1));
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(2).expect("get"), FieldValue::Int(3));
    }

    #[test]
    fn set_out_of_range_row_is_an_error() {
        let mut col = ColumnMut::new(FieldType::Int);
        col.push(&FieldValue::Int(1)).expect("push");
        assert!(col.set(5, &FieldValue::Int(2)).is_err());
    }

    #[test]
    fn column_mut_round_trips_text_values() {
        let mut col = ColumnMut::new(FieldType::Text);
        col.push(&FieldValue::Text(Arc::from("ab"))).expect("push");
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Text(Arc::from("cde"))).expect("push");
        let frozen = col.freeze();
        assert_eq!(frozen.len(), 3);
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Text(Arc::from("ab")));
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(2).expect("get"), FieldValue::Text(Arc::from("cde")));
    }

    #[test]
    fn column_mut_set_overwrites_a_text_cell_and_rebuilds_offsets() {
        let mut col = ColumnMut::new(FieldType::Text);
        col.push(&FieldValue::Text(Arc::from("ab"))).expect("push");
        col.push(&FieldValue::Text(Arc::from("cde"))).expect("push");
        col.set(0, &FieldValue::Text(Arc::from("longer"))).expect("set");
        let frozen = col.freeze();
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Text(Arc::from("longer")));
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Text(Arc::from("cde")));
    }

    #[test]
    fn column_mut_round_trips_vector_values() {
        let mut col = ColumnMut::new(FieldType::Vector);
        col.push(&FieldValue::Vector(Arc::from(vec![1.0f32, 2.0]))).expect("push");
        col.push(&FieldValue::Vector(Arc::from(vec![3.0f32, 4.0]))).expect("push");
        col.set(0, &FieldValue::Vector(Arc::from(vec![9.0f32, 9.5]))).expect("set");
        let frozen = col.freeze();
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Vector(Arc::from(vec![9.0f32, 9.5])));
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Vector(Arc::from(vec![3.0f32, 4.0])));
    }

    #[test]
    fn column_mut_vector_dim_mismatch_is_an_error() {
        let mut col = ColumnMut::new(FieldType::Vector);
        col.push(&FieldValue::Vector(Arc::from(vec![1.0f32, 2.0]))).expect("push");
        assert!(col.push(&FieldValue::Vector(Arc::from(vec![1.0f32]))).is_err());
    }

    #[test]
    fn record_batch_mut_implements_record_view_while_being_built() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 4);
        builder.append_row(&[FieldValue::Int(10)]).expect("append_row");
        builder.append_row(&[FieldValue::Int(20)]).expect("append_row");
        let view: &dyn RecordView = &builder;
        assert_eq!(view.len(), 2);
        assert_eq!(view.value(1, 0).expect("value"), FieldValue::Int(20));
    }

    #[test]
    fn record_batch_mut_column_range_out_of_range_column_is_an_error() {
        let builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 4);
        assert!(builder.column_range(5, 0..0).is_err());
    }

    #[test]
    fn column_mut_new_data_type_round_trips() {
        let col = ColumnMut::new(FieldType::Float);
        assert_eq!(col.data_type(), FieldType::Float);
        assert!(col.is_empty());
    }

    #[test]
    fn reserve_does_not_change_len() {
        let mut builder = RecordBatchMut::with_capacity(int_schema(&["value"]), 0);
        builder.reserve(100);
        assert_eq!(builder.len(), 0);
    }
}
