//! Records in Python: `lq.RecordView`, `lq.RecordSource` and the classes around them.
//!
//! The wrappers hold the `Arc`s of `ExtValue::RecordView` / `ExtValue::RecordSource` and read
//! through the `liquers-records` traits, so a table crosses into Python, and back into a command,
//! without a copy. Every class is frozen: they hold immutable `Arc`s, which makes them safe on a
//! free-threaded interpreter without locks.
//!
//! - [`view`]: `RecordView` (schema, columns, narrowing, `to_list` / `to_dict`).
//! - [`column`] and [`buffer`]: `RecordColumn`, `RecordBuffer` and the read-only buffer protocol.
//! - [`schema`]: `FieldType`, `KeyRole`, `FieldSchema`, `RecordSchema`, `ChunkOrigin`.
//! - [`convert`]: cells to Python scalars and back.
//! - [`build`]: views built from plain Python data (`from_records`, `from_dict`, `fields=`).
//! - [`arrow`]: the Arrow, pandas and polars exits and entrances, all through copied bytes.
//! - [`source`]: `RecordSource`, `RecordStream` and `RecordChunk`, traversed one chunk per step.
//! - [`resolver`]: where a source reads its chunks ([`Scope`]). Inside a command that is its
//!   context, so chunks become dependencies; afterwards it is the environment, held weakly, so a
//!   source never keeps an environment alive, and one shut down ends every traversal.
//!
//! Conformance: `RECORDS01`–`10` in `tests/test_RECORDS_*.py`; `RECORDS09` and `RECORDS11` are
//! `NA` (see `liquers-py/README.md`).
//!
//! See `specs/design/liquers-py-records/` and `specs/reference/PYTHON_INTEGRATION.md` §Records.

use pyo3::prelude::*;

pub mod arrow;
pub mod buffer;
pub mod build;
pub mod column;
pub mod convert;
pub mod resolver;
pub mod schema;
pub mod source;
pub mod view;

pub use resolver::{PyChunkResolver, Scope, SourceOrigin};

#[cfg(feature = "debug-handles")]
static LIVE_RECORD_HANDLES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Counts a live record wrapper while it exists (`live_record_handle_count`, `debug-handles`).
pub struct LiveRecord(());

impl LiveRecord {
    pub fn new() -> Self {
        #[cfg(feature = "debug-handles")]
        LIVE_RECORD_HANDLES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        LiveRecord(())
    }
}

impl Default for LiveRecord {
    fn default() -> Self {
        LiveRecord::new()
    }
}

impl Drop for LiveRecord {
    fn drop(&mut self) {
        #[cfg(feature = "debug-handles")]
        LIVE_RECORD_HANDLES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// The number of live record wrappers (views, columns, buffers, sources, streams); test-only.
#[cfg(feature = "debug-handles")]
#[pyfunction]
fn live_record_handle_count() -> usize {
    LIVE_RECORD_HANDLES.load(std::sync::atomic::Ordering::SeqCst)
}

/// Adds the record classes to the native module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<schema::FieldType>()?;
    m.add_class::<schema::KeyRole>()?;
    m.add_class::<schema::FieldSchema>()?;
    m.add_class::<schema::RecordSchema>()?;
    m.add_class::<schema::ChunkOrigin>()?;
    m.add_class::<view::RecordView>()?;
    m.add_class::<column::RecordColumn>()?;
    m.add_class::<column::RecordBuffer>()?;
    m.add_class::<source::RecordSource>()?;
    m.add_class::<source::RecordStream>()?;
    m.add_class::<source::RecordChunk>()?;
    #[cfg(feature = "debug-handles")]
    m.add_function(pyo3::wrap_pyfunction!(live_record_handle_count, m)?)?;
    Ok(())
}
