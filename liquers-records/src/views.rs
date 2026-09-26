//! The built-in `RecordView` implementations, and the `impl dyn RecordView` constructors that
//! build them.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Views: `RecordView` and its
//! implementations", §"Building views", §"Writing a view" (the `RowFnView` half — `ColumnMut` and
//! `RecordViewMut` are Step 2.5's `mutable.rs`) and §"A view as a value".
//!
//! **Views do not cache.** Reading the same column of a filtered view twice gathers twice; the
//! consumer's own variable, or a call to `materialize()`, is the cache.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::maybe_send::{MaybeSend, MaybeSync};

use crate::batch::{ChunkId, ChunkOrigin, RecordBatch, RecordView, RowId, RowRun};
use crate::buffer::{AlignedBuffer, Bitmap, Buffer};
use crate::column::{Column, FieldValue};
use crate::schema::{FieldSchema, FieldType, RecordSchema};

/// A column projection over `base`, built by [`select_columns`](RecordView::select_columns) —
/// see phase2-architecture.md §"Building views" ("Column selection always keeps the key
/// columns"). Row identity is untouched by a projection, so every position- and
/// provenance-dependent accessor passes straight through to `base`.
#[derive(Debug)]
pub struct ColumnsView {
    base: Arc<dyn RecordView>,
    schema: Arc<RecordSchema>,
    /// `column_map[i]` is `base`'s column index for this view's column `i`.
    column_map: Vec<usize>,
}

impl RecordView for ColumnsView {
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.base.len()
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let base_col = self.column_map.get(col).copied().ok_or_else(|| {
            Error::general_error(format!(
                "ColumnsView::column_range: column index {col} out of range (0..{})",
                self.column_map.len()
            ))
        })?;
        self.base.column_range(base_col, rows)
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.base.chunk_id()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        self.base.origins()
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        self.base.row_id(row)
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        self.base.row_number(row)
    }
}

/// A contiguous row window over `base`, built by `slice`/`row`. Positions inside the view are
/// `base`'s positions shifted by `offset` — see phase2-architecture.md §"Building views"
/// ("Position is a view concept").
#[derive(Debug)]
pub struct RowRangeView {
    base: Arc<dyn RecordView>,
    offset: usize,
    len: usize,
}

impl RecordView for RowRangeView {
    fn schema(&self) -> &Arc<RecordSchema> {
        self.base.schema()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        if rows.end > self.len {
            return Err(Error::general_error(format!(
                "RowRangeView::column_range: rows {}..{} exceed length {}",
                rows.start, rows.end, self.len
            )));
        }
        self.base
            .column_range(col, (self.offset + rows.start)..(self.offset + rows.end))
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.base.chunk_id()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        self.base.origins()
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        if row >= self.len {
            return Err(Error::general_error(format!(
                "RowRangeView::row_id: row {row} out of range (0..{})",
                self.len
            )));
        }
        self.base.row_id(self.offset + row)
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        if row >= self.len {
            return Err(Error::general_error(format!(
                "RowRangeView::row_number: row {row} out of range (0..{})",
                self.len
            )));
        }
        self.base.row_number(self.offset + row)
    }
}

/// A reordered/selected row set over `base`, built by `filter` (from a mask) and `take` (from
/// explicit indices) — see phase2-architecture.md §"Building views" ("A mask is turned into
/// indices when the view is built"). `indices[k]` is `base`'s row for this view's row `k`, so
/// **the `RowId` of a filtered row is the row's original position**, per §"Every row has an
/// implicit id".
#[derive(Debug)]
pub struct RowIndexView {
    base: Arc<dyn RecordView>,
    indices: Vec<u32>,
}

impl RecordView for RowIndexView {
    fn schema(&self) -> &Arc<RecordSchema> {
        self.base.schema()
    }

    fn len(&self) -> usize {
        self.indices.len()
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let selected = self.indices.get(rows.clone()).ok_or_else(|| {
            Error::general_error(format!(
                "RowIndexView::column_range: rows {}..{} exceed length {}",
                rows.start,
                rows.end,
                self.indices.len()
            ))
        })?;
        if selected.is_empty() {
            return self.base.column_range(col, 0..0);
        }
        // Read only the base span the selected indices fall in — proportional to that span, not
        // to the whole base column — then gather the exact rows out of it.
        let min = selected.iter().copied().min().unwrap_or(0);
        let max = selected.iter().copied().max().unwrap_or(0);
        let span = self
            .base
            .column_range(col, (min as usize)..(max as usize + 1))?;
        let relative: Vec<u32> = selected.iter().map(|&index| index - min).collect();
        span.take(&relative)
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.base.chunk_id()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        self.base.origins()
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        let &index = self.indices.get(row).ok_or_else(|| {
            Error::general_error(format!(
                "RowIndexView::row_id: row {row} out of range (0..{})",
                self.indices.len()
            ))
        })?;
        self.base.row_id(index as usize)
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        let &index = self.indices.get(row).ok_or_else(|| {
            Error::general_error(format!(
                "RowIndexView::row_number: row {row} out of range (0..{})",
                self.indices.len()
            ))
        })?;
        self.base.row_number(index as usize)
    }

    /// Overridden per phase2-architecture.md's `RecordView::materialize` doc: the provided default
    /// approximates a reordering view's rows as one contiguous run from row 0's id, which is
    /// wrong once rows have been reordered. This builds one exact `RowRun` per selected row
    /// instead — "a copy of the selected rows" per §"The implementations".
    fn materialize(&self) -> Result<Arc<RecordBatch>, Error> {
        let schema = self.schema().clone();
        let mut columns = Vec::with_capacity(schema.fields.len());
        for col in 0..schema.fields.len() {
            columns.push(self.column(col)?);
        }
        let mut rows = Vec::with_capacity(self.indices.len());
        for &index in &self.indices {
            let row_id = self.base.row_id(index as usize)?;
            let row_number = self.base.row_number(index as usize)?;
            rows.push(RowRun {
                chunk: row_id.chunk,
                first_row: row_id.row,
                first_number: row_number,
                len: 1,
            });
        }
        RecordBatch::new(
            schema,
            columns,
            self.chunk_id().cloned(),
            Some(rows),
            self.origins().to_vec(),
        )
        .map(Arc::new)
    }
}

/// A derived column appended over `base`, built by `with_column`. The base's own columns pass
/// through unchanged; the one appended column is `f` applied to `sources` over the requested row
/// range — see phase2-architecture.md §"Building views".
///
/// Generic over the closure rather than boxed, per phase2-architecture.md's compilation notes
/// (`dyn Fn(..) + MaybeSend` is `E0225`, since `MaybeSend` is not an auto trait): erased only when
/// coerced to `Arc<dyn RecordView>`. `Debug` is implemented by hand because the closure is not
/// `Debug` and `RecordView: Debug`.
pub struct DerivedColumnView<F>
where
    F: Fn(&[Column]) -> Result<Column, Error> + MaybeSend + MaybeSync + 'static,
{
    base: Arc<dyn RecordView>,
    schema: Arc<RecordSchema>,
    sources: Vec<usize>,
    f: F,
}

impl<F> fmt::Debug for DerivedColumnView<F>
where
    F: Fn(&[Column]) -> Result<Column, Error> + MaybeSend + MaybeSync + 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DerivedColumnView")
            .field("schema", &self.schema)
            .field("sources", &self.sources)
            .field("len", &self.base.len())
            .finish()
    }
}

impl<F> RecordView for DerivedColumnView<F>
where
    F: Fn(&[Column]) -> Result<Column, Error> + MaybeSend + MaybeSync + 'static,
{
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.base.len()
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let base_field_count = self.base.schema().fields.len();
        if col < base_field_count {
            return self.base.column_range(col, rows);
        }
        if col == base_field_count {
            let source_columns: Vec<Column> = self
                .sources
                .iter()
                .map(|&index| self.base.column_range(index, rows.clone()))
                .collect::<Result<_, Error>>()?;
            let derived = (self.f)(&source_columns)?;
            if derived.len() != rows.len() {
                return Err(Error::general_error(format!(
                    "DerivedColumnView::column_range: derived column has {} rows, expected {}",
                    derived.len(),
                    rows.len()
                )));
            }
            return Ok(derived);
        }
        Err(Error::general_error(format!(
            "DerivedColumnView::column_range: column index {col} out of range (0..{})",
            base_field_count + 1
        )))
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.base.chunk_id()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        self.base.origins()
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        self.base.row_id(row)
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        self.base.row_number(row)
    }
}

/// Precomputed columns appended over `base`, built by `with_columns` — search evidence, which is
/// not a function of other columns, unlike [`DerivedColumnView`]. Each appended column must have
/// `base.len()` rows.
#[derive(Debug)]
pub struct AppendedColumnsView {
    base: Arc<dyn RecordView>,
    schema: Arc<RecordSchema>,
    columns: Vec<Column>,
}

impl RecordView for AppendedColumnsView {
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.base.len()
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let base_field_count = self.base.schema().fields.len();
        if col < base_field_count {
            return self.base.column_range(col, rows);
        }
        let appended_index = col - base_field_count;
        let column = self.columns.get(appended_index).ok_or_else(|| {
            Error::general_error(format!(
                "AppendedColumnsView::column_range: column index {col} out of range (0..{})",
                base_field_count + self.columns.len()
            ))
        })?;
        column.slice(rows.start, rows.len())
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.base.chunk_id()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        self.base.origins()
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        self.base.row_id(row)
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        self.base.row_number(row)
    }
}

/// A table computed cell by cell — how a generator written as a closure meets the columnar
/// contract. `f(row, col)`, so a range read computes only the requested rows of the requested
/// column. See phase2-architecture.md §"Writing a view".
///
/// Generic over the closure for the same reason as [`DerivedColumnView`]; `Debug` is implemented
/// by hand (the schema and length, not the closure).
pub struct RowFnView<F>
where
    F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static,
{
    schema: Arc<RecordSchema>,
    len: usize,
    f: F,
}

impl<F> fmt::Debug for RowFnView<F>
where
    F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RowFnView")
            .field("schema", &self.schema)
            .field("len", &self.len)
            .finish()
    }
}

impl<F> RowFnView<F>
where
    F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static,
{
    /// `schema` and `len` are taken as given; nothing about a generator can be checked ahead of a
    /// call to `f`. The `Result` keeps this constructor consistent with every other view
    /// constructor rather than promising validation it cannot perform.
    pub fn new(schema: Arc<RecordSchema>, len: usize, f: F) -> Result<Self, Error> {
        Ok(RowFnView { schema, len, f })
    }
}

impl<F> RecordView for RowFnView<F>
where
    F: Fn(usize, usize) -> Result<FieldValue, Error> + MaybeSend + MaybeSync + 'static,
{
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.len
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        if col >= self.schema.fields.len() {
            return Err(Error::general_error(format!(
                "RowFnView::column_range: column index {col} out of range (0..{})",
                self.schema.fields.len()
            )));
        }
        if rows.end > self.len {
            return Err(Error::general_error(format!(
                "RowFnView::column_range: rows {}..{} exceed length {}",
                rows.start, rows.end, self.len
            )));
        }
        let data_type = self.schema.fields[col].data_type;
        let mut values = Vec::with_capacity(rows.len());
        for row in rows {
            values.push((self.f)(row, col)?);
        }
        column_from_values(data_type, &values)
    }
}

/// Builds a `Column` of `data_type` from a row range's worth of scalars, already computed — how
/// [`RowFnView::column_range`] turns its closure's per-cell results into Arrow-laid-out storage.
///
/// Not the general incremental builder: `ColumnMut` (Step 2.5's `mutable.rs`, per
/// phase2-architecture.md §"Writing a view") grows a column value by value with `reserve`/`push`;
/// this one only ever sees a range's values collected up front, so it stays private to this
/// module rather than exposed as a second builder API.
fn column_from_values(data_type: FieldType, values: &[FieldValue]) -> Result<Column, Error> {
    let len = values.len();
    let mut validity = Bitmap::new(len);
    let mut any_null = false;
    for (row, value) in values.iter().enumerate() {
        if matches!(value, FieldValue::Null) {
            any_null = true;
        } else {
            validity.set(row, true);
        }
    }
    let validity = any_null.then_some(validity);

    fn mismatch(value: &FieldValue, expected: &str) -> Error {
        Error::conversion_error(
            format!("{value:?}"),
            format!("{expected} (RowFnView::column_range)"),
        )
    }

    match data_type {
        FieldType::Bool => {
            let mut bits = Bitmap::new(len);
            for (row, value) in values.iter().enumerate() {
                match value {
                    FieldValue::Null => {}
                    FieldValue::Bool(v) => bits.set(row, *v),
                    other => return Err(mismatch(other, "Bool")),
                }
            }
            Ok(Column::Bool { validity, values: bits })
        }
        FieldType::Int => {
            let mut buf = Vec::with_capacity(len);
            for value in values {
                match value {
                    FieldValue::Null => buf.push(0i64),
                    FieldValue::Int(v) => buf.push(*v),
                    other => return Err(mismatch(other, "Int")),
                }
            }
            Ok(Column::Int { validity, values: Buffer::from_slice(&buf) })
        }
        FieldType::UInt => {
            let mut buf = Vec::with_capacity(len);
            for value in values {
                match value {
                    FieldValue::Null => buf.push(0u64),
                    FieldValue::UInt(v) => buf.push(*v),
                    other => return Err(mismatch(other, "UInt")),
                }
            }
            Ok(Column::UInt { validity, values: Buffer::from_slice(&buf) })
        }
        FieldType::Float => {
            let mut buf = Vec::with_capacity(len);
            for value in values {
                match value {
                    FieldValue::Null => buf.push(0f64),
                    FieldValue::Float(v) => buf.push(*v),
                    other => return Err(mismatch(other, "Float")),
                }
            }
            Ok(Column::Float { validity, values: Buffer::from_slice(&buf) })
        }
        FieldType::Text => {
            let mut offsets = Vec::with_capacity(len + 1);
            let mut data = Vec::new();
            offsets.push(0i32);
            for value in values {
                match value {
                    FieldValue::Null => {}
                    FieldValue::Text(v) => data.extend_from_slice(v.as_bytes()),
                    other => return Err(mismatch(other, "Text")),
                }
                offsets.push(data.len() as i32);
            }
            Ok(Column::Text {
                validity,
                offsets: Buffer::from_slice(&offsets),
                data: AlignedBuffer::from_slice(&data),
            })
        }
        FieldType::Binary => {
            let mut offsets = Vec::with_capacity(len + 1);
            let mut data = Vec::new();
            offsets.push(0i32);
            for value in values {
                match value {
                    FieldValue::Null => {}
                    FieldValue::Bytes(v) => data.extend_from_slice(v),
                    other => return Err(mismatch(other, "Bytes")),
                }
                offsets.push(data.len() as i32);
            }
            Ok(Column::Binary {
                validity,
                offsets: Buffer::from_slice(&offsets),
                data: AlignedBuffer::from_slice(&data),
            })
        }
        FieldType::Date => {
            let mut buf = Vec::with_capacity(len);
            for value in values {
                match value {
                    FieldValue::Null => buf.push(0i32),
                    FieldValue::Date(v) => buf.push(*v),
                    other => return Err(mismatch(other, "Date")),
                }
            }
            Ok(Column::Date { validity, values: Buffer::from_slice(&buf) })
        }
        FieldType::Timestamp => {
            let mut buf = Vec::with_capacity(len);
            for value in values {
                match value {
                    FieldValue::Null => buf.push(0i64),
                    FieldValue::Timestamp(v) => buf.push(*v),
                    other => return Err(mismatch(other, "Timestamp")),
                }
            }
            Ok(Column::Timestamp { validity, values: Buffer::from_slice(&buf) })
        }
        FieldType::Vector => {
            let dim = values
                .iter()
                .find_map(|value| match value {
                    FieldValue::Vector(v) => Some(v.len()),
                    _ => None,
                })
                .unwrap_or(0);
            let mut data = Vec::with_capacity(len * dim);
            for value in values {
                match value {
                    FieldValue::Null => data.extend(std::iter::repeat(0f32).take(dim)),
                    FieldValue::Vector(v) => {
                        if v.len() != dim {
                            return Err(Error::general_error(format!(
                                "RowFnView::column_range: Vector dim mismatch ({} vs {dim})",
                                v.len()
                            )));
                        }
                        data.extend_from_slice(v);
                    }
                    other => return Err(mismatch(other, "Vector")),
                }
            }
            Ok(Column::Vector { validity, dim, data: Buffer::from_slice(&data) })
        }
    }
}

/// The view constructors, as **inherent methods on `dyn RecordView`** rather than default trait
/// methods — see phase2-architecture.md §"Building views". A default body could not build these:
/// wrapping `Arc<Self>` into `Arc<dyn RecordView>` requires `Self: Sized`, which a default trait
/// method body is checked without. This is legal only because `RecordView` is defined in this
/// crate (E0116 would forbid it anywhere else).
impl dyn RecordView {
    /// A column projection: `names`, resolved by exact name, plus the declared `Id` and `Source`
    /// fields even when neither is named — phase2-architecture.md §"Building views" ("Column
    /// selection always keeps the key columns"). Named columns come first, in the order given;
    /// any of `Id`/`Source` not already among them is appended after — Phase 2 does not specify
    /// an order, since the fields it discusses keeping are identified by role, not position.
    pub fn select_columns(self: &Arc<Self>, names: &[&str]) -> Result<Arc<dyn RecordView>, Error> {
        let schema = self.schema();
        let mut indices = Vec::with_capacity(names.len());
        for &name in names {
            let index = schema.index_of(name).ok_or_else(|| {
                let available: Vec<&str> = schema.fields.iter().map(|field| field.name.as_str()).collect();
                Error::general_error(format!(
                    "RecordView::select_columns: no field named '{name}' (available: {})",
                    available.join(", ")
                ))
            })?;
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        for key_index in [schema.id_field(), schema.source_field()].into_iter().flatten() {
            if !indices.contains(&key_index) {
                indices.push(key_index);
            }
        }
        let fields: Vec<FieldSchema> = indices.iter().map(|&index| schema.fields[index].clone()).collect();
        let mut new_schema = RecordSchema::new(fields)?;
        new_schema.type_identifier = schema.type_identifier.clone();
        Ok(Arc::new(ColumnsView {
            base: Arc::clone(self),
            schema: Arc::new(new_schema),
            column_map: indices,
        }))
    }

    /// A contiguous row window, `len` rows starting at `offset`.
    pub fn slice(self: &Arc<Self>, offset: usize, len: usize) -> Result<Arc<dyn RecordView>, Error> {
        if offset + len > self.len() {
            return Err(Error::general_error(format!(
                "RecordView::slice: offset {offset} + len {len} exceeds length {}",
                self.len()
            )));
        }
        Ok(Arc::new(RowRangeView { base: Arc::clone(self), offset, len }))
    }

    /// One row, addressed by position within this view — phase2-architecture.md §"Building
    /// views" ("Position is a view concept").
    pub fn row(self: &Arc<Self>, row: usize) -> Result<Arc<dyn RecordView>, Error> {
        self.slice(row, 1)
    }

    /// One cell: the row, then the named column — the order
    /// `…/rec_id-42/select_columns-price` uses (phase2-architecture.md §"A cell is a chain, not a
    /// command").
    pub fn cell(self: &Arc<Self>, row: usize, column: &str) -> Result<Arc<dyn RecordView>, Error> {
        let row_view = self.row(row)?;
        row_view.select_columns(&[column])
    }

    /// The selected rows, from a mask — turned into indices once, at construction, per
    /// phase2-architecture.md §"Building views" ("A mask is turned into indices when the view is
    /// built").
    pub fn filter(self: &Arc<Self>, mask: &Bitmap) -> Result<Arc<dyn RecordView>, Error> {
        if mask.len() != self.len() {
            return Err(Error::general_error(format!(
                "RecordView::filter: mask length {} does not match view length {}",
                mask.len(),
                self.len()
            )));
        }
        let indices: Vec<u32> = mask.iter_ones().map(|row| row as u32).collect();
        Ok(Arc::new(RowIndexView { base: Arc::clone(self), indices }))
    }

    /// The selected rows, from explicit indices — the gather, `take` on a whole view rather than
    /// one column.
    pub fn take(self: &Arc<Self>, indices: &[u32]) -> Result<Arc<dyn RecordView>, Error> {
        let len = self.len();
        for &index in indices {
            if index as usize >= len {
                return Err(Error::general_error(format!(
                    "RecordView::take: index {index} out of bounds (len {len})"
                )));
            }
        }
        Ok(Arc::new(RowIndexView { base: Arc::clone(self), indices: indices.to_vec() }))
    }

    /// A derived column: `f` receives the same row range of each of `sources`' columns. `field`
    /// is appended after this view's own fields.
    pub fn with_column<F>(
        self: &Arc<Self>,
        field: FieldSchema,
        sources: &[usize],
        f: F,
    ) -> Result<Arc<dyn RecordView>, Error>
    where
        F: Fn(&[Column]) -> Result<Column, Error> + MaybeSend + MaybeSync + 'static,
    {
        let base_schema = self.schema();
        for &index in sources {
            if index >= base_schema.fields.len() {
                return Err(Error::general_error(format!(
                    "RecordView::with_column: source column index {index} out of range (0..{})",
                    base_schema.fields.len()
                )));
            }
        }
        let mut fields = base_schema.fields.clone();
        fields.push(field);
        let mut new_schema = RecordSchema::new(fields)?;
        new_schema.type_identifier = base_schema.type_identifier.clone();
        Ok(Arc::new(DerivedColumnView {
            base: Arc::clone(self),
            schema: Arc::new(new_schema),
            sources: sources.to_vec(),
            f,
        }))
    }

    /// Precomputed columns appended — search evidence, which is not a function of other columns.
    /// Each of `columns` must have `self.len()` rows and match its `fields` entry's declared
    /// type.
    pub fn with_columns(
        self: &Arc<Self>,
        fields: Vec<FieldSchema>,
        columns: Vec<Column>,
    ) -> Result<Arc<dyn RecordView>, Error> {
        if fields.len() != columns.len() {
            return Err(Error::general_error(format!(
                "RecordView::with_columns: {} fields given for {} columns",
                fields.len(),
                columns.len()
            )));
        }
        let len = self.len();
        for (field, column) in fields.iter().zip(columns.iter()) {
            if column.len() != len {
                return Err(Error::general_error(format!(
                    "RecordView::with_columns: column '{}' has {} rows, but the view has {len}",
                    field.name,
                    column.len()
                )));
            }
            if column.data_type() != field.data_type {
                return Err(Error::general_error(format!(
                    "RecordView::with_columns: column '{}' is {:?}, but the field declares {:?}",
                    field.name,
                    column.data_type(),
                    field.data_type
                )));
            }
        }
        let base_schema = self.schema();
        let mut all_fields = base_schema.fields.clone();
        all_fields.extend(fields);
        let mut new_schema = RecordSchema::new(all_fields)?;
        new_schema.type_identifier = base_schema.type_identifier.clone();
        Ok(Arc::new(AppendedColumnsView {
            base: Arc::clone(self),
            schema: Arc::new(new_schema),
            columns,
        }))
    }

    /// Scalar reading: a view of exactly one row and exactly one payload column reads as that
    /// cell — phase2-architecture.md §"A view as a value". *Payload* columns are those whose
    /// `KeyRole` is neither `Id` nor `Source`; when a view has none of those (e.g.
    /// `select_columns-id`, which keeps only the `Id` field), its one remaining column stands in
    /// for the value instead. Any other shape refuses, naming the row count and the payload
    /// column count.
    ///
    /// Phase 2 does not name this helper; `single_cell` is chosen for it here (see the phase 4
    /// implementation report).
    pub fn single_cell(&self) -> Result<FieldValue, Error> {
        let schema = self.schema();
        let payload = schema.payload_fields();
        if self.len() == 1 && payload.len() == 1 {
            return self.value(0, payload[0]);
        }
        if self.len() == 1 && payload.is_empty() && schema.fields.len() == 1 {
            return self.value(0, 0);
        }
        Err(Error::conversion_error(
            format!("{} rows, {} payload columns", self.len(), payload.len()),
            "a single scalar cell (RecordView::single_cell)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::KeyRole;

    fn make_simple_batch() -> Arc<RecordBatch> {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Int),
                FieldSchema::new("name", FieldType::Text),
                FieldSchema::new("value", FieldType::Float),
            ])
            .expect("schema"),
        );
        let id_col = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5]) };
        let name_col = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 3, 6, 9, 12, 15]),
            data: AlignedBuffer::from_slice(b"aaabbbcccdddeee"),
        };
        let val_col = Column::Float { validity: None, values: Buffer::from_slice(&[1.5f64, 2.5, 3.5, 4.5, 5.5]) };
        Arc::new(RecordBatch::new(schema, vec![id_col, name_col, val_col], None, None, vec![]).expect("batch"))
    }

    /// `column_range` and `value` must agree everywhere: a full-column read sliced to one row
    /// equals a one-row range, which equals `value(row, col)` — the three ways of reading a cell
    /// this design provides, checked against each other rather than against a hand copy.
    fn assert_reads_agree(view: &dyn RecordView, col: usize) {
        let full = view.column_range(col, 0..view.len()).expect("column_range full");
        for row in 0..view.len() {
            let single = view.column_range(col, row..row + 1).expect("column_range single");
            let from_full = full.get(row).expect("get");
            let from_single = single.get(0).expect("get");
            let from_value = view.value(row, col).expect("value");
            assert_eq!(from_full, from_single, "column_range(full)[{row}] vs column_range({row}..{row}+1)");
            assert_eq!(from_full, from_value, "column_range(full)[{row}] vs value({row}, {col})");
        }
    }

    #[test]
    fn record_batch_column_range_and_value_agree() {
        let batch = make_simple_batch();
        for col in 0..batch.schema.fields.len() {
            assert_reads_agree(&*batch, col);
        }
    }

    #[test]
    fn record_batch_materialize_is_a_shallow_clone() {
        let batch = make_simple_batch();
        let materialized = batch.materialize().expect("materialize");
        for i in 0..batch.columns.len() {
            match (&batch.columns[i], &materialized.columns[i]) {
                (Column::Int { values: a, .. }, Column::Int { values: b, .. }) => {
                    assert_eq!(a.as_slice(), b.as_slice())
                }
                _ => {} // other columns compared by value elsewhere; the point here is `len`/schema identity
            }
        }
        assert_eq!(batch.len, materialized.len);
    }

    #[test]
    fn columns_view_select_columns_keeps_id_and_maps_indices() {
        // CORRECTED from Phase 3 §2.4: `select_columns` (and every other `impl dyn RecordView`
        // constructor) is an inherent method on `dyn RecordView` taking `self: &Arc<Self>`
        // (phase2-architecture.md §"Building views"). `make_simple_batch()` returns
        // `Arc<RecordBatch>`, and calling a `dyn Trait` inherent method on `Arc<Concrete>`
        // directly does not compile (E0599: method resolution's unsized coercion applies to the
        // dereferenced pointee, not to the `Arc` itself) — verified against rustc. An explicit
        // `Arc<dyn RecordView>` binding is the minimal fix; Phase 2's constructor signatures win
        // over the test's untyped `let`.
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let view = batch_arc.select_columns(&["name", "value"]).expect("select");
        // No Id declared in this schema, so nothing is force-kept; the projection is exact.
        assert_eq!(view.schema().fields.len(), 2);
        assert_eq!(view.schema().fields[0].name, "name");
        assert_reads_agree(&*view, 0);
        assert_reads_agree(&*view, 1);
    }

    #[test]
    fn row_range_view_column_range_delegates_with_offset() {
        // CORRECTED from Phase 3 §2.4: same `Arc<dyn RecordView>` correction as above (`slice` is
        // also an inherent `dyn RecordView` constructor).
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let slice_view = batch_arc.slice(1, 3).expect("slice");
        assert_eq!(slice_view.len(), 3);
        for col in 0..slice_view.schema().fields.len() {
            assert_reads_agree(&*slice_view, col);
        }
    }

    #[test]
    fn row_index_view_gathers_selected_rows() {
        // CORRECTED from Phase 3 §2.4: same `Arc<dyn RecordView>` correction (`filter`).
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let mask = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered = batch_arc.filter(&mask).expect("filter");
        assert_eq!(filtered.len(), 3);
        for col in 0..filtered.schema().fields.len() {
            assert_reads_agree(&*filtered, col);
        }
    }

    #[test]
    fn filter_over_filter_composes() {
        // CORRECTED from Phase 3 §2.4: same `Arc<dyn RecordView>` correction for the first
        // `filter` call; `filtered1` is already `Arc<dyn RecordView>` (the constructors' return
        // type), so the second `.filter()` call needs no change.
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let mask1 = Bitmap::from_bools(&[true, true, true, true, true]);
        let filtered1 = batch_arc.filter(&mask1).expect("filter1");
        let mask2 = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered2 = filtered1.filter(&mask2).expect("filter2");
        assert_eq!(filtered2.len(), 3);
    }

    #[test]
    fn row_fn_view_calls_closure_only_for_the_requested_range_and_column() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let batch_arc = make_simple_batch();
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();
        let schema = batch_arc.schema().clone();
        let view: Arc<dyn RecordView> = Arc::new(
            RowFnView::new(schema, batch_arc.len(), move |_row, _col| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(FieldValue::Int(0))
            })
            .expect("RowFnView::new"),
        );
        let _col = view.column_range(0, 1..3).expect("column_range");
        assert_eq!(call_count.load(Ordering::SeqCst), 2); // exactly rows 1 and 2, column 0
    }

    #[test]
    fn derived_column_view_computes_from_source_columns() -> Result<(), Error> {
        // CORRECTED from Phase 3 §2.4, two independent corrections:
        // 1. Same `Arc<dyn RecordView>` correction as the other inherent-constructor tests
        //    (`with_column`).
        // 2. The closure built its output with `ColumnMut::with_capacity`/`.push`/`.freeze()`,
        //    but `ColumnMut` is Step 2.5's `mutable.rs` (phase4-implementation.md Step 2.5) and
        //    does not exist yet at this step. Rewritten to build the doubled values into a
        //    `Vec<i64>` and construct `Column::Int` directly via `Buffer::from_slice` — the same
        //    computation and the same assertions, using only APIs this step provides.
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let view = batch_arc.with_column(FieldSchema::new("double_id", FieldType::Int), &[0], |cols| {
            let id_col = &cols[0];
            let mut doubled = Vec::with_capacity(id_col.len());
            for i in 0..id_col.len() {
                match id_col.get(i)? {
                    FieldValue::Int(n) => doubled.push(n * 2),
                    other => return Err(Error::general_error(format!("expected Int, got {other:?}"))),
                }
            }
            Ok(Column::Int { validity: None, values: Buffer::from_slice(&doubled) })
        })?;
        assert_eq!(view.schema().fields.len(), 4);
        assert_eq!(view.value(0, 3)?, FieldValue::Int(2));
        assert_eq!(view.value(4, 3)?, FieldValue::Int(10));
        Ok(())
    }

    #[test]
    fn appended_columns_view_keeps_base_columns_readable() -> Result<(), Error> {
        // CORRECTED from Phase 3 §2.4: same `Arc<dyn RecordView>` correction (`with_columns`).
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let extra_col = Column::Int { validity: None, values: Buffer::from_slice(&[10i64, 20, 30, 40, 50]) };
        let view = batch_arc.with_columns(vec![FieldSchema::new("extra", FieldType::Int)], vec![extra_col])?;
        assert_eq!(view.schema().fields.len(), 4);
        assert_reads_agree(&*view, 0); // base column still readable through the wrapper
        assert_eq!(view.value(2, 3)?, FieldValue::Int(30));
        Ok(())
    }

    #[test]
    fn column_range_out_of_bounds_range_is_an_error() {
        let batch = make_simple_batch();
        assert!(batch.column_range(0, 0..100).is_err());
    }

    #[test]
    fn column_range_out_of_bounds_column_is_an_error() {
        let batch = make_simple_batch();
        assert!(batch.column_range(100, 0..2).is_err());
    }

    // --- Additional coverage beyond Phase 3 §2.4 ---

    #[test]
    fn select_columns_unknown_field_names_the_field_and_lists_available_ones() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let error = batch_arc.select_columns(&["missing"]).expect_err("unknown field");
        let message = format!("{error}");
        assert!(message.contains("missing"));
        assert!(message.contains("id"));
    }

    #[test]
    fn select_columns_keeps_id_field_even_when_not_named() {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
                FieldSchema::new("name", FieldType::Text),
            ])
            .expect("schema"),
        );
        let id_col = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) };
        let name_col = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1, 2]),
            data: AlignedBuffer::from_slice(b"ab"),
        };
        let batch: Arc<dyn RecordView> =
            Arc::new(RecordBatch::new(schema, vec![id_col, name_col], None, None, vec![]).expect("batch"));
        let view = batch.select_columns(&["name"]).expect("select");
        assert_eq!(view.schema().fields.len(), 2);
        assert_eq!(view.schema().id_field(), Some(1));
    }

    #[test]
    fn slice_out_of_range_is_an_error() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        assert!(batch_arc.slice(3, 10).is_err());
    }

    #[test]
    fn row_returns_a_one_row_view() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let row_view = batch_arc.row(2).expect("row");
        assert_eq!(row_view.len(), 1);
        assert_eq!(row_view.value(0, 0).expect("value"), FieldValue::Int(3));
    }

    #[test]
    fn cell_reads_row_then_column() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let cell_view = batch_arc.cell(2, "value").expect("cell");
        assert_eq!(cell_view.single_cell().expect("single_cell"), FieldValue::Float(3.5));
    }

    #[test]
    fn filter_mask_length_mismatch_is_an_error() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let mask = Bitmap::new(2);
        assert!(batch_arc.filter(&mask).is_err());
    }

    #[test]
    fn take_out_of_range_index_is_an_error() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        assert!(batch_arc.take(&[0, 99]).is_err());
    }

    #[test]
    fn take_selects_and_reorders_rows() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let taken = batch_arc.take(&[4, 0, 2]).expect("take");
        assert_eq!(taken.len(), 3);
        assert_eq!(taken.value(0, 0).expect("value"), FieldValue::Int(5));
        assert_eq!(taken.value(1, 0).expect("value"), FieldValue::Int(1));
        assert_eq!(taken.value(2, 0).expect("value"), FieldValue::Int(3));
    }

    #[test]
    fn row_index_view_materialize_gives_exact_per_row_provenance() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let taken = batch_arc.take(&[3, 1]).expect("take");
        let materialized = taken.materialize().expect("materialize");
        assert_eq!(materialized.rows.len(), 2); // one exact run per selected row, not one approximate run
        assert_eq!(materialized.row_id(0).expect("row_id"), RowId { chunk: 0, row: 3 });
        assert_eq!(materialized.row_id(1).expect("row_id"), RowId { chunk: 0, row: 1 });
    }

    #[test]
    fn with_column_rejects_out_of_range_source_index() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let result = batch_arc.with_column(FieldSchema::new("bad", FieldType::Int), &[99], |_| {
            Ok(Column::Int { validity: None, values: Buffer::from_slice(&[] as &[i64]) })
        });
        assert!(result.is_err());
    }

    #[test]
    fn with_columns_rejects_length_mismatch() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let too_short = Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) };
        let result = batch_arc.with_columns(vec![FieldSchema::new("extra", FieldType::Int)], vec![too_short]);
        assert!(result.is_err());
    }

    #[test]
    fn with_columns_rejects_type_mismatch() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let wrong_type = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32; 6]),
            data: AlignedBuffer::from_slice(b""),
        };
        let result = batch_arc.with_columns(vec![FieldSchema::new("extra", FieldType::Int)], vec![wrong_type]);
        assert!(result.is_err());
    }

    #[test]
    fn row_fn_view_out_of_range_column_is_an_error() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)]).expect("schema"));
        let view = RowFnView::new(schema, 3, |_row, _col| Ok(FieldValue::Int(0))).expect("RowFnView::new");
        assert!(view.column_range(1, 0..2).is_err());
    }

    #[test]
    fn row_fn_view_out_of_range_rows_is_an_error() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)]).expect("schema"));
        let view = RowFnView::new(schema, 3, |_row, _col| Ok(FieldValue::Int(0))).expect("RowFnView::new");
        assert!(view.column_range(0, 0..10).is_err());
    }

    #[test]
    fn row_fn_view_computes_text_values() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("label", FieldType::Text)]).expect("schema"));
        let view = RowFnView::new(schema, 3, |row, _col| Ok(FieldValue::Text(Arc::from(format!("row{row}")))))
            .expect("RowFnView::new");
        let column = view.column_range(0, 0..3).expect("column_range");
        assert_eq!(column.get(0).expect("get"), FieldValue::Text(Arc::from("row0")));
        assert_eq!(column.get(2).expect("get"), FieldValue::Text(Arc::from("row2")));
    }

    #[test]
    fn row_fn_view_computes_nullable_values() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)]).expect("schema"));
        let view = RowFnView::new(schema, 3, |row, _col| {
            if row == 1 {
                Ok(FieldValue::Null)
            } else {
                Ok(FieldValue::Int(row as i64))
            }
        })
        .expect("RowFnView::new");
        let column = view.column_range(0, 0..3).expect("column_range");
        assert_eq!(column.get(0).expect("get"), FieldValue::Int(0));
        assert_eq!(column.get(1).expect("get"), FieldValue::Null);
        assert_eq!(column.get(2).expect("get"), FieldValue::Int(2));
    }

    #[test]
    fn single_cell_reads_the_one_payload_column() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let view = batch_arc.row(1).expect("row").select_columns(&["value"]).expect("select");
        assert_eq!(view.single_cell().expect("single_cell"), FieldValue::Float(2.5));
    }

    #[test]
    fn single_cell_with_no_payload_columns_reads_the_sole_remaining_column() {
        let schema = Arc::new(
            RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id)]).expect("schema"),
        );
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[42i64]) };
        let batch: Arc<dyn RecordView> =
            Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"));
        assert_eq!(batch.single_cell().expect("single_cell"), FieldValue::Int(42));
    }

    #[test]
    fn single_cell_refuses_more_than_one_row() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let view = batch_arc.select_columns(&["value"]).expect("select");
        assert!(view.single_cell().is_err());
    }

    #[test]
    fn single_cell_refuses_more_than_one_payload_column() {
        let batch_arc: Arc<dyn RecordView> = make_simple_batch();
        let view = batch_arc.row(0).expect("row");
        assert!(view.single_cell().is_err());
    }
}

#[cfg(test)]
mod implicit_id_tests {
    use super::*;

    fn one_column_batch(values: &[i64]) -> Arc<RecordBatch> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)]).expect("schema"));
        let column = Column::Int { validity: None, values: Buffer::from_slice(values) };
        Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
    }

    #[test]
    fn standalone_batch_row_id_defaults_to_chunk_0_row_n() {
        let batch = one_column_batch(&[1, 2, 3]);
        assert_eq!(batch.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(batch.row_id(2).expect("row_id"), RowId { chunk: 0, row: 2 });
    }

    #[test]
    fn standalone_batch_row_number_defaults_to_row_index() {
        let batch = one_column_batch(&[1, 2, 3]);
        assert_eq!(batch.row_number(0).expect("row_number"), Some(0));
        assert_eq!(batch.row_number(2).expect("row_number"), Some(2));
    }

    #[test]
    fn filtered_row_keeps_its_original_row_id() {
        // CORRECTED from Phase 3 §2.5: same `Arc<dyn RecordView>` correction as §2.4's tests —
        // `filter` is an inherent `dyn RecordView` constructor, so `batch` must already be
        // `Arc<dyn RecordView>` before calling it.
        //
        // "The RowId of a filtered row is the row's original position, not its position in the
        // filtered view" (§"Every row has an implicit id").
        let batch: Arc<dyn RecordView> = one_column_batch(&[1, 2, 3, 4, 5]);
        let mask = Bitmap::from_bools(&[true, false, true, true, false]);
        let filtered = batch.filter(&mask).expect("filter");
        assert_eq!(filtered.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(filtered.row_id(1).expect("row_id"), RowId { chunk: 0, row: 2 });
        assert_eq!(filtered.row_id(2).expect("row_id"), RowId { chunk: 0, row: 3 });
    }

    #[test]
    fn batch_from_three_runs_has_correct_row_ids_and_numbers() {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)]).expect("schema"));
        let column = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3, 4, 5, 6]) };
        let runs = vec![
            RowRun { chunk: 0, first_row: 0, first_number: Some(0), len: 2 },
            RowRun { chunk: 1, first_row: 0, first_number: Some(2), len: 2 },
            RowRun { chunk: 2, first_row: 5, first_number: Some(4), len: 2 },
        ];
        let batch = RecordBatch::new(schema, vec![column], None, Some(runs), vec![]).expect("batch");
        assert_eq!(batch.row_id(0).expect("row_id"), RowId { chunk: 0, row: 0 });
        assert_eq!(batch.row_id(1).expect("row_id"), RowId { chunk: 0, row: 1 });
        assert_eq!(batch.row_id(2).expect("row_id"), RowId { chunk: 1, row: 0 });
        assert_eq!(batch.row_id(4).expect("row_id"), RowId { chunk: 2, row: 5 });
        assert_eq!(batch.row_number(4).expect("row_number"), Some(4));
    }

    // --- Additional coverage beyond Phase 3 §2.5 ---

    #[test]
    fn row_range_view_shifts_row_id_by_offset() {
        let batch: Arc<dyn RecordView> = one_column_batch(&[1, 2, 3, 4, 5]);
        let sliced = batch.slice(2, 2).expect("slice");
        assert_eq!(sliced.row_id(0).expect("row_id"), RowId { chunk: 0, row: 2 });
        assert_eq!(sliced.row_id(1).expect("row_id"), RowId { chunk: 0, row: 3 });
        assert_eq!(sliced.row_number(0).expect("row_number"), Some(2));
    }

    #[test]
    fn columns_view_passes_row_id_through_unchanged() {
        let batch: Arc<dyn RecordView> = one_column_batch(&[10, 20, 30]);
        let projected = batch.select_columns(&["value"]).expect("select");
        assert_eq!(projected.row_id(1).expect("row_id"), RowId { chunk: 0, row: 1 });
        assert_eq!(projected.row_number(1).expect("row_number"), Some(1));
    }
}
