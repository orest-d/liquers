//! Shared test fixtures for `liquers-core` integration suites.
//!
//! Reached with `mod fixtures;` from a test file. This directory also holds data files
//! (`commands.yaml`, the query corpora); this is its first Rust module.

#![allow(dead_code)] // each consumer uses a subset

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    error::Error,
    metadata::Metadata,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
};

/// A copy of selected store entries, taken from one environment and replayed into another.
///
/// **This is how a restart is simulated, and it is deliberately not a shared store.** The asset
/// manager is the system's synchronization mechanism; the store is not equipped for that, so two
/// environments over one live store is not a scenario the system supports and a fixture for it
/// would exercise a coordination point that does not exist. A second *process* does not share a
/// live store object either — it reads persisted bytes, which is exactly what replaying a snapshot
/// into a fresh store models.
///
/// The technique was proven inline in
/// `expiration_integration::test_get_any_status_and_to_override_from_store_only`; it is lifted here
/// because a third design has now needed it (`CROSS-PROCESS-RELOAD-IS-UNTESTED`).
pub struct StoreSnapshot {
    entries: Vec<(Key, Vec<u8>, Metadata)>,
}

impl StoreSnapshot {
    /// Read `keys` out of `store`. A key the store does not hold is skipped rather than failing:
    /// a snapshot is a picture of what was persisted, and "nothing was persisted" is a legitimate
    /// thing for a test to capture and assert on.
    pub async fn capture(store: &Arc<dyn AsyncStore>, keys: &[Key]) -> Result<Self, Error> {
        let mut entries = Vec::new();
        for key in keys {
            if store.contains(key).await? {
                let (bytes, metadata) = store.get(key).await?;
                entries.push((key.clone(), bytes, metadata));
            }
        }
        Ok(Self { entries })
    }

    /// Replay into a fresh store. Use with an environment built after the first one is dropped,
    /// so the second manager and dependency manager start genuinely empty.
    pub async fn replay_into(&self, store: &AsyncMemoryStore) -> Result<(), Error> {
        for (key, bytes, metadata) in &self.entries {
            store.set(key, bytes, metadata).await?;
        }
        Ok(())
    }

    /// Merge another snapshot in. Useful when entries must be captured at different moments —
    /// a dependent before its dependency expires, the dependency after.
    pub fn absorb(&mut self, other: StoreSnapshot) {
        self.entries.extend(other.entries);
    }

    /// Rewrite every dependency record in every captured entry to `Version::unknown()`, the way
    /// records written before computed assets carried versions look on disk. Used to prove the
    /// versions change is safe to deploy against a store that predates it.
    pub fn downgrade_dependency_versions_to_unknown(&mut self) {
        for (_key, _bytes, metadata) in self.entries.iter_mut() {
            if let Metadata::MetadataRecord(mr) = metadata {
                for record in mr.dependencies.iter_mut() {
                    record.version = liquers_core::metadata::Version::unknown();
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An `AsyncMemoryStore` that counts `get_metadata` calls, for asserting that a code path did
/// **not** read the store.
///
/// **Size a wrapper like this by compiling, not by counting required methods.** `AsyncStore` has
/// two required methods, but its other twenty defaults are not forwarding defaults — `set`'s
/// default is `Err(key_not_supported)`. A wrapper overriding only the required pair compiles and
/// then fails every write.
#[derive(Clone)]
pub struct CountingStore {
    inner: Arc<AsyncMemoryStore>,
    pub metadata_reads: Arc<AtomicUsize>,
}

impl CountingStore {
    pub fn new(inner: AsyncMemoryStore) -> Self {
        Self {
            inner: Arc::new(inner),
            metadata_reads: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn reads(&self) -> usize {
        self.metadata_reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AsyncStore for CountingStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.metadata_reads.fetch_add(1, Ordering::SeqCst);
        self.inner.get_metadata(key).await
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.inner.set(key, data, metadata).await
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.inner.remove(key).await
    }
}
