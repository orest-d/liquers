//! `PyStore`: an [`AsyncStore`] over a Python object (`specs/design/liquers-py-store/`, Phase 2).
//!
//! The object's protocol is resolved once, at adaptation, into a [`StoreMethods`] table: a method
//! added to the object later changes nothing (AC-2). Only `get` is mandatory. `get_metadata` and
//! `contains` are derived from `get` when absent; `is_dir` is `False` when `listdir` is absent too;
//! any other absent method raises `KeyNotSupported` naming it (AC-4). Data-only access is taken
//! from the object's five data methods, or derived from `get`, `set`, `remove` and `listdir`
//! (Decision 5), so a mount of the store can take a `metadata` section.
//!
//! Every async method calls Python through [`crate::pycall::call`], so a runtime worker never
//! takes the GIL; only `is_supported`, synchronous by the trait, attaches on the calling thread.

use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{BackendTime, Metadata as CoreMetadata, TimePrecision};
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, DataCapabilities, DataFacts};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

use crate::aio::{LazyLoopPool, Shut};
use crate::error::pyerr_to_store_error;
use crate::pycall::{call, FromPyReturn, LiveHandle, PyMethod};

type Method = Option<Arc<PyMethod>>;

/// The five methods of data-only access, as a store provides them.
pub(crate) struct DataMethods {
    get_data: Arc<PyMethod>,
    set_data: Arc<PyMethod>,
    remove_data: Arc<PyMethod>,
    listdir_data: Arc<PyMethod>,
    data_facts: Arc<PyMethod>,
}

/// Where a store's data-only access comes from.
pub(crate) enum DataAccess {
    /// The object has all five data methods.
    Provided(DataMethods),
    /// Derived from `get`, `set`, `remove` and `listdir` (Decision 5).
    Derived,
    /// None: no `listdir`, or the object sets `data_access = False`.
    None,
}

/// A store object's protocol, resolved once.
pub(crate) struct StoreMethods {
    get: Arc<PyMethod>,
    get_metadata: Method,
    contains: Method,
    is_dir: Method,
    listdir: Method,
    set: Method,
    set_metadata: Method,
    remove: Method,
    removedir: Method,
    makedir: Method,
    rename_data: Method,
    get_stored_metadata: Method,
    /// A plain callable; a coroutine function is refused at adaptation.
    is_supported: Option<Py<PyAny>>,
    data: DataAccess,
    /// The object's `atomic_write` attribute.
    atomic_write: bool,
}

impl StoreMethods {
    /// Resolves the protocol of `object`; the error names the refused method.
    pub(crate) fn resolve(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<Self, String> {
        let method = |name: &str| -> Result<Method, String> {
            PyMethod::resolve(py, object, name)
                .map(|m| m.map(Arc::new))
                .map_err(|e| format!("its '{name}' attribute cannot be used: {e}"))
        };
        let get = method("get")?.ok_or_else(|| "it has no 'get' method".to_string())?;
        let listdir = method("listdir")?;
        let is_dir = method("is_dir")?;
        if listdir.is_some() && is_dir.is_none() {
            return Err("it has 'listdir' but no 'is_dir' method".to_string());
        }
        let is_supported = match method("is_supported")? {
            Some(m) if m.is_async => {
                return Err("its 'is_supported' is `async def`; it must be a plain `def`".to_string())
            }
            Some(m) => Some(m.object.clone_ref(py)),
            None => None,
        };
        let set = method("set")?;
        let remove = method("remove")?;
        let data_methods = [
            method("get_data")?,
            method("set_data")?,
            method("remove_data")?,
            method("listdir_data")?,
            method("data_facts")?,
        ];
        let opted_out = object
            .getattr("data_access")
            .ok()
            .map(|v| !v.is_truthy().unwrap_or(true))
            .unwrap_or(false);
        let provided = data_methods.iter().filter(|m| m.is_some()).count();
        let data = if opted_out {
            DataAccess::None
        } else if provided == 5 {
            let [get_data, set_data, remove_data, listdir_data, data_facts] = data_methods;
            match (get_data, set_data, remove_data, listdir_data, data_facts) {
                (Some(get_data), Some(set_data), Some(remove_data), Some(listdir_data), Some(data_facts)) => {
                    DataAccess::Provided(DataMethods {
                        get_data,
                        set_data,
                        remove_data,
                        listdir_data,
                        data_facts,
                    })
                }
                _ => DataAccess::None,
            }
        } else if provided > 0 {
            return Err(
                "it has some of the data-only methods (get_data, set_data, remove_data, listdir_data, data_facts) but not all five"
                    .to_string(),
            );
        } else if listdir.is_some() {
            DataAccess::Derived
        } else {
            DataAccess::None
        };
        let atomic_write = object
            .getattr("atomic_write")
            .ok()
            .and_then(|v| v.is_truthy().ok())
            .unwrap_or(false);
        Ok(StoreMethods {
            get,
            get_metadata: method("get_metadata")?,
            contains: method("contains")?,
            is_dir,
            listdir,
            set,
            set_metadata: method("set_metadata")?,
            remove,
            removedir: method("removedir")?,
            makedir: method("makedir")?,
            rename_data: method("rename_data")?,
            get_stored_metadata: method("get_stored_metadata")?,
            is_supported,
            data,
            atomic_write,
        })
    }

    fn has_writes(&self) -> bool {
        self.set.is_some()
            || self.set_metadata.is_some()
            || self.remove.is_some()
            || self.removedir.is_some()
            || self.makedir.is_some()
            || matches!(self.data, DataAccess::Provided(_))
    }
}

/// A Python object as an [`AsyncStore`].
pub(crate) struct PyStore {
    name: String,
    prefix: Key,
    methods: Arc<StoreMethods>,
    loops: Arc<LazyLoopPool>,
    shut: Arc<Shut>,
    _handle: LiveHandle,
}

impl PyStore {
    /// Adapts `object`, the store of the mount `name` at `prefix`.
    pub(crate) fn adapt(
        py: Python<'_>,
        name: &str,
        prefix: &Key,
        object: &Bound<'_, PyAny>,
        loops: Arc<LazyLoopPool>,
        shut: Arc<Shut>,
    ) -> Result<Self, Error> {
        let methods = StoreMethods::resolve(py, object).map_err(|reason| {
            Error::configuration_error(format!(
                "The Python store of mount '{name}' (prefix '{prefix}') cannot be used: {reason}"
            ))
        })?;
        Ok(PyStore {
            name: name.to_string(),
            prefix: prefix.clone(),
            methods: Arc::new(methods),
            loops,
            shut,
            _handle: LiveHandle::new(),
        })
    }

    fn missing(&self, key: &Key, method: &str) -> Error {
        Error::key_not_supported(key, &format!("{} (no '{method}' method)", self.store_name()))
    }

    fn subject(&self) -> String {
        format!("store '{}'", self.name)
    }

    /// Calls `method` with the key and the extra arguments, converting the result with `f`.
    async fn invoke<T, F>(
        &self,
        method: &Arc<PyMethod>,
        key: &Key,
        extra: Extra,
        write: bool,
        f: F,
    ) -> Result<T, Error>
    where
        T: Send + 'static,
        F: for<'py> FnOnce(Python<'py>, Bound<'py, PyAny>) -> Result<T, Error> + Send + 'static,
    {
        let owned_key = key.clone();
        let args = arguments(move |py| {
            let mut items: Vec<Py<PyAny>> = vec![Py::new(py, crate::query::Key(owned_key))?.into_any()];
            match extra {
                Extra::None => {}
                Extra::Data(data) => items.push(PyBytes::new(py, &data).into_any().unbind()),
                Extra::Metadata(metadata) => items.push(metadata_to_py(py, metadata)?),
                Extra::DataAndMetadata(data, metadata) => {
                    items.push(PyBytes::new(py, &data).into_any().unbind());
                    items.push(metadata_to_py(py, metadata)?);
                }
                Extra::Key(other) => items.push(Py::new(py, crate::query::Key(other))?.into_any()),
            }
            PyTuple::new(py, items)
        });
        let convert = Convert {
            f,
            key: key.clone(),
            store: self.store_name(),
            write,
        };
        call(method.clone(), self.loops.clone(), self.shut.clone(), self.subject(), args, convert).await
    }

    /// `get` through Python, `None` when the key is absent.
    async fn py_get(&self, key: &Key) -> Result<Option<(Vec<u8>, Option<CoreMetadata>)>, Error> {
        let store = self.store_name();
        let k = key.clone();
        self.invoke(&self.methods.get, key, Extra::None, false, move |py, r| {
            if r.is_none() {
                return Ok(None);
            }
            if let Ok(tuple) = r.cast::<PyTuple>() {
                if tuple.len() == 2 {
                    let data = bytes_of(&tuple.get_item(0).map_err(|e| read_error(&k, &store, e))?, &k, &store, "get")?;
                    let metadata = metadata_of(py, &tuple.get_item(1).map_err(|e| read_error(&k, &store, e))?, &k, &store)?;
                    return Ok(Some((data, metadata)));
                }
            }
            Ok(Some((bytes_of(&r, &k, &store, "get")?, None)))
        })
        .await
    }

    fn finalized(&self, key: &Key, data: &[u8], metadata: Option<CoreMetadata>) -> CoreMetadata {
        let mut metadata =
            metadata.unwrap_or_else(|| CoreMetadata::MetadataRecord(self.default_metadata(key, false)));
        self.finalize_metadata(&mut metadata, key, data, false);
        metadata
    }

    async fn truthy(&self, method: &Arc<PyMethod>, key: &Key) -> Result<bool, Error> {
        self.invoke(method, key, Extra::None, false, |_py, r| {
            r.is_truthy().map_err(|e| Error::general_error(e.to_string()))
        })
        .await
    }

    async fn unit(&self, method: &Arc<PyMethod>, key: &Key, extra: Extra) -> Result<(), Error> {
        self.invoke(method, key, extra, true, |_py, _r| Ok(())).await
    }

    async fn names(&self, method: &Arc<PyMethod>, key: &Key, name: &'static str) -> Result<Vec<String>, Error> {
        let names = self.invoke(method, key, Extra::None, false, list_of).await?;
        match names {
            None => Ok(Vec::new()),
            Some(items) => names_of(items, key, &self.store_name(), name),
        }
    }
}

/// Pins a closure to the higher-ranked signature `pycall::call` takes.
fn arguments<F>(f: F) -> F
where
    F: for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>> + Send + 'static,
{
    f
}

/// The arguments after the key.
enum Extra {
    None,
    Data(Vec<u8>),
    Metadata(CoreMetadata),
    DataAndMetadata(Vec<u8>, CoreMetadata),
    Key(Key),
}

struct Convert<F> {
    f: F,
    key: Key,
    store: String,
    write: bool,
}

impl<T, F> FromPyReturn for Convert<F>
where
    T: Send + 'static,
    F: for<'py> FnOnce(Python<'py>, Bound<'py, PyAny>) -> Result<T, Error> + Send + 'static,
{
    type Output = T;
    fn convert(self, py: Python<'_>, returned: Bound<'_, PyAny>) -> Result<T, Error> {
        (self.f)(py, returned)
    }
    fn failure(&self, py: Python<'_>, error: PyErr) -> Error {
        pyerr_to_store_error(py, &error, &self.key, &self.store, self.write)
    }
}

fn read_error(key: &Key, store: &str, e: impl std::fmt::Display) -> Error {
    Error::key_read_error(key, store, &e.to_string())
}

fn bytes_of(value: &Bound<'_, PyAny>, key: &Key, store: &str, method: &str) -> Result<Vec<u8>, Error> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().to_vec());
    }
    if let Ok(array) = value.cast::<pyo3::types::PyByteArray>() {
        return Ok(array.to_vec());
    }
    let type_name = value.get_type().name().map(|n| n.to_string()).unwrap_or_default();
    Err(Error::key_read_error(
        key,
        store,
        &format!("'{method}' must return bytes, (bytes, metadata) or None, not {type_name}"),
    ))
}

/// Metadata as Python returns it: `lq.Metadata`, `lq.MetadataRecord`, a `dict`, or `None`.
fn metadata_of(py: Python<'_>, value: &Bound<'_, PyAny>, key: &Key, store: &str) -> Result<Option<CoreMetadata>, Error> {
    if value.is_none() {
        return Ok(None);
    }
    if let Ok(metadata) = value.cast::<crate::metadata::Metadata>() {
        return Ok(Some(metadata.borrow().inner.clone()));
    }
    if let Ok(record) = value.cast::<crate::metadata::MetadataRecord>() {
        return Ok(Some(CoreMetadata::MetadataRecord(record.borrow().inner.clone())));
    }
    if value.cast::<pyo3::types::PyDict>().is_ok() {
        let json = crate::environment::py_to_json(py, value)?;
        return CoreMetadata::from_json_value(json)
            .map(Some)
            .map_err(|e| read_error(key, store, format!("invalid metadata: {e}")));
    }
    let type_name = value.get_type().name().map(|n| n.to_string()).unwrap_or_default();
    Err(Error::key_read_error(
        key,
        store,
        &format!("metadata must be lq.Metadata, lq.MetadataRecord, dict or None, not {type_name}"),
    ))
}

fn metadata_to_py(py: Python<'_>, metadata: CoreMetadata) -> PyResult<Py<PyAny>> {
    Ok(Py::new(py, crate::metadata::Metadata { inner: metadata })?.into_any())
}

/// One entry of a listing as Python returned it.
enum Listed {
    Name(String),
    Key(Key),
    Other,
}

/// A listing's entries, read with the GIL; `None` for a `None` listing, `Other` entries refused
/// later, so the error can name the store.
fn list_of(_py: Python<'_>, value: Bound<'_, PyAny>) -> Result<Option<Vec<Listed>>, Error> {
    if value.is_none() {
        return Ok(None);
    }
    let Ok(iterator) = value.try_iter() else {
        return Ok(Some(vec![Listed::Other]));
    };
    let mut items = Vec::new();
    for item in iterator {
        let Ok(item) = item else {
            items.push(Listed::Other);
            continue;
        };
        if let Ok(k) = item.cast::<crate::query::Key>() {
            items.push(Listed::Key(k.borrow().0.clone()));
        } else if let Ok(name) = item.extract::<String>() {
            items.push(Listed::Name(name));
        } else {
            items.push(Listed::Other);
        }
    }
    Ok(Some(items))
}

/// A listing: `lq.Key`s or names; a key is reduced to its last segment.
fn names_of(items: Vec<Listed>, key: &Key, store: &str, method: &str) -> Result<Vec<String>, Error> {
    let refuse = || Error::key_read_error(key, store, &format!("'{method}' must return a list of names or lq.Key"));
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Listed::Name(name) => names.push(name),
            Listed::Key(k) => match k.filename() {
                Some(name) => names.push(name.to_string()),
                None => return Err(refuse()),
            },
            Listed::Other => return Err(refuse()),
        }
    }
    Ok(names)
}

fn facts_of(py: Python<'_>, value: &Bound<'_, PyAny>, key: &Key, store: &str) -> Result<Option<DataFacts>, Error> {
    if value.is_none() {
        return Ok(None);
    }
    let json = crate::environment::py_to_json(py, value)?;
    let field = |name: &str| json.get(name).cloned().unwrap_or(serde_json::Value::Null);
    let size = field("size").as_u64().ok_or_else(|| {
        Error::key_read_error(key, store, &"'data_facts' must return a dict with an integer 'size'")
    })?;
    let precision = match field("precision").as_str() {
        Some(name) => Some(TimePrecision::from_name(name).ok_or_else(|| {
            Error::key_read_error(
                key,
                store,
                &format!("'data_facts' precision '{name}' is not seconds, millis, micros or nanos"),
            )
        })?),
        None => None,
    };
    let modified = match field("modified").as_str() {
        Some(text) => Some(BackendTime::parse(text, precision)?),
        None => None,
    };
    Ok(Some(DataFacts {
        size,
        modified,
        etag: field("etag").as_str().map(str::to_string),
    }))
}

#[async_trait]
impl AsyncStore for PyStore {
    fn store_name(&self) -> String {
        format!("{} Python store", self.name)
    }

    fn key_prefix(&self) -> Key {
        self.prefix.clone()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, CoreMetadata), Error> {
        key.as_absolute()?;
        match self.py_get(key).await? {
            Some((data, metadata)) => {
                let metadata = self.finalized(key, &data, metadata);
                Ok((data, metadata))
            }
            None => Err(Error::key_not_found(key)),
        }
    }

    async fn get_metadata(&self, key: &Key) -> Result<CoreMetadata, Error> {
        key.as_absolute()?;
        let data_path = match &self.methods.get_metadata {
            Some(method) => {
                let store = self.store_name();
                let k = key.clone();
                let metadata = self
                    .invoke(method, key, Extra::None, false, move |py, r| metadata_of(py, &r, &k, &store))
                    .await?;
                match metadata {
                    Some(mut metadata) => {
                        self.finalize_metadata_empty(&mut metadata, key, false, false);
                        Ok(metadata)
                    }
                    None => Err(Error::key_not_found(key)),
                }
            }
            None => self.get(key).await.map(|(_, metadata)| metadata),
        };
        match data_path {
            Err(e) if e.error_type == ErrorType::KeyNotFound => {
                if self.is_dir(key).await? {
                    let mut record = self.default_metadata(key, true);
                    record.children = self.listdir_asset_info(key).await?;
                    Ok(CoreMetadata::MetadataRecord(record))
                } else {
                    Err(e)
                }
            }
            other => other,
        }
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &CoreMetadata) -> Result<(), Error> {
        key.as_absolute()?;
        let method = self.methods.set.as_ref().ok_or_else(|| self.missing(key, "set"))?;
        let mut metadata = metadata.clone();
        liquers_core::store::seed_filename_from_key(&mut metadata, key);
        self.unit(method, key, Extra::DataAndMetadata(data.to_vec(), metadata)).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &CoreMetadata) -> Result<(), Error> {
        key.as_absolute()?;
        let method = self
            .methods
            .set_metadata
            .as_ref()
            .ok_or_else(|| self.missing(key, "set_metadata"))?;
        self.unit(method, key, Extra::Metadata(metadata.clone())).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        let method = self.methods.remove.as_ref().ok_or_else(|| self.missing(key, "remove"))?;
        self.unit(method, key, Extra::None).await
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        let method = self
            .methods
            .removedir
            .as_ref()
            .ok_or_else(|| self.missing(key, "removedir"))?;
        self.unit(method, key, Extra::None).await
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        let method = self.methods.makedir.as_ref().ok_or_else(|| self.missing(key, "makedir"))?;
        self.unit(method, key, Extra::None).await
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
        match &self.methods.contains {
            Some(method) => self.truthy(method, key).await,
            None => match self.get_metadata(key).await {
                Ok(_) => Ok(true),
                Err(e) if e.error_type == ErrorType::KeyNotFound => Ok(false),
                Err(e) => Err(e),
            },
        }
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
        match &self.methods.is_dir {
            Some(method) => self.truthy(method, key).await,
            // Refused at adaptation when `listdir` exists, so this is a flat store.
            None => Ok(false),
        }
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        key.as_absolute()?;
        match (&self.methods.listdir, &self.methods.data) {
            (Some(method), _) => self.names(method, key, "listdir").await,
            // A store offering data-only access lists its data objects.
            (None, DataAccess::Provided(data)) => self.names(&data.listdir_data, key, "listdir_data").await,
            (None, DataAccess::Derived | DataAccess::None) => Err(self.missing(key, "listdir")),
        }
    }

    async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
        key.as_absolute()?;
        match &self.methods.data {
            DataAccess::Provided(data) => {
                let store = self.store_name();
                let k = key.clone();
                let bytes = self
                    .invoke(&data.get_data, key, Extra::None, false, move |_py, r| {
                        if r.is_none() {
                            Ok(None)
                        } else {
                            bytes_of(&r, &k, &store, "get_data").map(Some)
                        }
                    })
                    .await?;
                bytes.ok_or_else(|| Error::key_not_found(key))
            }
            DataAccess::Derived => match self.py_get(key).await? {
                Some((data, _)) => Ok(data),
                None => Err(Error::key_not_found(key)),
            },
            DataAccess::None => Err(no_data_access(key, &self.store_name())),
        }
    }

    async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error> {
        key.as_absolute()?;
        match &self.methods.data {
            DataAccess::Provided(methods) => {
                self.unit(&methods.set_data, key, Extra::Data(data.to_vec())).await
            }
            DataAccess::Derived => {
                let method = self.methods.set.as_ref().ok_or_else(|| self.missing(key, "set"))?;
                let metadata = CoreMetadata::MetadataRecord(liquers_core::metadata::MetadataRecord::new());
                self.unit(method, key, Extra::DataAndMetadata(data.to_vec(), metadata)).await
            }
            DataAccess::None => Err(no_data_access(key, &self.store_name())),
        }
    }

    async fn remove_data(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        match &self.methods.data {
            DataAccess::Provided(methods) => self.unit(&methods.remove_data, key, Extra::None).await,
            DataAccess::Derived => {
                let method = self.methods.remove.as_ref().ok_or_else(|| self.missing(key, "remove"))?;
                self.unit(method, key, Extra::None).await
            }
            DataAccess::None => Err(no_data_access(key, &self.store_name())),
        }
    }

    async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error> {
        from.as_absolute()?;
        to.as_absolute()?;
        match (&self.methods.data, &self.methods.rename_data) {
            (DataAccess::Provided(_) | DataAccess::Derived, Some(method)) => {
                self.unit(method, from, Extra::Key(to.clone())).await
            }
            (DataAccess::Provided(_) | DataAccess::Derived | DataAccess::None, _) => {
                Err(no_data_access(from, &self.store_name()))
            }
        }
    }

    async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
        key.as_absolute()?;
        match &self.methods.data {
            DataAccess::Provided(methods) => self.names(&methods.listdir_data, key, "listdir_data").await,
            DataAccess::Derived => match &self.methods.listdir {
                Some(method) => self.names(method, key, "listdir").await,
                None => Err(no_data_access(key, &self.store_name())),
            },
            DataAccess::None => Err(no_data_access(key, &self.store_name())),
        }
    }

    async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
        key.as_absolute()?;
        match &self.methods.data {
            DataAccess::Provided(methods) => {
                let store = self.store_name();
                let k = key.clone();
                self.invoke(&methods.data_facts, key, Extra::None, false, move |py, r| {
                    facts_of(py, &r, &k, &store)
                })
                .await
            }
            DataAccess::Derived => match self.py_get(key).await? {
                Some((data, _)) => Ok(Some(DataFacts {
                    size: data.len() as u64,
                    modified: None,
                    etag: None,
                })),
                None => Ok(None),
            },
            DataAccess::None => Err(no_data_access(key, &self.store_name())),
        }
    }

    async fn get_stored_metadata(&self, key: &Key) -> Result<Option<CoreMetadata>, Error> {
        key.as_absolute()?;
        let method = match (&self.methods.get_stored_metadata, &self.methods.get_metadata) {
            (Some(method), _) | (None, Some(method)) => method,
            // Never synthesized from the key: no metadata stored is the honest answer.
            (None, None) => return Ok(None),
        };
        let store = self.store_name();
        let k = key.clone();
        self.invoke(method, key, Extra::None, false, move |py, r| metadata_of(py, &r, &k, &store))
            .await
    }

    fn data_capabilities(&self) -> DataCapabilities {
        let provided = match &self.methods.data {
            DataAccess::Provided(_) => true,
            DataAccess::Derived => false,
            DataAccess::None => return DataCapabilities::default(),
        };
        DataCapabilities {
            data_access: true,
            atomic_write: self.methods.atomic_write,
            rename: self.methods.rename_data.is_some(),
            modified_time: provided,
            etag: provided,
        }
    }

    fn is_read_only(&self) -> bool {
        !self.methods.has_writes()
    }

    /// Supported when the key is absolute, under the prefix, and the object's `is_supported`
    /// (if any) says so. An exception, or an exiting interpreter, answers `false`.
    fn is_supported(&self, key: &Key) -> bool {
        if !key.has_key_prefix(&self.prefix) || key.as_absolute().is_err() {
            return false;
        }
        match &self.methods.is_supported {
            None => true,
            Some(callable) => {
                if crate::exit::exiting() {
                    return false;
                }
                crate::exit::attach_tracked(|py| {
                    Py::new(py, crate::query::Key(key.clone()))
                        .and_then(|k| callable.bind(py).call1((k,)))
                        .and_then(|r| r.is_truthy())
                        .unwrap_or(false)
                })
            }
        }
    }
}

fn no_data_access(key: &Key, store: &str) -> Error {
    Error::not_supported(format!("Store '{store}' does not offer data-only access (key '{key}')"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn store_class(py: Python<'_>, source: &str) -> PyResult<Py<PyAny>> {
        let code = CString::new(source)?;
        let module = PyModule::from_code(py, &code, c"fixture.py", c"fixture")?;
        Ok(module.getattr("Store")?.call0()?.unbind())
    }

    fn resolve(source: &str) -> Result<StoreMethods, String> {
        Python::initialize();
        Python::attach(|py| {
            let object = store_class(py, source).map_err(|e| e.to_string())?;
            StoreMethods::resolve(py, object.bind(py))
        })
    }

    #[test]
    fn method_table_records_presence_and_kind() {
        let methods = resolve(
            "class Store:\n    def get(self, key): return None\n    async def set(self, key, data, metadata): pass\n",
        )
        .expect("a store with get resolves");
        assert!(!methods.get.is_async);
        assert!(methods.set.as_ref().map(|m| m.is_async).unwrap_or(false));
        assert!(methods.listdir.is_none() && methods.is_dir.is_none());
        let missing = resolve("class Store:\n    def listdir(self, key): return []\n").err().unwrap_or_default();
        assert!(missing.contains("'get'"), "{missing}");
        let no_is_dir = resolve("class Store:\n    def get(self, key): return None\n    def listdir(self, key): return []\n")
            .err()
            .unwrap_or_default();
        assert!(no_is_dir.contains("'is_dir'"), "{no_is_dir}");
        let async_supported = resolve(
            "class Store:\n    def get(self, key): return None\n    async def is_supported(self, key): return True\n",
        )
        .err()
        .unwrap_or_default();
        assert!(async_supported.contains("is_supported"), "{async_supported}");
        let not_callable = resolve("class Store:\n    def get(self, key): return None\n    set = 3\n")
            .err()
            .unwrap_or_default();
        assert!(not_callable.contains("'set'"), "{not_callable}");
    }

    #[test]
    fn data_access_is_derived_only_with_get_and_listdir() {
        let flat = resolve("class Store:\n    def get(self, key): return None\n").expect("resolves");
        assert!(matches!(flat.data, DataAccess::None));
        let listed = resolve(
            "class Store:\n    def get(self, key): return None\n    def listdir(self, key): return []\n    def is_dir(self, key): return False\n",
        )
        .expect("resolves");
        assert!(matches!(listed.data, DataAccess::Derived));
        let opted_out = resolve(
            "class Store:\n    data_access = False\n    def get(self, key): return None\n    def listdir(self, key): return []\n    def is_dir(self, key): return False\n",
        )
        .expect("resolves");
        assert!(matches!(opted_out.data, DataAccess::None));
        let provided = resolve(
            "class Store:\n    def get(self, key): return None\n    def get_data(self, k): return None\n    def set_data(self, k, d): pass\n    def remove_data(self, k): pass\n    def listdir_data(self, k): return []\n    def data_facts(self, k): return None\n",
        )
        .expect("resolves");
        assert!(matches!(provided.data, DataAccess::Provided(_)));
        let partial = resolve("class Store:\n    def get(self, key): return None\n    def get_data(self, k): return None\n")
            .err()
            .unwrap_or_default();
        assert!(partial.contains("not all five"), "{partial}");
    }

    #[test]
    fn get_stored_metadata_needs_a_provided_get_metadata() -> Result<(), Error> {
        Python::initialize();
        let store = Python::attach(|py| -> Result<PyStore, Error> {
            let object = store_class(py, "class Store:\n    def get(self, key): return b'x'\n")
                .map_err(|e| Error::general_error(e.to_string()))?;
            PyStore::adapt(
                py,
                "t",
                &Key::new(),
                object.bind(py),
                Arc::new(LazyLoopPool::new(std::num::NonZeroUsize::MIN)),
                Arc::new(Shut::default()),
            )
        })?;
        let runtime = crate::runtime::runtime()?;
        let key = liquers_core::parse::parse_key("a.txt")?;
        let stored = Python::attach(|py| py.detach(|| runtime.block_on(store.get_stored_metadata(&key))))?;
        assert!(stored.is_none(), "metadata is never synthesized from the key");
        Ok(())
    }
}
