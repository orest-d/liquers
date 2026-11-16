//! The first-class roots: [`MemoryRoot`] (a variable), [`StoreRoot`] (data in the store),
//! [`KeyRoot`] (a keyed asset) and [`AssetRefRoot`] (any asset, read-only).

use std::sync::{Arc, Mutex};

use super::{
    AccessorHub, AccessorInfo, AccessorRoot, Capabilities, ExpiryPolicy, ValueAccessor,
};
use crate::error::Error;
use crate::metadata::{Status, Version};
use crate::query::Query;
use crate::state::State;
use crate::value::ValueInterface;

/// A variable in memory: a value, or none, and a counter version. Share it by cloning the
/// accessor [`memory_accessor`] returns.
#[derive(Debug)]
pub struct MemoryRoot<V: ValueInterface> {
    value: Mutex<Option<(State<V>, Version)>>,
}

impl<V: ValueInterface> MemoryRoot<V> {
    pub fn new(initial: Option<State<V>>) -> Self {
        MemoryRoot {
            value: Mutex::new(initial.map(|s| (s, Version::new_unique()))),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<V: ValueInterface> AccessorRoot<V> for MemoryRoot<V> {
    async fn read(&self) -> Result<Option<(State<V>, Version)>, Error> {
        self.value
            .lock()
            .map(|v| v.clone())
            .map_err(|_| Error::unexpected_error("memory accessor lock poisoned".into()))
    }
    async fn write(&self, mut state: State<V>) -> Result<(State<V>, Version), Error> {
        state.set_status(Status::Source)?;
        let version = Version::new_unique();
        let mut value = self
            .value
            .lock()
            .map_err(|_| Error::unexpected_error("memory accessor lock poisoned".into()))?;
        *value = Some((state.clone(), version));
        Ok((state, version))
    }
    async fn info(&self) -> Result<AccessorInfo, Error> {
        let present = self.value.lock().map(|v| v.is_some()).unwrap_or(false);
        Ok(AccessorInfo::with_status(if present {
            Status::Source
        } else {
            Status::None
        }))
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: true,
            notify: true,
        }
    }
    fn not_found(&self) -> Error {
        let mut error = Error::not_available();
        error.message = "the memory accessor holds no value".to_string();
        error
    }
}

/// A variable in memory, empty when `initial` is `None` (status `None` until the first set).
/// Clones of the returned `Arc` share it.
pub fn memory_accessor<V: ValueInterface>(
    initial: Option<State<V>>,
    address: Query,
) -> Arc<dyn ValueAccessor<V>> {
    AccessorHub::new(
        Arc::new(MemoryRoot::new(initial)),
        address,
        ExpiryPolicy::Relay,
    )
}
