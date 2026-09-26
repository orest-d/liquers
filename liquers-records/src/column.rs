//! `FieldValue`, the scalar type, and `Column`, Arrow-laid-out column storage plus the typed
//! kernels every `RecordView` gets for free.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"FieldValue — the scalar type" and
//! §"Columns, not rows: the batch is Arrow-laid-out", and its §"Kernels live on `Column`, so views
//! get the fast path too".

use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use serde::{Deserialize, Serialize};

use crate::buffer::{AlignedBuffer, Bitmap, Buffer};
use crate::schema::FieldType;

/// The scalar type: what a filter compares against, what a single-cell read returns, and what a
/// builder appends. Storage itself is [`Column`], not `FieldValue` — see phase2-architecture.md
/// §"FieldValue — the scalar type" for the size rationale (24 bytes, vs. `serde_json::Value`'s 32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Text(Arc<str>),
    Bytes(Arc<[u8]>),
    /// Days since the epoch, as [`Column::Date`] stores it.
    Date(i32),
    /// Microseconds since the epoch, as [`Column::Timestamp`] stores it.
    Timestamp(i64),
    Vector(Arc<[f32]>),
}

/// What [`Column::compare`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Column storage, laid out exactly as Arrow specifies — 64-byte aligned, validity omitted when
/// there are no nulls — so an export is a pointer hand-off rather than a conversion. `Buffer<T>`
/// is an `Arc`-shared, 64-byte-aligned `[T]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Column {
    Bool {
        validity: Option<Bitmap>,
        values: Bitmap,
    },
    Int {
        validity: Option<Bitmap>,
        values: Buffer<i64>,
    },
    UInt {
        validity: Option<Bitmap>,
        values: Buffer<u64>,
    },
    Float {
        validity: Option<Bitmap>,
        values: Buffer<f64>,
    },
    /// Arrow `Utf8`: `i32` offsets (one more than the row count) plus a contiguous byte buffer.
    /// No per-cell allocation.
    Text {
        validity: Option<Bitmap>,
        offsets: Buffer<i32>,
        data: AlignedBuffer,
    },
    Binary {
        validity: Option<Bitmap>,
        offsets: Buffer<i32>,
        data: AlignedBuffer,
    },
    /// Arrow `Date32` — days since the epoch.
    Date {
        validity: Option<Bitmap>,
        values: Buffer<i32>,
    },
    /// Arrow `Timestamp(Microsecond, None)`.
    Timestamp {
        validity: Option<Bitmap>,
        values: Buffer<i64>,
    },
    /// Arrow `FixedSizeList(Float32, dim)` — the embedding case, contiguous.
    Vector {
        validity: Option<Bitmap>,
        dim: usize,
        data: Buffer<f32>,
    },
}

/// Whether row `i` is valid (non-null). `None` means no validity buffer at all — every row valid.
fn is_valid(validity: &Option<Bitmap>, i: usize) -> bool {
    match validity {
        Some(bitmap) => bitmap.get(i),
        None => true,
    }
}

/// `validity.not()` when there is a validity buffer (null where it says invalid); an all-clear
/// bitmap — no nulls — otherwise.
fn validity_to_null_mask(validity: &Option<Bitmap>, len: usize) -> Bitmap {
    match validity {
        Some(bitmap) => bitmap.not(),
        None => Bitmap::new(len),
    }
}

fn apply_op<T: PartialOrd>(a: T, b: T, op: CompareOp) -> bool {
    match op {
        CompareOp::Eq => a == b,
        CompareOp::Ne => a != b,
        CompareOp::Lt => a < b,
        CompareOp::Le => a <= b,
        CompareOp::Gt => a > b,
        CompareOp::Ge => a >= b,
    }
}

fn compare_slice<T: PartialOrd + Copy>(
    values: &[T],
    validity: &Option<Bitmap>,
    target: T,
    op: CompareOp,
) -> Bitmap {
    let mut mask = Bitmap::new(values.len());
    for (i, &v) in values.iter().enumerate() {
        if is_valid(validity, i) && apply_op(v, target, op) {
            mask.set(i, true);
        }
    }
    mask
}

fn compare_text(
    offsets: &Buffer<i32>,
    data: &AlignedBuffer,
    validity: &Option<Bitmap>,
    target: &str,
    op: CompareOp,
) -> Result<Bitmap, Error> {
    let off = offsets.as_slice();
    let n = off.len().saturating_sub(1);
    let bytes = data.as_bytes();
    let mut mask = Bitmap::new(n);
    for i in 0..n {
        if !is_valid(validity, i) {
            continue;
        }
        let start = off[i] as usize;
        let end = off[i + 1] as usize;
        let text = std::str::from_utf8(&bytes[start..end])
            .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
        if apply_op(text, target, op) {
            mask.set(i, true);
        }
    }
    Ok(mask)
}

fn expect_bool(value: &FieldValue) -> Result<bool, Error> {
    match value {
        FieldValue::Bool(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Bool (Column::compare)",
        )),
    }
}

fn expect_int(value: &FieldValue) -> Result<i64, Error> {
    match value {
        FieldValue::Int(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Int (Column::compare)",
        )),
    }
}

fn expect_uint(value: &FieldValue) -> Result<u64, Error> {
    match value {
        FieldValue::UInt(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "UInt (Column::compare)",
        )),
    }
}

fn expect_float(value: &FieldValue) -> Result<f64, Error> {
    match value {
        FieldValue::Float(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Float (Column::compare)",
        )),
    }
}

fn expect_text(value: &FieldValue) -> Result<&str, Error> {
    match value {
        FieldValue::Text(v) => Ok(v.as_ref()),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Text (Column::compare)",
        )),
    }
}

fn expect_date(value: &FieldValue) -> Result<i32, Error> {
    match value {
        FieldValue::Date(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Date (Column::compare)",
        )),
    }
}

fn expect_timestamp(value: &FieldValue) -> Result<i64, Error> {
    match value {
        FieldValue::Timestamp(v) => Ok(*v),
        other => Err(Error::conversion_error(
            format!("{other:?}"),
            "Timestamp (Column::compare)",
        )),
    }
}

fn slice_bitmap(bitmap: &Bitmap, offset: usize, len: usize) -> Result<Bitmap, Error> {
    if offset + len > bitmap.len() {
        return Err(Error::general_error(format!(
            "Column::slice: offset {offset} + len {len} exceeds length {}",
            bitmap.len()
        )));
    }
    let mut out = Bitmap::new(len);
    for i in 0..len {
        if bitmap.get(offset + i) {
            out.set(i, true);
        }
    }
    Ok(out)
}

fn slice_validity(
    validity: &Option<Bitmap>,
    offset: usize,
    len: usize,
) -> Result<Option<Bitmap>, Error> {
    match validity {
        Some(bitmap) => Ok(Some(slice_bitmap(bitmap, offset, len)?)),
        None => Ok(None),
    }
}

fn slice_buffer<T: bytemuck::Pod>(
    buffer: &Buffer<T>,
    offset: usize,
    len: usize,
) -> Result<Buffer<T>, Error> {
    let slice = buffer.as_slice();
    if offset + len > slice.len() {
        return Err(Error::general_error(format!(
            "Column::slice: offset {offset} + len {len} exceeds length {}",
            slice.len()
        )));
    }
    Ok(Buffer::from_slice(&slice[offset..offset + len]))
}

fn slice_bytes(
    offsets: &Buffer<i32>,
    data: &AlignedBuffer,
    offset: usize,
    len: usize,
) -> Result<(Buffer<i32>, AlignedBuffer), Error> {
    let off = offsets.as_slice();
    let n = off.len().saturating_sub(1);
    if offset + len > n {
        return Err(Error::general_error(format!(
            "Column::slice: offset {offset} + len {len} exceeds length {n}"
        )));
    }
    let bytes = data.as_bytes();
    let base = off[offset];
    let start = base as usize;
    let end = off[offset + len] as usize;
    let new_offsets: Vec<i32> = off[offset..=offset + len].iter().map(|&o| o - base).collect();
    let new_data = bytes[start..end].to_vec();
    Ok((Buffer::from_slice(&new_offsets), AlignedBuffer::from_slice(&new_data)))
}

fn slice_vector(
    data: &Buffer<f32>,
    dim: usize,
    offset: usize,
    len: usize,
) -> Result<Buffer<f32>, Error> {
    let slice = data.as_slice();
    let n = if dim == 0 { 0 } else { slice.len() / dim };
    if offset + len > n {
        return Err(Error::general_error(format!(
            "Column::slice: offset {offset} + len {len} exceeds length {n}"
        )));
    }
    Ok(Buffer::from_slice(&slice[offset * dim..(offset + len) * dim]))
}

fn gather_bitmap(bitmap: &Bitmap, indices: &[u32]) -> Result<Bitmap, Error> {
    let mut out = Bitmap::new(indices.len());
    for (k, &idx) in indices.iter().enumerate() {
        let idx = idx as usize;
        if idx >= bitmap.len() {
            return Err(Error::general_error(format!(
                "Column::take: index {idx} out of bounds (len {})",
                bitmap.len()
            )));
        }
        if bitmap.get(idx) {
            out.set(k, true);
        }
    }
    Ok(out)
}

fn gather_validity(validity: &Option<Bitmap>, indices: &[u32]) -> Result<Option<Bitmap>, Error> {
    match validity {
        Some(bitmap) => Ok(Some(gather_bitmap(bitmap, indices)?)),
        None => Ok(None),
    }
}

fn gather_buffer<T: bytemuck::Pod>(buffer: &Buffer<T>, indices: &[u32]) -> Result<Buffer<T>, Error> {
    let slice = buffer.as_slice();
    let mut out = Vec::with_capacity(indices.len());
    for &idx in indices {
        let idx = idx as usize;
        let value = slice.get(idx).copied().ok_or_else(|| {
            Error::general_error(format!(
                "Column::take: index {idx} out of bounds (len {})",
                slice.len()
            ))
        })?;
        out.push(value);
    }
    Ok(Buffer::from_slice(&out))
}

fn gather_bytes(
    offsets: &Buffer<i32>,
    data: &AlignedBuffer,
    indices: &[u32],
) -> Result<(Buffer<i32>, AlignedBuffer), Error> {
    let off = offsets.as_slice();
    let n = off.len().saturating_sub(1);
    let bytes = data.as_bytes();
    let mut new_offsets = Vec::with_capacity(indices.len() + 1);
    let mut new_data = Vec::new();
    new_offsets.push(0i32);
    for &idx in indices {
        let idx = idx as usize;
        if idx >= n {
            return Err(Error::general_error(format!(
                "Column::take: index {idx} out of bounds (len {n})"
            )));
        }
        let start = off[idx] as usize;
        let end = off[idx + 1] as usize;
        new_data.extend_from_slice(&bytes[start..end]);
        new_offsets.push(new_data.len() as i32);
    }
    Ok((Buffer::from_slice(&new_offsets), AlignedBuffer::from_slice(&new_data)))
}

fn gather_vector(data: &Buffer<f32>, dim: usize, indices: &[u32]) -> Result<Buffer<f32>, Error> {
    let slice = data.as_slice();
    let n = if dim == 0 { 0 } else { slice.len() / dim };
    let mut out = Vec::with_capacity(indices.len() * dim);
    for &idx in indices {
        let idx = idx as usize;
        if idx >= n {
            return Err(Error::general_error(format!(
                "Column::take: index {idx} out of bounds (len {n})"
            )));
        }
        out.extend_from_slice(&slice[idx * dim..(idx + 1) * dim]);
    }
    Ok(Buffer::from_slice(&out))
}

fn concat_type_mismatch(expected: FieldType, actual: FieldType) -> Error {
    Error::general_error(format!(
        "Column::concat: type mismatch ({expected:?} vs {actual:?})"
    ))
}

fn concat_bool(columns: &[Column]) -> Result<Column, Error> {
    let mut parts: Vec<(&Option<Bitmap>, &Bitmap)> = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Bool { validity, values } => parts.push((validity, values)),
            Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => {
                return Err(concat_type_mismatch(FieldType::Bool, column.data_type()))
            }
        }
    }
    let total: usize = parts.iter().map(|(_, v)| v.len()).sum();
    let mut values = Bitmap::new(total);
    let mut offset = 0;
    for (_, v) in &parts {
        for i in 0..v.len() {
            if v.get(i) {
                values.set(offset + i, true);
            }
        }
        offset += v.len();
    }
    let has_validity = parts.iter().any(|(validity, _)| validity.is_some());
    let validity = if has_validity {
        let mut out = Bitmap::new(total);
        let mut offset = 0;
        for (validity, v) in &parts {
            match validity {
                Some(bitmap) => {
                    for i in 0..v.len() {
                        if bitmap.get(i) {
                            out.set(offset + i, true);
                        }
                    }
                }
                None => {
                    for i in 0..v.len() {
                        out.set(offset + i, true);
                    }
                }
            }
            offset += v.len();
        }
        Some(out)
    } else {
        None
    };
    Ok(Column::Bool { validity, values })
}

fn concat_buffer_parts<T: bytemuck::Pod>(parts: &[(&Option<Bitmap>, &Buffer<T>)]) -> (Option<Bitmap>, Buffer<T>) {
    let total: usize = parts.iter().map(|(_, v)| v.as_slice().len()).sum();
    let mut values: Vec<T> = Vec::with_capacity(total);
    for (_, v) in parts {
        values.extend_from_slice(v.as_slice());
    }
    let has_validity = parts.iter().any(|(validity, _)| validity.is_some());
    let validity = if has_validity {
        let mut out = Bitmap::new(total);
        let mut offset = 0;
        for (validity, v) in parts {
            let len = v.as_slice().len();
            match validity {
                Some(bitmap) => {
                    for i in 0..len {
                        if bitmap.get(i) {
                            out.set(offset + i, true);
                        }
                    }
                }
                None => {
                    for i in 0..len {
                        out.set(offset + i, true);
                    }
                }
            }
            offset += len;
        }
        Some(out)
    } else {
        None
    };
    (validity, Buffer::from_slice(&values))
}

fn concat_int(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Int { validity, values } => parts.push((validity, values)),
            Column::Bool { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::Int, column.data_type())),
        }
    }
    let (validity, values) = concat_buffer_parts(&parts);
    Ok(Column::Int { validity, values })
}

fn concat_uint(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::UInt { validity, values } => parts.push((validity, values)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::UInt, column.data_type())),
        }
    }
    let (validity, values) = concat_buffer_parts(&parts);
    Ok(Column::UInt { validity, values })
}

fn concat_float(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Float { validity, values } => parts.push((validity, values)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::Float, column.data_type())),
        }
    }
    let (validity, values) = concat_buffer_parts(&parts);
    Ok(Column::Float { validity, values })
}

fn concat_date(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Date { validity, values } => parts.push((validity, values)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::Date, column.data_type())),
        }
    }
    let (validity, values) = concat_buffer_parts(&parts);
    Ok(Column::Date { validity, values })
}

fn concat_timestamp(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Timestamp { validity, values } => parts.push((validity, values)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Vector { .. } => {
                return Err(concat_type_mismatch(FieldType::Timestamp, column.data_type()))
            }
        }
    }
    let (validity, values) = concat_buffer_parts(&parts);
    Ok(Column::Timestamp { validity, values })
}

fn concat_bytes_core(
    parts: &[(&Option<Bitmap>, &Buffer<i32>, &AlignedBuffer)],
) -> (Option<Bitmap>, Buffer<i32>, AlignedBuffer) {
    let total_rows: usize = parts
        .iter()
        .map(|(_, offsets, _)| offsets.as_slice().len().saturating_sub(1))
        .sum();
    let mut new_offsets: Vec<i32> = Vec::with_capacity(total_rows + 1);
    let mut new_data: Vec<u8> = Vec::new();
    new_offsets.push(0);
    for (_, offsets, data) in parts {
        let off = offsets.as_slice();
        let bytes = data.as_bytes();
        let n = off.len().saturating_sub(1);
        for i in 0..n {
            let start = off[i] as usize;
            let end = off[i + 1] as usize;
            new_data.extend_from_slice(&bytes[start..end]);
            new_offsets.push(new_data.len() as i32);
        }
    }
    let has_validity = parts.iter().any(|(validity, _, _)| validity.is_some());
    let validity = if has_validity {
        let mut out = Bitmap::new(total_rows);
        let mut offset = 0;
        for (validity, offsets, _) in parts {
            let n = offsets.as_slice().len().saturating_sub(1);
            match validity {
                Some(bitmap) => {
                    for i in 0..n {
                        if bitmap.get(i) {
                            out.set(offset + i, true);
                        }
                    }
                }
                None => {
                    for i in 0..n {
                        out.set(offset + i, true);
                    }
                }
            }
            offset += n;
        }
        Some(out)
    } else {
        None
    };
    (validity, Buffer::from_slice(&new_offsets), AlignedBuffer::from_slice(&new_data))
}

fn concat_text(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Text { validity, offsets, data } => parts.push((validity, offsets, data)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::Text, column.data_type())),
        }
    }
    let (validity, offsets, data) = concat_bytes_core(&parts);
    Ok(Column::Text { validity, offsets, data })
}

fn concat_binary(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Binary { validity, offsets, data } => parts.push((validity, offsets, data)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. }
            | Column::Vector { .. } => return Err(concat_type_mismatch(FieldType::Binary, column.data_type())),
        }
    }
    let (validity, offsets, data) = concat_bytes_core(&parts);
    Ok(Column::Binary { validity, offsets, data })
}

fn concat_vector(columns: &[Column]) -> Result<Column, Error> {
    let mut parts = Vec::with_capacity(columns.len());
    for column in columns {
        match column {
            Column::Vector { validity, dim, data } => parts.push((validity, *dim, data)),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. } => {
                return Err(concat_type_mismatch(FieldType::Vector, column.data_type()))
            }
        }
    }
    let dim = parts.first().map(|(_, d, _)| *d).unwrap_or(0);
    for (_, d, _) in &parts {
        if *d != dim {
            return Err(Error::general_error(format!(
                "Column::concat: Vector dim mismatch ({dim} vs {d})"
            )));
        }
    }
    let total_rows: usize = parts
        .iter()
        .map(|(_, d, data)| if *d == 0 { 0 } else { data.as_slice().len() / d })
        .sum();
    let mut values: Vec<f32> = Vec::new();
    for (_, _, data) in &parts {
        values.extend_from_slice(data.as_slice());
    }
    let has_validity = parts.iter().any(|(validity, _, _)| validity.is_some());
    let validity = if has_validity {
        let mut out = Bitmap::new(total_rows);
        let mut offset = 0;
        for (validity, d, data) in &parts {
            let n = if *d == 0 { 0 } else { data.as_slice().len() / d };
            match validity {
                Some(bitmap) => {
                    for i in 0..n {
                        if bitmap.get(i) {
                            out.set(offset + i, true);
                        }
                    }
                }
                None => {
                    for i in 0..n {
                        out.set(offset + i, true);
                    }
                }
            }
            offset += n;
        }
        Some(out)
    } else {
        None
    };
    Ok(Column::Vector {
        validity,
        dim,
        data: Buffer::from_slice(&values),
    })
}

impl Column {
    /// Rows, not bytes — for `Text`/`Binary` this is the row count, not `data.len()`.
    pub fn len(&self) -> usize {
        match self {
            Column::Bool { values, .. } => values.len(),
            Column::Int { values, .. } => values.as_slice().len(),
            Column::UInt { values, .. } => values.as_slice().len(),
            Column::Float { values, .. } => values.as_slice().len(),
            Column::Text { offsets, .. } => offsets.as_slice().len().saturating_sub(1),
            Column::Binary { offsets, .. } => offsets.as_slice().len().saturating_sub(1),
            Column::Date { values, .. } => values.as_slice().len(),
            Column::Timestamp { values, .. } => values.as_slice().len(),
            Column::Vector { dim, data, .. } => {
                if *dim == 0 {
                    0
                } else {
                    data.as_slice().len() / dim
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The logical type — the inverse of `FieldSchema::data_type` matching this variant.
    pub fn data_type(&self) -> FieldType {
        match self {
            Column::Bool { .. } => FieldType::Bool,
            Column::Int { .. } => FieldType::Int,
            Column::UInt { .. } => FieldType::UInt,
            Column::Float { .. } => FieldType::Float,
            Column::Text { .. } => FieldType::Text,
            Column::Binary { .. } => FieldType::Binary,
            Column::Date { .. } => FieldType::Date,
            Column::Timestamp { .. } => FieldType::Timestamp,
            Column::Vector { .. } => FieldType::Vector,
        }
    }

    /// One cell. Out of range is an error, never a panic — see phase2-architecture.md §"Why the
    /// required method is a column *range*".
    pub fn get(&self, i: usize) -> Result<FieldValue, Error> {
        if i >= self.len() {
            return Err(Error::general_error(format!(
                "Column::get: index {i} out of bounds (len {})",
                self.len()
            )));
        }
        match self {
            Column::Bool { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::Bool(values.get(i)))
            }
            Column::Int { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::Int(values.as_slice()[i]))
            }
            Column::UInt { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::UInt(values.as_slice()[i]))
            }
            Column::Float { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::Float(values.as_slice()[i]))
            }
            Column::Text { validity, offsets, data } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                let off = offsets.as_slice();
                let start = off[i] as usize;
                let end = off[i + 1] as usize;
                let text = std::str::from_utf8(&data.as_bytes()[start..end])
                    .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
                Ok(FieldValue::Text(Arc::from(text)))
            }
            Column::Binary { validity, offsets, data } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                let off = offsets.as_slice();
                let start = off[i] as usize;
                let end = off[i + 1] as usize;
                Ok(FieldValue::Bytes(Arc::from(&data.as_bytes()[start..end])))
            }
            Column::Date { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::Date(values.as_slice()[i]))
            }
            Column::Timestamp { validity, values } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                Ok(FieldValue::Timestamp(values.as_slice()[i]))
            }
            Column::Vector { validity, dim, data } => {
                if !is_valid(validity, i) {
                    return Ok(FieldValue::Null);
                }
                let slice = data.as_slice();
                let start = i * dim;
                Ok(FieldValue::Vector(Arc::from(&slice[start..start + dim])))
            }
        }
    }

    /// Zero-copy in intent: `Buffer`/`AlignedBuffer` (Step 2.2) have no windowed view of their
    /// own storage, so this copies the selected range into a fresh buffer rather than sharing the
    /// original `Arc`. Filed as `RECORDS-COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING`.
    pub fn slice(&self, offset: usize, len: usize) -> Result<Column, Error> {
        match self {
            Column::Bool { validity, values } => Ok(Column::Bool {
                validity: slice_validity(validity, offset, len)?,
                values: slice_bitmap(values, offset, len)?,
            }),
            Column::Int { validity, values } => Ok(Column::Int {
                validity: slice_validity(validity, offset, len)?,
                values: slice_buffer(values, offset, len)?,
            }),
            Column::UInt { validity, values } => Ok(Column::UInt {
                validity: slice_validity(validity, offset, len)?,
                values: slice_buffer(values, offset, len)?,
            }),
            Column::Float { validity, values } => Ok(Column::Float {
                validity: slice_validity(validity, offset, len)?,
                values: slice_buffer(values, offset, len)?,
            }),
            Column::Text { validity, offsets, data } => {
                let (new_offsets, new_data) = slice_bytes(offsets, data, offset, len)?;
                Ok(Column::Text {
                    validity: slice_validity(validity, offset, len)?,
                    offsets: new_offsets,
                    data: new_data,
                })
            }
            Column::Binary { validity, offsets, data } => {
                let (new_offsets, new_data) = slice_bytes(offsets, data, offset, len)?;
                Ok(Column::Binary {
                    validity: slice_validity(validity, offset, len)?,
                    offsets: new_offsets,
                    data: new_data,
                })
            }
            Column::Date { validity, values } => Ok(Column::Date {
                validity: slice_validity(validity, offset, len)?,
                values: slice_buffer(values, offset, len)?,
            }),
            Column::Timestamp { validity, values } => Ok(Column::Timestamp {
                validity: slice_validity(validity, offset, len)?,
                values: slice_buffer(values, offset, len)?,
            }),
            Column::Vector { validity, dim, data } => Ok(Column::Vector {
                validity: slice_validity(validity, offset, len)?,
                dim: *dim,
                data: slice_vector(data, *dim, offset, len)?,
            }),
        }
    }

    /// The gather: `column[indices[0]], column[indices[1]], …`.
    pub fn take(&self, indices: &[u32]) -> Result<Column, Error> {
        match self {
            Column::Bool { validity, values } => Ok(Column::Bool {
                validity: gather_validity(validity, indices)?,
                values: gather_bitmap(values, indices)?,
            }),
            Column::Int { validity, values } => Ok(Column::Int {
                validity: gather_validity(validity, indices)?,
                values: gather_buffer(values, indices)?,
            }),
            Column::UInt { validity, values } => Ok(Column::UInt {
                validity: gather_validity(validity, indices)?,
                values: gather_buffer(values, indices)?,
            }),
            Column::Float { validity, values } => Ok(Column::Float {
                validity: gather_validity(validity, indices)?,
                values: gather_buffer(values, indices)?,
            }),
            Column::Text { validity, offsets, data } => {
                let (new_offsets, new_data) = gather_bytes(offsets, data, indices)?;
                Ok(Column::Text {
                    validity: gather_validity(validity, indices)?,
                    offsets: new_offsets,
                    data: new_data,
                })
            }
            Column::Binary { validity, offsets, data } => {
                let (new_offsets, new_data) = gather_bytes(offsets, data, indices)?;
                Ok(Column::Binary {
                    validity: gather_validity(validity, indices)?,
                    offsets: new_offsets,
                    data: new_data,
                })
            }
            Column::Date { validity, values } => Ok(Column::Date {
                validity: gather_validity(validity, indices)?,
                values: gather_buffer(values, indices)?,
            }),
            Column::Timestamp { validity, values } => Ok(Column::Timestamp {
                validity: gather_validity(validity, indices)?,
                values: gather_buffer(values, indices)?,
            }),
            Column::Vector { validity, dim, data } => Ok(Column::Vector {
                validity: gather_validity(validity, indices)?,
                dim: *dim,
                data: gather_vector(data, *dim, indices)?,
            }),
        }
    }

    /// `mask.len()` must equal `self.len()`. Built on [`Column::take`]: `Bitmap::iter_ones`
    /// turns the mask into indices once, as phase2-architecture.md §"Views" describes for
    /// `RowIndexView`.
    pub fn filter(&self, mask: &Bitmap) -> Result<Column, Error> {
        if mask.len() != self.len() {
            return Err(Error::general_error(format!(
                "Column::filter: mask length {} does not match column length {}",
                mask.len(),
                self.len()
            )));
        }
        let indices: Vec<u32> = mask.iter_ones().map(|i| i as u32).collect();
        self.take(&indices)
    }

    /// Compares every valid row against `value`, which must match this column's scalar type.
    /// An invalid (null) row never matches. `Binary` and `Vector` have no ordering and always
    /// refuse.
    pub fn compare(&self, op: CompareOp, value: &FieldValue) -> Result<Bitmap, Error> {
        match self {
            Column::Bool { validity, values } => {
                let target = expect_bool(value)?;
                let mut mask = Bitmap::new(values.len());
                for i in 0..values.len() {
                    if is_valid(validity, i) && apply_op(values.get(i), target, op) {
                        mask.set(i, true);
                    }
                }
                Ok(mask)
            }
            Column::Int { validity, values } => {
                Ok(compare_slice(values.as_slice(), validity, expect_int(value)?, op))
            }
            Column::UInt { validity, values } => {
                Ok(compare_slice(values.as_slice(), validity, expect_uint(value)?, op))
            }
            Column::Float { validity, values } => {
                Ok(compare_slice(values.as_slice(), validity, expect_float(value)?, op))
            }
            Column::Text { validity, offsets, data } => {
                compare_text(offsets, data, validity, expect_text(value)?, op)
            }
            Column::Binary { .. } => Err(Error::general_error(
                "Column::compare: Binary columns support no ordered comparison".to_string(),
            )),
            Column::Date { validity, values } => {
                Ok(compare_slice(values.as_slice(), validity, expect_date(value)?, op))
            }
            Column::Timestamp { validity, values } => {
                Ok(compare_slice(values.as_slice(), validity, expect_timestamp(value)?, op))
            }
            Column::Vector { .. } => Err(Error::general_error(
                "Column::compare: Vector columns support no ordered comparison".to_string(),
            )),
        }
    }

    /// Which rows are null — the inverse of validity, all-clear when there is no validity buffer.
    pub fn null_mask(&self) -> Bitmap {
        let len = self.len();
        match self {
            Column::Bool { validity, .. } => validity_to_null_mask(validity, len),
            Column::Int { validity, .. } => validity_to_null_mask(validity, len),
            Column::UInt { validity, .. } => validity_to_null_mask(validity, len),
            Column::Float { validity, .. } => validity_to_null_mask(validity, len),
            Column::Text { validity, .. } => validity_to_null_mask(validity, len),
            Column::Binary { validity, .. } => validity_to_null_mask(validity, len),
            Column::Date { validity, .. } => validity_to_null_mask(validity, len),
            Column::Timestamp { validity, .. } => validity_to_null_mask(validity, len),
            Column::Vector { validity, .. } => validity_to_null_mask(validity, len),
        }
    }

    /// Fails when the columns are not all the same variant, naming both types.
    pub fn concat(columns: &[Column]) -> Result<Column, Error> {
        let first = columns
            .first()
            .ok_or_else(|| Error::general_error("Column::concat: no columns given".to_string()))?;
        match first {
            Column::Bool { .. } => concat_bool(columns),
            Column::Int { .. } => concat_int(columns),
            Column::UInt { .. } => concat_uint(columns),
            Column::Float { .. } => concat_float(columns),
            Column::Text { .. } => concat_text(columns),
            Column::Binary { .. } => concat_binary(columns),
            Column::Date { .. } => concat_date(columns),
            Column::Timestamp { .. } => concat_timestamp(columns),
            Column::Vector { .. } => concat_vector(columns),
        }
    }

    /// Zero rows of `data_type` — what a `RecordBatch` field gets when a stream yields no views
    /// at all but has declared a schema. Crate-private: not part of Phase 2's public surface.
    pub(crate) fn empty(data_type: FieldType) -> Column {
        match data_type {
            FieldType::Bool => Column::Bool {
                validity: None,
                values: Bitmap::new(0),
            },
            FieldType::Int => Column::Int {
                validity: None,
                values: Buffer::from_slice(&[]),
            },
            FieldType::UInt => Column::UInt {
                validity: None,
                values: Buffer::from_slice(&[]),
            },
            FieldType::Float => Column::Float {
                validity: None,
                values: Buffer::from_slice(&[]),
            },
            FieldType::Text => Column::Text {
                validity: None,
                offsets: Buffer::from_slice(&[0i32]),
                data: AlignedBuffer::from_slice(&[]),
            },
            FieldType::Binary => Column::Binary {
                validity: None,
                offsets: Buffer::from_slice(&[0i32]),
                data: AlignedBuffer::from_slice(&[]),
            },
            FieldType::Date => Column::Date {
                validity: None,
                values: Buffer::from_slice(&[]),
            },
            FieldType::Timestamp => Column::Timestamp {
                validity: None,
                values: Buffer::from_slice(&[]),
            },
            FieldType::Vector => Column::Vector {
                validity: None,
                dim: 0,
                data: Buffer::from_slice(&[]),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Phase 3 §2.3 (11 tests), copied verbatim except `field_value_date_variant_holds_days_
    // since_epoch`, corrected per the report. ---

    #[test]
    fn column_get_returns_field_value() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[10i64, 20, 30]),
        };
        assert_eq!(column.get(0).expect("get"), FieldValue::Int(10));
        assert_eq!(column.get(1).expect("get"), FieldValue::Int(20));
    }

    #[test]
    fn column_slice_returns_the_requested_range() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]),
        };
        let sliced = column.slice(1, 3).expect("slice");
        assert_eq!(sliced.len(), 3);
        assert_eq!(sliced.get(0).expect("get"), FieldValue::Int(2));
        assert_eq!(sliced.get(2).expect("get"), FieldValue::Int(4));
    }

    #[test]
    fn column_take_gathers_values() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[10i64, 20, 30, 40, 50]),
        };
        let gathered = column.take(&[0, 2, 4]).expect("take");
        assert_eq!(gathered.len(), 3);
        assert_eq!(gathered.get(0).expect("get"), FieldValue::Int(10));
        assert_eq!(gathered.get(1).expect("get"), FieldValue::Int(30));
        assert_eq!(gathered.get(2).expect("get"), FieldValue::Int(50));
    }

    #[test]
    fn column_filter_applies_bitmap() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]),
        };
        let mask = Bitmap::from_bools(&[true, false, true, false, true]);
        let filtered = column.filter(&mask).expect("filter");
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered.get(0).expect("get"), FieldValue::Int(1));
        assert_eq!(filtered.get(1).expect("get"), FieldValue::Int(3));
        assert_eq!(filtered.get(2).expect("get"), FieldValue::Int(5));
    }

    #[test]
    fn column_compare_eq() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[10i64, 20, 30]),
        };
        let mask = column.compare(CompareOp::Eq, &FieldValue::Int(20)).expect("compare");
        assert!(!mask.get(0));
        assert!(mask.get(1));
        assert!(!mask.get(2));
    }

    #[test]
    fn column_compare_lt() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[10i64, 20, 30]),
        };
        let mask = column.compare(CompareOp::Lt, &FieldValue::Int(25)).expect("compare");
        assert!(mask.get(0));
        assert!(mask.get(1));
        assert!(!mask.get(2));
    }

    #[test]
    fn column_null_mask_reflects_validity() {
        let validity = Bitmap::from_bools(&[true, true, true, false, true, true, true, true]);
        let column = Column::Int {
            validity: Some(validity),
            values: Buffer::from_slice(&[1i64; 8]),
        };
        let mask = column.null_mask();
        assert!(!mask.get(0));
        assert!(!mask.get(2));
        assert!(mask.get(3)); // validity false => null
    }

    #[test]
    fn column_concat_same_type() {
        let c1 = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        let c2 = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[4i64, 5]),
        };
        let concatenated = Column::concat(&[c1, c2]).expect("concat");
        assert_eq!(concatenated.len(), 5);
        assert_eq!(concatenated.get(2).expect("get"), FieldValue::Int(3));
        assert_eq!(concatenated.get(3).expect("get"), FieldValue::Int(4));
    }

    #[test]
    fn column_concat_type_mismatch_error() {
        let c1 = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2]),
        };
        let c2 = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 4]),
            data: AlignedBuffer::from_slice(b"text"),
        };
        assert!(Column::concat(&[c1, c2]).is_err());
    }

    #[test]
    fn field_value_stays_reasonably_small() {
        // Not a hard architectural promise the way `Value`'s 704-byte ceiling is
        // (CORE-VALUE-ENUM-OVERSIZED), but a regression here — a variant growing the enum's
        // largest arm — is worth noticing rather than discovering by profiling later.
        assert!(std::mem::size_of::<FieldValue>() <= 32);
    }

    #[test]
    fn field_value_date_variant_holds_days_since_epoch() {
        // CORRECTED from Phase 3 §2.3: the original test matched with a `_ => panic!(...)` arm,
        // which is a default match arm on `FieldValue` — a Liquers-owned enum — and CLAUDE.md's
        // "Match Statements" convention forbids `_ =>` on those so a future variant is a compile
        // error, not a silent pass. `matches!` asserts the same shape without one.
        let value = FieldValue::Date(19570);
        assert!(matches!(value, FieldValue::Date(19570)));
    }

    // --- Additional coverage beyond Phase 3 §2.3 ---

    #[test]
    fn column_get_out_of_range_is_an_error_not_a_panic() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2]),
        };
        assert!(column.get(5).is_err());
    }

    #[test]
    fn column_get_returns_null_for_invalid_row() {
        let validity = Bitmap::from_bools(&[true, false, true]);
        let column = Column::Int {
            validity: Some(validity),
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        assert_eq!(column.get(1).expect("get"), FieldValue::Null);
        assert_eq!(column.get(0).expect("get"), FieldValue::Int(1));
    }

    #[test]
    fn column_slice_out_of_range_is_an_error() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        assert!(column.slice(2, 5).is_err());
    }

    #[test]
    fn column_take_out_of_range_index_is_an_error() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        assert!(column.take(&[0, 10]).is_err());
    }

    #[test]
    fn column_filter_mask_length_mismatch_is_an_error() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        let mask = Bitmap::new(5);
        assert!(column.filter(&mask).is_err());
    }

    #[test]
    fn column_compare_type_mismatch_is_an_error() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        assert!(column.compare(CompareOp::Eq, &FieldValue::Text(Arc::from("x"))).is_err());
    }

    #[test]
    fn column_compare_skips_null_rows() {
        let validity = Bitmap::from_bools(&[true, false, true]);
        let column = Column::Int {
            validity: Some(validity),
            values: Buffer::from_slice(&[5i64, 5, 5]),
        };
        let mask = column.compare(CompareOp::Eq, &FieldValue::Int(5)).expect("compare");
        assert!(mask.get(0));
        assert!(!mask.get(1)); // null, never matches
        assert!(mask.get(2));
    }

    #[test]
    fn column_null_mask_all_clear_when_no_validity_buffer() {
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        assert_eq!(column.null_mask().count_ones(), 0);
    }

    #[test]
    fn column_text_get_slice_take_filter_roundtrip() {
        // offsets: "ab"(0..2), "cd"(2..4), "ef"(4..6)
        let column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 2, 4, 6]),
            data: AlignedBuffer::from_slice(b"abcdef"),
        };
        assert_eq!(column.len(), 3);
        assert_eq!(column.get(1).expect("get"), FieldValue::Text(Arc::from("cd")));
        let sliced = column.slice(1, 2).expect("slice");
        assert_eq!(sliced.len(), 2);
        assert_eq!(sliced.get(0).expect("get"), FieldValue::Text(Arc::from("cd")));
        assert_eq!(sliced.get(1).expect("get"), FieldValue::Text(Arc::from("ef")));
        let taken = column.take(&[2, 0]).expect("take");
        assert_eq!(taken.get(0).expect("get"), FieldValue::Text(Arc::from("ef")));
        assert_eq!(taken.get(1).expect("get"), FieldValue::Text(Arc::from("ab")));
    }

    #[test]
    fn column_text_concat_joins_bytes_and_offsets() {
        let c1 = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 2]),
            data: AlignedBuffer::from_slice(b"ab"),
        };
        let c2 = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 3]),
            data: AlignedBuffer::from_slice(b"cde"),
        };
        let joined = Column::concat(&[c1, c2]).expect("concat");
        assert_eq!(joined.len(), 2);
        assert_eq!(joined.get(0).expect("get"), FieldValue::Text(Arc::from("ab")));
        assert_eq!(joined.get(1).expect("get"), FieldValue::Text(Arc::from("cde")));
    }

    #[test]
    fn column_vector_get_slice_and_concat() {
        let column = Column::Vector {
            validity: None,
            dim: 2,
            data: Buffer::from_slice(&[1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]),
        };
        assert_eq!(column.len(), 3);
        assert_eq!(
            column.get(1).expect("get"),
            FieldValue::Vector(Arc::from(vec![3.0f32, 4.0]))
        );
        let sliced = column.slice(1, 2).expect("slice");
        assert_eq!(sliced.len(), 2);
        assert_eq!(
            sliced.get(1).expect("get"),
            FieldValue::Vector(Arc::from(vec![5.0f32, 6.0]))
        );
    }

    #[test]
    fn column_bool_get_slice_and_filter() {
        let column = Column::Bool {
            validity: None,
            values: Bitmap::from_bools(&[true, false, true, false]),
        };
        assert_eq!(column.get(0).expect("get"), FieldValue::Bool(true));
        let sliced = column.slice(1, 2).expect("slice");
        assert_eq!(sliced.get(0).expect("get"), FieldValue::Bool(false));
        assert_eq!(sliced.get(1).expect("get"), FieldValue::Bool(true));
    }

    #[test]
    fn column_concat_validity_defaults_to_valid_when_one_side_has_none() {
        let c1 = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2]),
        };
        let c2 = Column::Int {
            validity: Some(Bitmap::from_bools(&[false, true])),
            values: Buffer::from_slice(&[3i64, 4]),
        };
        let joined = Column::concat(&[c1, c2]).expect("concat");
        assert_eq!(joined.get(0).expect("get"), FieldValue::Int(1)); // c1 had no validity: valid
        assert_eq!(joined.get(1).expect("get"), FieldValue::Int(2));
        assert_eq!(joined.get(2).expect("get"), FieldValue::Null); // c2's false
        assert_eq!(joined.get(3).expect("get"), FieldValue::Int(4));
    }

    #[test]
    fn column_data_type_matches_field_type() {
        let column = Column::Float {
            validity: None,
            values: Buffer::from_slice(&[1.0f64]),
        };
        assert_eq!(column.data_type(), FieldType::Float);
    }

    #[test]
    fn column_serde_roundtrips_through_json() {
        let column = Column::Int {
            validity: Some(Bitmap::from_bools(&[true, false, true])),
            values: Buffer::from_slice(&[1i64, 2, 3]),
        };
        let json = serde_json::to_string(&column).expect("serialize");
        let restored: Column = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, column);
    }
}
