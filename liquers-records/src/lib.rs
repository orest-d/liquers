//! Columnar record streams for Liquers.
//!
//! `liquers-records` holds the Arrow-compatible, columnar data model for tabular record data —
//! aligned buffers, bitmaps, columns and record batches, plus the sources, streams and views that
//! move data through them. It depends on `liquers-core` only, so it can never form a dependency
//! cycle with the richer crates (`liquers-lib`, `liquers-web`, `liquers-py`) that consume it.

extern crate self as liquers_records;

pub mod batch;
pub mod buffer;
pub mod column;
pub mod mutable;
pub mod schema;
pub mod views;

pub use batch::{
    record_stream, BoxRecordStream, ChunkDescriptor, ChunkId, ChunkList, ChunkOrigin, LocatorRule,
    RecordBatch, RecordStream, RecordStreamExt, RecordView, RowId, RowRun,
};
pub use buffer::{AlignedBuffer, Bitmap, Buffer};
pub use column::{Column, CompareOp, FieldValue};
pub use mutable::{ColumnMut, RecordBatchMut, RecordViewMut};
pub use schema::{
    Analyzer, FieldRole, FieldSchema, FieldType, IndexKind, KeyRole, RecordSchema, VectorMetric,
};
pub use views::{
    AppendedColumnsView, ColumnsView, DerivedColumnView, RowFnView, RowIndexView, RowRangeView,
};
