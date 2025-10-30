#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json::{self, Value};

use crate::error::Error;
use crate::icons::DEFAULT_ICON;
use crate::parse;
use crate::query::{Key, Position, Query};

/// Status of the asset
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Status {
    /// Status does not exist or is not available. May be used as an initial value.
    None,
    /// Directory can only have a "Directory" status.
    Directory,
    /// Asset is not ready, but it has a recipe that can be used to create it.
    Recipe,
    /// Asset has been submitted for processing.
    Submitted,
    /// Asset is waiting for its dependencies to become ready.
    Dependencies,
    /// Asset is currently being processed.
    Processing,
    /// Asset is still processing but it published partial results.
    Partial,
    /// Asset finished with an error.
    Error,
    /// Asset is being stored. It is not yet ready to be used.
    /// This is automatically maintained by the store when the asset is being stored.
    /// AssetRef should not be in this state.
    /// If asset loads from store with status Storing, the loading is considered as failed.
    Storing,
    /// Asset is fully calculated and ready to be used.
    Ready,
    /// Asset is no longer valid and should not be used.
    Expired,
    /// Asset processing was cancelled.
    Cancelled,
    /// Asset is the source of the data. It is ready, and has neither dependencies nor a recipe.
    Source,
}

impl Default for Status {
    fn default() -> Self {
        Self::None
    }
}

impl Status {
    /// Returns true if some data is associated with the status
    /// For Ready and Source it is a fully valid data,
    /// otherwise it may be Partial or Expired.
    pub fn has_data(&self) -> bool {
        match self {
            Status::Ready => true,
            Status::None => false,
            Status::Submitted => false,
            Status::Processing => false,
            Status::Partial => true,
            Status::Error => false,
            Status::Recipe => false,
            Status::Expired => true,
            Status::Source => true,
            Status::Cancelled => false,
            Status::Storing => false,
            Status::Dependencies => false,
            Status::Directory => false,
        }
    }
    pub fn can_have_tracked_dependencies(&self) -> bool {
        match self {
            Status::Ready => true,
            Status::None => false,
            Status::Submitted => false,
            Status::Processing => false,
            Status::Partial => true,
            Status::Error => false,
            Status::Recipe => false,
            Status::Expired => false,
            Status::Source => false,
            Status::Cancelled => false,
            Status::Storing => true,
            Status::Dependencies => false,
            Status::Directory => false,
        }
    }
    /// Returns true if the calculation of the asset is finished
    /// and the asset is either valid and ready to be used or ended up with an error.
    pub fn is_finished(&self) -> bool {
        match self {
            Status::Ready => true,
            Status::None => false,
            Status::Submitted => false,
            Status::Processing => false,
            Status::Partial => false,
            Status::Error => true,
            Status::Recipe => false,
            Status::Expired => true,
            Status::Source => true,
            Status::Cancelled => true,
            Status::Storing => false,
            Status::Dependencies => false,
            Status::Directory => true,
        }
    }

    /// Returns true if the asset is being evaluated
    /// Asset is processing when it is in [Processing](Status::Processing) state
    /// or in [Partial](Status::Partial) state.
    /// Asset is not considered to be processing if it is waiting for  [dependencies](Status::Dependencies)
    /// or waiting in the queue ([Submitted](Status::Submitted)).
    pub fn is_processing(&self) -> bool {
        match self {
            Status::Ready => false,
            Status::None => false,
            Status::Submitted => false,
            Status::Processing => true,
            Status::Partial => true,
            Status::Error => false,
            Status::Recipe => false,
            Status::Expired => false,
            Status::Source => false,
            Status::Cancelled => false,
            Status::Storing => false,
            Status::Dependencies => false,
            Status::Directory => false,
        }
    }

    /// Status is None
    pub(crate) fn is_none(&self) -> bool {
        *self == Status::None
    }
}

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub kind: LogEntryKind,
    pub message: String,
    #[serde(default)]
    pub message_html: Option<String>,
    pub timestamp: String,
    #[serde(with = "option_query_format", default)]
    pub query: Option<Query>,
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub traceback: Option<String>,
}

impl LogEntry {
    pub fn new(kind: LogEntryKind, message: String) -> LogEntry {
        LogEntry {
            kind,
            message,
            ..Self::default()
        }
        .with_timestamp()
    }

    pub fn from_error(error: &Error) -> LogEntry {
        let mut log_entry = LogEntry::error(error.to_string());
        log_entry = log_entry.with_position(error.position.clone());

        if let Some(query) = error.query.as_ref() {
            if let Ok(query) = parse::parse_query(query) {
                log_entry = log_entry.with_query(query);
            } else {
                log_entry.message = format!("{} (unparseable query: {})", log_entry.message, query);
            }
        }
        // TODO: Set/support traceback somehow
        //if let Some(e) = error.source(){
        //    log_entry = log_entry.with_traceback(e.to_string());
        //}
        log_entry
    }
    pub fn info(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Info, message)
    }
    pub fn debug(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Debug, message)
    }
    pub fn warning(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Warning, message)
    }
    pub fn error(message: String) -> LogEntry {
        LogEntry::new(LogEntryKind::Error, message)
    }
    pub fn with_query(mut self, query: Query) -> Self {
        self.query = Some(query);
        self
    }
    pub fn with_position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }
    pub fn with_traceback(mut self, traceback: String) -> Self {
        self.traceback = Some(traceback);
        self
    }
    pub fn with_message_html(mut self, message_html: String) -> Self {
        self.message_html = Some(message_html);
        self
    }
    pub fn with_custom_timestamp(mut self, timestamp: String) -> Self {
        self.timestamp = timestamp;
        self
    }
    pub fn with_timestamp(mut self) -> Self {
        self.timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self
    }
}

impl Default for LogEntry {
    fn default() -> Self {
        LogEntry {
            kind: LogEntryKind::Info,
            message: "".to_string(),
            message_html: None,
            timestamp: "".to_string(),
            query: None,
            position: Position::default(),
            traceback: None,
        }
    }
}

/// Structure to capture progress of asset creation
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProgressEntry {
    pub message: String,
    pub done: u32,
    pub total: u32,
    pub timestamp: String,
    pub eta: Option<String>,
}

impl ProgressEntry {
    /// Create a new ProgressEntry with the given message, done and total values.
    pub fn new(message: String, done: u32, total: u32) -> ProgressEntry {
        ProgressEntry {
            message,
            done,
            total,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            eta: None,
        }
    }
    /// Create a ProgressEntry indicating no progress (off).
    pub fn off() -> ProgressEntry {
        ProgressEntry::new("".to_string(), 0, 0)
    }
    /// Create a ProgressEntry indicating a tick - i.e. progress step with unknown total.
    pub fn tick(message: String) -> ProgressEntry {
        ProgressEntry::new(message, 1, 0)
    }
    /// Create a ProgressEntry indicating that the progress is done.
    pub fn done(message: String) -> ProgressEntry {
        ProgressEntry::new(message, 1, 1)
    }
    /// Set a custom message.
    pub fn with_message(mut self, message: String) -> Self {
        self.message = message;
        self
    }
    /// Set an estimated time of arrival (ETA).
    pub fn with_eta(mut self, eta: String) -> Self {
        self.eta = Some(eta);
        self
    }
    /// Check if the progress is off
    pub fn is_off(&self) -> bool {
        (self.total == 0) && (self.done == 0)
    }
    /// Check if the progress is done
    pub fn is_done(&self) -> bool {
        (self.total > 0) && (self.done == self.total)
    }
    /// Check if the progress is a tick (progress is an activity indicator with unknown total)
    pub fn is_tick(&self) -> bool {
        (self.total == 0) && (self.done > 0)
    }
    pub fn set(&mut self, progress: &ProgressEntry) {
        self.message = progress.message.clone();
        if self.is_tick() && progress.is_tick() {
            self.done += 1;
            return;
        }
        self.done = progress.done;
        self.total = progress.total;
        self.timestamp = progress.timestamp.clone();
        self.eta = progress.eta.clone();
    }
}

impl Default for ProgressEntry {
    fn default() -> Self {
        ProgressEntry::off()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]

/// Structure containing the most important information about the asset
/// It is can be used as a shorter version of the metadata
pub struct AssetInfo {
    /// If value is a result of a query
    /// If a key is available, this is a query representation of a key
    #[serde(with = "option_query_format")]
    pub query: Option<Query>,
    /// If value is an asset (e.g. a file in a store), the key is key of the asset
    #[serde(with = "option_key_format")]
    pub key: Option<Key>,
    /// Status of the value
    pub status: Status,
    /// Type identifier of the value
    pub type_identifier: String,
    /// Data format of the value - format how the data was serialized.
    /// Whenever possible, this is a filename extension. It may be different from the file extension though,
    /// e.g. if the file extension is ambiguous.
    /// Method get_data_format() returns the data format, using extension as a default.
    pub data_format: Option<String>,
    /// Last message from the log
    pub message: String,
    /// Title of the asset
    pub title: String,
    /// Description of the asset
    pub description: String,  
    /// Indicates that the value failed to be created
    pub is_error: bool,
    /// Media type of the value
    pub media_type: String,
    /// Filename of the value
    pub filename: Option<String>,
    /// Unicode icon representing the file type as an emoji
    pub unicode_icon: String,
    /// File size in bytes
    pub file_size: Option<u64>,
    /// Is directory
    pub is_dir: bool,
    /// Progress
    pub progress: ProgressEntry,
    /// Time of the last update
    pub updated:String,
}

impl AssetInfo {
    pub fn new() -> AssetInfo {
        AssetInfo {
            is_error: false,
            ..Self::default()
        }
    }
    /// Sets the key.
    /// Note that a query and filename (if available in the key) is also set.
    pub fn with_key(&mut self, key: Key) -> &mut Self {
        self.query = Some((&key).into());
        self.key = Some(key);
        if let Some(filename) = self.key.as_ref().unwrap().filename() {
            self.with_filename(filename.name.clone());
        }
        self
    }

    /// Sets the query.
    /// Note that if query is a key, a key and filename (if available in the query) is also set.
    pub fn with_query(&mut self, query: Query) -> &mut Self {
        if query.is_key() {
            if let Some(key) = query.key() {
                self.key = Some(key);
                if let Some(filename) = self.key.as_ref().unwrap().filename() {
                    self.with_filename(filename.name.clone());
                }
            }
        }
        self.query = Some(query);
        self
    }

    /// Sets the filename.
    fn with_filename(&mut self, filename: String) -> &mut Self {
        self.filename = Some(filename);
        self.media_type = crate::media_type::file_extension_to_media_type(
            self.extension().unwrap_or("".to_string()).as_str(),
        )
        .to_owned();
        if self.unicode_icon.is_empty() {
            self.unicode_icon = DEFAULT_ICON.to_string();
        }
        self.data_format = self.extension();
        self
    }

    pub fn extension(&self) -> Option<String> {
        if let Some(filename) = &self.filename {
            let parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                return Some(parts.last().unwrap().to_string());
            }
        }
        None
    }
}

impl From<AssetInfo> for MetadataRecord {
    fn from(asset_info: AssetInfo) -> Self {
        let mut metadata = MetadataRecord::new();
        metadata.query = asset_info.query.unwrap_or(Query::new());
        metadata.key = asset_info.key;
        metadata.status = asset_info.status;
        metadata.type_identifier = asset_info.type_identifier;
        metadata.data_format = asset_info.data_format;
        metadata.message = asset_info.message;
        metadata.title = asset_info.title;
        metadata.description = asset_info.description;
        metadata.is_error = asset_info.is_error;
        metadata.media_type = asset_info.media_type;
        metadata.filename = asset_info.filename;
        metadata.unicode_icon = asset_info.unicode_icon;
        metadata.file_size = asset_info.file_size;
        metadata.is_dir = asset_info.is_dir;
        metadata.progress = vec![asset_info.progress];
        metadata.updated = asset_info.updated;
        metadata
    }
}

impl From<AssetInfo> for Metadata {
    fn from(asset_info: AssetInfo) -> Self {
        let m: MetadataRecord = asset_info.into();
        m.into()
    }
}

impl From<MetadataRecord> for AssetInfo {
    fn from(metadata: MetadataRecord) -> Self {
        metadata.get_asset_info()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct MetadataRecord {
    /// Log data
    pub log: Vec<LogEntry>,
    /// Query constructing the value with which the metadata is associated with
    #[serde(with = "query_format")]
    pub query: Query,
    /// If value is an asset (e.g. a file in a store), the key is key of the asset
    #[serde(with = "option_key_format")]
    pub key: Option<Key>,
    /// Status of the value
    pub status: Status,
    /// Type identifier of the value
    pub type_identifier: String,
    /// Data format of the value - format how the data was serialized.
    /// Whenever possible, this is a filename extension. It may be different from the file extension though,
    /// e.g. if the file extension is ambiguous.
    /// Method get_data_format() returns the data format, using extension as a default.
    pub data_format: Option<String>,
    /// Last message from the log
    pub message: String,
    /// Title of the asset
    pub title: String,
    /// Description of the asset
    pub description: String,   
    /// Indicates that the value failed to be created
    pub is_error: bool,
    /// Structure containing the error information
    pub error_data: Option<Error>,
    /// Media type of the value
    pub media_type: String,
    /// Filename of the value
    pub filename: Option<String>,
    /// Unicode icon representing the file type as an emoji
    pub unicode_icon: String,
    /// File size in bytes
    pub file_size: Option<u64>,
    /// Is directory
    pub is_dir: bool,
    /// Progress
    pub progress: Vec<ProgressEntry>,
    /// Time of the last update
    pub updated: String,
    /// Children are populated if the value is a directory
    #[serde(default)]
    pub children: Vec<AssetInfo>,
}

mod query_format {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(query: &Query, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&query.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Query, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse::parse_query(&s).map_err(de::Error::custom)
    }
}

mod key_format {
    use crate::query::Key;

    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &Key, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&key.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Key, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse::parse_key(&s).map_err(de::Error::custom)
    }
}

mod option_query_format {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(query: &Option<Query>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match query {
            Some(q) => serializer.serialize_str(&q.encode()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Query>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer);
        if s.is_err() {
            Ok(None)
        } else {
            let s = s.unwrap();
            if s.is_empty() {
                Ok(Some(Query::new()))
            } else {
                parse::parse_query(&s).map_err(de::Error::custom).map(Some)
            }
        }
    }
}

mod option_key_format {
    use crate::query::Key;

    use super::*;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(key: &Option<Key>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match key {
            Some(k) => serializer.serialize_str(&k.encode()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Key>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer);
        if s.is_err() {
            Ok(None)
        } else {
            let s = s.unwrap();
            if s.is_empty() {
                Ok(Some(Key::new()))
            } else {
                parse::parse_key(&s).map_err(de::Error::custom).map(Some)
            }
        }
    }
}

impl MetadataRecord {
    /// Create a new empty MetadataRecord with default values
    pub fn new() -> MetadataRecord {
        let mut metadata = MetadataRecord {
            is_error: false,
            ..Self::default()
        };
        metadata.set_updated_now();
        metadata
    }

    pub fn from_error(error: Error) -> MetadataRecord {
        let mut metadata = MetadataRecord::new();
        metadata.with_error(error);
        metadata.set_updated_now();
        metadata
    }

    /// Get most important features in form of an AssetInfo
    pub fn get_asset_info(&self) -> AssetInfo {
        AssetInfo {
            query: Some(self.query.clone()),
            key: self.key.clone(),
            status: self.status,
            type_identifier: self.type_identifier.clone(),
            data_format: self.data_format.clone(),
            message: self.message.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            is_error: self.is_error,
            media_type: self.media_type.clone(),
            filename: self.filename.clone(),
            unicode_icon: self.unicode_icon.clone(),
            file_size: self.file_size,
            is_dir: self.is_dir,
            progress: if self.progress.is_empty() {
                ProgressEntry::off()
            } else {
                self.progress[0].clone()
            },
            updated: self.updated.clone(),
        }
    }

    /// Set the query of the MetadataRecord
    pub fn with_query(&mut self, query: Query) -> &mut Self {
        self.query = query;
        if let Some(filename) = self.query.filename().as_ref() {
            self.with_filename(filename.name.clone());
        }
        self
    }
    /*
    pub fn from_query(query: &str) -> Result<Self, Error> {
        let mut metadata = self::MetadataRecord::new();
        metadata.query = query.to_string();
        Ok(metadata)
    }
    */
    pub fn with_key(&mut self, key: Key) -> &mut Self {
        self.key = Some(key);
        if let Some(filename) = self.key.as_ref().unwrap().filename() {
            self.with_filename(filename.name.clone());
        }
        self
    }
    pub fn with_status(&mut self, status: Status) -> &mut Self {
        self.status = status;
        self.is_error = status == Status::Error;
        self.set_updated_now();
        self
    }
    pub fn with_type_identifier(&mut self, type_identifier: String) -> &mut Self {
        self.type_identifier = type_identifier;
        self.set_updated_now();
        self
    }
    pub fn with_message(&mut self, message: String) -> &mut Self {
        self.message = message;
        self.set_updated_now();
        self
    }
    pub fn with_title(&mut self, title: String) -> &mut Self {
        self.title = title;
        self.set_updated_now();
        self
    }
    pub fn with_description(&mut self, description: String) -> &mut Self {
        self.description = description;
        self.set_updated_now();
        self
    }

    pub fn with_error(&mut self, error: Error) -> &mut Self {
        self.error(&error.to_string());
        self.is_error = true;
        self.error_data = Some(error);
        self.set_updated_now();
        self
    }

    pub fn with_error_message(&mut self, message: String) -> &mut Self {
        self.is_error = true;
        self.message = message;
        self.status = Status::Error;
        self.set_updated_now();
        self
    }

    pub fn with_media_type(&mut self, media_type: String) -> &mut Self {
        self.media_type = media_type;
        self.set_updated_now();
        self
    }
    pub fn add_log_entry(&mut self, log_entry: LogEntry) -> &mut Self {
        if log_entry.kind == LogEntryKind::Error {
            self.is_error = true;
            self.status = Status::Error;
        }
        self.message = log_entry.message.clone();
        self.log.push(log_entry);
        self.set_updated_now();
        self
    }
    pub fn with_filename(&mut self, filename: String) -> &mut Self {
        self.filename = Some(filename);
        self.media_type = crate::media_type::file_extension_to_media_type(
            self.extension().unwrap_or("".to_string()).as_str(),
        )
        .to_owned();
        if self.unicode_icon.is_empty() {
            self.unicode_icon = self.default_unicode_icon().to_string();
        }
        self.set_updated_now();
        self
    }
    pub fn clean_log(&mut self) -> &mut Self {
        self.log = vec![];
        self
    }
    pub fn info(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::info(message.to_owned()));
        self
    }
    pub fn debug(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::debug(message.to_owned()));
        self
    }
    pub fn warning(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::warning(message.to_owned()));
        self
    }
    pub fn error(&mut self, message: &str) -> &mut Self {
        self.add_log_entry(LogEntry::error(message.to_owned()));
        self.with_status(Status::Error);
        self
    }
    pub fn type_identifier(&self) -> String {
        self.type_identifier.to_string()
    }
    pub fn filename(&self) -> Option<String> {
        self.filename.clone()
    }
    pub fn set_filename(&mut self, filename: &str) {
        self.filename = Some(filename.to_string());
    }
    pub fn extension(&self) -> Option<String> {
        if let Some(filename) = &self.filename {
            let parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                return Some(parts.last().unwrap().to_string());
            }
        }
        None
    }
    pub fn set_extension(&mut self, extension: &str) {
        if let Some(filename) = &mut self.filename {
            let mut parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                parts.pop();
                parts.push(extension);
                *filename = parts.join(".");
            } else {
                filename.push('.');
                filename.push_str(extension);
            }
        } else {
            self.filename = Some(format!("file.{}", extension));
        }
    }
    pub fn get_media_type(&self) -> String {
        if self.media_type.is_empty() {
            if let Some(extension) = self.extension() {
                return crate::media_type::file_extension_to_media_type(extension.as_str())
                    .to_owned();
            }
            return "application/octet-stream".to_string();
        }
        self.media_type.to_string()
    }

    /// Return data format
    /// If data_format is not set, return extension.
    /// If extension is not set, return "bin"
    pub fn get_data_format(&self) -> String {
        if let Some(data_format) = &self.data_format {
            return data_format.to_string();
        }
        if let Some(extension) = self.extension() {
            return extension.to_string();
        }
        "bin".to_string()
    }

    /// Return unicode icon representing the file type as an emoji
    /// Unicode is inferred from the extension.
    /// Note, that a custom unicode icon can be set in the attribute unicode_icon.
    /// If extension is not set, return DEFAULT_ICON
    pub fn default_unicode_icon(&self) -> &'static str {
        if let Some(extension) = self.extension() {
            crate::icons::file_extension_to_unicode_icon(&extension)
        } else {
            crate::icons::DEFAULT_ICON
        }
    }

    /// Return an Error object if metadata describes a failed execution
    pub fn error_result(&self) -> Result<(), Error> {
        if self.is_error {
            if let Some(error) = &self.error_data {
                return Err(error.clone());
            }
            return Err(Error::general_error(self.message.clone()));
        }
        Ok(())
    }
    pub fn primary_progress(&self) -> ProgressEntry {
        if self.progress.is_empty() {
            ProgressEntry::off()
        } else {
            self.progress[0].clone()
        }
    }
    pub fn set_primary_progress(&mut self, progress: &ProgressEntry) -> &mut Self {
        if self.progress.is_empty() {
            self.progress.push(progress.clone());
        } else {
            self.progress[0].set(progress);
        }
        self
    }
    pub fn secondary_progress(&self) -> ProgressEntry {
        if self.progress.len() < 2 {
            ProgressEntry::off()
        } else {
            self.progress[1].clone()
        }
    }
    pub fn set_secondary_progress(&mut self, progress: &ProgressEntry) -> &mut Self {
        if self.progress.is_empty() {
            self.progress.push(ProgressEntry::off());
            self.progress.push(progress.clone());
        } else if self.progress.len() < 2 {
            self.progress.push(progress.clone());
        } else {
            self.progress[1].set(progress);
        }
        self
    }
    /// Update the updated timestamp to now
    pub fn set_updated_now(&mut self) -> &mut Self {
        self.updated = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self
    }
}

#[derive(Debug, Clone)]
pub enum Metadata {
    LegacyMetadata(serde_json::Value),
    MetadataRecord(MetadataRecord),
}

impl Default for Metadata {
    fn default() -> Self {
        Self::new()
    }
}

impl Metadata {
    pub fn new() -> Metadata {
        Metadata::MetadataRecord(MetadataRecord::new())
    }

    pub fn from_error(error: Error) -> Metadata {
        Metadata::MetadataRecord(MetadataRecord::from_error(error))
    }
    /// Get most important features in form of an AssetInfo
    pub fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                let mut m = AssetInfo::new();
                if let Some(key) = o.get("key") {
                    m.key = Some(parse::parse_key(key.to_string())?);
                }
                m.status = self.status();
                m.type_identifier = self.type_identifier().unwrap_or("".to_string());
                m.data_format = Some(self.get_data_format());
                m.message = self.message().to_string();
                m.title = self.title().to_string();
                m.description = self.description().to_string();
                m.is_error = self.is_error().unwrap_or(false);
                m.media_type = self.get_media_type();
                m.filename = self.filename();
                m.unicode_icon = self.unicode_icon().to_string();
                m.file_size = self.file_size();
                m.is_dir = self.is_dir();
                Ok(m)
            }
            Metadata::MetadataRecord(m) => Ok(m.get_asset_info()),
            _ => Err(Error::general_error(
                "Failed to extract asset info from an unsupported metadata type".to_string(),
            )),
        }
    }

    pub fn with_query(&mut self, query: Query) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("query".to_string(), Value::String(query.encode()));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_query(query);
                self
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.query = query;
                *self = Metadata::MetadataRecord(m);
                self
            }

            _ => {
                panic!("Cannot set query on unsupported legacy metadata")
            }
        }
    }

    pub fn from_json(json: &str) -> serde_json::Result<Metadata> {
        match serde_json::from_str::<MetadataRecord>(json) {
            Ok(m) => Ok(Metadata::MetadataRecord(m)),
            Err(_) => match serde_json::from_str::<serde_json::Value>(json) {
                Ok(v) => Ok(Metadata::LegacyMetadata(v)),
                Err(e) => Err(e),
            },
        }
    }

    pub fn from_json_value(json: serde_json::Value) -> serde_json::Result<Metadata> {
        match serde_json::from_value::<MetadataRecord>(json.clone()) {
            Ok(m) => Ok(Metadata::MetadataRecord(m)),
            Err(_) => match serde_json::from_value::<serde_json::Value>(json) {
                Ok(v) => Ok(Metadata::LegacyMetadata(v)),
                Err(e) => Err(e),
            },
        }
    }

    /// Check if there was an error
    pub fn is_error(&self) -> Result<bool, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(e) = o.get("is_error") {
                    return e.as_bool().ok_or(Error::general_error(
                        "is_error not a boolean in legacy metadata".to_owned(),
                    ));
                }
                Err(Error::general_error(
                    "is_error not available in legacy metadata".to_owned(),
                ))
            }
            Metadata::MetadataRecord(m) => Ok(m.is_error),
            Metadata::LegacyMetadata(serde_json::Value::Null) => Err(Error::general_error(
                "legacy metadata is null, thus is_error is not available".to_owned(),
            )),
            _ => Err(Error::general_error(
                "legacy metadata is not an object, thus is_error is not available".to_owned(),
            )),
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        match self {
            Metadata::LegacyMetadata(v) => serde_json::to_string(v),
            Metadata::MetadataRecord(m) => serde_json::to_string(m),
        }
    }

    pub fn get_media_type(&self) -> String {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(mimetype) = o.get("mimetype") {
                    return mimetype.to_string();
                }
                if let Some(media_type) = o.get("media_type") {
                    return media_type.to_string();
                }
                "application/octet-stream".to_string()
            }
            Metadata::MetadataRecord(m) => m.get_media_type(),
            _ => "application/octet-stream".to_string(),
        }
    }

    pub fn query(&self) -> Result<Query, crate::error::Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(query)) = o.get("query") {
                    return parse::parse_query(query);
                }
                Err(Error::general_error(
                    "Query not found in legacy metadata".to_string(),
                ))
            }
            Metadata::MetadataRecord(m) => Ok(m.query.to_owned()),
            _ => Err(Error::general_error(
                "Query not found in unsupported legacy metadata".to_string(),
            )),
        }
    }
    pub fn with_type_identifier(&mut self, type_identifier: String) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "type_identifier".to_string(),
                    Value::String(type_identifier),
                );
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_type_identifier(type_identifier);
                self
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.type_identifier = type_identifier;
                *self = Metadata::MetadataRecord(m);
                self
            }

            _ => {
                panic!("Cannot set type_identifier on unsupported legacy metadata")
            }
        }
    }
    pub fn type_identifier(&self) -> Result<String, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(type_identifier)) = o.get("type_identifier") {
                    Ok(type_identifier.to_string())
                } else {
                    let error = Error::general_error(
                        "type_identifier not found in legacy metadata".to_string(),
                    );
                    if let Ok(query) = self.query() {
                        Err(error.with_query(&query))
                    } else {
                        Err(error)
                    }
                }
            }
            Metadata::MetadataRecord(m) => Ok(m.type_identifier()),
            _ => {
                let error = Error::general_error(
                    "type_identifier is not defined in non-object legacy metadata".to_string(),
                );
                if let Ok(query) = self.query() {
                    Err(error.with_query(&query))
                } else {
                    Err(error)
                }
            }
        }
    }
    pub fn filename(&self) -> Option<String> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(filename)) = o.get("filename") {
                    Some(filename.to_string())
                } else {
                    self.query()
                        .unwrap_or_default()
                        .filename()
                        .map(|f| f.encode().to_string())
                }
            }
            Metadata::MetadataRecord(m) => m.filename(),
            _ => None,
        }
    }
    pub fn set_filename(&mut self, filename: &str) -> Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set filename on legacy metadata".to_string(),
            )),
            Metadata::MetadataRecord(m) => {
                m.set_filename(filename);
                Ok(self)
            }
        }
    }
    pub fn extension(&self) -> Option<String> {
        if let Some(filename) = self.filename() {
            let parts: Vec<&str> = filename.split('.').collect();
            if parts.len() > 1 {
                return Some(parts.last().unwrap().to_string());
            }
        }
        None
    }

    pub fn set_extension(&mut self, extension: &str) -> Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(_) => {
                let error =
                    Error::general_error("Cannot set extension on legacy metadata".to_string());
                if let Ok(query) = self.query() {
                    Err(error.with_query(&query))
                } else {
                    Err(error)
                }
            }
            Metadata::MetadataRecord(m) => {
                m.set_extension(extension);
                Ok(self)
            }
        }
    }

    /// Return data format
    /// If data_format is not set, return extension
    /// If extension is not set, return "bin"
    pub fn get_data_format(&self) -> String {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(data_format) = o.get("data_format") {
                    return data_format.to_string();
                }
                if let Some(extension) = self.extension() {
                    return extension.to_string();
                }
                "bin".to_string()
            }
            Metadata::MetadataRecord(m) => m.get_data_format(),
            _ => "bin".to_string(),
        }
    }

    pub fn with_error(&mut self, e: Error) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("is_error".to_string(), Value::Bool(true));
                o.insert("message".to_string(), Value::String(e.to_string()));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_error(e);
                self
            }
            _ => {
                panic!("Cannot set error on unsupported legacy metadata")
            }
        }
    }

    pub fn add_log_entry(&mut self, log_entry: LogEntry) -> Result<(), Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::Array(log)) = o.get_mut("log") {
                    log.push(serde_json::to_value(log_entry).unwrap());
                } else {
                    o.insert(
                        "log".to_string(),
                        Value::Array(vec![serde_json::to_value(log_entry).unwrap()]),
                    );
                }
                Ok(())
            }
            Metadata::MetadataRecord(m) => {
                m.add_log_entry(log_entry);
                Ok(())
            }
            _ => Err(Error::general_error(
                "Cannot add log entry on unsupported legacy metadata".to_string(),
            )),
        }
    }

    pub fn status(&self) -> Status {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(status) = o.get("status") {
                    return serde_json::from_value(status.clone()).unwrap_or(Status::None);
                }
                Status::None
            }
            Metadata::MetadataRecord(m) => m.status,
            _ => Status::None,
        }
    }

    pub fn set_status(&mut self, status: Status) -> Result<(), Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("status".to_string(), serde_json::to_value(status).unwrap());
                Ok(())
            }
            Metadata::MetadataRecord(m) => {
                m.with_status(status);
                Ok(())
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.status = status;
                *self = Metadata::MetadataRecord(m);
                Ok(())
            }

            _ => Err(Error::general_error(
                "Cannot set status on unsupported legacy metadata".to_string(),
            )),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(message) = o.get("message") {
                    return message.as_str().unwrap_or("");
                }
                ""
            }
            Metadata::MetadataRecord(m) => m.message.as_str(),
            _ => "",
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(title) = o.get("title") {
                    return title.as_str().unwrap_or("");
                }
                ""
            }
            Metadata::MetadataRecord(m) => m.title.as_str(),
            _ => "",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(description) = o.get("description") {
                    return description.as_str().unwrap_or("");
                }
                ""
            }
            Metadata::MetadataRecord(m) => m.description.as_str(),
            _ => "",
        }
    }

    pub fn unicode_icon(&self) -> &str {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(unicode_icon) = o.get("unicode_icon") {
                    return unicode_icon.as_str().unwrap_or(crate::icons::DEFAULT_ICON);
                }
                crate::icons::DEFAULT_ICON
            }
            Metadata::MetadataRecord(m) => m.unicode_icon.as_str(),
            _ => crate::icons::DEFAULT_ICON,
        }
    }

    pub fn file_size(&self) -> Option<u64> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(file_size) = o.get("file_size") {
                    return file_size.as_u64();
                }
                None
            }
            Metadata::MetadataRecord(m) => m.file_size,
            _ => None,
        }
    }

    pub fn is_dir(&self) -> bool {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(is_dir) = o.get("is_dir") {
                    return is_dir.as_bool().unwrap_or(false);
                }
                false
            }
            Metadata::MetadataRecord(m) => m.is_dir,
            _ => false,
        }
    }

    pub fn with_is_dir(&mut self, is_dir: bool) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("is_dir".to_string(), Value::Bool(is_dir));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.is_dir = is_dir;
                self
            }
            _ => self,
        }
    }
    pub fn with_file_size(&mut self, file_size: u64) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "file_size".to_string(),
                    Value::Number(serde_json::Number::from(file_size)),
                );
                self
            }
            Metadata::MetadataRecord(m) => {
                m.file_size = Some(file_size);
                self
            }
            _ => self,
        }
    }

    pub fn with_key(&mut self, key: Key) -> Result<&mut Self,Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("key".to_string(), Value::String(key.encode()));
                Ok(self)
            }
            Metadata::MetadataRecord(m) => {
                m.with_key(key);
                Ok(self)
            }
            _ => {
                Err(Error::general_error("Cannot set key on unsupported legacy metadata".to_string()))
            }
        }
    }
    /// Get primary progress
    /// If not available or for legacy metadata, return ProgressEntry::off()
    pub fn primary_progress(&self)-> ProgressEntry {
        match self {
            Metadata::MetadataRecord(m) => m.primary_progress(),
            _ => ProgressEntry::off(),
        }
    }
    
    /// Set primary progress
    /// No-op for legacy metadata
    pub fn set_primary_progress(&mut self, progress: &ProgressEntry) -> &mut Self {
        match self {
            Metadata::MetadataRecord(m) => {
                m.set_primary_progress(progress);
                self
            }
            _ => self,
        }
    }

    /// Get secondary progress
    /// If not available or for legacy metadata, return ProgressEntry::off()
    pub fn secondary_progress(&self) -> ProgressEntry {
        match self {
            Metadata::MetadataRecord(m) => m.secondary_progress(),
            _ => ProgressEntry::off(),
        }
    }

    /// Set secondary progress
    /// No-op for legacy metadata
    pub fn set_secondary_progress(&mut self, progress: &ProgressEntry) -> &mut Self {
        match self {
            Metadata::MetadataRecord(m) => {
                m.set_secondary_progress(progress);
                self
            }
            _ => self,
        }
    }

    pub fn updated(&self) -> &str {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(updated) = o.get("updated") {
                    return updated.as_str().unwrap_or("");
                }
                ""
            }
            Metadata::MetadataRecord(m) => m.updated.as_str(),
            _ => "",
        }
    }

    /// Set the updated timestamp
    pub fn set_updated(&mut self, updated: String) -> Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("updated".to_string(), Value::String(updated));
                Ok(self)
            }
            Metadata::MetadataRecord(m) => {
                m.updated = updated;
                Ok(self)
            }
            _ => Err(Error::general_error("Unsupported metadata type".to_string())),
        }
    }

    /// Update the updated timestamp to now
    pub fn set_updated_now(&mut self) -> Result<&mut Self, Error> {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.set_updated(now)
    }

    /// Check if the metadata contains an error and return an error result
    /// If the metadata is a legacy metadata, it relies on "is_error" and "message" fields
    pub fn error_result(&self) -> Result<(), Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(is_error) = o.get("is_error") {
                    if is_error.as_bool().unwrap_or(false) {
                        if let Some(message) = o.get("message") {
                            return Err(Error::general_error(message.to_string()));
                        }
                        return Err(Error::general_error("Unknown error".to_string()));
                    }
                }
                Ok(())
            }
            Metadata::MetadataRecord(m) => m.error_result(),
            _ => Err(Error::general_error(
                "Unsupported metadata type".to_string(),
            )),
        }
    }

    /// Return MetadataRecord if the metadata is of that type
    pub fn metadata_record(&self) -> Option<MetadataRecord> {
        match self {
            Metadata::LegacyMetadata(_) => None,
            Metadata::MetadataRecord(m) => Some(m.clone()),
        }
    }
}

impl From<MetadataRecord> for Metadata {
    fn from(m: MetadataRecord) -> Self {
        Metadata::MetadataRecord(m)
    }
}
