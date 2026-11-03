//! `lq.RecordSource`, `lq.RecordStream` and `lq.RecordChunk`: a chunked table, read lazily.
//!
//! `for view in source` pulls the source's own stream one chunk at a time, waiting with the
//! interpreter lock released, and `async for` awaits the same stream: each step reads one chunk
//! through the manifest's schema, a mapped source's projection and the row identity, and keeps
//! one chunk resident. `chunks()` lists the chunk ids without reading any; `chunk(i)` reads the
//! `i`-th the way iteration would (`liquers_records::read_chunk`); `materialize(max_rows)` reads
//! them all into one view, refusing past `max_rows`.
//!
//! A source resolves its chunks where it came from ([`SourceOrigin`]): through the command's
//! context while the command that received it runs, then through the environment; after
//! `shutdown()` every read fails, and a read waiting at that moment ends `Cancelled`.

use std::sync::{Arc, Mutex, OnceLock};

use futures::StreamExt;
use liquers_core::error::Error;
use liquers_records::{
    read_chunk, BoxRecordStream, ChunkId, ChunkList, ChunkResolver, InMemorySource, RecordSource as CoreRecordSource,
    RecordView as CoreRecordView, DEFAULT_MATERIALIZE_MAX_ROWS,
};
use pyo3::exceptions::PyStopIteration;
use pyo3::prelude::*;

use crate::aio::{awaitable, check_blocking, check_blocking_call, Shut, ToPy};
use crate::environment::{block_on, shut_down_error};
use crate::error::error_to_pyerr;
use crate::records::resolver::{PyChunkResolver, SourceOrigin};
use crate::records::schema::RecordSchema;
use crate::records::view::RecordView;
use crate::records::LiveRecord;

/// The shut signal of a source with no environment; never fired, since such a source refuses
/// before it waits.
fn detached_shut() -> &'static Arc<Shut> {
    static SHUT: OnceLock<Arc<Shut>> = OnceLock::new();
    SHUT.get_or_init(|| Arc::new(Shut::default()))
}

/// A chunked table whose chunks are read on demand.
#[pyclass(frozen, module = "liquers")]
pub struct RecordSource {
    pub inner: Arc<dyn CoreRecordSource>,
    origin: Option<SourceOrigin>,
    _live: LiveRecord,
}

impl RecordSource {
    pub fn new(inner: Arc<dyn CoreRecordSource>, origin: Option<SourceOrigin>) -> Self {
        RecordSource { inner, origin, _live: LiveRecord::new() }
    }

    fn shut(&self) -> Arc<Shut> {
        match &self.origin {
            Some(origin) => origin.shut().clone(),
            None => detached_shut().clone(),
        }
    }

    fn resolver(&self) -> Result<Arc<dyn ChunkResolver>, Error> {
        if self.shut().is_set() {
            return Err(shut_down_error());
        }
        Ok(Arc::new(PyChunkResolver::new(self.origin.clone())))
    }

    fn stream(&self) -> RecordStream {
        RecordStream {
            state: Arc::new(Mutex::new(StreamState::Unopened(self.inner.clone(), self.origin.clone()))),
            shut: self.shut(),
            _live: LiveRecord::new(),
        }
    }
}

#[pymethods]
impl RecordSource {
    /// The chunks known without reading any, in order. A manifest with a template lists only
    /// its explicit chunks (`bounded` is `False`); iteration walks further.
    fn chunks(&self) -> Vec<RecordChunk> {
        let ids = match self.inner.chunks() {
            ChunkList::Known(ids) => ids,
            ChunkList::Unbounded { computed } => computed,
        };
        ids.iter().enumerate().map(|(index, id)| RecordChunk::new(index, id)).collect()
    }

    /// Whether `chunks()` lists every chunk.
    #[getter]
    fn bounded(&self) -> bool {
        match self.inner.chunks() {
            ChunkList::Known(_) => true,
            ChunkList::Unbounded { .. } => false,
        }
    }

    /// The schema every chunk has, when the source declares one (a manifest's
    /// `uniform_schema`), else `None`.
    #[getter]
    fn schema(&self) -> Option<RecordSchema> {
        self.inner.schema().map(RecordSchema::from_core)
    }

    /// The manifest the source was made from, as Python data, or `None`.
    #[getter]
    fn manifest(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.inner.manifest() {
            Some(spec) => {
                let json = serde_json::to_value(spec)
                    .map_err(|e| error_to_pyerr(py, &Error::general_error(e.to_string())))?;
                Ok(Some(crate::store::json_to_py(py, &json)?))
            }
            None => Ok(None),
        }
    }

    /// A stream over the chunks: `for view in source`.
    fn __iter__(&self) -> RecordStream {
        self.stream()
    }

    /// The same stream, awaited: `async for view in source`.
    fn __aiter__(&self) -> RecordStream {
        self.stream()
    }

    /// The chunk at position `index` of `chunks()`, read as iteration reads it.
    fn chunk(&self, py: Python<'_>, index: usize) -> PyResult<RecordView> {
        check_blocking_call(py, "chunk")?;
        let source = self.inner.clone();
        let resolver = self.resolver().map_err(|e| error_to_pyerr(py, &e))?;
        block_on(py, &self.shut(), async move { read_chunk(source, resolver, index).await })
            .map(RecordView::new)
            .map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `chunk`.
    fn chunk_async<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyAny>> {
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        let source = self.inner.clone();
        let resolver = self.resolver().map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(py, &self.shut(), async move { read_chunk(source, resolver, index).await }, ToPy::View)
    }

    /// Every chunk, read into one view. Raises the source's error past `max_rows` rows, or when
    /// the chunks' schemas differ.
    #[pyo3(signature = (max_rows=DEFAULT_MATERIALIZE_MAX_ROWS))]
    fn materialize(&self, py: Python<'_>, max_rows: usize) -> PyResult<RecordView> {
        check_blocking_call(py, "materialize")?;
        let source = self.inner.clone();
        let resolver = self.resolver().map_err(|e| error_to_pyerr(py, &e))?;
        block_on(py, &self.shut(), async move {
            let batch = source.materialize(resolver, max_rows).await?;
            Ok(batch as Arc<dyn CoreRecordView>)
        })
        .map(RecordView::new)
        .map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `materialize`.
    #[pyo3(signature = (max_rows=DEFAULT_MATERIALIZE_MAX_ROWS))]
    fn materialize_async<'py>(&self, py: Python<'py>, max_rows: usize) -> PyResult<Bound<'py, PyAny>> {
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        let source = self.inner.clone();
        let resolver = self.resolver().map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(
            py,
            &self.shut(),
            async move {
                let batch = source.materialize(resolver, max_rows).await?;
                Ok(batch as Arc<dyn CoreRecordView>)
            },
            ToPy::View,
        )
    }

    fn __repr__(&self) -> String {
        let kind = if self.inner.manifest().is_some() { "manifest" } else { "in memory" };
        format!("RecordSource({kind}, {} chunk(s) known)", self.chunks().len())
    }
}

/// Where a stream is.
enum StreamState {
    /// Not opened yet: opened by the first step.
    Unopened(Arc<dyn CoreRecordSource>, Option<SourceOrigin>),
    Open(BoxRecordStream),
    /// A step is waiting; a second one is refused rather than queued.
    Busy,
    Done,
}

/// One pass over a source's chunks. A step takes the stream out of its state and puts it back
/// when it ends, so a second step while one waits fails at once instead of blocking.
#[pyclass(frozen, module = "liquers")]
pub struct RecordStream {
    state: Arc<Mutex<StreamState>>,
    shut: Arc<Shut>,
    _live: LiveRecord,
}

/// Puts the stream back when a step ends, or marks it done when the step was abandoned (a
/// shutdown or an interrupt dropped it).
struct Restore {
    state: Arc<Mutex<StreamState>>,
    value: Option<StreamState>,
}

impl Drop for Restore {
    fn drop(&mut self) {
        let value = self.value.take().unwrap_or(StreamState::Done);
        match self.state.lock() {
            Ok(mut state) => *state = value,
            Err(poisoned) => *poisoned.into_inner() = value,
        }
    }
}

impl RecordStream {
    /// Takes the stream out for one step, refusing while another step waits.
    fn take(&self) -> Result<StreamState, Error> {
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| Error::general_error("another step of this record stream is running".to_string()))?;
        match std::mem::replace(&mut *state, StreamState::Busy) {
            StreamState::Busy => Err(Error::general_error(
                "another step of this record stream is waiting; a stream is read one step at a time".to_string(),
            )),
            taken @ (StreamState::Unopened(..) | StreamState::Open(_) | StreamState::Done) => Ok(taken),
        }
    }

    /// The future of one step: the next chunk, or `None` at the end.
    fn step(&self) -> Result<impl std::future::Future<Output = Result<Option<Arc<dyn CoreRecordView>>, Error>> + Send + 'static, Error> {
        let taken = self.take()?;
        let state = self.state.clone();
        Ok(async move {
            let mut restore = Restore { state, value: None };
            let mut stream = match taken {
                StreamState::Unopened(source, origin) => {
                    if origin.as_ref().map(|o| o.shut().is_set()).unwrap_or(false) {
                        return Err(shut_down_error());
                    }
                    let resolver: Arc<dyn ChunkResolver> = Arc::new(PyChunkResolver::new(origin));
                    source.stream(resolver).await?
                }
                StreamState::Open(stream) => stream,
                StreamState::Done => {
                    restore.value = Some(StreamState::Done);
                    return Ok(None);
                }
                StreamState::Busy => return Err(Error::unexpected_error("record stream taken twice".to_string())),
            };
            match stream.next().await {
                Some(Ok(view)) => {
                    restore.value = Some(StreamState::Open(stream));
                    Ok(Some(view))
                }
                Some(Err(error)) => {
                    // A failed chunk ends the stream, as the source's own stream ends after it.
                    restore.value = Some(StreamState::Open(stream));
                    Err(error)
                }
                None => {
                    restore.value = Some(StreamState::Done);
                    Ok(None)
                }
            }
        })
    }
}

#[pymethods]
impl RecordStream {
    fn __iter__(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        slf
    }

    fn __aiter__(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        slf
    }

    /// The next chunk as an `lq.RecordView`; `StopIteration` at the end.
    fn __next__(&self, py: Python<'_>) -> PyResult<RecordView> {
        check_blocking(py, "next(stream)", "`await anext(stream)` or `async for`")?;
        if self.shut.is_set() {
            return Err(error_to_pyerr(py, &shut_down_error()));
        }
        let step = self.step().map_err(|e| error_to_pyerr(py, &e))?;
        match block_on(py, &self.shut, step).map_err(|e| e.into_pyerr(py))? {
            Some(view) => Ok(RecordView::new(view)),
            None => Err(PyStopIteration::new_err(())),
        }
    }

    /// Awaitable next chunk; `StopAsyncIteration` at the end.
    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        if self.shut.is_set() {
            return Err(error_to_pyerr(py, &shut_down_error()));
        }
        let step = self.step().map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(py, &self.shut, step, ToPy::Next)
    }
}

/// One entry of `RecordSource.chunks()`: its position, how it is identified, and whether it can
/// be evaluated on its own.
#[pyclass(frozen, module = "liquers")]
#[derive(Clone)]
pub struct RecordChunk {
    index: usize,
    kind: &'static str,
    query: liquers_core::query::Query,
    evaluable: bool,
}

impl RecordChunk {
    fn new(index: usize, id: &ChunkId) -> Self {
        let memory = InMemorySource::is_placeholder_chunk_id(id);
        let (kind, query) = match id {
            ChunkId::Query(query) => (if memory { "memory" } else { "query" }, query.clone()),
            ChunkId::Key(key) => ("key", liquers_core::query::Query::from(key.clone())),
        };
        RecordChunk { index, kind, query, evaluable: !memory }
    }
}

#[pymethods]
impl RecordChunk {
    /// The chunk's position in `chunks()`, which `RecordSource.chunk` takes.
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// `"query"` (a computed chunk), `"key"` (a stored one) or `"memory"` (a view held in memory,
    /// whose id is a placeholder).
    #[getter]
    fn kind(&self) -> &'static str {
        self.kind
    }

    /// The query that re-produces the chunk (a stored chunk's key as a resource query).
    #[getter]
    fn query(&self) -> crate::query::Query {
        crate::query::Query(self.query.clone())
    }

    /// Whether `query` can be evaluated on its own; `False` for a chunk held in memory.
    #[getter]
    fn evaluable(&self) -> bool {
        self.evaluable
    }

    fn __repr__(&self) -> String {
        format!("RecordChunk({}, {}, '{}')", self.index, self.kind, self.query.encode())
    }
}
