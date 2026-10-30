use liquers_core::error::Error;
use liquers_core::metadata::{
    AssetInfo as CoreAssetInfo, DependencyKey as CoreDependencyKey,
    DependencyRecord as CoreDependencyRecord, LogEntry as CoreLogEntry,
    LogEntryKind as CoreLogEntryKind, Metadata as CoreMetadata,
    MetadataRecord as CoreMetadataRecord, Status as CoreStatus, Version as CoreVersion,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::error::serialization_pyerr;
use crate::parse::{Key, Position, Query};

/// An opaque version of an asset's content: only equality matters.
///
/// `Version(value)` takes a non-negative integer below 2**128.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    pub inner: CoreVersion,
}

#[pymethods]
impl Version {
    #[new]
    pub fn new(value: u128) -> Self {
        Self {
            inner: CoreVersion::new(value),
        }
    }

    /// The version as text.
    pub fn encode(&self) -> String {
        self.inner.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("Version('{}')", self.inner)
    }

    pub fn __str__(&self) -> String {
        self.inner.to_string()
    }
}

/// The name of something an asset can depend on: a query, a store key, a command's metadata, ...
///
/// `DependencyKey(text)` takes the encoded form; any text is accepted.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DependencyKey {
    pub inner: CoreDependencyKey,
}

#[pymethods]
impl DependencyKey {
    #[new]
    pub fn new(value: String) -> Self {
        Self {
            inner: CoreDependencyKey::new(value),
        }
    }

    /// The dependency key as text.
    pub fn encode(&self) -> String {
        self.inner.to_string()
    }

    pub fn __repr__(&self) -> String {
        format!("DependencyKey('{}')", self.inner)
    }

    pub fn __str__(&self) -> String {
        self.inner.to_string()
    }
}

/// One dependency of an evaluated asset and the version it had at the time.
///
/// `DependencyRecord(key, version)` takes a `DependencyKey` and a `Version`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRecord {
    pub inner: CoreDependencyRecord,
}

#[pymethods]
impl DependencyRecord {
    #[new]
    pub fn new(key: &DependencyKey, version: &Version) -> Self {
        Self {
            inner: CoreDependencyRecord::new(key.inner.clone(), version.inner),
        }
    }

    /// What the asset depended on, as a `DependencyKey`.
    #[getter]
    pub fn key(&self) -> DependencyKey {
        DependencyKey {
            inner: self.inner.key.clone(),
        }
    }

    /// The `Version` the dependency had when the asset was evaluated.
    #[getter]
    pub fn version(&self) -> Version {
        Version {
            inner: self.inner.version,
        }
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// The metadata of an asset: its status, type, media type, log and more.
///
/// Returned by `State.metadata` and `AssetRef.metadata()`. It wraps either a structured record or a
/// legacy JSON document kept as it was read; the methods work on both, and those that need a field
/// a legacy document lacks raise an exception. `Metadata()` is an empty record. See
/// `PYTHON_GUIDE.md` §6, "Reading metadata".
#[pyclass(module = "liquers")]
pub struct Metadata {
    pub inner: CoreMetadata,
}

#[pymethods]
impl Metadata {
    #[new]
    pub fn new() -> Self {
        Metadata {
            inner: CoreMetadata::new(),
        }
    }

    /// The most important fields as an `AssetInfo`.
    ///
    /// Raises an exception when legacy metadata cannot be read as one.
    pub fn get_asset_info(&self) -> PyResult<AssetInfo> {
        self.inner
            .get_asset_info()
            .map(|info| AssetInfo { inner: info })
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    /// Sets the query; when the query names a store key, the key and filename are set too.
    pub fn with_query(&mut self, query: Query) {
        let _ = self.inner.with_query(query.0);
    }

    /// Reads metadata from JSON text, as `to_json()` writes it.
    ///
    /// A document that is not a metadata record is kept as legacy metadata. Raises an exception
    /// when the text is not JSON.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        CoreMetadata::from_json(json)
            .map(|m| Metadata { inner: m })
            .map_err(serialization_pyerr)
    }

    /// True when the metadata describes a failed evaluation.
    pub fn is_error(&self) -> PyResult<bool> {
        self.inner
            .is_error()
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    /// The metadata serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        self.inner.to_json().map_err(serialization_pyerr)
    }

    /// The media type of the data, such as `text/plain`.
    pub fn get_media_type(&self) -> String {
        self.inner.get_media_type()
    }

    /// The query that produced the asset. Raises an exception when legacy metadata has no readable
    /// query.
    pub fn query(&self) -> PyResult<Query> {
        self.inner
            .query()
            .map(Query)
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    /// Sets the type identifier of the value.
    pub fn with_type_identifier(&mut self, type_identifier: String) {
        self.inner.with_type_identifier(type_identifier);
    }

    /// The type identifier of the value, such as `text`. Raises an exception when legacy metadata
    /// has none.
    pub fn type_identifier(&self) -> PyResult<String> {
        self.inner
            .type_identifier()
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    /// The asset's filename, or `None`.
    pub fn filename(&self) -> Option<String> {
        self.inner.filename()
    }

    /// The extension of the asset's filename, or `None`.
    pub fn extension(&self) -> Option<String> {
        self.inner.extension()
    }

    /// Replaces the extension of the asset's filename, or appends one; nothing changes without a
    /// filename. Raises an exception on legacy metadata.
    pub fn set_extension(&mut self, extension: &str) -> PyResult<()> {
        self.inner
            .set_extension(extension)
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))?;
        Ok(())
    }

    /// The format the data is serialized in; the filename extension when none is set, else `bin`.
    pub fn get_data_format(&self) -> String {
        self.inner.get_data_format()
    }

    /// Marks the metadata as failed with a general error carrying the message `error`.
    pub fn with_error(&mut self, error: String) {
        self.inner.with_error(Error::general_error(error));
    }

    /// The log entries, as `LogEntry` objects; empty for legacy metadata, which has no log.
    pub fn log(&self) -> Vec<LogEntry> {
        match self.inner.metadata_record() {
            Some(record) => record
                .log
                .iter()
                .map(|entry| LogEntry {
                    inner: entry.clone(),
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// The asset's `Status`.
    pub fn status(&self) -> Status {
        Status {
            inner: self.inner.status(),
        }
    }

    /// The current status message; for a failed asset, the error message.
    pub fn message(&self) -> String {
        self.inner.message().to_string()
    }

    /// An emoji that stands for the asset's kind of data.
    pub fn unicode_icon(&self) -> String {
        self.inner.unicode_icon().to_string()
    }

    /// The size of the data in bytes, or `None` when not known.
    pub fn file_size(&self) -> Option<u64> {
        self.inner.file_size()
    }

    /// True when the asset is a directory.
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    /// Sets whether the asset is a directory.
    pub fn with_isdir(&mut self, is_dir: bool) {
        self.inner.with_is_dir(is_dir);
    }

    /// Sets the size of the data in bytes.
    pub fn with_file_size(&mut self, file_size: u64) {
        self.inner.with_file_size(file_size);
    }

    /// Raises the asset's error when the metadata describes a failed evaluation; returns `None`
    /// otherwise.
    pub fn error_result(&self) -> PyResult<()> {
        self.inner
            .error_result()
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// The structured form of asset metadata, with every field readable and settable.
///
/// `MetadataRecord()` is an empty record. Most fields are properties; the `with_*` methods set a
/// field the way the evaluator does, keeping related fields consistent.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct MetadataRecord {
    pub inner: CoreMetadataRecord,
}

#[pymethods]
impl MetadataRecord {
    #[new]
    pub fn new() -> Self {
        MetadataRecord {
            inner: CoreMetadataRecord::new(),
        }
    }

    /// The log entries, as `LogEntry` objects. Settable.
    #[getter]
    pub fn log(&self) -> Vec<LogEntry> {
        self.inner
            .log
            .iter()
            .map(|entry| LogEntry {
                inner: entry.clone(),
            })
            .collect()
    }

    #[setter]
    pub fn set_log(&mut self, log: Vec<LogEntry>) {
        self.inner.log = log.into_iter().map(|entry| entry.inner).collect();
    }

    /// The query that produced the asset. Settable.
    #[getter]
    pub fn query(&self) -> Query {
        Query(self.inner.query.clone())
    }

    #[setter]
    pub fn set_query(&mut self, query: Query) {
        self.inner.query = query.0;
    }

    /// The store key the asset is kept under, or `None` for a value with no key of its own.
    /// Settable.
    #[getter]
    pub fn key(&self) -> Option<Key> {
        if let Some(key) = &self.inner.key {
            Some(Key(key.clone()))
        } else {
            None
        }
    }
    #[setter]
    pub fn set_key(&mut self, key: Option<Key>) {
        self.inner.key = key.map(|k| k.0);
    }

    /// The asset's `Status`. Settable.
    #[getter]
    pub fn status(&self) -> Status {
        Status {
            inner: self.inner.status.clone(),
        }
    }

    #[setter]
    pub fn set_status(&mut self, status: Status) {
        self.inner.status = status.inner;
    }

    /// The type identifier of the value, such as `text`. Settable.
    #[getter]
    pub fn type_identifier(&self) -> String {
        self.inner.type_identifier.clone()
    }
    #[setter]
    pub fn set_type_identifier(&mut self, type_identifier: String) {
        self.inner.type_identifier = type_identifier;
    }

    /// The format the data is serialized in, usually a filename extension, or `None`. Settable.
    #[getter]
    pub fn data_format(&self) -> Option<String> {
        self.inner.data_format.clone()
    }
    #[setter]
    pub fn set_data_format(&mut self, data_format: Option<String>) {
        self.inner.data_format = data_format;
    }

    /// The current status message; for a failed asset, the error message. Settable.
    #[getter]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }
    #[setter]
    pub fn set_message(&mut self, message: String) {
        self.inner.message = message;
    }

    /// True when the asset failed. Settable.
    #[getter]
    pub fn is_error(&self) -> bool {
        self.inner.is_error
    }
    #[setter]
    pub fn set_is_error(&mut self, is_error: bool) {
        self.inner.is_error = is_error;
    }

    // TODO: Implement error_data getter and setter
    /*
    #[getter]
    pub fn error_data(&self) -> Option<Error> {
        self.inner.error_data.clone()
    }
    */
    /*
    #[setter]
    pub fn set_error_data(&mut self, error_data: Option<Error>) {
        self.inner.error_data = error_data;
    }
    */

    /// The media type of the data, such as `text/plain`, as it will be served. Settable.
    #[getter]
    pub fn media_type(&self) -> String {
        // Python sees the *resolved* media type: the override/derive distinction is a Rust-side
        // authoring concern, and a binding caller wants the value that will actually be served.
        self.inner.get_media_type()
    }
    #[setter]
    pub fn set_media_type(&mut self, media_type: String) {
        self.inner.with_media_type(media_type);
    }

    /// The asset's filename, or `None`. Settable.
    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.inner.filename.clone()
    }
    #[setter]
    pub fn set_filename(&mut self, filename: Option<String>) {
        self.inner.filename = filename;
    }

    /// An emoji that stands for the asset's kind of data. Settable.
    #[getter]
    pub fn unicode_icon(&self) -> String {
        self.inner.unicode_icon.clone()
    }
    #[setter]
    pub fn set_unicode_icon(&mut self, unicode_icon: String) {
        self.inner.unicode_icon = unicode_icon;
    }

    /// The size of the data in bytes, or `None`. Settable.
    #[getter]
    pub fn file_size(&self) -> Option<u64> {
        self.inner.file_size
    }
    #[setter]
    pub fn set_file_size(&mut self, file_size: Option<u64>) {
        self.inner.file_size = file_size;
    }

    /// When the backend wrote the data object (RFC 3339, UTC), if it reported one.
    #[getter]
    pub fn created(&self) -> Option<String> {
        self.inner.created.as_ref().map(|t| t.as_str().to_string())
    }

    /// The backend's ETag of the data object, if it reported one.
    #[getter]
    pub fn etag(&self) -> Option<String> {
        self.inner.etag.clone()
    }

    /// True when the asset is a directory. Settable.
    #[getter]
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir
    }
    #[setter]
    pub fn set_is_dir(&mut self, is_dir: bool) {
        self.inner.is_dir = is_dir;
    }

    /// For a directory, the `AssetInfo` of each entry. Settable.
    #[getter]
    pub fn children(&self) -> Vec<AssetInfo> {
        self.inner
            .children
            .iter()
            .map(|c| AssetInfo { inner: c.clone() })
            .collect()
    }

    #[setter]
    pub fn set_children(&mut self, children: Vec<AssetInfo>) {
        self.inner.children = children.into_iter().map(|c| c.inner).collect();
    }

    /// The most important fields as an `AssetInfo`.
    pub fn get_asset_info(&self) -> AssetInfo {
        AssetInfo {
            inner: self.inner.get_asset_info(),
        }
    }

    /// Sets the query; when the query names a store key, the key and filename are set too.
    pub fn with_query(&mut self, query: Query) {
        let _ = self.inner.with_query(query.0);
    }

    /// Sets the store key.
    pub fn with_key(&mut self, key: Key) {
        self.inner.with_key(key.0);
    }

    /// Sets the status.
    pub fn with_status(&mut self, status: Status) {
        self.inner.with_status(status.inner);
    }

    /// Sets the type identifier of the value.
    pub fn with_type_identifier(&mut self, type_identifier: String) {
        self.inner.with_type_identifier(type_identifier);
    }

    /// Sets the status message.
    pub fn with_message(&mut self, message: String) {
        self.inner.with_message(message);
    }

    /*
    pub fn with_error(&mut self, error: Error) {
        self.inner.with_error(error);
    }
    */

    /// Marks the asset as failed with `message`: sets the message, `is_error` and the `Error`
    /// status.
    pub fn with_error_message(&mut self, message: String) {
        self.inner.with_error_message(message);
    }

    /// Sets the media type of the data.
    pub fn with_media_type(&mut self, media_type: String) {
        self.inner.with_media_type(media_type);
    }

    /// Appends a `LogEntry` to the log.
    pub fn add_log_entry(&mut self, log_entry: LogEntry) {
        self.inner.add_log_entry(log_entry.inner);
    }

    /// Sets the filename.
    pub fn with_filename(&mut self, filename: String) {
        self.inner.with_filename(filename);
    }

    /// Removes every log entry.
    pub fn clean_log(&mut self) {
        self.inner.clean_log();
    }

    /// Appends an info entry with `message` to the log.
    pub fn info(&mut self, message: &str) {
        self.inner.info(message);
    }

    /// Appends a debug entry with `message` to the log.
    pub fn debug(&mut self, message: &str) {
        self.inner.debug(message);
    }

    /// Appends a warning entry with `message` to the log.
    pub fn warning(&mut self, message: &str) {
        self.inner.warning(message);
    }

    /// Appends an error entry with `message` to the log.
    pub fn error(&mut self, message: &str) {
        self.inner.error(message);
    }

    /// The extension of the asset's filename, or `None`.
    pub fn extension(&self) -> Option<String> {
        self.inner.extension()
    }

    /// Replaces the extension of the asset's filename, or appends one; nothing changes without a
    /// filename.
    pub fn set_extension(&mut self, extension: &str) {
        self.inner.set_extension(extension);
    }

    /// The emoji the filename extension suggests, used when no icon is set.
    pub fn default_unicode_icon(&self) -> String {
        self.inner.default_unicode_icon().to_string()
    }

    /// Raises the asset's error when the record describes a failed evaluation; returns `None`
    /// otherwise.
    // TODO: Rename error_result to something more descriptive - check?
    pub fn error_result(&self) -> PyResult<()> {
        self.inner
            .error_result()
            .map_err(|e| PyErr::from(crate::error::Error::from(e)))
    }

    /// Why the asset expired, as JSON text, or `None`. Meaningful only while the status is
    /// `Expired`.
    #[getter]
    pub fn expiry_reason(&self) -> Option<String> {
        self.inner
            .expiry_reason
            .as_ref()
            .map(|reason| serde_json::to_string(reason).unwrap_or_else(|_| "null".to_string()))
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }

    /// The record serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(serialization_pyerr)
    }

    /// Reads a record from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not a valid metadata record.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let m: CoreMetadataRecord = serde_json::from_str(json).map_err(serialization_pyerr)?;
        Ok(MetadataRecord { inner: m })
    }
}

/// A short summary of an asset: key, status, type, media type, size and a few more fields.
///
/// Directory listings use it for their entries (`MetadataRecord.children`). Every field is a
/// settable property; `AssetInfo()` is empty.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct AssetInfo {
    pub inner: CoreAssetInfo,
}

#[pymethods]
impl AssetInfo {
    #[new]
    pub fn new() -> Self {
        AssetInfo {
            inner: CoreAssetInfo::new(),
        }
    }

    /// The store key of the asset, or `None`. Settable.
    #[getter]
    pub fn key(&self) -> Option<Key> {
        self.inner.key.clone().map(Key)
    }
    #[setter]
    pub fn set_key(&mut self, key: Option<Key>) {
        self.inner.key = key.map(|k| k.0);
    }

    /// The asset's `Status`. Settable.
    #[getter]
    pub fn status(&self) -> Status {
        Status {
            inner: self.inner.status.clone(),
        }
    }
    #[setter]
    pub fn set_status(&mut self, status: Status) {
        self.inner.status = status.inner;
    }

    /// The type identifier of the value, such as `text`. Settable.
    #[getter]
    pub fn type_identifier(&self) -> String {
        self.inner.type_identifier.clone()
    }
    #[setter]
    pub fn set_type_identifier(&mut self, type_identifier: String) {
        self.inner.type_identifier = type_identifier;
    }

    /// The format the data is serialized in, or `None`. Settable.
    #[getter]
    pub fn data_format(&self) -> Option<String> {
        self.inner.data_format.clone()
    }
    #[setter]
    pub fn set_data_format(&mut self, data_format: Option<String>) {
        self.inner.data_format = data_format;
    }

    /// The current status message. Settable.
    #[getter]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }
    #[setter]
    pub fn set_message(&mut self, message: String) {
        self.inner.message = message;
    }

    /// True when the asset failed. Settable.
    #[getter]
    pub fn is_error(&self) -> bool {
        self.inner.is_error
    }
    #[setter]
    pub fn set_is_error(&mut self, is_error: bool) {
        self.inner.is_error = is_error;
    }

    /// The media type of the data, such as `text/plain`. Settable.
    #[getter]
    pub fn media_type(&self) -> String {
        // Python sees the *resolved* media type: the override/derive distinction is a Rust-side
        // `AssetInfo` carries the already-resolved media type, so there is nothing to resolve.
        self.inner.media_type.clone()
    }
    #[setter]
    pub fn set_media_type(&mut self, media_type: String) {
        self.inner.media_type = media_type;
    }

    /// The asset's filename, or `None`. Settable.
    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.inner.filename.clone()
    }
    #[setter]
    pub fn set_filename(&mut self, filename: Option<String>) {
        self.inner.filename = filename;
    }

    /// An emoji that stands for the asset's kind of data. Settable.
    #[getter]
    pub fn unicode_icon(&self) -> String {
        self.inner.unicode_icon.clone()
    }
    #[setter]
    pub fn set_unicode_icon(&mut self, unicode_icon: String) {
        self.inner.unicode_icon = unicode_icon;
    }

    /// The size of the data in bytes, or `None`. Settable.
    #[getter]
    pub fn file_size(&self) -> Option<u64> {
        self.inner.file_size
    }
    #[setter]
    pub fn set_file_size(&mut self, file_size: Option<u64>) {
        self.inner.file_size = file_size;
    }

    /// When the backend wrote the data object (RFC 3339, UTC), if it reported one.
    #[getter]
    pub fn created(&self) -> Option<String> {
        self.inner.created.as_ref().map(|t| t.as_str().to_string())
    }

    /// The backend's ETag of the data object, if it reported one.
    #[getter]
    pub fn etag(&self) -> Option<String> {
        self.inner.etag.clone()
    }

    /// True when the asset is a directory. Settable.
    #[getter]
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir
    }
    #[setter]
    pub fn set_is_dir(&mut self, is_dir: bool) {
        self.inner.is_dir = is_dir;
    }

    /// Why the asset expired, as JSON text, or `None`.
    #[getter]
    pub fn expiry_reason(&self) -> Option<String> {
        self.inner
            .expiry_reason
            .as_ref()
            .map(|reason| serde_json::to_string(reason).unwrap_or_else(|_| "null".to_string()))
    }

    /// The summary serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(serialization_pyerr)
    }

    /// Reads a summary from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not a valid `AssetInfo`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let m: CoreAssetInfo = serde_json::from_str(json).map_err(serialization_pyerr)?;
        Ok(AssetInfo { inner: m })
    }
}

/// The status of an asset, such as `Ready`, `Processing` or `Error`.
///
/// `Status(name)` takes the variant name and raises `ValueError` for an unknown one. A status
/// equals another of the same variant and also its name as a string (`status == "Ready"`).
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct Status {
    pub inner: CoreStatus,
}

#[pymethods]
impl Status {
    /// A status by its variant name (`"Ready"`); an unknown name raises `ValueError`.
    #[new]
    pub fn new(name: &str) -> PyResult<Self> {
        let inner = match name {
            "None" => CoreStatus::None,
            "Directory" => CoreStatus::Directory,
            "Recipe" => CoreStatus::Recipe,
            "Submitted" => CoreStatus::Submitted,
            "Dependencies" => CoreStatus::Dependencies,
            "Processing" => CoreStatus::Processing,
            "Partial" => CoreStatus::Partial,
            "Error" => CoreStatus::Error,
            "Storing" => CoreStatus::Storing,
            "Ready" => CoreStatus::Ready,
            "Expired" => CoreStatus::Expired,
            "Cancelled" => CoreStatus::Cancelled,
            "Source" => CoreStatus::Source,
            "Override" => CoreStatus::Override,
            "Volatile" => CoreStatus::Volatile,
            other => return Err(PyValueError::new_err(format!("unknown status {other:?}"))),
        };
        Ok(Status { inner })
    }

    /// Equal to another `Status` of the same variant, or to its name as a string.
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other) = other.extract::<Status>() {
            return other.inner == self.inner;
        }
        match other.extract::<String>() {
            Ok(name) => name == self.name(),
            Err(_) => false,
        }
    }

    /// The hash of the status name, so that a status and its name string — which compare
    /// equal — hash alike.
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        pyo3::types::PyString::new(py, &self.name()).hash()
    }

    fn __repr__(&self) -> String {
        format!("Status({:?})", self.name())
    }

    fn __str__(&self) -> String {
        self.name()
    }

    /// The variant name, such as `"Ready"`.
    pub fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    /// True when data is associated with the status: `Ready`, `Source`, `Partial`, `Expired`,
    /// `Override` or `Volatile`.
    pub fn has_data(&self) -> bool {
        self.inner.has_data()
    }

    /// True for the statuses whose dependencies are tracked: `Ready`, `Partial` and `Storing`.
    pub fn can_have_tracked_dependencies(&self) -> bool {
        self.inner.can_have_tracked_dependencies()
    }

    /// True when the evaluation has ended, with a usable value or with an error.
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

/// The level of a log entry: `Debug`, `Info`, `Warning` or `Error`.
///
/// `LogEntryKind(name)` takes the lower-case name (`"info"`) and raises `ValueError` for any other.
#[pyclass(from_py_object, eq, eq_int, module = "liquers")]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LogEntryKind {
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}

impl From<CoreLogEntryKind> for LogEntryKind {
    fn from(kind: CoreLogEntryKind) -> Self {
        match kind {
            CoreLogEntryKind::Debug => LogEntryKind::Debug,
            CoreLogEntryKind::Info => LogEntryKind::Info,
            CoreLogEntryKind::Warning => LogEntryKind::Warning,
            CoreLogEntryKind::Error => LogEntryKind::Error,
        }
    }
}

impl From<LogEntryKind> for CoreLogEntryKind {
    fn from(kind: LogEntryKind) -> Self {
        match kind {
            LogEntryKind::Debug => CoreLogEntryKind::Debug,
            LogEntryKind::Info => CoreLogEntryKind::Info,
            LogEntryKind::Warning => CoreLogEntryKind::Warning,
            LogEntryKind::Error => CoreLogEntryKind::Error,
        }
    }
}

#[pymethods]
impl LogEntryKind {
    #[new]
    pub fn new(name: &str) -> PyResult<Self> {
        match name {
            "debug" => Ok(LogEntryKind::Debug),
            "info" => Ok(LogEntryKind::Info),
            "warning" => Ok(LogEntryKind::Warning),
            "error" => Ok(LogEntryKind::Error),
            _ => Err(PyErr::new::<PyValueError, _>("Invalid log entry kind")),
        }
    }

    /// The level's name, such as `"Info"`.
    pub fn name(&self) -> String {
        format!("{:?}", self)
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self)
    }

    pub fn __repr__(&self) -> String {
        format!("'{:?}'", self)
    }
}

/// One entry of an asset's log: a level, a message, a timestamp, and where it came from.
///
/// `LogEntry(kind, message)` makes one with the current time. Every field is a settable property.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub inner: CoreLogEntry,
}

#[pymethods]
impl LogEntry {
    #[new]
    pub fn new(kind: LogEntryKind, message: String) -> Self {
        LogEntry {
            inner: CoreLogEntry::new(kind.into(), message),
        }
    }

    /// The entry's level, a `LogEntryKind`. Settable.
    #[getter]
    pub fn kind(&self) -> LogEntryKind {
        LogEntryKind::from(self.inner.kind.clone())
    }

    #[setter]
    pub fn set_kind(&mut self, kind: LogEntryKind) {
        self.inner.kind = kind.into();
    }

    /// The message text. Settable.
    #[getter]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }
    #[setter]
    pub fn set_message(&mut self, message: String) {
        self.inner.message = message;
    }

    /// The message as HTML, or `None`. Settable.
    #[getter]
    pub fn message_html(&self) -> Option<String> {
        self.inner.message_html.clone()
    }
    #[setter]
    pub fn set_message_html(&mut self, message_html: Option<String>) {
        self.inner.message_html = message_html;
    }

    /// When the entry was made, as text. Settable.
    #[getter]
    pub fn timestamp(&self) -> String {
        self.inner.timestamp.clone()
    }
    #[setter]
    pub fn set_timestamp(&mut self, timestamp: String) {
        self.inner.timestamp = timestamp;
    }

    /// The query being evaluated when the entry was made, or `None`. Settable.
    #[getter]
    pub fn query(&self) -> Option<Query> {
        self.inner.query.clone().map(Query)
    }
    #[setter]
    pub fn set_query(&mut self, query: Option<Query>) {
        self.inner.query = query.map(|q| q.0);
    }

    /// The position in the query the entry is about. Settable.
    #[getter]
    pub fn position(&self) -> Position {
        Position(self.inner.position.clone())
    }
    #[setter]
    pub fn set_position(&mut self, position: Position) {
        self.inner.position = position.0;
    }

    /// The traceback of an error entry, or `None`. Settable.
    #[getter]
    pub fn traceback(&self) -> Option<String> {
        self.inner.traceback.clone()
    }
    #[setter]
    pub fn set_traceback(&mut self, traceback: Option<String>) {
        self.inner.traceback = traceback;
    }

    /// The entry serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner).map_err(serialization_pyerr)
    }

    /// Reads an entry from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not a valid `LogEntry`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let m: CoreLogEntry = serde_json::from_str(json).map_err(serialization_pyerr)?;
        Ok(LogEntry { inner: m })
    }

    pub fn __str__(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}
