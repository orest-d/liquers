#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json::{self, Value};

use crate::command_metadata::CommandKey;
use crate::error::Error;
use crate::expiration::{ExpirationTime, Expires};
use crate::icons::DEFAULT_ICON;
use crate::parse;
use crate::parse::parse_key;
use crate::query::{Key, Position, Query};

/// A version is a 128-bit integer that identifies a specific revision of an asset's content.
/// Versions are opaque — only equality matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version(pub(crate) u128);

impl Version {
    pub fn new(v: u128) -> Self {
        Version(v)
    }

    /// Creates a version by hashing `bytes` with BLAKE3 and taking the first 16 bytes as u128.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        Version(u128::from_be_bytes(
            hash.as_bytes()[0..16]
                .try_into()
                .unwrap_or([0u8; 16]),
        ))
    }

    /// Creates a version from the current system time (nanoseconds since UNIX epoch).
    pub fn from_time_now() -> Self {
        Self::from_specific_time(std::time::SystemTime::now())
    }

    /// Creates a version from a specific `SystemTime`.
    pub fn from_specific_time(time: std::time::SystemTime) -> Self {
        let nanos = time
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .unwrap_or_default()
            .as_nanos();
        Version(nanos)
    }

    /// Creates a version that is unique within the process.
    /// Combines a monotonic counter (low 64 bits) with nanosecond timestamp (high 64 bits).
    /// Returns `true` if this version is unknown (zero).
    pub fn is_unknown(&self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if `self` is compatible with `other`.
    /// Version(0) means "unknown" and is compatible with any version.
    /// Otherwise, versions must be equal to be compatible.
    pub fn matches(&self, other: &Version) -> bool {
        self.is_unknown() || other.is_unknown() || self == other
    }

    pub fn new_unique() -> Self {
        static UNIQUE_COUNTER: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .unwrap_or_default()
            .as_nanos();
        let counter =
            UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
        Version(nanos.wrapping_shl(64) | counter)
    }
}

impl serde::Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:032x}", self.0))
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as serde::Deserialize<'de>>::deserialize(deserializer)?;
        u128::from_str_radix(&s, 16)
            .map(Version)
            .map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// A key that uniquely identifies a dependency within the dependency manager.
///
/// Encodes the type of the resource as a prefix:
/// - `-R/{encoded_key}`             — a keyed asset (the most common kind)
/// - `-R-dir/{encoded_key}`         — a directory listing asset
/// - `-R-recipe/{encoded_key}`      — the recipe file for a keyed asset
/// - `ns-dep/command_metadata-{ck}` — command metadata for a registered command
/// - `ns-dep/command_impl-{ck}`     — command implementation stamp for a registered command
/// - Any other string               — a raw / ad-hoc dependency key
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DependencyKey(String);

impl DependencyKey {
    /// Construct from any string. The caller is responsible for using a well-known prefix.
    pub fn new(s: impl Into<String>) -> Self {
        DependencyKey(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to a `Query` by parsing the inner string.
    pub fn to_query(&self) -> Result<Query, Error> {
        crate::parse::parse_query(&self.0)
    }

    /// `-R-recipe/{key}` — dependency on the recipe definition for `key`.
    pub fn from_recipe_key(key: &Key) -> Self {
        DependencyKey(format!("-R-recipe/{}", key.encode()))
    }

    /// `-R-dir/{key}` — dependency on the directory listing at `key`.
    pub fn from_dir_key(key: &Key) -> Self {
        DependencyKey(format!("-R-dir/{}", key.encode()))
    }

    /// `ns-dep/command_metadata-{ck}` — dependency on a command's metadata (signature/docs).
    pub fn for_command_metadata(key: &CommandKey) -> Self {
        DependencyKey(format!("ns-dep/command_metadata-{}", key))
    }

    /// `ns-dep/command_impl-{ck}` — dependency on a command's implementation version.
    pub fn for_command_implementation(key: &CommandKey) -> Self {
        DependencyKey(format!("ns-dep/command_impl-{}", key))
    }
}

/// `-R/{encoded_key}` — standard asset key dependency.
impl From<&Key> for DependencyKey {
    fn from(key: &Key) -> Self {
        DependencyKey(format!("-R/{}", key.encode()))
    }
}

/// Convert a `DependencyKey` back to a `Key` — only succeeds for `-R/` prefixed keys.
impl TryFrom<&DependencyKey> for Key {
    type Error = Error;

    fn try_from(value: &DependencyKey) -> Result<Self, Self::Error> {
        if let Some(encoded) = value.as_str().strip_prefix("-R/") {
            parse_key(encoded)
        } else {
            Err(Error::not_supported(format!(
                "DependencyKey '{}' does not represent a plain asset key",
                value.as_str()
            )))
        }
    }
}

impl From<&Query> for DependencyKey {
    fn from(query: &Query) -> Self {
        DependencyKey(query.encode())
    }
}

impl std::fmt::Display for DependencyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Records the version of a single dependency as observed when the dependent was evaluated.
/// Stored in `MetadataRecord.dependencies` and used to detect stale dependents on reload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub key: DependencyKey,
    pub version: Version,
}

impl DependencyRecord {
    pub fn new(key: DependencyKey, version: Version) -> Self {
        DependencyRecord { key, version }
    }
}

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
    /// Asset has data that overrides the recipe calculation.
    /// The recipe exists but was not used to calculate this data.
    /// Override can be cleared to recalculate using the recipe.
    Override,
    /// Asset has volatile value (use once, then expires).
    /// Volatile assets are never cached and must be re-evaluated each time.
    /// Similar to Expired, but indicates the value is currently valid for single use.
    Volatile,
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
            Status::Override => true,
            Status::Volatile => true, // Volatile has data (use once)
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
            Status::Override => false,
            Status::Volatile => false, // Like Expired, volatile is terminal
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
            Status::Override => true,
            Status::Volatile => true, // Volatile is finished state
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
            Status::Override => false,
            Status::Volatile => false, // Volatile is finished, not processing
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
    /// Detailed type name of the value (runtime/debug oriented)
    #[serde(default)]
    pub type_name: String,
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
    pub updated: String,
    /// Structure containing the error information
    pub error_data: Option<Error>,

    /// If true, this asset is or will be volatile
    #[serde(default)] // Legacy support: old AssetInfo without this field defaults to false
    pub is_volatile: bool,

    /// Expiration specification (human-readable, e.g. "in 5 min", "never")
    #[serde(default)]
    pub expires: Expires,
    /// Resolved expiration time (UTC timestamp, Never, or Immediately)
    #[serde(default)]
    pub expiration_time: ExpirationTime,
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
        metadata.type_name = asset_info.type_name;
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
        metadata.error_data = asset_info.error_data;
        metadata.is_volatile = asset_info.is_volatile;
        metadata.expires = asset_info.expires;
        metadata.expiration_time = asset_info.expiration_time;
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
    /// Detailed type name of the value (runtime/debug oriented)
    #[serde(default)]
    pub type_name: String,
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

    /// If true, this value is known to be volatile even if status is not yet Volatile.
    /// Useful for in-flight assets (Submitted, Dependencies, Processing) where final
    /// value will be volatile when ready.
    /// NOTE: No #[serde(default)] - always required in serialized format per Phase 2
    pub is_volatile: bool,

    /// Expiration specification (human-readable, e.g. "in 5 min", "never")
    #[serde(default)]
    pub expires: Expires,
    /// Resolved expiration time (UTC timestamp, Never, or Immediately)
    #[serde(default)]
    pub expiration_time: ExpirationTime,

    /// Content-hash version of this asset, computed at save time as `Version::from_bytes(content)`.
    /// `None` for assets whose version has not been recorded (treated as `Version(0)` = unknown).
    #[serde(default)]
    pub version: Option<Version>,

    /// Versions of dependencies observed when this asset was last evaluated.
    /// Used by the dependency manager to detect stale dependents on reload.
    /// Absent in older serialized records (defaults to empty).
    #[serde(default)]
    pub dependencies: Vec<DependencyRecord>,
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
            type_name: self.type_name.clone(),
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
            error_data: self.error_data.clone(),
            is_volatile: self.is_volatile,
            expires: self.expires.clone(),
            expiration_time: self.expiration_time.clone(),
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
    pub fn with_type_name(&mut self, type_name: String) -> &mut Self {
        self.type_name = type_name;
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
    pub fn type_name(&self) -> String {
        self.type_name.to_string()
    }
    pub fn filename(&self) -> Option<String> {
        self.filename.clone()
    }
    pub fn set_filename(&mut self, filename: &str) {
        self.filename = Some(filename.to_string());
        self.media_type = crate::media_type::file_extension_to_media_type(
            self.extension().unwrap_or("".to_string()).as_str(),
        )
        .to_owned();
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
        self.media_type = crate::media_type::file_extension_to_media_type(extension).to_owned();
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
    pub fn remove_progress(&mut self) -> &mut Self {
        self.progress.clear();
        self
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

    /// Returns true if the value is or will be volatile
    pub fn is_volatile(&self) -> bool {
        self.is_volatile || self.status == Status::Volatile
    }

    /// Returns true if this asset has a non-Never expiration time
    pub fn has_expiration(&self) -> bool {
        !self.expiration_time.is_never()
    }

    /// Returns true if this asset is expired (expiration time has passed)
    pub fn is_expired(&self) -> bool {
        self.expiration_time.is_expired()
    }

    /// Mark metadata as volatile result (single-use semantics).
    pub fn set_volatile(&mut self) -> &mut Self {
        self.status = Status::Volatile;
        self.is_volatile = true;
        self.expires = Expires::Immediately;
        self.expiration_time = ExpirationTime::Immediately;
        self.set_updated_now();
        self
    }

    /// Set resolved expiration time and keep it safely in the future for At(..).
    pub fn set_expiration_time(&mut self, expiration_time: ExpirationTime) -> &mut Self {
        self.expiration_time = expiration_time.ensure_future(std::time::Duration::from_millis(500));
        self.set_updated_now();
        self
    }

    /// Resolve expiration from expires policy and set both fields consistently.
    pub fn set_expiration_time_from(&mut self, expires: &Expires) -> &mut Self {
        self.expires = expires.clone();
        let expiration_time = expires.to_expiration_time(chrono::Utc::now(), 0);
        self.set_expiration_time(expiration_time);
        self
    }

    /// Get the dependency records.
    pub fn get_dependencies(&self) -> &[DependencyRecord] {
        &self.dependencies
    }

    /// Replace all dependency records.
    pub fn set_dependencies(&mut self, deps: Vec<DependencyRecord>) {
        self.dependencies = deps;
    }

    /// Upsert a dependency record: if a record with the same key exists, replace its version;
    /// otherwise append a new record.
    pub fn add_dependency(&mut self, record: DependencyRecord) {
        if let Some(existing) = self.dependencies.iter_mut().find(|d| d.key == record.key) {
            existing.version = record.version;
        } else {
            self.dependencies.push(record);
        }
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
                m.type_name = self.type_name().unwrap_or("".to_string());
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
                // Try to extract is_volatile from JSON, default to false if not present
                m.is_volatile = if let Some(is_volatile) = o.get("is_volatile") {
                    is_volatile.as_bool().unwrap_or(false)
                } else {
                    false
                };
                // Try to extract expires from JSON, default to Never
                if let Some(expires_val) = o.get("expires") {
                    if let Some(s) = expires_val.as_str() {
                        if let Ok(expires) = s.parse() {
                            m.expires = expires;
                        }
                    }
                }
                // Try to extract expiration_time from JSON, default to Never
                if let Some(et_val) = o.get("expiration_time") {
                    if let Some(s) = et_val.as_str() {
                        if let Ok(et) = serde_json::from_value::<ExpirationTime>(
                            serde_json::Value::String(s.to_string()),
                        ) {
                            m.expiration_time = et;
                        }
                    }
                }
                Ok(m)
            }
            Metadata::MetadataRecord(m) => Ok(m.get_asset_info()),
            _ => Err(Error::general_error(
                "Failed to extract asset info from an unsupported metadata type".to_string(),
            )),
        }
    }

    pub fn with_query(&mut self, query: Query) -> Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("query".to_string(), Value::String(query.encode()));
                Ok(self)
            }
            Metadata::MetadataRecord(m) => {
                m.with_query(query);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.query = query;
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }

            _ => Err(Error::general_error(
                "Cannot set query on unsupported legacy metadata".to_string(),
            )
            .with_query(&query)),
        }
    }

    pub fn with_key(&mut self, key: Key) -> Result<&mut Self, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("key".to_string(), Value::String(key.encode()));
                Ok(self)
            }
            Metadata::MetadataRecord(m) => {
                m.with_key(key);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.key = Some(key);
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }

            _ => Err(Error::general_error(
                "Cannot set key on unsupported legacy metadata".to_string(),
            )
            .with_key(&key)),
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

    pub fn key(&self) -> Result<Option<Key>, crate::error::Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(key)) = o.get("key") {
                    return Ok(Some(parse::parse_key(key)?));
                }
                Ok(None)
            }
            Metadata::MetadataRecord(m) => Ok(m.key.to_owned()),
            _ => Err(Error::general_error(
                "Key not found in unsupported legacy metadata".to_string(),
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
    pub fn with_type_name(&mut self, type_name: String) -> &mut Self {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("type_name".to_string(), Value::String(type_name));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.with_type_name(type_name);
                self
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.type_name = type_name;
                *self = Metadata::MetadataRecord(m);
                self
            }

            _ => {
                panic!("Cannot set type_name on unsupported legacy metadata")
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
    pub fn type_name(&self) -> Result<String, Error> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(type_name)) = o.get("type_name") {
                    Ok(type_name.to_string())
                } else {
                    let error =
                        Error::general_error("type_name not found in legacy metadata".to_string());
                    if let Ok(query) = self.query() {
                        Err(error.with_query(&query))
                    } else {
                        Err(error)
                    }
                }
            }
            Metadata::MetadataRecord(m) => Ok(m.type_name()),
            _ => {
                let error = Error::general_error(
                    "type_name is not defined in non-object legacy metadata".to_string(),
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
                o.insert(
                    "type_identifier".to_string(),
                    Value::String("error".to_string()),
                );
                o.insert("is_error".to_string(), Value::Bool(true));
                o.insert("message".to_string(), Value::String(e.to_string()));
                self
            }
            Metadata::MetadataRecord(m) => {
                m.type_identifier = "error".to_string();
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

    /// Get the version from metadata, if available.
    pub fn version(&self) -> Option<Version> {
        match self {
            Metadata::MetadataRecord(m) => m.version,
            Metadata::LegacyMetadata(_) => None,
        }
    }

    /// Set the version in metadata.
    pub fn set_version(&mut self, version: Option<Version>) -> Result<(), Error> {
        match self {
            Metadata::MetadataRecord(m) => {
                m.version = version;
                Ok(())
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.version = version;
                *self = Metadata::MetadataRecord(m);
                Ok(())
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set version on unsupported legacy metadata".to_string(),
            )),
        }
    }

    /// Get the dependency records from metadata.
    pub fn get_dependencies(&self) -> &[DependencyRecord] {
        match self {
            Metadata::MetadataRecord(m) => &m.dependencies,
            Metadata::LegacyMetadata(_) => &[],
        }
    }

    /// Replace all dependency records in metadata.
    pub fn set_dependencies(&mut self, deps: Vec<DependencyRecord>) -> Result<(), Error> {
        match self {
            Metadata::MetadataRecord(m) => {
                m.set_dependencies(deps);
                Ok(())
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.set_dependencies(deps);
                *self = Metadata::MetadataRecord(m);
                Ok(())
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set dependencies on unsupported legacy metadata".to_string(),
            )),
        }
    }

    /// Upsert a dependency record into metadata.
    pub fn add_dependency(&mut self, record: DependencyRecord) -> Result<(), Error> {
        match self {
            Metadata::MetadataRecord(m) => {
                m.add_dependency(record);
                Ok(())
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.add_dependency(record);
                *self = Metadata::MetadataRecord(m);
                Ok(())
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot add dependency on unsupported legacy metadata".to_string(),
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

    /// Get primary progress
    /// If not available or for legacy metadata, return ProgressEntry::off()
    pub fn primary_progress(&self) -> ProgressEntry {
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

    /// Remove progress
    pub fn remove_progress(&mut self) -> &mut Self {
        match self {
            Metadata::MetadataRecord(m) => {
                m.remove_progress();
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
            _ => Err(Error::general_error(
                "Unsupported metadata type".to_string(),
            )),
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

    /// Returns true if the value is or will be volatile.
    /// For legacy metadata without is_volatile field or Status::Volatile,
    /// defaults to false (non-volatile). Such cases should be detected in
    /// the future and marked as expired or override by the user.
    pub fn is_volatile(&self) -> bool {
        match self {
            Metadata::MetadataRecord(mr) => mr.is_volatile || mr.status == Status::Volatile,
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                // Try to extract is_volatile from JSON, default to false if not present
                if let Some(is_volatile) = o.get("is_volatile") {
                    is_volatile.as_bool().unwrap_or(false)
                } else {
                    // Check if status is Volatile
                    self.status() == Status::Volatile
                }
            }
            Metadata::LegacyMetadata(_) => false, // Non-object legacy: default non-volatile
        }
    }

    /// Get the expiration specification
    pub fn expires(&self) -> Expires {
        match self {
            Metadata::MetadataRecord(mr) => mr.expires.clone(),
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(serde_json::Value::String(s)) = o.get("expires") {
                    s.parse().unwrap_or(Expires::Never)
                } else {
                    Expires::Never
                }
            }
            Metadata::LegacyMetadata(_) => Expires::Never,
        }
    }

    /// Get the resolved expiration time
    pub fn expiration_time(&self) -> ExpirationTime {
        match self {
            Metadata::MetadataRecord(mr) => mr.expiration_time.clone(),
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(serde_json::Value::String(s)) = o.get("expiration_time") {
                    serde_json::from_value::<ExpirationTime>(serde_json::Value::String(
                        s.to_string(),
                    ))
                    .unwrap_or(ExpirationTime::Never)
                } else {
                    ExpirationTime::Never
                }
            }
            Metadata::LegacyMetadata(_) => ExpirationTime::Never,
        }
    }

    /// Returns true if this asset has a non-Never expiration time
    pub fn has_expiration(&self) -> bool {
        !self.expiration_time().is_never()
    }

    /// Returns true if this asset is expired (status is Expired or expiration time has passed)
    pub fn is_expired(&self) -> bool {
        self.status() == Status::Expired || self.expiration_time().is_expired()
    }

    /// Set the expiration specification
    pub fn set_expires(&mut self, expires: Expires) -> Result<&mut Self, Error> {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.expires = expires;
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "expires".to_string(),
                    serde_json::Value::String(expires.to_string()),
                );
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.expires = expires;
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set expires on unsupported legacy metadata".to_string(),
            )),
        }
    }

    /// Set the resolved expiration time
    pub fn set_expiration_time(
        &mut self,
        expiration_time: ExpirationTime,
    ) -> Result<&mut Self, Error> {
        let expiration_time = expiration_time.ensure_future(std::time::Duration::from_millis(500));
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.set_expiration_time(expiration_time);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                let val = serde_json::to_value(&expiration_time).map_err(|e| {
                    Error::general_error(format!("Failed to serialize expiration_time: {}", e))
                })?;
                o.insert("expiration_time".to_string(), val);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.set_expiration_time(expiration_time);
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set expiration_time on unsupported legacy metadata".to_string(),
            )),
        }
    }

    pub fn set_expiration_time_from(&mut self, expires: &Expires) -> Result<&mut Self, Error> {
        self.set_expires(expires.clone())?;
        let expiration_time = expires.to_expiration_time(chrono::Utc::now(), 0);
        self.set_expiration_time(expiration_time)
    }

    pub fn set_volatile(&mut self) -> Result<&mut Self, Error> {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.set_volatile();
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "status".to_string(),
                    serde_json::to_value(Status::Volatile).unwrap(),
                );
                o.insert("is_volatile".to_string(), serde_json::Value::Bool(true));
                o.insert(
                    "expires".to_string(),
                    serde_json::Value::String(Expires::Immediately.to_string()),
                );
                let expiration_time_value = serde_json::to_value(ExpirationTime::Immediately)
                    .map_err(|e| {
                        Error::general_error(format!(
                            "Failed to serialize expiration_time for volatile metadata: {}",
                            e
                        ))
                    })?;
                o.insert("expiration_time".to_string(), expiration_time_value);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut mr = MetadataRecord::new();
                mr.set_volatile();
                *self = Metadata::MetadataRecord(mr);
                Ok(self)
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set volatile on unsupported legacy metadata".to_string(),
            )),
        }
    }
}

impl From<MetadataRecord> for Metadata {
    fn from(m: MetadataRecord) -> Self {
        Metadata::MetadataRecord(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_serialization_roundtrip_hex_width_32() {
        let version = Version::new(0xdead_beef_cafe_babe_1234_5678_90ab_cdef);
        let json = serde_json::to_string(&version).unwrap();

        // Quoted 32-char lowercase hex string.
        assert_eq!(json, "\"deadbeefcafebabe1234567890abcdef\"");

        let decoded: Version = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, version);
    }

    #[test]
    fn test_version_deserialize_rejects_non_hex() {
        let result: Result<Version, _> = serde_json::from_str("\"xyz\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_version_is_unknown() {
        assert!(Version::new(0).is_unknown());
        assert!(!Version::new(1).is_unknown());
        assert!(!Version::new(42).is_unknown());
    }

    #[test]
    fn test_version_matches() {
        let v0 = Version::new(0);
        let v1 = Version::new(1);
        let v2 = Version::new(2);

        // Zero matches anything
        assert!(v0.matches(&v0));
        assert!(v0.matches(&v1));
        assert!(v1.matches(&v0));

        // Equal non-zero versions match
        assert!(v1.matches(&v1));

        // Different non-zero versions don't match
        assert!(!v1.matches(&v2));
        assert!(!v2.matches(&v1));
    }

    #[test]
    fn test_status_volatile_has_data() {
        let status = Status::Volatile;
        assert!(status.has_data());
    }

    #[test]
    fn test_status_volatile_is_finished() {
        let status = Status::Volatile;
        assert!(status.is_finished());
    }

    #[test]
    fn test_status_volatile_cannot_track_dependencies() {
        let status = Status::Volatile;
        assert!(!status.can_have_tracked_dependencies());
    }

    #[test]
    fn test_status_volatile_serialization() {
        let status = Status::Volatile;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Volatile\"");
        let deserialized: Status = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Status::Volatile);
    }

    #[test]
    fn test_metadata_record_is_volatile_helper() {
        let mut mr = MetadataRecord::default();
        mr.is_volatile = true;
        assert!(mr.is_volatile());

        mr.is_volatile = false;
        mr.status = Status::Volatile;
        assert!(mr.is_volatile());
    }

    #[test]
    fn test_metadata_record_expiration_defaults() {
        let mr = MetadataRecord::new();
        assert_eq!(mr.expires, Expires::Never);
        assert_eq!(mr.expiration_time, ExpirationTime::Never);
        assert!(!mr.has_expiration());
        assert!(!mr.is_expired());
    }

    #[test]
    fn test_metadata_record_has_expiration() {
        let mut mr = MetadataRecord::new();
        mr.expires = Expires::Immediately;
        mr.expiration_time = ExpirationTime::Immediately;
        assert!(mr.has_expiration());
        assert!(mr.is_expired());
    }

    #[test]
    fn test_metadata_record_expiration_future() {
        let mut mr = MetadataRecord::new();
        let future = chrono::Utc::now() + chrono::Duration::hours(1);
        mr.expires = Expires::InDuration(std::time::Duration::from_secs(3600));
        mr.expiration_time = ExpirationTime::At(future);
        assert!(mr.has_expiration());
        assert!(!mr.is_expired());
    }

    #[test]
    fn test_asset_info_expiration_roundtrip() {
        let mut mr = MetadataRecord::new();
        mr.expires = Expires::Immediately;
        mr.expiration_time = ExpirationTime::Immediately;

        let ai = mr.get_asset_info();
        assert_eq!(ai.expires, Expires::Immediately);
        assert_eq!(ai.expiration_time, ExpirationTime::Immediately);

        let mr2 = MetadataRecord::from(ai);
        assert_eq!(mr2.expires, Expires::Immediately);
        assert_eq!(mr2.expiration_time, ExpirationTime::Immediately);
    }

    #[test]
    fn test_metadata_set_expires() {
        let mut m = Metadata::MetadataRecord(MetadataRecord::new());
        m.set_expires(Expires::Immediately).unwrap();
        assert_eq!(m.expires(), Expires::Immediately);
    }

    #[test]
    fn test_metadata_set_expiration_time() {
        let mut m = Metadata::MetadataRecord(MetadataRecord::new());
        let future = chrono::Utc::now() + chrono::Duration::hours(1);
        m.set_expiration_time(ExpirationTime::At(future)).unwrap();
        assert_eq!(m.expiration_time(), ExpirationTime::At(future));
    }

    #[test]
    fn test_metadata_set_expiration_time_from_enforces_future() {
        let mut m = Metadata::MetadataRecord(MetadataRecord::new());
        let expires = Expires::InDuration(std::time::Duration::from_millis(0));
        m.set_expiration_time_from(&expires).unwrap();
        match m.expiration_time() {
            ExpirationTime::At(dt) => {
                assert!(dt > chrono::Utc::now());
            }
            _ => panic!("Expected ExpirationTime::At"),
        }
    }

    #[test]
    fn test_metadata_set_volatile() {
        let mut m = Metadata::MetadataRecord(MetadataRecord::new());
        m.set_volatile().unwrap();
        assert_eq!(m.status(), Status::Volatile);
        assert!(m.is_volatile());
        assert_eq!(m.expires(), Expires::Immediately);
        assert_eq!(m.expiration_time(), ExpirationTime::Immediately);
    }

    #[test]
    fn test_metadata_has_expiration_never() {
        let m = Metadata::MetadataRecord(MetadataRecord::new());
        assert!(!m.has_expiration());
        assert!(!m.is_expired());
    }

    #[test]
    fn test_metadata_has_expiration_immediately() {
        let mut mr = MetadataRecord::new();
        mr.expiration_time = ExpirationTime::Immediately;
        let m = Metadata::MetadataRecord(mr);
        assert!(m.has_expiration());
        assert!(m.is_expired());
    }

    #[test]
    fn test_add_dependency_inserts_new() {
        let mut mr = MetadataRecord::new();
        assert!(mr.get_dependencies().is_empty());
        let dep = DependencyRecord::new(DependencyKey::new("dep-a"), Version::new(1));
        mr.add_dependency(dep);
        assert_eq!(mr.get_dependencies().len(), 1);
        assert_eq!(mr.get_dependencies()[0].key, DependencyKey::new("dep-a"));
        assert_eq!(mr.get_dependencies()[0].version, Version::new(1));
    }

    #[test]
    fn test_add_dependency_replaces_version() {
        let mut mr = MetadataRecord::new();
        mr.add_dependency(DependencyRecord::new(DependencyKey::new("dep-a"), Version::new(1)));
        mr.add_dependency(DependencyRecord::new(DependencyKey::new("dep-a"), Version::new(42)));
        assert_eq!(mr.get_dependencies().len(), 1);
        assert_eq!(mr.get_dependencies()[0].version, Version::new(42));
    }

    #[test]
    fn test_set_dependencies_replaces_all() {
        let mut mr = MetadataRecord::new();
        mr.add_dependency(DependencyRecord::new(DependencyKey::new("dep-a"), Version::new(1)));
        mr.add_dependency(DependencyRecord::new(DependencyKey::new("dep-b"), Version::new(2)));
        assert_eq!(mr.get_dependencies().len(), 2);
        mr.set_dependencies(vec![
            DependencyRecord::new(DependencyKey::new("dep-c"), Version::new(3)),
        ]);
        assert_eq!(mr.get_dependencies().len(), 1);
        assert_eq!(mr.get_dependencies()[0].key, DependencyKey::new("dep-c"));
    }

    #[test]
    fn test_metadata_enum_add_dependency_legacy() {
        // Null legacy promotes to MetadataRecord
        let mut m = Metadata::LegacyMetadata(serde_json::Value::Null);
        let dep = DependencyRecord::new(DependencyKey::new("dep-a"), Version::new(1));
        assert!(m.add_dependency(dep).is_ok());
        assert_eq!(m.get_dependencies().len(), 1);

        // Non-null legacy returns error
        let mut m2 = Metadata::LegacyMetadata(serde_json::json!({"foo": "bar"}));
        let dep2 = DependencyRecord::new(DependencyKey::new("dep-b"), Version::new(2));
        assert!(m2.add_dependency(dep2).is_err());
        assert!(m2.get_dependencies().is_empty());
    }
}
