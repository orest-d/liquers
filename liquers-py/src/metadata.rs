use pyo3::prelude::*;
use pyo3::types::PyList;
use liquers_core::metadata::{MetadataRecord as CoreMetadataRecord, Status, LogEntry, AssetInfo};
use liquers_core::query::{Key, Query};
use liquers_core::error::Error;

#[pyclass]
pub struct Metadata(pub liquers_core::metadata::Metadata);

#[pymethods]
impl Metadata {
    #[new]
    pub fn new() -> Self {
        Metadata(liquers_core::metadata::Metadata::new())
    }
}

#[pyclass]
pub struct MetadataRecord{
    pub inner: CoreMetadataRecord,
}

// TODO: Create AssetInfo
// TODO: Create LogEntry
// TODO: implement MetadataRecord methods correctly
// TODO: implement Metadata methods
// TODO: implement Status
// TODO: implement Error

#[pymethods]
impl MetadataRecord {
    #[new]
    pub fn new() -> Self {
        MetadataRecord {
            inner: CoreMetadataRecord::new(),
        }
    }

    /*
    #[getter]
    pub fn log(&self) -> Vec<LogEntry> {
        self.inner.log.clone()
    }
    */
    /*
    #[setter]
    pub fn set_log(&mut self, log: Vec<LogEntry>) {
        self.inner.log = log;
    }
    */

    #[getter]
    pub fn query(&self) -> crate::parse::Query {
        crate::parse::Query(self.inner.query.clone())
    }
    /*
    #[setter]
    pub fn set_query(&mut self, query: Query) {
        self.inner.query = query;
    }
    */

    #[getter]
    pub fn key(&self) -> Option<crate::parse::Key> {
        if let Some(key) = &self.inner.key {
            Some(crate::parse::Key(key.clone()))
        } else {
            None
        }
    }
    /*
    #[setter]
    pub fn set_key(&mut self, key: Option<Key>) {
        self.inner.key = key;
    }
    */

    // TODO: Implement Status as an enum wrapper
    #[getter]
    pub fn status(&self) -> String {
        format!("{:?}", self.inner.status)
    }
    /*
    #[setter]
    pub fn set_status(&mut self, status: Status) {
        self.inner.status = status;
    }
    */

    #[getter]
    pub fn type_identifier(&self) -> String {
        self.inner.type_identifier.clone()
    }
    #[setter]
    pub fn set_type_identifier(&mut self, type_identifier: String) {
        self.inner.type_identifier = type_identifier;
    }

    #[getter]
    pub fn data_format(&self) -> Option<String> {
        self.inner.data_format.clone()
    }
    #[setter]
    pub fn set_data_format(&mut self, data_format: Option<String>) {
        self.inner.data_format = data_format;
    }

    #[getter]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }
    #[setter]
    pub fn set_message(&mut self, message: String) {
        self.inner.message = message;
    }

    #[getter]
    pub fn is_error(&self) -> bool {
        self.inner.is_error
    }
    #[setter]
    pub fn set_is_error(&mut self, is_error: bool) {
        self.inner.is_error = is_error;
    }

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

    #[getter]
    pub fn media_type(&self) -> String {
        self.inner.media_type.clone()
    }
    #[setter]
    pub fn set_media_type(&mut self, media_type: String) {
        self.inner.media_type = media_type;
    }

    #[getter]
    pub fn filename(&self) -> Option<String> {
        self.inner.filename.clone()
    }
    #[setter]
    pub fn set_filename(&mut self, filename: Option<String>) {
        self.inner.filename = filename;
    }

    #[getter]
    pub fn unicode_icon(&self) -> String {
        self.inner.unicode_icon.clone()
    }
    #[setter]
    pub fn set_unicode_icon(&mut self, unicode_icon: String) {
        self.inner.unicode_icon = unicode_icon;
    }

    #[getter]
    pub fn file_size(&self) -> Option<u64> {
        self.inner.file_size
    }
    #[setter]
    pub fn set_file_size(&mut self, file_size: Option<u64>) {
        self.inner.file_size = file_size;
    }

    #[getter]
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir
    }
    #[setter]
    pub fn set_is_dir(&mut self, is_dir: bool) {
        self.inner.is_dir = is_dir;
    }

    /*
    #[getter]
    pub fn children(&self) -> Vec<AssetInfo> {
        self.inner.children.clone()
    }
    */

    /*
    #[setter]
    pub fn set_children(&mut self, children: Vec<AssetInfo>) {
        self.inner.children = children;
    }
    */

    /*
    pub fn get_asset_info(&self) -> AssetInfo {
        self.inner.get_asset_info()
    }
    */

    /*
    pub fn with_query(&mut self, query: Query) {
        self.inner.with_query(query);
    }
    */

    /*
    pub fn with_key(&mut self, key: Key) {
        self.inner.with_key(key);
    }
    */

    /*
    pub fn with_status(&mut self, status: Status) {
        self.inner.with_status(status);
    }
    */

    pub fn with_type_identifier(&mut self, type_identifier: String) {
        self.inner.with_type_identifier(type_identifier);
    }

    pub fn with_message(&mut self, message: String) {
        self.inner.with_message(message);
    }

    /*
    pub fn with_error(&mut self, error: Error) {
        self.inner.with_error(error);
    }
    */

    pub fn with_error_message(&mut self, message: String) {
        self.inner.with_error_message(message);
    }

    pub fn with_media_type(&mut self, media_type: String) {
        self.inner.with_media_type(media_type);
    }

    /*
    pub fn add_log_entry(&mut self, log_entry: LogEntry) {
        self.inner.add_log_entry(log_entry);
    }
    */

    pub fn with_filename(&mut self, filename: String) {
        self.inner.with_filename(filename);
    }

    pub fn clean_log(&mut self) {
        self.inner.clean_log();
    }

    pub fn info(&mut self, message: &str) {
        self.inner.info(message);
    }

    pub fn debug(&mut self, message: &str) {
        self.inner.debug(message);
    }

    pub fn warning(&mut self, message: &str) {
        self.inner.warning(message);
    }

    pub fn error(&mut self, message: &str) {
        self.inner.error(message);
    }

    pub fn extension(&self) -> Option<String> {
        self.inner.extension()
    }

    pub fn set_extension(&mut self, extension: &str) {
        self.inner.set_extension(extension);
    }

    pub fn default_unicode_icon(&self) -> String {
        self.inner.default_unicode_icon().to_string()
    }

    pub fn error_result(&self) -> PyResult<()> {
        self.inner.error_result().map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }
}

