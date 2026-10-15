//! Write strategies of the metadata pipeline (`specs/design/store-adapters/`, Phase 2 §5).
//!
//! A write is a [`WriteSet`]: the data object and the objects the storage model planned. A
//! [`WriteStrategy`] commits it — in order, atomically per object, or all-or-nothing through a
//! journal — and recovers what an interrupted commit left behind.

use crate::error::Error;
use crate::query::Key;

use super::AsyncStore;

/// Which store an [`ObjectWrite`] goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteTarget {
    /// The backend's data object.
    Data,
    /// An object of the storage model, in the store the model names (the backend unless the model
    /// keeps its own).
    Metadata,
    /// The backend's own metadata of the key, through `set_metadata` (bytes are the JSON record)
    /// or, for a delete, `remove`.
    Native,
    /// An object of a member's backup storage model, by its index in [`TargetStores::backups`].
    Backup(usize),
}

/// One object to write; `bytes: None` deletes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectWrite {
    pub target: WriteTarget,
    pub key: Key,
    pub bytes: Option<Vec<u8>>,
}

impl ObjectWrite {
    pub fn put(target: WriteTarget, key: Key, bytes: Vec<u8>) -> Self {
        ObjectWrite {
            target,
            key,
            bytes: Some(bytes),
        }
    }

    pub fn delete(target: WriteTarget, key: Key) -> Self {
        ObjectWrite {
            target,
            key,
            bytes: None,
        }
    }
}

/// The stores the targets of a [`WriteSet`] resolve to.
pub struct TargetStores<'a> {
    /// The backend: data objects, native metadata, and the strategy's own staging and journal.
    pub data: &'a dyn AsyncStore,
    /// Where the storage model keeps its objects.
    pub metadata: &'a dyn AsyncStore,
    /// The members' backup stores, under central storage.
    pub backups: Vec<&'a dyn AsyncStore>,
}

impl<'a> TargetStores<'a> {
    /// Every target in one store.
    pub fn single(store: &'a dyn AsyncStore) -> Self {
        TargetStores {
            data: store,
            metadata: store,
            backups: Vec::new(),
        }
    }

    /// The store a target resolves to.
    pub fn store_for(&self, target: WriteTarget) -> Result<&'a dyn AsyncStore, Error> {
        match target {
            WriteTarget::Data | WriteTarget::Native => Ok(self.data),
            WriteTarget::Metadata => Ok(self.metadata),
            WriteTarget::Backup(index) => self.backups.get(index).copied().ok_or_else(|| {
                Error::unexpected_error(format!("No backup store with index {index}"))
            }),
        }
    }
}

/// Everything one store operation writes.
#[derive(Debug, Clone, Default)]
pub struct WriteSet {
    /// The key the operation is about; names the journal.
    pub primary: Key,
    pub objects: Vec<ObjectWrite>,
}
