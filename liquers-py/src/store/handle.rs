//! `lq.Store`: the Python handle on a store (`specs/design/liquers-py-store/`, Phase 2).
//!
//! `env.store` is the environment's store, configured mounts and their `read_only` and `metadata`
//! layers included, resolved on every call, so a rebuild is followed. `lq.Store.from_object(obj)`
//! adapts a Python object directly, without an environment, for testing a store against the
//! protocol. Every method has a blocking form and an awaitable `_async` twin.

use std::sync::{Arc, Mutex, Weak};

use liquers_core::error::Error;
use liquers_core::metadata::Metadata as CoreMetadata;
use liquers_core::query::Key as CoreKey;
use liquers_core::store::AsyncStore;
use pyo3::prelude::*;

use crate::aio::{awaitable, LazyLoopPool, Shut, ToPy};
use crate::environment::{block_on, shut_down_error, EnvCell};
use crate::error::{error_to_pyerr, PyFailure};
use crate::store::adapter::PyStore;

enum StoreSource {
    Environment(Weak<EnvCell>),
    Direct {
        store: Mutex<Option<Arc<dyn AsyncStore>>>,
        loops: Arc<LazyLoopPool>,
        shut: Arc<Shut>,
    },
}

/// A store, as Python sees it.
///
/// `env.store` is an environment's store; `Store.from_object(obj, prefix="")` adapts a Python
/// object implementing the store protocol directly. Keys are `lq.Key` or key text, data is
/// `bytes`, metadata `lq.Metadata` (`lq.MetadataRecord` and `dict` are accepted too). Every data
/// method has an awaitable `_async` twin; `is_supported` and `is_read_only` do not. After the environment's `shutdown()`, or `close()` of a direct
/// handle, every call raises. See `PYTHON_GUIDE.md`, "Stores".
#[pyclass(frozen, name = "Store", module = "liquers")]
pub struct Store {
    source: StoreSource,
}

impl Store {
    pub(crate) fn of_environment(cell: &Arc<EnvCell>) -> Self {
        Store {
            source: StoreSource::Environment(Arc::downgrade(cell)),
        }
    }

    /// The store and the shut signal its waits watch, resolved on the calling thread.
    fn resolve(&self, py: Python<'_>) -> Result<(Arc<dyn AsyncStore>, Arc<Shut>), Error> {
        crate::runtime::runtime()?;
        match &self.source {
            StoreSource::Environment(cell) => {
                let cell = cell.upgrade().ok_or_else(shut_down_error)?;
                let envref = cell.envref(py)?;
                Ok((envref.get_async_store(), cell.shut.clone()))
            }
            StoreSource::Direct { store, shut, .. } => {
                let store = match store.lock() {
                    Ok(guard) => guard.clone(),
                    Err(poisoned) => poisoned.into_inner().clone(),
                };
                store.map(|store| (store, shut.clone())).ok_or_else(shut_down_error)
            }
        }
    }

    fn blocking<T, F, Fut>(&self, py: Python<'_>, f: F) -> PyResult<T>
    where
        T: Send,
        F: FnOnce(Arc<dyn AsyncStore>) -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>> + Send,
    {
        crate::aio::check_blocking_call(py, "a Store method")?;
        let result = (|| -> Result<T, PyFailure> {
            let (store, shut) = self.resolve(py)?;
            block_on(py, &shut, f(store))
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    fn deferred<'py, T, F, Fut>(
        &self,
        py: Python<'py>,
        f: F,
        wrap: fn(Result<T, Error>) -> ToPy,
    ) -> PyResult<Bound<'py, PyAny>>
    where
        T: Send + 'static,
        F: FnOnce(Arc<dyn AsyncStore>) -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>> + Send + 'static,
    {
        let (store, shut) = self.resolve(py).map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(py, &shut, f(store), wrap)
    }
}

/// A key from `lq.Key` or text.
pub(crate) fn key_of(py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<CoreKey> {
    if let Ok(k) = key.cast::<crate::query::Key>() {
        return Ok(k.borrow().0.clone());
    }
    let text: String = key.extract()?;
    liquers_core::parse::parse_key(&text).map_err(|e| error_to_pyerr(py, &e))
}

/// Metadata from `lq.Metadata`, `lq.MetadataRecord`, a `dict`, or `None` (an empty record).
fn metadata_of(py: Python<'_>, metadata: Option<&Bound<'_, PyAny>>) -> PyResult<CoreMetadata> {
    let Some(metadata) = metadata.filter(|m| !m.is_none()) else {
        return Ok(CoreMetadata::MetadataRecord(liquers_core::metadata::MetadataRecord::new()));
    };
    if let Ok(m) = metadata.cast::<crate::metadata::Metadata>() {
        return Ok(m.borrow().inner.clone());
    }
    if let Ok(record) = metadata.cast::<crate::metadata::MetadataRecord>() {
        return Ok(CoreMetadata::MetadataRecord(record.borrow().inner.clone()));
    }
    let json = crate::environment::py_to_json(py, metadata).map_err(|e| error_to_pyerr(py, &e))?;
    CoreMetadata::from_json_value(json).map_err(crate::error::serialization_pyerr)
}

#[pymethods]
impl Store {
    /// Adapts a Python object implementing the store protocol, without an environment.
    ///
    /// `prefix` is the key prefix it serves. An `async def` method runs on a private loop, which
    /// `close()` stops.
    #[staticmethod]
    #[pyo3(signature = (obj, prefix=""))]
    fn from_object(py: Python<'_>, obj: &Bound<'_, PyAny>, prefix: &str) -> PyResult<Self> {
        let prefix = liquers_core::parse::parse_key(prefix).map_err(|e| error_to_pyerr(py, &e))?;
        let loops = Arc::new(LazyLoopPool::new(std::num::NonZeroUsize::MIN));
        let shut = Arc::new(Shut::default());
        let name = obj
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "object".to_string());
        let store = PyStore::adapt(py, &name, &prefix, obj, loops.clone(), shut.clone())
            .map_err(|e| error_to_pyerr(py, &e))?;
        Ok(Store {
            source: StoreSource::Direct {
                store: Mutex::new(Some(Arc::new(store))),
                loops,
                shut,
            },
        })
    }

    /// Releases a direct handle's store and stops its loop; later calls raise. Does nothing for
    /// `env.store`, which `Environment.shutdown()` ends.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        match &self.source {
            StoreSource::Environment(_) => Ok(()),
            StoreSource::Direct { store, loops, shut } => {
                shut.fire();
                let taken = match store.lock() {
                    Ok(mut guard) => guard.take(),
                    Err(poisoned) => poisoned.into_inner().take(),
                };
                drop(taken);
                loops.close(py)
            }
        }
    }

    /// The data stored under `key`, as `bytes`. Raises `KeyNotFoundError` when it is absent.
    fn get(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.get_bytes(&key).await })
    }

    /// Awaitable `get`.
    fn get_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.get_bytes(&key).await }, ToPy::Bytes)
    }

    /// The same as `get`.
    fn get_bytes(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
        self.get(py, key)
    }

    /// Awaitable `get_bytes`.
    fn get_bytes_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        self.get_async(py, key)
    }

    /// The metadata of `key`, as `lq.Metadata`; a directory's lists its children.
    fn get_metadata(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<crate::metadata::Metadata> {
        let key = key_of(py, key)?;
        let inner = self.blocking(py, |s| async move { s.get_metadata(&key).await })?;
        Ok(crate::metadata::Metadata { inner })
    }

    /// Awaitable `get_metadata`.
    fn get_metadata_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.get_metadata(&key).await }, ToPy::Metadata)
    }

    /// Stores `data` (bytes) with `metadata` under `key`.
    #[pyo3(signature = (key, data, metadata=None))]
    fn set(&self, py: Python<'_>, key: &Bound<'_, PyAny>, data: Vec<u8>, metadata: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let key = key_of(py, key)?;
        let metadata = metadata_of(py, metadata)?;
        self.blocking(py, |s| async move { s.set(&key, &data, &metadata).await })
    }

    /// Awaitable `set`.
    #[pyo3(signature = (key, data, metadata=None))]
    fn set_async<'py>(
        &self,
        py: Python<'py>,
        key: &Bound<'py, PyAny>,
        data: Vec<u8>,
        metadata: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        let metadata = metadata_of(py, metadata)?;
        self.deferred(py, |s| async move { s.set(&key, &data, &metadata).await }, ToPy::Unit)
    }

    /// Replaces the metadata of `key`.
    fn set_metadata(&self, py: Python<'_>, key: &Bound<'_, PyAny>, metadata: &Bound<'_, PyAny>) -> PyResult<()> {
        let key = key_of(py, key)?;
        let metadata = metadata_of(py, Some(metadata))?;
        self.blocking(py, |s| async move { s.set_metadata(&key, &metadata).await })
    }

    /// Awaitable `set_metadata`.
    fn set_metadata_async<'py>(
        &self,
        py: Python<'py>,
        key: &Bound<'py, PyAny>,
        metadata: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        let metadata = metadata_of(py, Some(metadata))?;
        self.deferred(py, |s| async move { s.set_metadata(&key, &metadata).await }, ToPy::Unit)
    }

    /// Removes the data and metadata of `key`.
    fn remove(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.remove(&key).await })
    }

    /// Awaitable `remove`.
    fn remove_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.remove(&key).await }, ToPy::Unit)
    }

    /// Removes the directory `key`.
    fn removedir(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.removedir(&key).await })
    }

    /// Awaitable `removedir`.
    fn removedir_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.removedir(&key).await }, ToPy::Unit)
    }

    /// Creates the directory `key`.
    fn makedir(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.makedir(&key).await })
    }

    /// Awaitable `makedir`.
    fn makedir_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.makedir(&key).await }, ToPy::Unit)
    }

    /// True when `key` holds data or is a directory.
    fn contains(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.contains(&key).await })
    }

    /// Awaitable `contains`.
    fn contains_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.contains(&key).await }, ToPy::Bool)
    }

    /// True when `key` is a directory.
    fn is_dir(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.is_dir(&key).await })
    }

    /// Awaitable `is_dir`.
    fn is_dir_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.is_dir(&key).await }, ToPy::Bool)
    }

    /// The names in the directory `key`.
    fn listdir(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
        let key = key_of(py, key)?;
        self.blocking(py, |s| async move { s.listdir(&key).await })
    }

    /// Awaitable `listdir`.
    fn listdir_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |s| async move { s.listdir(&key).await }, ToPy::Strings)
    }

    /// Settles interrupted writes; returns the keys repaired. Raises the first error, if any.
    fn sync(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        self.blocking(py, |s| async move { sync_keys(s).await })
    }

    /// Awaitable `sync`.
    fn sync_async<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.deferred(py, |s| async move { sync_keys(s).await }, ToPy::Strings)
    }

    /// Whether the store serves `key` (routing). Never raises for a bad key: answers `False`.
    fn is_supported(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let key = key_of(py, key)?;
        let (store, _) = self.resolve(py).map_err(|e| error_to_pyerr(py, &e))?;
        Ok(py.detach(|| store.is_supported(&key)))
    }

    /// True when the store refuses every write.
    fn is_read_only(&self, py: Python<'_>) -> PyResult<bool> {
        let (store, _) = self.resolve(py).map_err(|e| error_to_pyerr(py, &e))?;
        Ok(store.is_read_only())
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match self.resolve(py) {
            Ok((store, _)) => format!("Store({:?})", store.store_name()),
            Err(_) => "Store(closed)".to_string(),
        }
    }
}

async fn sync_keys(store: Arc<dyn AsyncStore>) -> Result<Vec<String>, Error> {
    let report = store.sync().await?;
    if let Some((_, error)) = report.errors.into_iter().next() {
        return Err(error);
    }
    Ok(report.repaired.iter().map(|r| r.key.encode()).collect())
}
