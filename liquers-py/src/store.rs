use pyo3::prelude::*;

#[pyclass]
pub struct Store(Box<dyn liquers_core::store::Store + Send>);

#[pyfunction]
pub fn local_filesystem_store(path: &str, prefix: &str) -> PyResult<Store> {
    let key = liquers_core::parse::parse_key(prefix)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;
    Ok(Store(Box::new(liquers_core::store::FileStore::new(
        path, &key,
    ))))
}

/// TODO: Add AssetInfo support

#[pymethods]
impl Store {
    /// Get store name
    pub fn store_name(&self) -> String {
        self.0.store_name()
    }

    /// Key prefix common to all keys in this store.
    pub fn key_prefix(&self) -> crate::parse::Key {
        crate::parse::Key(self.0.key_prefix().to_owned())
    }

    /// Get data and metadata
    fn get(&self, key: &crate::parse::Key) -> PyResult<(Vec<u8>, crate::metadata::Metadata)> {
        match self.0.get(&key.0) {
            Ok((data, metadata)) => Ok((data, crate::metadata::Metadata(metadata))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Get data as bytes
    fn get_bytes(&self, key: &crate::parse::Key) -> PyResult<Vec<u8>> {
        match self.0.get_bytes(&key.0) {
            Ok(data) => Ok(data),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Get metadata
    fn get_metadata(&self, key: &crate::parse::Key) -> PyResult<crate::metadata::Metadata> {
        match self.0.get_metadata(&key.0) {
            Ok(metadata) => Ok(crate::metadata::Metadata(metadata)),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Store data and metadata.
    fn set(&mut self, key: &crate::parse::Key, data: &[u8], metadata: &crate::metadata::Metadata) -> PyResult<()> {
        match self.0.set(&key.0, data, &metadata.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Store metadata.
    fn set_metadata(&mut self, key: &crate::parse::Key, metadata: &crate::metadata::Metadata) -> PyResult<()> {
        match self.0.set_metadata(&key.0, &metadata.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Remove data and metadata associated with the key
    fn remove(&mut self, key: &crate::parse::Key) -> PyResult<()> {
        match self.0.remove(&key.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    fn removedir(&mut self, key: &crate::parse::Key) -> PyResult<()> {
        match self.0.removedir(&key.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }
    
    /// Returns true if store contains the key.
    fn contains(&self, key: &crate::parse::Key) -> PyResult<bool> {
        match self.0.contains(&key.0) {
            Ok(b) => Ok(b),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }
    
    /// Returns true if key points to a directory.
    fn is_dir(&self, key: &crate::parse::Key) -> PyResult<bool> {
        match self.0.is_dir(&key.0) {
            Ok(b) => Ok(b),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// List or iterator of all keys
    fn keys(&self) -> PyResult<Vec<crate::parse::Key>> {
        match self.0.keys() {
            Ok(keys) => Ok(keys.into_iter().map(|k| crate::parse::Key(k)).collect()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    fn listdir(&self, key: &crate::parse::Key) -> PyResult<Vec<String>> {
        match self.0.listdir(&key.0) {
            Ok(names) => Ok(names),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    fn listdir_keys(&self, key: &crate::parse::Key) -> PyResult<Vec<crate::parse::Key>> {
        match self.0.listdir_keys(&key.0) {
            Ok(keys) => Ok(keys.into_iter().map(|k| crate::parse::Key(k)).collect()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    fn listdir_keys_deep(&self, key: &crate::parse::Key) -> PyResult<Vec<crate::parse::Key>> {
        match self.0.listdir_keys_deep(&key.0) {
            Ok(keys) => Ok(keys.into_iter().map(|k| crate::parse::Key(k)).collect()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Make a directory
    fn makedir(&self, key: &crate::parse::Key) -> PyResult<()> {
        match self.0.makedir(&key.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())),
        }
    }

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, key: &crate::parse::Key) -> bool {
        self.0.is_supported(&key.0)
    }
}
