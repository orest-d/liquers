use std::{borrow::Cow, sync::Arc};

use crate::{
    error::Error,
    metadata::{AssetInfo, Metadata, Status},
    value::ValueInterface,
};

/// State encapsulates the data (Value) and metadata (Metadata) of a value.
/// It is typically used to represent the result of an evaluation.
/// State is meant to be chached and shared, therefore it should be considered as read-only.
///  It is thread-safe and can be cloned.
#[derive(Debug)]
pub struct State<V: ValueInterface> {
    // TODO: try to remove rwlock
    // `data` is private: a State is always potentially an error/cancelled state, so value
    // extraction must go through the guarded accessors (`value`, `value_state`,
    // `try_into_string`, `as_bytes`). Use `data_unchecked()` only to forward/inspect a
    // terminal state without extracting a value (delegation copy, UI rendering).
    data: Arc<V>,
    pub metadata: Arc<Metadata>,
}

impl<V: ValueInterface> State<V> {
    fn sync_metadata_with_value(metadata: &mut Metadata, value: &V) {
        metadata.with_type_identifier(value.identifier().to_string());
        metadata.with_type_name(value.type_name().to_string());
    }

    /// Creates a new State with an empty value and default metadata.
    pub fn new() -> State<V> {
        let data = Arc::new(V::none());
        let mut metadata = Metadata::new();
        Self::sync_metadata_with_value(&mut metadata, &data);
        State {
            data,
            metadata: Arc::new(metadata),
        }
    }
    /// Creates a State directly from an already-shared value handle and metadata, without
    /// syncing type identifiers. Low-level constructor for the asset layer (e.g. building a
    /// terminal error/none state from stored metadata). Prefer `from_value_and_metadata` for
    /// value states that should have their type info synced.
    pub fn from_parts(data: Arc<V>, metadata: Arc<Metadata>) -> State<V> {
        State { data, metadata }
    }

    /// Creates a new State with the given value and metadata.
    pub fn from_value_and_metadata(value: V, metadata: Arc<Metadata>) -> State<V> {
        let data = Arc::new(value);
        let mut metadata_value = (*metadata).clone();
        Self::sync_metadata_with_value(&mut metadata_value, &data);
        State {
            data,
            metadata: Arc::new(metadata_value),
        }
    }

    pub fn with_metadata(self, mut metadata: Metadata) -> Self {
        Self::sync_metadata_with_value(&mut metadata, &self.data);
        State {
            data: self.data,
            metadata: Arc::new(metadata),
        }
    }

    /// Sets the status in metadata.
    /// Avoid this method, since it creates a copy of the metadata with a changed status.
    pub fn set_status(&mut self, status: Status) -> Result<(), Error> {
        let mut metadata = (*self.metadata).clone();
        metadata.set_status(status)?;
        self.metadata = Arc::new(metadata);
        Ok(())
    }

    /// Creates a new State with the given error and default metadata.
    pub fn from_error(error: Error) -> Self {
        let mut metadata = Metadata::new();
        metadata.with_error(error);
        let data = Arc::new(V::none());
        Self::sync_metadata_with_value(&mut metadata, &data);
        State {
            data,
            metadata: Arc::new(metadata),
        }
    }

    pub fn with_data(self, value: V) -> Self {
        let mut metadata = (*self.metadata).clone();
        Self::sync_metadata_with_value(&mut metadata, &value);
        State {
            data: Arc::new(value),
            metadata: Arc::new(metadata),
        }
    }

    pub fn with_string(&self, text: &str) -> Self {
        self.clone().with_data(V::new(text))
    }
    /// The single "can I take a value from this state?" gate.
    /// Returns `None` if the state carries an extractable value (`status().has_data()`);
    /// otherwise the typed error that value extraction should yield:
    /// - `Status::Cancelled` → a synthesized `Error::cancelled` (`ErrorType::Cancelled`);
    /// - `Status::Error` (or any other non-data terminal) → the stored computed error if any,
    ///   else a generic "no value" error.
    ///
    /// NOTE: this is intentionally not `error_result()`. A cancelled state has `is_error ==
    /// false`, so its `error_result()` is `Ok`; value extraction must consult the status.
    pub fn value_error(&self) -> Option<Error> {
        match self.status() {
            // Failure statuses: extracting a value is an error.
            Status::Error => Some(match self.metadata.error_result() {
                Err(e) => e,
                // Error status without a stored error (should not happen): synthesize one.
                Ok(()) => Error::general_error("Asset finished with an error".to_string()),
            }),
            Status::Cancelled => {
                let msg = self.message();
                if msg.is_empty() {
                    Some(Error::cancelled("Asset was cancelled"))
                } else {
                    Some(Error::cancelled(msg.to_string()))
                }
            }
            // All other statuses carry (or may legitimately carry) an extractable value,
            // including non-terminal intermediate states produced during evaluation and
            // success-with-none. Value extraction is allowed.
            Status::None
            | Status::Directory
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Storing
            | Status::Ready
            | Status::Expired
            | Status::Source
            | Status::Override
            | Status::Volatile => None,
        }
    }

    /// Validating projection: `Ok(self)` if this is a value-bearing state, otherwise the typed
    /// error from [`Self::value_error`]. Ergonomic terminal-value path: `asset.get().await?.value_state()?`.
    pub fn value_state(self) -> Result<Self, Error> {
        match self.value_error() {
            Some(e) => Err(e),
            None => Ok(self),
        }
    }

    /// Error-checked value accessor: `Err` on an error/cancelled state (via [`Self::value_error`]),
    /// else a cheap clone of the shared value handle.
    pub fn value(&self) -> Result<Arc<V>, Error> {
        match self.value_error() {
            Some(e) => Err(e),
            None => Ok(self.data.clone()),
        }
    }

    /// Raw, UNCHECKED access to the underlying value handle. Use only to forward/inspect a
    /// terminal state without extracting a value (delegation copy, UI rendering); prefer
    /// [`Self::value`]/[`Self::value_state`] everywhere else.
    pub fn data_unchecked(&self) -> &Arc<V> {
        &self.data
    }

    pub fn as_bytes(&self) -> Result<Vec<u8>, Error> {
        if let Some(e) = self.value_error() {
            return Err(e);
        }
        self.data.as_bytes(&self.metadata.get_data_format())
    }
    pub fn is_none(&self) -> bool {
        self.data.is_none()
    }
    pub fn try_into_string(&self) -> Result<String, Error> {
        if let Some(e) = self.value_error() {
            return Err(e);
        }
        self.data.try_into_string()
    }
    /// Checks metadata for error.
    pub fn is_error(&self) -> Result<bool, Error> {
        (*self.metadata).is_error()
    }
    /// Convinience method to get file extension from metadata.
    pub fn extension(&self) -> String {
        if let Some(ext) = (*self.metadata).extension() {
            ext
        } else {
            self.data.default_extension().to_string()
        }
    }

    /// Get type identifier from data.
    pub fn type_identifier(&self) -> Cow<'static, str> {
        self.data.identifier()
    }

    /// Get the data format
    /// Wrapper for metadata.get_data_format()
    pub fn get_data_format(&self) -> String {
        (*self.metadata).get_data_format()
    }

    /// Wrapper for metadata.error_result()
    pub fn error_result(&self) -> Result<(), Error> {
        self.metadata.error_result()
    }

    /// Get status from metadata.
    pub fn status(&self) -> Status {
        self.metadata.status()
    }

    /// Get message from metadata.
    pub fn message(&self) -> &str {
        self.metadata.message()
    }

    /// Get unicode icon from metadata.
    pub fn unicode_icon(&self) -> &str {
        self.metadata.unicode_icon()
    }

    /// Get file size from metadata.
    pub fn file_size(&self) -> Option<u64> {
        self.metadata.file_size()
    }

    /// Get asset info from metadata.
    pub fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        self.metadata.get_asset_info()
    }

    /// Serialize data to bytes with the given data format.
    pub fn as_bytes_with_data_format(&self, data_format: &str) -> Result<Vec<u8>, Error> {
        if let Some(e) = self.value_error() {
            return Err(e);
        }
        self.data.as_bytes(data_format)
    }
}

impl<V: ValueInterface> Default for State<V> {
    fn default() -> Self {
        Self::new()
    }
}
impl<V: ValueInterface> Clone for State<V> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            metadata: self.metadata.clone(),
        }
    }
}
/*
impl<V: ValueInterface> ToOwned for State<V> {
    type Owned = State<V>;

    fn to_owned(&self) -> Self::Owned {
        State{data:self.data.clone(), metadata:self.metadata.clone()}
    }
}
*/

impl<V: ValueInterface> From<Result<State<V>, Error>> for State<V> {
    fn from(result: Result<State<V>, Error>) -> Self {
        match result {
            Ok(state) => state,
            Err(e) => {
                let mut metadata = Metadata::new();
                metadata.with_error(e);
                State {
                    data: Arc::new(V::none()),
                    metadata: Arc::new(metadata),
                }
            }
        }
    }
}
