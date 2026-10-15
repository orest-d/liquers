//! Store decorators for tests of the store adapters (`specs/design/store-adapters/`).
//!
//! [`RecordingStore`] logs every fallible call it forwards; [`FailingStore`] fails its *n*-th
//! data-only call, to interrupt a multi-step write at a chosen point. Both wrap a shared store and
//! otherwise forward unchanged. Compiled for unit tests and, public, with the `store-conformance`
//! feature so integration tests can use them.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::{AsyncStore, DataCapabilities, DataFacts, SyncReport};
use crate::error::Error;
use crate::metadata::{self, AssetInfo, Metadata, MetadataRecord};
use crate::query::Key;

/// One call a [`RecordingStore`] forwarded: the method name and the key it named (`to` keys of
/// `rename_data` are recorded as a second key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCall {
    pub method: &'static str,
    pub keys: Vec<Key>,
}

/// Implements every [`AsyncStore`] method of `$ty` by forwarding to `self.inner`, calling
/// `self.before_call(method, keys, data_only)` ahead of each fallible method.
macro_rules! forward_with_hook {
    ($ty:ty) => {
        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        impl AsyncStore for $ty {
            fn store_name(&self) -> String {
                self.inner.store_name()
            }
            fn key_prefix(&self) -> Key {
                self.inner.key_prefix()
            }
            fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
                self.inner.default_metadata(key, is_dir)
            }
            fn finalize_metadata(
                &self,
                metadata: &mut Metadata,
                key: &Key,
                data: &[u8],
                update: bool,
            ) {
                self.inner.finalize_metadata(metadata, key, data, update)
            }
            fn finalize_metadata_empty(
                &self,
                metadata: &mut Metadata,
                key: &Key,
                is_dir: bool,
                update: bool,
            ) {
                self.inner
                    .finalize_metadata_empty(metadata, key, is_dir, update)
            }
            async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
                self.before_call("get", &[key], false)?;
                self.inner.get(key).await
            }
            async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
                self.before_call("get_bytes", &[key], false)?;
                self.inner.get_bytes(key).await
            }
            async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
                self.before_call("get_metadata", &[key], false)?;
                self.inner.get_metadata(key).await
            }
            async fn get_asset_info(&self, key: &Key) -> Result<metadata::AssetInfo, Error> {
                self.before_call("get_asset_info", &[key], false)?;
                self.inner.get_asset_info(key).await
            }
            async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
                self.before_call("set", &[key], false)?;
                self.inner.set(key, data, metadata).await
            }
            async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
                self.before_call("set_metadata", &[key], false)?;
                self.inner.set_metadata(key, metadata).await
            }
            async fn remove(&self, key: &Key) -> Result<(), Error> {
                self.before_call("remove", &[key], false)?;
                self.inner.remove(key).await
            }
            async fn removedir(&self, key: &Key) -> Result<(), Error> {
                self.before_call("removedir", &[key], false)?;
                self.inner.removedir(key).await
            }
            async fn contains(&self, key: &Key) -> Result<bool, Error> {
                self.before_call("contains", &[key], false)?;
                self.inner.contains(key).await
            }
            async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
                self.before_call("is_dir", &[key], false)?;
                self.inner.is_dir(key).await
            }
            async fn keys(&self) -> Result<Vec<Key>, Error> {
                self.before_call("keys", &[], false)?;
                self.inner.keys().await
            }
            async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
                self.before_call("listdir", &[key], false)?;
                self.inner.listdir(key).await
            }
            async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
                self.before_call("listdir_keys", &[key], false)?;
                self.inner.listdir_keys(key).await
            }
            async fn listdir_asset_info(&self, key: &Key) -> Result<Vec<AssetInfo>, Error> {
                self.before_call("listdir_asset_info", &[key], false)?;
                self.inner.listdir_asset_info(key).await
            }
            async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
                self.before_call("listdir_keys_deep", &[key], false)?;
                self.inner.listdir_keys_deep(key).await
            }
            async fn makedir(&self, key: &Key) -> Result<(), Error> {
                self.before_call("makedir", &[key], false)?;
                self.inner.makedir(key).await
            }
            fn is_supported(&self, key: &Key) -> bool {
                self.inner.is_supported(key)
            }
            async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
                self.before_call("get_data", &[key], true)?;
                self.inner.get_data(key).await
            }
            async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error> {
                self.before_call("set_data", &[key], true)?;
                self.inner.set_data(key, data).await
            }
            async fn remove_data(&self, key: &Key) -> Result<(), Error> {
                self.before_call("remove_data", &[key], true)?;
                self.inner.remove_data(key).await
            }
            async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error> {
                self.before_call("rename_data", &[from, to], true)?;
                self.inner.rename_data(from, to).await
            }
            async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
                self.before_call("listdir_data", &[key], true)?;
                self.inner.listdir_data(key).await
            }
            async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
                self.before_call("data_facts", &[key], true)?;
                self.inner.data_facts(key).await
            }
            async fn get_stored_metadata(&self, key: &Key) -> Result<Option<Metadata>, Error> {
                self.before_call("get_stored_metadata", &[key], true)?;
                self.inner.get_stored_metadata(key).await
            }
            fn data_capabilities(&self) -> DataCapabilities {
                self.inner.data_capabilities()
            }
            fn is_read_only(&self) -> bool {
                self.inner.is_read_only()
            }
            async fn sync(&self) -> Result<SyncReport, Error> {
                self.before_call("sync", &[], false)?;
                self.inner.sync().await
            }
            fn version_verification(
                &self,
                key: &Key,
            ) -> Option<crate::environment_builder::VersionVerification> {
                self.inner.version_verification(key)
            }
        }
    };
}

/// Forwards every call to a shared store and logs the fallible ones, in order.
pub struct RecordingStore {
    inner: Arc<dyn AsyncStore>,
    calls: Mutex<Vec<RecordedCall>>,
}

impl RecordingStore {
    pub fn new(inner: Arc<dyn AsyncStore>) -> Self {
        RecordingStore {
            inner,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Every fallible call so far, oldest first.
    pub fn calls(&self) -> Vec<RecordedCall> {
        match self.calls.lock() {
            Ok(calls) => calls.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The method names of [`Self::calls`], oldest first.
    pub fn methods(&self) -> Vec<&'static str> {
        self.calls().into_iter().map(|c| c.method).collect()
    }

    /// Forgets the calls recorded so far.
    pub fn clear(&self) {
        match self.calls.lock() {
            Ok(mut calls) => calls.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }

    fn before_call(
        &self,
        method: &'static str,
        keys: &[&Key],
        _data_only: bool,
    ) -> Result<(), Error> {
        let call = RecordedCall {
            method,
            keys: keys.iter().map(|k| (*k).clone()).collect(),
        };
        match self.calls.lock() {
            Ok(mut calls) => calls.push(call),
            Err(poisoned) => poisoned.into_inner().push(call),
        }
        Ok(())
    }
}

forward_with_hook!(RecordingStore);

/// Forwards every call to a shared store, but fails its `n`-th data-only call (counting from 1)
/// and every one after it, as a backend that went away mid-write would.
pub struct FailingStore {
    inner: Arc<dyn AsyncStore>,
    fail_at: usize,
    data_calls: AtomicUsize,
}

impl FailingStore {
    pub fn new(inner: Arc<dyn AsyncStore>, fail_at: usize) -> Self {
        FailingStore {
            inner,
            fail_at,
            data_calls: AtomicUsize::new(0),
        }
    }

    /// How many data-only calls were made, including the failed ones.
    pub fn data_calls(&self) -> usize {
        self.data_calls.load(Ordering::SeqCst)
    }

    fn before_call(
        &self,
        method: &'static str,
        keys: &[&Key],
        data_only: bool,
    ) -> Result<(), Error> {
        if !data_only {
            return Ok(());
        }
        let n = self.data_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if n >= self.fail_at {
            let key = keys.first().map(|k| k.to_string()).unwrap_or_default();
            return Err(Error::general_error(format!(
                "FailingStore: injected failure of data-only call {n} ({method} '{key}')"
            )));
        }
        Ok(())
    }
}

forward_with_hook!(FailingStore);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_key;
    use crate::store::AsyncMemoryStore;

    #[tokio::test]
    async fn recording_store_logs_calls_in_order() {
        let store = RecordingStore::new(Arc::new(AsyncMemoryStore::new(&Key::new())));
        let key = parse_key("a/b.txt").expect("key");
        let _ = store.contains(&key).await;
        let _ = store.get(&key).await;
        assert_eq!(store.methods(), vec!["contains", "get"]);
        assert_eq!(store.calls()[0].keys, vec![key]);
    }

    #[tokio::test]
    async fn failing_store_fails_from_its_nth_data_call() {
        let store = FailingStore::new(Arc::new(AsyncMemoryStore::new(&Key::new())), 2);
        let key = parse_key("a/b.txt").expect("key");
        // The memory store does not offer data-only access until step 3, so the first call is
        // refused by the store itself and the second by the injected failure.
        let first = store.get_data(&key).await;
        let second = store.get_data(&key).await;
        assert!(first.is_err());
        let message = second.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("injected failure"), "{message}");
        // Calls that are not data-only are never counted.
        let _ = store.contains(&key).await;
        assert_eq!(store.data_calls(), 2);
    }
}
