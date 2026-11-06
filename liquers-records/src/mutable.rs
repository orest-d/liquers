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
use crate::buffer::{AlignedBytesMut, Bitmap, BitmapMut, Buffer};
use crate::column::{offset_from_len, Column, FieldValue};
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

/// A `ColumnMut`'s validity: a row count, and a packed bitmap that exists only once a row is
/// null (or when it came in with the column through `into_mut`). Until then every row is valid
/// and nothing is allocated, so a column with no nulls freezes with `validity: None` as `Column`
/// prefers, and a bitmap the batch held alone moves both ways (`Bitmap::into_mut`,
/// `BitmapMut::freeze`).
#[derive(Debug, Clone)]
struct ValidityMut {
    bits: Option<BitmapMut>,
    len: usize,
}

impl ValidityMut {
    fn new() -> Self {
        ValidityMut { bits: None, len: 0 }
    }

    /// `Column`'s validity for `len` rows, taken over without copying when held alone. A bitmap
    /// of another length (one that bypassed `Column::validate`) is cut or padded with nulls, as
    /// reading it bit by bit did.
    fn from_bitmap(validity: Option<Bitmap>, len: usize) -> Self {
        let bits = validity.map(|bitmap| {
            let mut bits = bitmap.into_mut();
            bits.truncate(len);
            while bits.len() < len {
                bits.push(false);
            }
            bits
        });
        ValidityMut { bits, len }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn reserve(&mut self, additional: usize) {
        if let Some(bits) = &mut self.bits {
            bits.reserve(additional);
        }
    }

    /// The bitmap, allocated with every row so far valid when there is none yet.
    fn bits_mut(&mut self) -> &mut BitmapMut {
        let len = self.len;
        self.bits.get_or_insert_with(|| BitmapMut::filled(len, true))
    }

    fn push(&mut self, valid: bool) {
        if valid {
            if let Some(bits) = &mut self.bits {
                bits.push(true);
            }
        } else {
            self.bits_mut().push(false);
        }
        self.len += 1;
    }

    fn set(&mut self, row: usize, valid: bool) {
        if row >= self.len {
            return;
        }
        if valid {
            if let Some(bits) = &mut self.bits {
                bits.set(row, true);
            }
        } else {
            self.bits_mut().set(row, false);
        }
    }

    fn truncate(&mut self, rows: usize) {
        if rows >= self.len {
            return;
        }
        if let Some(bits) = &mut self.bits {
            bits.truncate(rows);
        }
        self.len = rows;
    }

    fn any_valid(&self) -> bool {
        match &self.bits {
            Some(bits) => bits.count_ones() > 0,
            None => self.len > 0,
        }
    }

    /// `None` when every row is valid — the convention `Column` follows. A bitmap with a null
    /// moves into the column without copying.
    fn freeze(self) -> Option<Bitmap> {
        match self.bits {
            Some(bits) if bits.count_ones() < self.len => Some(bits.freeze()),
            Some(_) | None => None,
        }
    }

    /// A bitmap even when every row is valid, for a non-empty column — a `Vector` of width 0,
    /// whose data cannot count its rows.
    fn freeze_with_bitmap(self) -> Option<Bitmap> {
        if self.len == 0 {
            return self.freeze();
        }
        let len = self.len;
        Some(self.bits.unwrap_or_else(|| BitmapMut::filled(len, true)).freeze())
    }
}

/// Offsets of a `Text`/`Binary` `ColumnMut`, as native-endian `i32`s in an aligned buffer, so
/// `freeze` hands them to the column's `Buffer<i32>` and `into_mut` takes them over, both without
/// copying.
const OFFSET_SIZE: usize = std::mem::size_of::<i32>();

fn offsets_with_capacity(rows: usize) -> AlignedBytesMut {
    let mut offsets = AlignedBytesMut::with_capacity((rows + 1) * OFFSET_SIZE);
    offsets.extend_from_slice(&0i32.to_ne_bytes());
    offsets
}

/// The number of offsets held.
fn offset_count(offsets: &AlignedBytesMut) -> usize {
    offsets.len() / OFFSET_SIZE
}

/// Offset `i` as a byte position; an error for a missing or negative one.
fn offset_at(offsets: &AlignedBytesMut, i: usize) -> Result<usize, Error> {
    let bytes = offsets
        .as_bytes()
        .get(i * OFFSET_SIZE..(i + 1) * OFFSET_SIZE)
        .ok_or_else(|| Error::general_error(format!("ColumnMut: offset {i} out of range")))?;
    let mut raw = [0u8; OFFSET_SIZE];
    raw.copy_from_slice(bytes);
    usize::try_from(i32::from_ne_bytes(raw))
        .map_err(|_| Error::general_error(format!("ColumnMut: offset {i} is negative")))
}

fn push_offset(offsets: &mut AlignedBytesMut, offset: i32) {
    offsets.extend_from_slice(&offset.to_ne_bytes());
}

/// Overwrites the `row`-th variable-length cell of a `Text`/`Binary` column. A replacement of the
/// same byte length is written in place and leaves the offsets alone; one of another length moves
/// the bytes after the cell once and adds the difference to the later offsets.
/// `is_text` picks which `FieldValue` variant is accepted and how the mismatch is named.
fn set_bytes_cell(
    validity: &mut ValidityMut,
    offsets: &mut AlignedBytesMut,
    data: &mut AlignedBytesMut,
    row: usize,
    value: &FieldValue,
    is_text: bool,
) -> Result<(), Error> {
    let expected = if is_text { "Text" } else { "Bytes" };
    let new_cell: Option<&[u8]> = match value {
        FieldValue::Null => None,
        FieldValue::Text(v) if is_text => Some(v.as_bytes()),
        FieldValue::Bytes(v) if !is_text => Some(v.as_ref()),
        other => return Err(type_mismatch(other, expected, "ColumnMut::set")),
    };
    let new_bytes = new_cell.unwrap_or(&[]);
    let start = offset_at(offsets, row)?;
    let end = offset_at(offsets, row + 1)?;
    if end < start || end > data.len() {
        return Err(Error::general_error(format!(
            "ColumnMut::set: inconsistent offsets {start}..{end} for row {row}"
        )));
    }
    if new_bytes.len() == end - start {
        data.set(start, new_bytes);
    } else {
        // Checked before anything changes: every later offset ends at or below the new total.
        offset_from_len(data.len() - (end - start) + new_bytes.len())?;
        let delta = new_bytes.len() as i64 - (end - start) as i64;
        data.splice(start..end, new_bytes);
        for raw in offsets.as_bytes_mut()[(row + 1) * OFFSET_SIZE..].chunks_exact_mut(OFFSET_SIZE) {
            let mut current = [0u8; OFFSET_SIZE];
            current.copy_from_slice(raw);
            let shifted = i64::from(i32::from_ne_bytes(current)) + delta;
            let shifted = i32::try_from(shifted).map_err(|_| {
                Error::general_error(format!("ColumnMut::set: offset {shifted} out of i32 range"))
            })?;
            raw.copy_from_slice(&shifted.to_ne_bytes());
        }
    }
    validity.set(row, new_cell.is_some());
    Ok(())
}

/// The growable storage behind [`ColumnMut`], one variant per [`FieldType`] — the mutable mirror
/// of [`Column`]. Private: `ColumnMut` is the public surface, so a private type here never leaks a
/// `pub(crate)` type (`AlignedBytesMut`) into a public field.
#[derive(Debug, Clone)]
enum ColumnMutInner {
    Bool { validity: ValidityMut, values: BitmapMut },
    Int { validity: ValidityMut, values: AlignedBytesMut },
    UInt { validity: ValidityMut, values: AlignedBytesMut },
    Float { validity: ValidityMut, values: AlignedBytesMut },
    Text { validity: ValidityMut, offsets: AlignedBytesMut, data: AlignedBytesMut },
    Binary { validity: ValidityMut, offsets: AlignedBytesMut, data: AlignedBytesMut },
    Date { validity: ValidityMut, values: AlignedBytesMut },
    Timestamp { validity: ValidityMut, values: AlignedBytesMut },
    /// `dim_known` is false while every row pushed so far is null: nothing has fixed the width
    /// yet, so those rows hold validity only. The first vector fixes `dim` and backfills them
    /// with `dim` zeros each.
    Vector { validity: ValidityMut, dim: usize, dim_known: bool, data: AlignedBytesMut },
}

/// Where a column stood before a row was appended — what [`ColumnMutInner::restore`] rolls back
/// to when a later column of the same row refuses its value.
#[derive(Debug, Clone, Copy)]
struct ColumnMark {
    rows: usize,
    dim: usize,
    dim_known: bool,
}

/// Fixes a `Vector` column's width at `new_dim` the first time a vector arrives, backfilling the
/// `rows` null rows pushed before it with `new_dim` zeros each, so every row then holds exactly
/// `dim` values.
fn fix_vector_dim(
    rows: usize,
    dim: &mut usize,
    dim_known: &mut bool,
    data: &mut AlignedBytesMut,
    new_dim: usize,
) {
    *dim = new_dim;
    *dim_known = true;
    *data = AlignedBytesMut::with_capacity(rows * new_dim * std::mem::size_of::<f32>());
    data.extend_from_slice(&vec![0u8; rows * new_dim * std::mem::size_of::<f32>()]);
}

impl ColumnMutInner {
    fn with_capacity(data_type: FieldType, rows: usize) -> Self {
        match data_type {
            FieldType::Bool => ColumnMutInner::Bool {
                validity: ValidityMut::new(),
                values: BitmapMut::with_capacity(rows),
            },
            FieldType::Int => ColumnMutInner::Int {
                validity: ValidityMut::new(),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i64>()),
            },
            FieldType::UInt => ColumnMutInner::UInt {
                validity: ValidityMut::new(),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<u64>()),
            },
            FieldType::Float => ColumnMutInner::Float {
                validity: ValidityMut::new(),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<f64>()),
            },
            FieldType::Text => ColumnMutInner::Text {
                validity: ValidityMut::new(),
                offsets: offsets_with_capacity(rows),
                data: AlignedBytesMut::new(),
            },
            FieldType::Binary => ColumnMutInner::Binary {
                validity: ValidityMut::new(),
                offsets: offsets_with_capacity(rows),
                data: AlignedBytesMut::new(),
            },
            FieldType::Date => ColumnMutInner::Date {
                validity: ValidityMut::new(),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i32>()),
            },
            FieldType::Timestamp => ColumnMutInner::Timestamp {
                validity: ValidityMut::new(),
                values: AlignedBytesMut::with_capacity(rows * std::mem::size_of::<i64>()),
            },
            FieldType::Vector => ColumnMutInner::Vector {
                validity: ValidityMut::new(),
                dim: 0,
                dim_known: false,
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
            ColumnMutInner::Text { offsets, .. } => offset_count(offsets).saturating_sub(1),
            ColumnMutInner::Binary { offsets, .. } => offset_count(offsets).saturating_sub(1),
            ColumnMutInner::Date { values, .. } => values.len() / std::mem::size_of::<i32>(),
            ColumnMutInner::Timestamp { values, .. } => values.len() / std::mem::size_of::<i64>(),
            // Rows pushed before the width is known hold no data, so validity counts the rows.
            ColumnMutInner::Vector { validity, .. } => validity.len(),
        }
    }

    fn mark(&self) -> ColumnMark {
        match self {
            ColumnMutInner::Vector { dim, dim_known, .. } => {
                ColumnMark { rows: self.len(), dim: *dim, dim_known: *dim_known }
            }
            ColumnMutInner::Bool { .. }
            | ColumnMutInner::Int { .. }
            | ColumnMutInner::UInt { .. }
            | ColumnMutInner::Float { .. }
            | ColumnMutInner::Text { .. }
            | ColumnMutInner::Binary { .. }
            | ColumnMutInner::Date { .. }
            | ColumnMutInner::Timestamp { .. } => ColumnMark { rows: self.len(), dim: 0, dim_known: false },
        }
    }

    /// Drops every row pushed since `mark` was taken, restoring a `Vector`'s width state too.
    fn restore(&mut self, mark: ColumnMark) {
        let rows = mark.rows;
        match self {
            ColumnMutInner::Bool { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows);
            }
            ColumnMutInner::Int { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows * std::mem::size_of::<i64>());
            }
            ColumnMutInner::UInt { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows * std::mem::size_of::<u64>());
            }
            ColumnMutInner::Float { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows * std::mem::size_of::<f64>());
            }
            ColumnMutInner::Text { validity, offsets, data }
            | ColumnMutInner::Binary { validity, offsets, data } => {
                if let Ok(end) = offset_at(offsets, rows) {
                    data.truncate(end);
                }
                offsets.truncate((rows + 1) * OFFSET_SIZE);
                validity.truncate(rows);
            }
            ColumnMutInner::Date { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows * std::mem::size_of::<i32>());
            }
            ColumnMutInner::Timestamp { validity, values } => {
                validity.truncate(rows);
                values.truncate(rows * std::mem::size_of::<i64>());
            }
            ColumnMutInner::Vector { validity, dim, dim_known, data } => {
                validity.truncate(rows);
                *dim = mark.dim;
                *dim_known = mark.dim_known;
                if mark.dim_known {
                    data.truncate(rows * mark.dim * std::mem::size_of::<f32>());
                } else {
                    // The width was fixed by the rolled-back row; the rows before it hold none.
                    *data = AlignedBytesMut::new();
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
                offsets.reserve(additional * OFFSET_SIZE);
            }
            ColumnMutInner::Binary { validity, offsets, .. } => {
                validity.reserve(additional);
                offsets.reserve(additional * OFFSET_SIZE);
            }
            ColumnMutInner::Date { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<i32>());
            }
            ColumnMutInner::Timestamp { validity, values } => {
                validity.reserve(additional);
                values.reserve(additional * std::mem::size_of::<i64>());
            }
            ColumnMutInner::Vector { validity, dim, data, .. } => {
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
                    let end = offset_from_len(data.len())?;
                    validity.push(false);
                    push_offset(offsets, end);
                }
                FieldValue::Text(v) => {
                    let end = offset_from_len(data.len() + v.len())?;
                    validity.push(true);
                    data.extend_from_slice(v.as_bytes());
                    push_offset(offsets, end);
                }
                other => return Err(type_mismatch(other, "Text", "ColumnMut::push")),
            },
            ColumnMutInner::Binary { validity, offsets, data } => match value {
                FieldValue::Null => {
                    let end = offset_from_len(data.len())?;
                    validity.push(false);
                    push_offset(offsets, end);
                }
                FieldValue::Bytes(v) => {
                    let end = offset_from_len(data.len() + v.len())?;
                    validity.push(true);
                    data.extend_from_slice(v);
                    push_offset(offsets, end);
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
            ColumnMutInner::Vector { validity, dim, dim_known, data } => match value {
                FieldValue::Null => {
                    // `dim` is 0 while the width is unknown, so this writes nothing then.
                    validity.push(false);
                    let width = *dim * std::mem::size_of::<f32>();
                    data.extend_from_slice(&vec![0u8; width]);
                }
                FieldValue::Vector(v) => {
                    if !*dim_known {
                        fix_vector_dim(validity.len(), dim, dim_known, data, v.len());
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
                    validity.set(row, false);
                    values.set(row, false);
                }
                FieldValue::Bool(v) => {
                    validity.set(row, true);
                    values.set(row, *v);
                }
                other => return Err(type_mismatch(other, "Bool", "ColumnMut::set")),
            },
            ColumnMutInner::Int { validity, values } => match value {
                FieldValue::Null => {
                    validity.set(row, false);
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(&0i64));
                }
                FieldValue::Int(v) => {
                    validity.set(row, true);
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Int", "ColumnMut::set")),
            },
            ColumnMutInner::UInt { validity, values } => match value {
                FieldValue::Null => {
                    validity.set(row, false);
                    values.set(row * std::mem::size_of::<u64>(), bytemuck::bytes_of(&0u64));
                }
                FieldValue::UInt(v) => {
                    validity.set(row, true);
                    values.set(row * std::mem::size_of::<u64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "UInt", "ColumnMut::set")),
            },
            ColumnMutInner::Float { validity, values } => match value {
                FieldValue::Null => {
                    validity.set(row, false);
                    values.set(row * std::mem::size_of::<f64>(), bytemuck::bytes_of(&0f64));
                }
                FieldValue::Float(v) => {
                    validity.set(row, true);
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
                    validity.set(row, false);
                    values.set(row * std::mem::size_of::<i32>(), bytemuck::bytes_of(&0i32));
                }
                FieldValue::Date(v) => {
                    validity.set(row, true);
                    values.set(row * std::mem::size_of::<i32>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Date", "ColumnMut::set")),
            },
            ColumnMutInner::Timestamp { validity, values } => match value {
                FieldValue::Null => {
                    validity.set(row, false);
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(&0i64));
                }
                FieldValue::Timestamp(v) => {
                    validity.set(row, true);
                    values.set(row * std::mem::size_of::<i64>(), bytemuck::bytes_of(v));
                }
                other => return Err(type_mismatch(other, "Timestamp", "ColumnMut::set")),
            },
            ColumnMutInner::Vector { validity, dim, dim_known, data } => match value {
                FieldValue::Null => {
                    validity.set(row, false);
                    let width = *dim * std::mem::size_of::<f32>();
                    data.set(row * width, &vec![0u8; width]);
                }
                FieldValue::Vector(v) => {
                    if !*dim_known {
                        fix_vector_dim(validity.len(), dim, dim_known, data, v.len());
                    }
                    if v.len() != *dim {
                        return Err(Error::general_error(format!(
                            "ColumnMut::set: Vector dim mismatch ({} vs {dim})",
                            v.len()
                        )));
                    }
                    validity.set(row, true);
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
                validity: validity.freeze(),
                values: values.freeze(),
            },
            ColumnMutInner::Int { validity, values } => Column::Int {
                validity: validity.freeze(),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::UInt { validity, values } => Column::UInt {
                validity: validity.freeze(),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Float { validity, values } => Column::Float {
                validity: validity.freeze(),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Text { validity, offsets, data } => Column::Text {
                validity: validity.freeze(),
                offsets: Buffer::from_aligned(offsets.freeze()),
                data: data.freeze(),
            },
            ColumnMutInner::Binary { validity, offsets, data } => Column::Binary {
                validity: validity.freeze(),
                offsets: Buffer::from_aligned(offsets.freeze()),
                data: data.freeze(),
            },
            ColumnMutInner::Date { validity, values } => Column::Date {
                validity: validity.freeze(),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Timestamp { validity, values } => Column::Timestamp {
                validity: validity.freeze(),
                values: Buffer::from_aligned(values.freeze()),
            },
            ColumnMutInner::Vector { validity, dim, data, .. } => Column::Vector {
                // With `dim == 0` the data holds nothing to count rows by, so validity is kept
                // even when every row is valid — it carries the row count.
                validity: if dim == 0 {
                    validity.freeze_with_bitmap()
                } else {
                    validity.freeze()
                },
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

    /// Whether no validity bitmap has been allocated (every row so far valid).
    #[cfg(test)]
    fn inner_validity_is_unallocated(&self) -> bool {
        match &self.inner {
            ColumnMutInner::Int { validity, .. } => validity.bits.is_none(),
            other => panic!("test helper for Int columns, got {other:?}"),
        }
    }
}

/// Converts an existing, immutable [`Column`] into a growable [`ColumnMut`] — the per-column
/// half of [`RecordBatch::into_mut`]. Takes over the value buffer without copying when this
/// `Column` is its only owner (`Buffer::into_aligned`/[`crate::buffer::AlignedBuffer::into_mut`]
/// succeed via `Arc::try_unwrap`); copies it otherwise. Validity, `Bool` values and `Text`/`Binary`
/// offsets move the same way.
fn column_into_mut(column: Column) -> ColumnMut {
    let inner = match column {
        Column::Bool { validity, values } => {
            let len = values.len();
            ColumnMutInner::Bool {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_mut(),
            }
        }
        Column::Int { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Int {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::UInt { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::UInt {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Float { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Float {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Text { validity, offsets, data } => {
            let len = offsets.as_slice().len().saturating_sub(1);
            ColumnMutInner::Text {
                validity: ValidityMut::from_bitmap(validity, len),
                offsets: offsets.into_aligned().into_mut(),
                data: data.into_mut(),
            }
        }
        Column::Binary { validity, offsets, data } => {
            let len = offsets.as_slice().len().saturating_sub(1);
            ColumnMutInner::Binary {
                validity: ValidityMut::from_bitmap(validity, len),
                offsets: offsets.into_aligned().into_mut(),
                data: data.into_mut(),
            }
        }
        Column::Date { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Date {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Timestamp { validity, values } => {
            let len = values.as_slice().len();
            ColumnMutInner::Timestamp {
                validity: ValidityMut::from_bitmap(validity, len),
                values: values.into_aligned().into_mut(),
            }
        }
        Column::Vector { validity, dim, data } => {
            let len = match &validity {
                Some(bitmap) if dim == 0 => bitmap.len(),
                Some(_) | None => {
                    if dim == 0 {
                        0
                    } else {
                        data.as_slice().len() / dim
                    }
                }
            };
            let validity = ValidityMut::from_bitmap(validity, len);
            // A width of 0 is a fixed width once any row holds a (necessarily empty) vector.
            let dim_known = dim > 0 || validity.any_valid();
            ColumnMutInner::Vector {
                validity,
                dim,
                dim_known,
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
        // Atomic: a value a later column refuses must not leave earlier columns one row ahead,
        // or the next row would be assembled from cells of two different rows.
        let marks: Vec<ColumnMark> = self.columns.iter().map(|column| column.inner.mark()).collect();
        for (index, (column, value)) in self.columns.iter_mut().zip(values.iter()).enumerate() {
            if let Err(error) = column.push(value) {
                for (column, mark) in self.columns.iter_mut().zip(marks.iter()).take(index) {
                    column.inner.restore(*mark);
                }
                return Err(error);
            }
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

    // ---- `column-mut-zero-copy-validity` --------------------------------------------------

    fn text(value: &str) -> FieldValue {
        FieldValue::Text(Arc::from(value))
    }

    fn texts(column: &Column) -> Vec<FieldValue> {
        (0..column.len()).map(|i| column.get(i).expect("get")).collect()
    }

    fn text_column(values: &[Option<&str>]) -> Column {
        let mut col = ColumnMut::new(FieldType::Text);
        for value in values {
            col.push(&value.map_or(FieldValue::Null, text)).expect("push");
        }
        col.freeze()
    }

    /// AC-1: validity the column holds alone moves through `into_mut` and `freeze`.
    #[test]
    fn into_mut_freeze_reuses_validity_held_alone() {
        let column = Column::Int {
            validity: Some(Bitmap::from_bools(&[true, false, true])),
            values: Buffer::from_slice(&[1i64, 0, 3]),
        };
        let before = match &column {
            Column::Int { validity: Some(bitmap), .. } => bitmap.storage_ptr(),
            other => panic!("unexpected column {other:?}"),
        };
        let mut col = column_into_mut(column);
        col.set(2, &FieldValue::Int(30)).expect("set");
        let frozen = col.freeze();
        match &frozen {
            Column::Int { validity: Some(bitmap), .. } => {
                assert_eq!(bitmap.storage_ptr(), before);
                assert_eq!(bitmap, &Bitmap::from_bools(&[true, false, true]));
            }
            other => panic!("unexpected column {other:?}"),
        }
        assert_eq!(frozen.get(2).expect("get"), FieldValue::Int(30));
    }

    /// AC-2: no null, no validity — and a null set back to a value leaves none either.
    #[test]
    fn all_valid_column_mut_freezes_without_validity() {
        let mut col = ColumnMut::new(FieldType::Int);
        for v in 0..20 {
            col.push(&FieldValue::Int(v)).expect("push");
        }
        assert!(col.inner_validity_is_unallocated());
        col.set(3, &FieldValue::Null).expect("set");
        col.set(3, &FieldValue::Int(3)).expect("set");
        match col.freeze() {
            Column::Int { validity, .. } => assert!(validity.is_none()),
            other => panic!("unexpected column {other:?}"),
        }
    }

    /// AC-3: `Bool` values move too, and stay equal.
    #[test]
    fn bool_column_round_trip_reuses_values() {
        let values = Bitmap::from_bools(&[true, false, false, true, true]);
        let before = values.storage_ptr();
        let column = Column::Bool { validity: None, values };
        let mut col = column_into_mut(column);
        col.set(1, &FieldValue::Bool(true)).expect("set");
        let frozen = col.freeze();
        match &frozen {
            Column::Bool { validity: None, values } => {
                assert_eq!(values.storage_ptr(), before);
                assert_eq!(values, &Bitmap::from_bools(&[true, true, false, true, true]));
            }
            other => panic!("unexpected column {other:?}"),
        }
    }

    /// AC-4: a same-length replacement writes the cell and leaves the offsets as they were.
    #[test]
    fn text_set_same_length_writes_in_place() {
        let mut col = column_into_mut(text_column(&[Some("ab"), Some("cde"), Some("f")]));
        col.set(1, &text("XYZ")).expect("set");
        let frozen = col.freeze();
        match &frozen {
            Column::Text { offsets, data, .. } => {
                assert_eq!(offsets.as_slice(), &[0, 2, 5, 6]);
                assert_eq!(data.as_bytes(), b"abXYZf");
            }
            other => panic!("unexpected column {other:?}"),
        }
    }

    /// AC-5: longer, shorter, to null and from null each shift only what follows the cell.
    #[test]
    fn text_and_binary_set_longer_shorter_and_null_shift_the_tail() {
        let mut col = column_into_mut(text_column(&[Some("ab"), Some("cde"), None, Some("f")]));
        col.set(0, &text("longer")).expect("longer");
        col.set(1, &text("c")).expect("shorter");
        col.set(3, &FieldValue::Null).expect("to null");
        col.set(2, &text("new")).expect("from null");
        let frozen = col.freeze();
        assert_eq!(
            texts(&frozen),
            vec![text("longer"), text("c"), text("new"), FieldValue::Null]
        );
        match &frozen {
            Column::Text { offsets, data, .. } => {
                assert_eq!(offsets.as_slice(), &[0, 6, 7, 10, 10]);
                assert_eq!(data.as_bytes(), b"longercnew");
            }
            other => panic!("unexpected column {other:?}"),
        }

        let mut bin = ColumnMut::new(FieldType::Binary);
        for cell in [&b"\x00\x01"[..], b"", b"\x02"] {
            bin.push(&FieldValue::Bytes(Arc::from(cell))).expect("push");
        }
        bin.set(1, &FieldValue::Bytes(Arc::from(&b"\x09\x09\x09"[..]))).expect("longer");
        bin.set(0, &FieldValue::Null).expect("to null");
        let frozen = bin.freeze();
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Bytes(Arc::from(&b"\x09\x09\x09"[..])));
        assert_eq!(frozen.get(2).expect("get"), FieldValue::Bytes(Arc::from(&b"\x02"[..])));
        assert!(frozen.validate().is_ok());
    }

    /// AC-6: offsets the column holds alone move both ways.
    #[test]
    fn text_round_trip_reuses_offsets() {
        let column = text_column(&[Some("ab"), Some("cde")]);
        let before = match &column {
            Column::Text { offsets, .. } => offsets.storage_ptr(),
            other => panic!("unexpected column {other:?}"),
        };
        let mut col = column_into_mut(column);
        col.set(0, &text("xy")).expect("set");
        match col.freeze() {
            Column::Text { offsets, .. } => assert_eq!(offsets.storage_ptr(), before),
            other => panic!("unexpected column {other:?}"),
        }
    }

    /// AC-7: a batch still shared elsewhere is copied, never written.
    #[test]
    fn editing_a_shared_batch_leaves_the_original() {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("name", FieldType::Text),
                FieldSchema::new("flag", FieldType::Bool),
                FieldSchema::new("n", FieldType::Int),
            ])
            .expect("schema"),
        );
        let mut builder = RecordBatchMut::new(schema);
        builder.append_row(&[text("ab"), FieldValue::Bool(true), FieldValue::Null]).expect("row");
        builder.append_row(&[FieldValue::Null, FieldValue::Bool(false), FieldValue::Int(2)]).expect("row");
        let original = Arc::new(builder.freeze().expect("freeze"));
        let before: Vec<Vec<FieldValue>> = (0..2)
            .map(|row| (0..3).map(|col| original.value(row, col).expect("value")).collect())
            .collect();

        let mut edited = (*original).clone().into_mut();
        edited.set_value(0, 0, &text("much longer")).expect("set");
        edited.set_value(1, 0, &text("x")).expect("set");
        edited.set_value(0, 1, &FieldValue::Null).expect("set");
        edited.set_value(0, 2, &FieldValue::Int(1)).expect("set");
        let edited = edited.freeze().expect("freeze");

        let after: Vec<Vec<FieldValue>> = (0..2)
            .map(|row| (0..3).map(|col| original.value(row, col).expect("value")).collect())
            .collect();
        assert_eq!(after, before);
        assert_eq!(edited.value(0, 0).expect("value"), text("much longer"));
        assert_eq!(edited.value(0, 1).expect("value"), FieldValue::Null);
        assert_eq!(edited.value(0, 2).expect("value"), FieldValue::Int(1));
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

    // --- Review findings DM2 (nulls before the first vector) and DM3 (atomic append_row) ---

    #[test]
    fn column_mut_vector_null_before_the_first_vector_keeps_its_row() {
        let mut col = ColumnMut::new(FieldType::Vector);
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Vector(Arc::from(vec![1.0f32, 2.0]))).expect("push");
        assert_eq!(col.len(), 3);
        let frozen = col.freeze();
        assert_eq!(frozen.len(), 3);
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(2).expect("get"), FieldValue::Vector(Arc::from(vec![1.0f32, 2.0])));
    }

    #[test]
    fn column_mut_all_null_vector_column_keeps_its_row_count() {
        let mut col = ColumnMut::new(FieldType::Vector);
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Null).expect("push");
        assert_eq!(col.len(), 2);
        let frozen = col.freeze();
        assert_eq!(frozen.len(), 2);
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Null);
        assert_eq!(frozen.slice(1, 1).expect("slice").len(), 1);
        assert_eq!(frozen.take(&[1, 0]).expect("take").len(), 2);
    }

    #[test]
    fn column_mut_vector_set_on_an_all_null_column_establishes_the_dim() {
        let mut col = ColumnMut::new(FieldType::Vector);
        col.push(&FieldValue::Null).expect("push");
        col.push(&FieldValue::Null).expect("push");
        col.set(1, &FieldValue::Vector(Arc::from(vec![7.0f32, 8.0, 9.0]))).expect("set");
        let frozen = col.freeze();
        assert_eq!(frozen.get(0).expect("get"), FieldValue::Null);
        assert_eq!(frozen.get(1).expect("get"), FieldValue::Vector(Arc::from(vec![7.0f32, 8.0, 9.0])));
    }

    #[test]
    fn append_row_is_atomic_when_a_later_column_refuses_its_value() {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("a", FieldType::Int),
                FieldSchema::new("b", FieldType::Text),
            ])
            .expect("schema"),
        );
        let mut builder = RecordBatchMut::new(schema);
        // Column "b" refuses an Int: nothing of this row may be kept.
        assert!(builder
            .append_row(&[FieldValue::Int(1), FieldValue::Int(2)])
            .is_err());
        assert_eq!(builder.column_mut(0).expect("a").len(), 0);
        assert_eq!(builder.column_mut(1).expect("b").len(), 0);
        builder
            .append_row(&[FieldValue::Int(3), FieldValue::Text(Arc::from("x"))])
            .expect("append_row");
        let frozen = builder.freeze().expect("freeze");
        assert_eq!(frozen.len, 1);
        assert_eq!(frozen.value(0, 0).expect("value"), FieldValue::Int(3));
        assert_eq!(frozen.value(0, 1).expect("value"), FieldValue::Text(Arc::from("x")));
    }

    #[test]
    fn append_row_is_atomic_on_a_vector_dim_mismatch() {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("a", FieldType::Int),
                FieldSchema::new("v", FieldType::Vector),
            ])
            .expect("schema"),
        );
        let mut builder = RecordBatchMut::new(schema);
        builder
            .append_row(&[FieldValue::Int(1), FieldValue::Vector(Arc::from(vec![1.0f32, 2.0]))])
            .expect("append_row");
        assert!(builder
            .append_row(&[FieldValue::Int(2), FieldValue::Vector(Arc::from(vec![1.0f32]))])
            .is_err());
        let frozen = builder.freeze().expect("freeze");
        assert_eq!(frozen.len, 1);
    }
}
