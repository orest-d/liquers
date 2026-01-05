use std::{borrow::Cow, sync::Arc};

use crate::{error::Error, metadata::{AssetInfo, Metadata, Status}, value::ValueInterface};

/// State encapsulates the data (Value) and metadata (Metadata) of a value.
/// It is typically used to represent the result of an evaluation.
/// State is meant to be chached and shared, therefore it should be considered as read-only.
///  It is thread-safe and can be cloned.
#[derive(Debug)]
pub struct State<V: ValueInterface> {
    // TODO: remove pub
    // TODO: try to remove rwlock
    pub data: Arc<V>,
    pub metadata: Arc<Metadata>,
}

impl<V: ValueInterface> State<V> {
    /// Creates a new State with an empty value and default metadata.
    pub fn new() -> State<V> {
        State {
            data: Arc::new(V::none()),
            metadata: Arc::new(Metadata::new()),
        }
    }
    /// Creates a new State with the given value and metadata.
    pub fn from_value_and_metadata(value: V, metadata: Arc<Metadata>) -> State<V> {
        State {
            data: Arc::new(value),
            metadata,
        }
    }

    pub fn with_metadata(self, mut metadata: Metadata) -> Self {
        metadata.with_type_identifier(self.data.identifier().to_string());
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
        State {
            data: Arc::new(V::none()),
            metadata: Arc::new(metadata),
        }
    }
    
    pub fn with_data(self, value: V) -> Self {
        State {
            data: Arc::new(value),
            metadata: self.metadata,
        }
    }

    pub fn with_string(&self, text: &str) -> Self {
        let mut metadata = (*self.metadata).clone();
        metadata.with_type_identifier("text".to_owned());
        State {
            data: Arc::new(V::new(text)),
            metadata: Arc::new(metadata),
        }
    }
    pub fn as_bytes(&self) -> Result<Vec<u8>, Error> {
        self.data.as_bytes(&self.metadata.get_data_format())
    }
    pub fn is_none(&self) -> bool {
        self.data.is_none()
    }
    pub fn try_into_string(&self) -> Result<String, Error> {
        self.data.try_into_string()
    }
    /// Checks metadata for error.
    pub fn is_error(&self) -> Result<bool, Error> {
        (*self.metadata).is_error()
    }
    /// Convinience method to get file extension from metadata.
    pub fn extension(&self) -> String {
        if let Some(ext) = (*self.metadata).extension(){
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

impl<V:ValueInterface> From<Result<State<V>, Error>> for State<V> {
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
