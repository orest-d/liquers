//! Arrow, pandas and polars: the exits from a record view and the entrances into one.
//!
//! Every route copies, through bytes:
//! - `to_arrow` writes the view as an Arrow IPC file (`liquers-records`' writer, with the
//!   interpreter lock released) and `_records.ipc_to_arrow` reads it with pyarrow, copying the
//!   chunk id and origins into the table's schema metadata (`liquers.chunk_id`,
//!   `liquers.origins`) beside the roles `liquers.schema` already carries;
//! - `to_pandas` is `to_arrow().to_pandas()`, `to_polars` is `polars.read_ipc` of the same bytes;
//! - `from_arrow` (and `from_pandas`, through `pyarrow.Table.from_pandas`) normalizes the table in
//!   Python (`_records.arrow_to_ipc`) and reads the IPC bytes back in Rust;
//! - `from_polars` crosses as Parquet, the way DataFrames already cross, and goes through the
//!   polars bridge (`liquers_lib::records::polars`), so it never needs pyarrow.
//!
//! Only `to_arrow` promises metadata; pandas and polars drop it.

use std::io::Cursor;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_records::{
    read_table, write_table, ReadOptions, ReadSchema, RecordView as CoreRecordView, TableFormat, WriteOptions,
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::error::pyerr_to_error;
use crate::records::build::{self, Fields, RawColumns};

fn records_module(py: Python<'_>) -> Result<Bound<'_, PyModule>, Error> {
    py.import("liquers._records").map_err(|e| pyerr_to_error(py, &e))
}

/// The view as IPC bytes, written with the interpreter lock released.
pub fn to_ipc(py: Python<'_>, view: &Arc<dyn CoreRecordView>) -> Result<Vec<u8>, Error> {
    let view = view.clone();
    py.detach(move || write_table(view.as_ref(), TableFormat::Ipc, &WriteOptions::default()))
}

/// A view read from IPC bytes, with the interpreter lock released.
pub fn from_ipc(py: Python<'_>, bytes: Vec<u8>) -> Result<Arc<dyn CoreRecordView>, Error> {
    let batch = py.detach(move || read_table(&bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default()))?;
    Ok(Arc::new(batch))
}

/// `pyarrow.Table` of the view, metadata included.
pub fn to_arrow(py: Python<'_>, view: &Arc<dyn CoreRecordView>) -> Result<Py<PyAny>, Error> {
    let module = records_module(py)?;
    // Imported before the bytes are written, so a missing pyarrow fails first and cheaply.
    module.call_method0("_pyarrow").map_err(|e| pyerr_to_error(py, &e))?;
    let bytes = to_ipc(py, view)?;
    let chunk_id = match view.chunk_id() {
        Some(id) => Some(serde_json::to_string(id).map_err(|e| Error::general_error(e.to_string()))?),
        None => None,
    };
    let origins = if view.origins().is_empty() {
        None
    } else {
        Some(serde_json::to_string(view.origins()).map_err(|e| Error::general_error(e.to_string()))?)
    };
    module
        .call_method1("ipc_to_arrow", (PyBytes::new(py, &bytes), chunk_id, origins))
        .map(Bound::unbind)
        .map_err(|e| pyerr_to_error(py, &e))
}

/// `pandas.DataFrame` of the view.
pub fn to_pandas(py: Python<'_>, view: &Arc<dyn CoreRecordView>) -> Result<Py<PyAny>, Error> {
    let module = records_module(py)?;
    module.call_method1("_require", ("pandas",)).map_err(|e| pyerr_to_error(py, &e))?;
    let table = to_arrow(py, view)?;
    module
        .call_method1("arrow_to_pandas", (table,))
        .map(Bound::unbind)
        .map_err(|e| pyerr_to_error(py, &e))
}

/// `polars.DataFrame` of the view.
pub fn to_polars(py: Python<'_>, view: &Arc<dyn CoreRecordView>) -> Result<Py<PyAny>, Error> {
    let module = records_module(py)?;
    module.call_method1("_require", ("polars",)).map_err(|e| pyerr_to_error(py, &e))?;
    let bytes = to_ipc(py, view)?;
    module
        .call_method1("ipc_to_polars", (PyBytes::new(py, &bytes),))
        .map(Bound::unbind)
        .map_err(|e| pyerr_to_error(py, &e))
}

/// A view of a `pyarrow.Table` or `RecordBatch`.
pub fn from_arrow(table: &Bound<'_, PyAny>) -> Result<Arc<dyn CoreRecordView>, Error> {
    let py = table.py();
    let bytes: Vec<u8> = records_module(py)?
        .call_method1("arrow_to_ipc", (table,))
        .and_then(|b| b.extract())
        .map_err(|e| pyerr_to_error(py, &e))?;
    from_ipc(py, bytes)
}

/// A view of a `pandas.DataFrame`, through Arrow.
pub fn from_pandas(frame: &Bound<'_, PyAny>) -> Result<Arc<dyn CoreRecordView>, Error> {
    let py = frame.py();
    let table = records_module(py)?
        .call_method1("pandas_to_arrow", (frame,))
        .map_err(|e| pyerr_to_error(py, &e))?;
    from_arrow(&table)
}

/// A view of a `polars.DataFrame`, through the polars bridge.
pub fn from_polars(frame: &Bound<'_, PyAny>) -> Result<Arc<dyn CoreRecordView>, Error> {
    let py = frame.py();
    let parquet: Vec<u8> = records_module(py)?
        .call_method1("polars_to_parquet", (frame,))
        .and_then(|b| b.extract())
        .map_err(|e| pyerr_to_error(py, &e))?;
    let batch = py.detach(move || -> Result<_, Error> {
        let frame = liquers_lib::polars::serde::deserialize_dataframe_from_reader(Cursor::new(parquet), "parquet")?;
        // A dtype the bridge cannot carry is a conversion failure here, as on the Arrow route.
        liquers_lib::records::polars::dataframe_to_record_batch(&frame).map_err(|e| {
            if e.error_type == liquers_core::error::ErrorType::NotSupported {
                Error::conversion_error_with_message("polars.DataFrame", "RecordView", &e.message)
            } else {
                e
            }
        })
    })?;
    Ok(Arc::new(batch))
}

/// `view` with `fields` declared: the view itself when nothing is declared, else its cells built
/// again with the declarations applied (`build.rs`'s second pass).
pub fn with_fields(view: Arc<dyn CoreRecordView>, fields: Fields) -> Result<Arc<dyn CoreRecordView>, Error> {
    if matches!(fields, Fields::Infer) {
        return Ok(view);
    }
    let schema = view.schema().clone();
    let rows = view.len();
    let mut cells = Vec::with_capacity(schema.fields.len());
    for col in 0..schema.fields.len() {
        let mut column = Vec::with_capacity(rows);
        for row in 0..rows {
            column.push(view.value(row, col)?);
        }
        cells.push(column);
    }
    let raw = RawColumns { names: schema.fields.iter().map(|f| f.name.clone()).collect(), cells, rows };
    build::build_typed(raw, schema.fields.iter().map(|f| f.data_type).collect(), fields)
}
