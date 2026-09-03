#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json::{self, Value};

use crate::command_metadata::{CommandKey, PayloadRequirement};
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

    /// The sentinel version for an unknown dependency revision.
    ///
    /// `Version(0)` is intentionally not a concrete asset/content version:
    /// dependency checks treat it as compatible with any known version.
    pub fn unknown() -> Self {
        Version(0)
    }

    /// Creates a version by hashing `bytes` with BLAKE3 and taking the first 16 bytes as u128.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        Version(u128::from_be_bytes(
            hash.as_bytes()[0..16].try_into().unwrap_or([0u8; 16]),
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
        static UNIQUE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .unwrap_or_default()
            .as_nanos();
        let counter = UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
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

    pub fn is_pure_key(&self) -> bool {
        self.0 == "-R" || self.0.starts_with("-R/")
    }

    pub fn is_recipe_key(&self) -> bool {
        self.0 == "-R-recipe" || self.0.starts_with("-R-recipe/")
    }

    pub fn is_dir_key(&self) -> bool {
        self.0 == "-R-dir" || self.0.starts_with("-R-dir/")
    }

    pub fn is_command_metadata(&self) -> bool {
        self.0.starts_with("ns-dep/command_metadata-")
    }

    pub fn is_command_implementation(&self) -> bool {
        self.0.starts_with("ns-dep/command_impl-")
    }

    fn extract_prefixed_key(&self, prefix: &str) -> Result<Option<Key>, Error> {
        if self.0 == prefix {
            return Ok(Some(Key::new()));
        }
        if let Some(encoded) = self.0.strip_prefix(&format!("{}/", prefix)) {
            return parse_key(encoded).map(Some);
        }
        Ok(None)
    }

    fn extract_command_key(&self, prefix: &str) -> Result<Option<CommandKey>, Error> {
        let Some(encoded) = self.0.strip_prefix(prefix) else {
            return Ok(None);
        };

        // TODO: CommandKey currently formats as realm-namespace-name, which is ambiguous
        // if any component can contain '-'. Switch to an unambiguous encoding when possible.
        let mut parts = encoded.splitn(3, '-');
        let realm = parts.next().unwrap_or_default();
        let namespace = parts.next().ok_or_else(|| {
            Error::not_supported(format!(
                "DependencyKey {} does not contain a valid command key",
                self.as_str()
            ))
        })?;
        let name = parts.next().ok_or_else(|| {
            Error::not_supported(format!(
                "DependencyKey {} does not contain a valid command key",
                self.as_str()
            ))
        })?;

        Ok(Some(CommandKey::new(realm, namespace, name)))
    }

    pub fn key(&self) -> Result<Option<Key>, Error> {
        self.extract_prefixed_key("-R")
    }

    pub fn recipe_key(&self) -> Result<Option<Key>, Error> {
        self.extract_prefixed_key("-R-recipe")
    }

    pub fn dir_key(&self) -> Result<Option<Key>, Error> {
        self.extract_prefixed_key("-R-dir")
    }

    pub fn command_key(&self) -> Result<Option<CommandKey>, Error> {
        if self.is_command_metadata() {
            self.extract_command_key("ns-dep/command_metadata-")
        } else if self.is_command_implementation() {
            self.extract_command_key("ns-dep/command_impl-")
        } else {
            Ok(None)
        }
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
        match value.key()? {
            Some(key) => Ok(key),
            None => Err(Error::not_supported(format!(
                "DependencyKey '{}' does not represent a plain asset key",
                value.as_str()
            ))),
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
    /// Reserved for future support for publishing intermediate results.
    ///
    /// Partial-result production and retrieval are not completely implemented.
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

/// What a read of an asset in a given [`Status`] is permitted to expose.
///
/// This is the single decision point shared by the state-read family
/// ([`AssetData::poll_state`](crate::assets::AssetData::poll_state), `get`, …) and the binary-read
/// family (`poll_binary`, `get_binary`, …). Each family renders the same classification in its own
/// terms; neither re-derives it from [`Status`] directly, so the two cannot drift apart.
///
/// Deliberately **not** expressible via [`Status::has_data`], which answers a different question
/// ("is there a value in there") and returns `true` for both `Expired` and `Partial` — statuses a
/// normal read must hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadExposure {
    /// A real value is available: `Ready`, `Source`, `Override`, `Volatile`.
    Value,
    /// No value, but metadata is meaningful: `Directory`, `Error`, `Cancelled`.
    /// There is no binary counterpart of a metadata-only state.
    MetadataOnly,
    /// Data is retained but stale. Hidden from normal reads; returned by the
    /// `*_any_status` recovery reads: `Expired`.
    Expired,
    /// Nothing to expose yet. A waiting read blocks; a polling read returns `None`:
    /// `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing`.
    Pending,
}

impl Status {
    /// Classifies this status for the read families — see [`ReadExposure`].
    ///
    /// Every variant is matched explicitly, with no default arm: adding a `Status` must be a
    /// compile error here rather than a silent fallthrough into the wrong bucket at each of the
    /// eight read methods.
    pub(crate) fn read_exposure(&self) -> ReadExposure {
        match self {
            Status::Ready => ReadExposure::Value,
            Status::Source => ReadExposure::Value,
            Status::Override => ReadExposure::Value,
            Status::Volatile => ReadExposure::Value,

            Status::Directory => ReadExposure::MetadataOnly,
            Status::Error => ReadExposure::MetadataOnly,
            Status::Cancelled => ReadExposure::MetadataOnly,

            Status::Expired => ReadExposure::Expired,

            Status::None => ReadExposure::Pending,
            Status::Recipe => ReadExposure::Pending,
            Status::Submitted => ReadExposure::Pending,
            Status::Dependencies => ReadExposure::Pending,
            Status::Processing => ReadExposure::Pending,
            Status::Partial => ReadExposure::Pending,
            Status::Storing => ReadExposure::Pending,
        }
    }

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
    /// Media type of the value, already resolved.
    ///
    /// `AssetInfo` is a projection for clients, not a place to record how a media type came
    /// about, so it carries the effective value with any override already applied. Only
    /// [`MetadataRecord`], the thing an author writes, needs the override/derive distinction.
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

    /// Whether producing this asset needed an evaluation payload.
    ///
    /// Diagnostic only: the operational consequence (never cached, never shared) is already
    /// carried by [`Self::is_volatile`], which a payload requirement always implies.
    #[serde(default)] // Legacy support: old AssetInfo without this field defaults to None
    pub payload_required: PayloadRequirement,

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
        // Level-2 seeding: the extension names the data format unless one was declared. The media
        // type is *not* written here — it derives from the effective format, and writing it would
        // make an ordinary filename look like a deliberate override.
        if self.data_format.is_none() {
            self.data_format = self.extension();
        }
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
        metadata.media_type = media_type_override(asset_info.media_type);
        metadata.filename = asset_info.filename;
        metadata.unicode_icon = asset_info.unicode_icon;
        metadata.file_size = asset_info.file_size;
        metadata.is_dir = asset_info.is_dir;
        metadata.progress = vec![asset_info.progress];
        metadata.updated = asset_info.updated;
        metadata.error_data = asset_info.error_data;
        metadata.is_volatile = asset_info.is_volatile;
        metadata.payload_required = asset_info.payload_required;
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

/// A *partial* JSON document deserializes into a record; a document carrying fields this struct
/// does not know stays legacy.
///
/// `#[serde(default)]` is what makes the partial case work. Without it,
/// `Metadata::from_json(r#"{"media_type":"text/plain"}"#)` fell through to
/// `Metadata::LegacyMetadata`, and the legacy accessors then returned quoted strings — that was the
/// root cause behind `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`, of which the accessors were only
/// the symptom.
///
/// `#[serde(deny_unknown_fields)]` is what keeps it honest. With defaults alone, *almost any* JSON
/// object deserializes as a record and serde silently drops the fields it does not recognise, so a
/// legacy document such as `{"media_type":"text/plain","custom":{…}}` would be converted and lose
/// `custom` on the next write. Refusing unknown fields sends exactly those documents down the
/// legacy branch, which exists to preserve them.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
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
    /// Media type of the value.
    ///
    /// `None` means "derive from the effective data format"; `Some` is a deliberate override that
    /// is preserved verbatim and never re-derived. The override is an intended capability — it is
    /// how a caller shapes an HTTP response, and how a remotely fetched file keeps the origin
    /// server's declared `Content-Type` — so it survives the write-path checks rather than being
    /// normalized away. Resolve it with [`MetadataRecord::effective_media_type`].
    #[serde(default)]
    pub media_type: Option<String>,
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

    /// Whether producing this value needed an evaluation payload.
    ///
    /// Diagnostic only: the operational consequence is already carried by
    /// [`Self::is_volatile`], which a payload requirement always implies. Unlike
    /// `is_volatile` this field DOES have `#[serde(default)]` — records written before the
    /// field existed must still load — and is skipped when `None` so that metadata of
    /// payload-free assets serializes unchanged.
    #[serde(skip_serializing_if = "PayloadRequirement::is_none")]
    #[serde(default)]
    pub payload_required: PayloadRequirement,

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
            media_type: self.get_media_type(),
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
            payload_required: self.payload_required,
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

    /// Declares a level-3 media-type override, kept verbatim.
    pub fn with_media_type(&mut self, media_type: String) -> &mut Self {
        self.media_type = media_type_override(media_type);
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
        // Level-2 seeding: the extension names the data format unless one was declared. The
        // media type is *not* written here — it derives from the effective format, and writing it
        // would make an ordinary filename indistinguishable from a deliberate override.
        if self.data_format.is_none() {
            self.data_format = self.extension();
        }
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
        // Level-2 seeding: the extension names the data format unless one was declared. The
        // media type is *not* written here — it derives from the effective format, and writing it
        // would make an ordinary filename indistinguishable from a deliberate override.
        if self.data_format.is_none() {
            self.data_format = self.extension();
        }
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
        // Level-2 seeding, as in `set_filename`.
        if self.data_format.is_none() {
            self.data_format = Some(extension.to_string());
        }
    }

    /// The declared level-3 media-type override, if there is one.
    pub fn declared_media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    /// The media type to serve: a declared override verbatim, else derived from `data_format`.
    pub fn effective_media_type(&self, value_default_format: &str) -> String {
        match &self.media_type {
            Some(declared) => declared.clone(),
            None => crate::media_type::file_extension_to_media_type(base_data_format(
                &self.effective_data_format(value_default_format),
            ))
            .to_owned(),
        }
    }

    pub fn get_media_type(&self) -> String {
        self.effective_media_type("bin")
    }

    /// The declared data format, if one was specified.
    ///
    /// `None` is meaningful: it says no format was chosen, so the value's own default applies.
    /// It is *not* a missing value to be patched — knowing that nobody chose is what lets a
    /// caller reason about how a format came to be selected.
    pub fn declared_data_format(&self) -> Option<&str> {
        self.data_format.as_deref()
    }

    /// The data format to use, given the value's own default for level 1.
    pub fn effective_data_format(&self, value_default: &str) -> String {
        match &self.data_format {
            Some(declared) => declared.clone(),
            None => value_default.to_string(),
        }
    }

    /// Return the effective data format, with `bin` standing in for the value's own default.
    ///
    /// Prefer [`MetadataRecord::effective_data_format`], which takes the real level-1 default from
    /// the value. This form exists for the callers that have no value in hand.
    pub fn get_data_format(&self) -> String {
        self.effective_data_format("bin")
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

    /// Returns whether producing this value needed an evaluation payload.
    ///
    /// Note the deliberate asymmetry with [`Self::is_volatile`]: that method also consults
    /// `Status::Volatile`, because volatility is a lifecycle fact a value can reach. A
    /// payload requirement is a property of the plan, known before evaluation, so there is
    /// no corresponding status and this is a plain field read.
    pub fn payload_required(&self) -> PayloadRequirement {
        self.payload_required
    }

    /// Mark metadata as having required an evaluation payload.
    pub fn set_payload_required(&mut self) -> &mut Self {
        self.payload_required = PayloadRequirement::Required;
        self.set_updated_now();
        self
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

/// Extracts a string field from a `LegacyMetadata` object.
///
/// `serde_json::Value::to_string()` *serializes*, so for a JSON string it returns the value with
/// its quotes — `"json"` rather than `json` — which matches nothing downstream. A string field
/// must therefore be read with `as_str()`. A non-string value falls back to the serialized form,
/// which is the best available answer for a caller that asked for a string.
///
/// See `specs/issues/CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON.md`.
/// Interprets a resolved media-type string as an override.
///
/// An empty string is how the previous, unwrapped field said "unspecified"; `None` is how the
/// current one does. Anything else is a deliberate override and is kept verbatim.
fn base_data_format(data_format: &str) -> &str {
    match data_format.split_once(':') {
        Some((base, _refinement)) => base,
        None => data_format,
    }
}

fn media_type_override(media_type: String) -> Option<String> {
    if media_type.trim().is_empty() {
        None
    } else {
        Some(media_type)
    }
}

fn legacy_string_field(
    o: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    o.get(key).map(|value| match value.as_str() {
        Some(text) => text.to_string(),
        None => value.to_string(),
    })
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
                // Try to extract payload_required from JSON, default to None if not present
                m.payload_required = o
                    .get("payload_required")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or(PayloadRequirement::None);
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
                if let Some(mimetype) = legacy_string_field(o, "mimetype") {
                    return mimetype;
                }
                if let Some(media_type) = legacy_string_field(o, "media_type") {
                    return media_type;
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

    /// The declared data format, if one was specified.
    ///
    /// `None` is meaningful: it says no format was chosen, so the value's own default applies.
    /// Resolve it where a value is in hand — see `State::effective_data_format`.
    pub fn declared_data_format(&self) -> Option<String> {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                legacy_string_field(o, "data_format")
            }
            Metadata::MetadataRecord(m) => m.declared_data_format().map(str::to_owned),
            Metadata::LegacyMetadata(_) => None,
        }
    }

    /// Return data format
    /// If data_format is not set, return extension
    /// If extension is not set, return "bin"
    pub fn get_data_format(&self) -> String {
        match self {
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(data_format) = legacy_string_field(o, "data_format") {
                    return data_format;
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

    /// Records an error. **Does not touch the type fields.**
    ///
    /// There is no error type: an errored state holds `V::none()`, so its identifier is the none
    /// type's, set from the value like any other state's. Overwriting the identifier here would
    /// put a type on the type axis that no value can have, and would leave `type_name` empty,
    /// which the write path refuses.
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

    /// Returns whether producing this value needed an evaluation payload.
    ///
    /// Legacy metadata without the field defaults to [`PayloadRequirement::None`], mirroring
    /// the treatment of `is_volatile`. Unlike `is_volatile` there is no status fallback: a
    /// payload requirement is a property of the plan rather than a state an asset reaches.
    pub fn payload_required(&self) -> PayloadRequirement {
        match self {
            Metadata::MetadataRecord(mr) => mr.payload_required,
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => o
                .get("payload_required")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(PayloadRequirement::None),
            Metadata::LegacyMetadata(_) => PayloadRequirement::None,
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

    /// Records that producing this value required an evaluation payload.
    ///
    /// Unlike [`Self::set_volatile`] this sets no status and no expiration: a payload requirement
    /// is a property of the plan, not a state the asset reaches. Volatility is set separately, by
    /// the command registration that declares `payload: required`.
    pub fn set_payload_required(&mut self) -> Result<&mut Self, Error> {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.set_payload_required();
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                let value = serde_json::to_value(PayloadRequirement::Required).map_err(|e| {
                    Error::general_error(format!(
                        "Failed to serialize payload_required for legacy metadata: {}",
                        e
                    ))
                })?;
                o.insert("payload_required".to_string(), value);
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut mr = MetadataRecord::new();
                mr.set_payload_required();
                *self = Metadata::MetadataRecord(mr);
                Ok(self)
            }
            Metadata::LegacyMetadata(_) => Err(Error::general_error(
                "Cannot set payload_required on unsupported legacy metadata".to_string(),
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

    /// U1 — every one of the fifteen statuses, asserted individually rather than in a loop,
    /// so a wrong bucket names itself in the failure output.
    #[test]
    fn test_read_exposure_all_statuses() {
        assert_eq!(Status::Ready.read_exposure(), ReadExposure::Value);
        assert_eq!(Status::Source.read_exposure(), ReadExposure::Value);
        assert_eq!(Status::Override.read_exposure(), ReadExposure::Value);
        assert_eq!(Status::Volatile.read_exposure(), ReadExposure::Value);

        assert_eq!(
            Status::Directory.read_exposure(),
            ReadExposure::MetadataOnly
        );
        assert_eq!(Status::Error.read_exposure(), ReadExposure::MetadataOnly);
        assert_eq!(
            Status::Cancelled.read_exposure(),
            ReadExposure::MetadataOnly
        );

        assert_eq!(Status::Expired.read_exposure(), ReadExposure::Expired);

        assert_eq!(Status::None.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Recipe.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Submitted.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Dependencies.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Processing.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Partial.read_exposure(), ReadExposure::Pending);
        assert_eq!(Status::Storing.read_exposure(), ReadExposure::Pending);
    }

    /// U2 — the exhaustiveness guard.
    ///
    /// The guarantee comes from `expected`'s match, not from the array below: adding a `Status`
    /// variant makes that match non-exhaustive, which is a compile error. The array alone would
    /// happily keep compiling, so it is not the guard — it only drives the comparison.
    #[test]
    fn test_read_exposure_guard_is_exhaustive() {
        fn expected(s: Status) -> ReadExposure {
            match s {
                Status::Ready | Status::Source | Status::Override | Status::Volatile => {
                    ReadExposure::Value
                }
                Status::Directory | Status::Error | Status::Cancelled => ReadExposure::MetadataOnly,
                Status::Expired => ReadExposure::Expired,
                Status::None
                | Status::Recipe
                | Status::Submitted
                | Status::Dependencies
                | Status::Processing
                | Status::Partial
                | Status::Storing => ReadExposure::Pending,
            }
        }

        for s in [
            Status::None,
            Status::Directory,
            Status::Recipe,
            Status::Submitted,
            Status::Dependencies,
            Status::Processing,
            Status::Partial,
            Status::Error,
            Status::Storing,
            Status::Expired,
            Status::Cancelled,
            Status::Ready,
            Status::Source,
            Status::Override,
            Status::Volatile,
        ] {
            assert_eq!(s.read_exposure(), expected(s), "wrong bucket for {:?}", s);
        }
    }

    /// U3 — `has_data()` looks like the read gate and is not: it is `true` for `Expired` and
    /// `Partial`, both of which a normal read must hide. Pinned so nobody "simplifies"
    /// `read_exposure` into a call to it.
    #[test]
    fn test_has_data_is_not_the_read_gate() {
        assert!(Status::Expired.has_data());
        assert_ne!(Status::Expired.read_exposure(), ReadExposure::Value);

        assert!(Status::Partial.has_data());
        assert_ne!(Status::Partial.read_exposure(), ReadExposure::Value);

        // For contrast: for Ready the two agree.
        assert!(Status::Ready.has_data());
        assert_eq!(Status::Ready.read_exposure(), ReadExposure::Value);
    }

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
        mr.add_dependency(DependencyRecord::new(
            DependencyKey::new("dep-a"),
            Version::new(1),
        ));
        mr.add_dependency(DependencyRecord::new(
            DependencyKey::new("dep-a"),
            Version::new(42),
        ));
        assert_eq!(mr.get_dependencies().len(), 1);
        assert_eq!(mr.get_dependencies()[0].version, Version::new(42));
    }

    #[test]
    fn test_set_dependencies_replaces_all() {
        let mut mr = MetadataRecord::new();
        mr.add_dependency(DependencyRecord::new(
            DependencyKey::new("dep-a"),
            Version::new(1),
        ));
        mr.add_dependency(DependencyRecord::new(
            DependencyKey::new("dep-b"),
            Version::new(2),
        ));
        assert_eq!(mr.get_dependencies().len(), 2);
        mr.set_dependencies(vec![DependencyRecord::new(
            DependencyKey::new("dep-c"),
            Version::new(3),
        )]);
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

    // ---- Payload requirement diagnostic surface (U6) ----

    #[test]
    fn test_metadata_record_payload_required_helper() {
        let mut mr = MetadataRecord::new();
        assert_eq!(mr.payload_required(), PayloadRequirement::None);
        mr.set_payload_required();
        assert_eq!(mr.payload_required(), PayloadRequirement::Required);
    }

    #[test]
    fn test_volatile_status_does_not_imply_payload_required() {
        // is_volatile() consults Status::Volatile; payload_required() must NOT. The two
        // concepts are related by implication in one direction only and must not be
        // conflated.
        let mut mr = MetadataRecord::new();
        mr.set_volatile();
        assert!(mr.is_volatile());
        assert_eq!(mr.payload_required(), PayloadRequirement::None);
    }

    #[test]
    fn test_asset_info_round_trip_preserves_payload_required() {
        // Guards the two copy sites: get_asset_info() and From<AssetInfo> for MetadataRecord.
        let mut mr = MetadataRecord::new();
        mr.set_payload_required();
        let asset_info = mr.get_asset_info();
        assert_eq!(asset_info.payload_required, PayloadRequirement::Required);
        let back: MetadataRecord = asset_info.into();
        assert_eq!(back.payload_required(), PayloadRequirement::Required);
    }

    #[test]
    fn test_metadata_record_payload_required_serialization(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mr = MetadataRecord::new();
        let json = serde_json::to_string(&mr)?;
        assert!(
            !json.contains("payload_required"),
            "None must be skipped, got: {}",
            json
        );

        let mut mr = MetadataRecord::new();
        mr.set_payload_required();
        let json = serde_json::to_string(&mr)?;
        let back: MetadataRecord = serde_json::from_str(&json)?;
        assert_eq!(back.payload_required(), PayloadRequirement::Required);
        Ok(())
    }

    #[test]
    fn test_metadata_record_without_payload_field_deserializes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // A record serialized before this field existed must still load.
        let mut mr = MetadataRecord::new();
        let mut value = serde_json::to_value(&mr)?;
        if let serde_json::Value::Object(ref mut o) = value {
            o.remove("payload_required");
        }
        let back: MetadataRecord = serde_json::from_value(value)?;
        assert_eq!(back.payload_required(), PayloadRequirement::None);
        mr.set_payload_required();
        Ok(())
    }

    #[test]
    fn test_legacy_metadata_payload_required_defaults_to_none() {
        // Legacy JSON object with no payload_required key.
        let m = Metadata::LegacyMetadata(serde_json::json!({"status": "ready"}));
        assert_eq!(m.payload_required(), PayloadRequirement::None);

        // Non-object legacy metadata.
        let m = Metadata::LegacyMetadata(serde_json::json!("something"));
        assert_eq!(m.payload_required(), PayloadRequirement::None);
    }

    #[test]
    fn test_legacy_metadata_payload_required_is_read_when_present() {
        let m = Metadata::LegacyMetadata(serde_json::json!({"payload_required": "Required"}));
        assert_eq!(m.payload_required(), PayloadRequirement::Required);
    }

    #[test]
    fn test_metadata_enum_payload_required_from_record() {
        let mut mr = MetadataRecord::new();
        mr.set_payload_required();
        let m = Metadata::MetadataRecord(mr);
        assert_eq!(m.payload_required(), PayloadRequirement::Required);
    }

    /// `vts5.5` — `LegacyMetadata` accessors must extract strings, not serialize them.
    ///
    /// `serde_json::Value::to_string()` returns `"\"json\""` for a JSON string, which matches no
    /// data format and no media type. Regression test for
    /// `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`. The legacy variant is constructed directly,
    /// because a partial document no longer *reaches* it — see `vts5.6`.
    #[test]
    fn legacy_accessors_return_unquoted_strings() -> Result<(), Box<dyn std::error::Error>> {
        let legacy = |json: serde_json::Value| Metadata::LegacyMetadata(json);

        assert_eq!(
            legacy(serde_json::json!({"media_type": "text/plain"})).get_media_type(),
            "text/plain"
        );
        assert_eq!(
            legacy(serde_json::json!({"mimetype": "text/csv"})).get_media_type(),
            "text/csv"
        );
        assert_eq!(
            legacy(serde_json::json!({"data_format": "json"})).get_data_format(),
            "json"
        );

        // The accessors that already destructured `Value::String` must keep working.
        let identifiers = legacy(serde_json::json!({
            "type_identifier": "Text", "type_name": "text"
        }));
        assert_eq!(identifiers.type_identifier()?, "Text");
        assert_eq!(identifiers.type_name()?, "text");
        Ok(())
    }

    /// A legacy document carrying fields the record does not know stays legacy, and keeps them.
    ///
    /// Regression test for a defect found in review of PR #37: `#[serde(default)]` alone made
    /// almost any JSON object deserialize as a record, and serde drops unrecognised fields
    /// silently — so a legacy document was converted and lost its extra data on the next write.
    #[test]
    fn legacy_document_with_unknown_fields_is_preserved() -> Result<(), Box<dyn std::error::Error>>
    {
        let metadata = Metadata::from_json(r#"{"media_type":"text/plain","custom":{"a":1}}"#)?;
        match &metadata {
            Metadata::LegacyMetadata(value) => {
                assert!(
                    value.get("custom").is_some(),
                    "the unknown field must survive: {value}"
                );
            }
            Metadata::MetadataRecord(_) => {
                panic!("a document with unknown fields must not be converted to a record")
            }
        }
        assert!(
            metadata.to_json()?.contains("custom"),
            "and must survive a round trip"
        );
        // The accessor still reads correctly through the legacy branch.
        assert_eq!(metadata.get_media_type(), "text/plain");
        Ok(())
    }

    /// `vts5.6` — a partial document deserializes into a record, not into the legacy branch.
    ///
    /// This is the root cause behind `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON`: a document that
    /// did not deserialize as a *complete* `MetadataRecord` fell through to `LegacyMetadata`, so
    /// any caller supplying partial metadata silently landed on the quoted-string accessors.
    #[test]
    fn partial_document_deserializes_into_a_record() -> Result<(), Box<dyn std::error::Error>> {
        let metadata = Metadata::from_json(r#"{"media_type":"text/plain"}"#)?;
        match &metadata {
            Metadata::MetadataRecord(record) => {
                assert_eq!(record.declared_media_type(), Some("text/plain"));
            }
            Metadata::LegacyMetadata(_) => {
                panic!("a partial document must deserialize into a record")
            }
        }
        assert_eq!(metadata.get_media_type(), "text/plain");
        Ok(())
    }

    /// A non-string value has no `as_str()`; falling back to the serialized form is the best
    /// available answer and must not panic.
    #[test]
    fn legacy_non_string_field_falls_back() -> Result<(), Box<dyn std::error::Error>> {
        let numeric = Metadata::LegacyMetadata(serde_json::json!({"data_format": 7}));
        assert_eq!(numeric.get_data_format(), "7");
        Ok(())
    }

    /// `vts5.1` — an absent data format is a distinguishable state, not a missing value.
    #[test]
    fn declared_data_format_distinguishes_none() {
        let mut record = MetadataRecord::new();
        assert_eq!(record.declared_data_format(), None);
        record.data_format = Some("csv".to_string());
        assert_eq!(record.declared_data_format(), Some("csv"));
    }

    /// `vts5.2` — resolution falls through to the value's own default, not to a constant.
    #[test]
    fn effective_data_format_uses_value_default() {
        let mut record = MetadataRecord::new();
        assert_eq!(record.effective_data_format("txt"), "txt");
        assert_eq!(record.effective_data_format("png"), "png");
        record.data_format = Some("csv".to_string());
        assert_eq!(
            record.effective_data_format("txt"),
            "csv",
            "a declared format wins over the value default"
        );
    }

    /// Level-2 seeding: a filename writes the extension into `data_format`, and does not touch
    /// the media type — writing it would make an ordinary filename look like an override.
    #[test]
    fn filename_seeds_the_data_format_only() {
        let mut record = MetadataRecord::new();
        record.set_filename("notes.csv");
        assert_eq!(record.declared_data_format(), Some("csv"));
        assert_eq!(
            record.declared_media_type(),
            None,
            "a filename is level 2, not a level-3 media-type override"
        );

        // A declared format is not overwritten by a later filename.
        let mut declared = MetadataRecord::new();
        declared.data_format = Some("csv:comma".to_string());
        declared.set_filename("notes.csv");
        assert_eq!(declared.declared_data_format(), Some("csv:comma"));
    }

    /// `vts5.3` and `vts5.4` — an override is verbatim, absence derives.
    #[test]
    fn effective_media_type_prefers_the_override() {
        let mut record = MetadataRecord::new();
        record.data_format = Some("csv".to_string());
        assert_eq!(record.effective_media_type("txt"), "text/csv");

        record.with_media_type("text/plain".to_string());
        assert_eq!(
            record.effective_media_type("txt"),
            "text/plain",
            "a declared override is never re-derived"
        );

        // A refinement derives from its base.
        let mut refined = MetadataRecord::new();
        refined.data_format = Some("csv:comma".to_string());
        assert_eq!(refined.effective_media_type("txt"), "text/csv");
    }

    /// An empty string is not an override — it is how the previously unwrapped field said
    /// "unspecified", and it must keep meaning that.
    #[test]
    fn empty_media_type_is_not_an_override() {
        let mut record = MetadataRecord::new();
        record.with_media_type(String::new());
        assert_eq!(record.declared_media_type(), None);
    }
}
