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
    /// Keeps the metadata's type fields agreeing with the value it describes.
    ///
    /// This is level-1 of the seeding cascade, and it deliberately writes only the *type* fields.
    /// The data format is left alone: an absent `data_format` means "no format was specified, so
    /// the value's own default applies", and writing the default in would destroy that
    /// distinction — nobody could then tell a deliberate choice from a fall-through. Resolution
    /// happens where the value is in hand, through [`State::effective_data_format`].
    ///
    /// An **error state keeps its `error` identifier**. `Metadata::with_error` sets it, and this
    /// helper used to overwrite it from `V::none()` immediately afterwards in `from_error`, so the
    /// same situation reached the store under two different identifiers depending on which path
    /// built it.
    /// Makes the metadata's type fields describe the value it accompanies.
    ///
    /// Applies to an **errored** state too, which holds `V::none()` and is therefore typed like
    /// any other none-valued state. This used to return early for error states, to stop `"None"`
    /// overwriting the `"error"` identifier that `Metadata::with_error` set — and since nothing
    /// else ever set `type_name`, an errored state came out with an empty one, which the write
    /// path refuses. Removing the error type removed the reason for the guard.
    fn sync_metadata_with_value(metadata: &mut Metadata, value: &V) {
        metadata.with_type_identifier(value.identifier().to_string());
        metadata.with_type_name(value.type_name().to_string());
    }

    /// The data format this state will serialize to.
    ///
    /// Resolves the seeding cascade with the value in hand: a declared format wins, otherwise the
    /// value's own default applies.
    pub fn effective_data_format(&self) -> String {
        match self.metadata.declared_data_format() {
            Some(declared) => declared,
            None => self.data.default_data_format().to_string(),
        }
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
        // Cancellation is a status, not a stored error: synthesize a typed cancellation error.
        if self.status() == Status::Cancelled {
            let msg = self.message();
            return Some(if msg.is_empty() {
                Error::cancelled("Asset was cancelled")
            } else {
                Error::cancelled(msg.to_string())
            });
        }
        // A computed error is recorded in the metadata (is_error / error_data), whether the
        // status was explicitly set to Error (asset path) or only the metadata was flagged
        // (e.g. `State::from_error`). Non-error states — including non-terminal intermediate
        // states and success-with-none — return None and allow value extraction.
        if let Err(e) = self.metadata.error_result() {
            return Some(e);
        }
        None
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
        self.data.as_bytes(&self.effective_data_format())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::metadata::MetadataRecord;
    use crate::value::Value;

    /// `vts6.1` — every constructor leaves the type fields agreeing with the value.
    #[test]
    fn every_constructor_syncs_the_type_fields() -> Result<(), Box<dyn std::error::Error>> {
        let text = Value::Text("hello".to_string());

        let new_state: State<Value> = State::new();
        assert_eq!(new_state.metadata.type_identifier()?, "None");

        let with_data = State::<Value>::new().with_data(text.clone());
        assert_eq!(with_data.metadata.type_identifier()?, "Text");
        assert_eq!(with_data.metadata.type_name()?, "text");

        let from_value =
            State::from_value_and_metadata(text.clone(), Arc::new(Metadata::new()));
        assert_eq!(from_value.metadata.type_identifier()?, "Text");

        let with_metadata = State::<Value>::new()
            .with_data(text)
            .with_metadata(Metadata::new());
        assert_eq!(with_metadata.metadata.type_identifier()?, "Text");
        Ok(())
    }

    /// `vts8.9` — an error state is typed by the value it holds, which is none.
    ///
    /// There is no `error` type. The type axis says what a value *is*, and "failed" is not
    /// something a value can be — it is a metadata property (`is_error`, `Status::Error`,
    /// `error_data`). An errored state holds `V::none()`, so it reports the none type, and the
    /// intent it failed to produce survives in the query, key and filename rather than on the
    /// type axis.
    ///
    /// This previously asserted the opposite. `Metadata::with_error` set the identifier to
    /// `"error"` and nothing set `type_name`, so an errored state reached the store with an empty
    /// name and `validate_required_fields` refused it — the whole class of defect disappears with
    /// the error type.
    #[test]
    fn from_error_is_typed_as_none() -> Result<(), Box<dyn std::error::Error>> {
        let state: State<Value> = State::from_error(Error::general_error("boom".to_string()));

        assert_eq!(state.metadata.type_identifier()?, "None");
        assert_eq!(
            state.metadata.type_name()?,
            "none",
            "both halves of the type are set, which is what the write path requires"
        );
        assert!(
            state.metadata.is_error()?,
            "and the error itself is recorded in the metadata"
        );
        assert!(state.value_error().is_some(), "so the state reports as failed");

        // The same situation reached through a value rather than the error constructor agrees.
        let mut equivalent = Metadata::new();
        equivalent.with_error(Error::general_error("boom".to_string()));
        let equivalent: State<Value> = State::new().with_metadata(equivalent);
        assert_eq!(
            equivalent.metadata.type_identifier()?,
            state.metadata.type_identifier()?,
            "both routes to an errored none-valued state name the same type"
        );
        Ok(())
    }

    /// `vts6.2` — a declared data format survives, and an absent one stays absent.
    ///
    /// Level 1 is a *resolution*, not a write: seeding the value's default into the field would
    /// destroy the distinction between a deliberate choice and a fall-through.
    #[test]
    fn level_one_resolves_without_writing() -> Result<(), Box<dyn std::error::Error>> {
        let state = State::<Value>::new().with_data(Value::Text("hello".to_string()));
        assert_eq!(
            state.metadata.declared_data_format(),
            None,
            "level 1 must not write the default into the field"
        );
        assert_eq!(state.effective_data_format(), "txt");

        let mut declared = MetadataRecord::new();
        declared.data_format = Some("json".to_string());
        let declared_state = State::<Value>::new()
            .with_data(Value::Text("hello".to_string()))
            .with_metadata(Metadata::MetadataRecord(declared));
        assert_eq!(declared_state.effective_data_format(), "json");
        Ok(())
    }
}
