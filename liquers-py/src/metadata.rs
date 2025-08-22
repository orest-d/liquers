use liquers_core::error::Error;
use liquers_core::metadata::{
    AssetInfo as CoreAssetInfo, LogEntry as CoreLogEntry, LogEntryKind as CoreLogEntryKind,
    MetadataRecord as CoreMetadataRecord, Status as CoreStatus,
};
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::parse::{Key, Position, Query};

// implement to_json and from_json in MetadataRecord
// implement to_json and from_json in AssetInfo

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

    #[getter]
    pub fn log(&self) -> Vec<LogEntry> {
        self.inner.log.iter()
            .map(|entry| LogEntry { inner: entry.clone() })
            .collect()
    }

    #[setter]
    pub fn set_log(&mut self, log: Vec<LogEntry>) {
        self.inner.log = log.into_iter()
            .map(|entry| entry.inner)
            .collect();
    }

    #[getter]
    pub fn query(&self) -> Query {
        Query(self.inner.query.clone())
    }
    
    #[setter]
    pub fn set_query(&mut self, query: Query) {
        self.inner.query = query.0;
    }
    
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

    pub fn get_asset_info(&self) -> AssetInfo {
        AssetInfo {
            inner: self.inner.get_asset_info(),
        }
    }

    pub fn with_query(&mut self, query: Query) {
        self.inner.with_query(query.0);
    }

    pub fn with_key(&mut self, key: Key) {
        self.inner.with_key(key.0);
    }

    
    pub fn with_status(&mut self, status: Status) {
        self.inner.with_status(status.inner);
    }
    
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

    pub fn add_log_entry(&mut self, log_entry: LogEntry) {
        self.inner.add_log_entry(log_entry.inner);
    }

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

    // TODO: Rename error_result to something more descriptive - check?
    pub fn error_result(&self) -> PyResult<()> {
        self.inner
            .error_result()
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }
}

#[pyclass]
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

    #[getter]
    pub fn key(&self) -> Option<Key> {
        self.inner.key.clone().map(Key)
    }
    #[setter]
    pub fn set_key(&mut self, key: Option<Key>) {
        self.inner.key = key.map(|k| k.0);
    }

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
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct Status {
    pub inner: CoreStatus,
}

#[pymethods]
impl Status {
    #[new]
    pub fn new(name: &str) -> Self {
        let inner = match name {
            "None" => CoreStatus::None,
            "Submitted" => CoreStatus::Submitted,
            "Processing" => CoreStatus::Processing,
            "Partial" => CoreStatus::Partial,
            "Error" => CoreStatus::Error,
            "Recipe" => CoreStatus::Recipe,
            "Ready" => CoreStatus::Ready,
            "Expired" => CoreStatus::Expired,
            "Source" => CoreStatus::Source,
            _ => CoreStatus::None,
        };
        Status { inner }
    }

    pub fn name(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn has_data(&self) -> bool {
        self.inner.has_data()
    }

    pub fn can_have_tracked_dependencies(&self) -> bool {
        self.inner.can_have_tracked_dependencies()
    }

    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

// TODO: Convert LogEntryKind to python/pyo3 enum
#[pyclass]
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
    pub fn new(name: &str) -> Self {
        match name {
            "debug" => LogEntryKind::Debug,
            "info" => LogEntryKind::Info,
            "warning" => LogEntryKind::Warning,
            "error" => LogEntryKind::Error,
            _ => LogEntryKind::Info, // TODO: Raise an error
        }
    }

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

#[pyclass]
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

    #[getter]
    pub fn kind(&self) -> LogEntryKind {
        LogEntryKind::from(self.inner.kind.clone())
    }

    #[setter]
    pub fn set_kind(&mut self, kind: LogEntryKind) {
        self.inner.kind = kind.into();
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
    pub fn message_html(&self) -> Option<String> {
        self.inner.message_html.clone()
    }
    #[setter]
    pub fn set_message_html(&mut self, message_html: Option<String>) {
        self.inner.message_html = message_html;
    }

    #[getter]
    pub fn timestamp(&self) -> String {
        self.inner.timestamp.clone()
    }
    #[setter]
    pub fn set_timestamp(&mut self, timestamp: String) {
        self.inner.timestamp = timestamp;
    }

    #[getter]
    pub fn query(&self) -> Option<Query> {
        self.inner.query.clone().map(Query)
    }
    #[setter]
    pub fn set_query(&mut self, query: Option<Query>) {
        self.inner.query = query.map(|q| q.0);
    }

    #[getter]
    pub fn position(&self) -> Position {
        Position(self.inner.position.clone())
    }
    #[setter]
    pub fn set_position(&mut self, position: Position) {
        self.inner.position = position.0;
    }

    #[getter]
    pub fn traceback(&self) -> Option<String> {
        self.inner.traceback.clone()
    }
    #[setter]
    pub fn set_traceback(&mut self, traceback: Option<String>) {
        self.inner.traceback = traceback;
    }
}
