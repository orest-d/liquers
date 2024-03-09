use std::sync::Arc;

use crate::{metadata::Metadata, value::ValueInterface};

#[derive(Debug)]
pub struct State<V: ValueInterface> {
    pub data: Arc<V>,
    pub metadata: Arc<Metadata>,
}

impl<V: ValueInterface> State<V> {
    pub fn new() -> State<V> {
        State {
            data: Arc::new(V::none()),
            metadata: Arc::new(Metadata::new()),
        }
    }
    pub fn from_value_and_metadata(value:V, metadata:Arc<Metadata>) -> State<V> {
        State {
            data: value.into(),
            metadata: metadata,
        }
    }

    pub fn with_metadata(&self, metadata: Metadata) -> Self {
        State {
            data: self.data.clone(),
            metadata: Arc::new(metadata),
        }
    }
    pub fn with_data(&self, value: V) -> Self {
        State {
            data: Arc::new(value),
            metadata: Arc::new((*self.metadata).clone()),
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
    pub fn is_empty(&self) -> bool {
        (*self.data).is_none()
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
