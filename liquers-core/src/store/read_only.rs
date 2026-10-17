//! [`ReadOnlyStore`]: the access policy of `specs/design/store-adapters/` (Phase 2 §2).
//!
//! Every write is refused with `NotSupported`, and the reads are arranged so that nothing is
//! written on read either. Over a store with data-only access ([`DataCapabilities::data_access`]),
//! data and the stored record are read through the data-only methods, which never repair, and a
//! missing record is synthesized in memory with the inner store's `default_metadata` plus
//! `finalize_metadata(update = false)`. Any other store — a metadata pipeline among them, which
//! reports no data access — is forwarded unchanged: if it writes on read, this decorator cannot
//! stop it.

use std::sync::Arc;

use async_trait::async_trait;

use super::{AsyncStore, DataCapabilities, DataFacts, SyncReport};
use crate::error::Error;
use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;

/// A store that refuses every write and never writes on read.
pub struct ReadOnlyStore {
    inner: Arc<dyn AsyncStore>,
}

impl ReadOnlyStore {
    pub fn new(inner: Arc<dyn AsyncStore>) -> Self {
        ReadOnlyStore { inner }
    }

    /// The wrapped store.
    pub fn inner(&self) -> &Arc<dyn AsyncStore> {
        &self.inner
    }

    /// True when reads go through the data-only methods instead of being forwarded.
    fn reads_data_only(&self) -> bool {
        self.inner.data_capabilities().data_access
    }

    fn refuse(&self, key: &Key, operation: &str) -> Error {
        Error::not_supported(format!(
            "Store '{}' is read-only: {operation} of '{key}' refused",
            self.store_name()
        ))
    }

    /// The record of a data object that has no stored one: built in memory, never written.
    fn synthesize(&self, key: &Key, facts: &DataFacts) -> Metadata {
        let mut metadata = Metadata::MetadataRecord(self.inner.default_metadata(key, false));
        self.inner.finalize_metadata(&mut metadata, key, &[], false);
        metadata.with_file_size(facts.size);
        metadata
    }

    /// The metadata of a key, read without writing anything.
    async fn read_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.inner.is_dir(key).await? {
            let mut record: MetadataRecord = self.inner.default_metadata(key, true);
            record.children = self.listdir_asset_info(key).await?;
            return Ok(Metadata::MetadataRecord(record));
        }
        if let Some(metadata) = self.inner.get_stored_metadata(key).await? {
            return Ok(metadata);
        }
        match self.inner.data_facts(key).await? {
            Some(facts) => Ok(self.synthesize(key, &facts)),
            None => Err(Error::key_not_found(key)),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for ReadOnlyStore {
    fn store_name(&self) -> String {
        format!("read-only {}", self.inner.store_name())
    }

    fn key_prefix(&self) -> Key {
        self.inner.key_prefix()
    }

    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.inner.default_metadata(key, is_dir)
    }

    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
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
        if !self.reads_data_only() {
            return self.inner.get(key).await;
        }
        let data = self.inner.get_data(key).await?;
        let metadata = match self.inner.get_stored_metadata(key).await? {
            Some(metadata) => metadata,
            None => self.synthesize(
                key,
                &DataFacts {
                    size: data.len() as u64,
                    modified: None,
                    etag: None,
                },
            ),
        };
        Ok((data, metadata))
    }

    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        if self.reads_data_only() {
            self.inner.get_data(key).await
        } else {
            self.inner.get_bytes(key).await
        }
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.reads_data_only() {
            self.read_metadata(key).await
        } else {
            self.inner.get_metadata(key).await
        }
    }

    async fn get_asset_info(&self, key: &Key) -> Result<crate::metadata::AssetInfo, Error> {
        if !self.reads_data_only() {
            return self.inner.get_asset_info(key).await;
        }
        // As the trait default, over this decorator's own reads.
        if self.inner.is_dir(key).await? {
            let mut info = self.inner.default_metadata(key, true).get_asset_info();
            info.with_key(key.to_owned());
            info.is_dir = true;
            return Ok(info);
        }
        let mut info = self
            .read_metadata(key)
            .await?
            .get_asset_info()
            .unwrap_or_else(|_e| crate::metadata::AssetInfo::new());
        info.with_key(key.to_owned());
        info.is_dir = false;
        Ok(info)
    }

    async fn listdir_asset_info(
        &self,
        key: &Key,
    ) -> Result<Vec<crate::metadata::AssetInfo>, Error> {
        if !self.reads_data_only() {
            return self.inner.listdir_asset_info(key).await;
        }
        let key = key.as_absolute()?;
        let mut infos = Vec::new();
        for name in self.inner.listdir(key).await? {
            infos.push(self.get_asset_info(&key.join(&name)).await?);
        }
        Ok(infos)
    }

    async fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "set"))
    }

    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "set_metadata"))
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "remove"))
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "removedir"))
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.inner.is_dir(key).await
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.inner.keys().await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.inner.listdir(key).await
    }

    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.inner.listdir_keys(key).await
    }

    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.inner.listdir_keys_deep(key).await
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "makedir"))
    }

    fn is_supported(&self, key: &Key) -> bool {
        self.inner.is_supported(key)
    }

    async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.inner.get_data(key).await
    }

    async fn set_data(&self, key: &Key, _data: &[u8]) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "set_data"))
    }

    async fn remove_data(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(self.refuse(key, "remove_data"))
    }

    async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error> {
        from.as_absolute()?;
        to.as_absolute()?;
        Err(self.refuse(from, "rename_data"))
    }

    async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.inner.listdir_data(key).await
    }

    async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
        self.inner.data_facts(key).await
    }

    async fn get_stored_metadata(&self, key: &Key) -> Result<Option<Metadata>, Error> {
        self.inner.get_stored_metadata(key).await
    }

    /// The inner store's reads; every data-only write is refused whatever this reports.
    fn data_capabilities(&self) -> DataCapabilities {
        self.inner.data_capabilities()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    /// Nothing: repairing an interrupted write is a write.
    async fn sync(&self) -> Result<SyncReport, Error> {
        Ok(SyncReport::default())
    }

    fn version_verification(
        &self,
        key: &Key,
    ) -> Option<crate::environment_builder::VersionVerification> {
        self.inner.version_verification(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorType;
    use crate::parse::parse_key;
    use crate::store::{AsyncFileStore, AsyncMemoryStore};
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("lq-readonly-{label}-{nanos}"))
    }

    /// Every file under `root`, relative and sorted.
    fn tree(root: &PathBuf) -> Vec<String> {
        fn walk(dir: &PathBuf, root: &PathBuf, out: &mut Vec<String>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    out.push(
                        path.strip_prefix(root)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    );
                    if path.is_dir() {
                        walk(&path, root, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn is_not_supported(result: Result<(), Error>) -> bool {
        matches!(result, Err(e) if e.error_type == ErrorType::NotSupported)
    }

    /// AC-1: every write, ordinary or data-only, is refused with `NotSupported`.
    #[tokio::test]
    async fn read_only_refuses_every_write_with_not_supported() -> Result<(), Error> {
        let memory = AsyncMemoryStore::new(&Key::new());
        let key = parse_key("a/b.txt")?;
        memory.set(&key, b"x", &Metadata::new()).await?;
        let store = ReadOnlyStore::new(Arc::new(memory));
        let other = parse_key("a/c.txt")?;
        assert!(is_not_supported(
            store.set(&key, b"y", &Metadata::new()).await
        ));
        assert!(is_not_supported(
            store.set_metadata(&key, &Metadata::new()).await
        ));
        assert!(is_not_supported(store.remove(&key).await));
        assert!(is_not_supported(store.makedir(&other).await));
        assert!(is_not_supported(store.removedir(&parse_key("a")?).await));
        assert!(is_not_supported(store.set_data(&key, b"y").await));
        assert!(is_not_supported(store.remove_data(&key).await));
        assert!(is_not_supported(store.rename_data(&key, &other).await));
        // Nothing changed underneath.
        assert_eq!(store.get_bytes(&key).await?, b"x".to_vec());
        assert!(!store.contains(&other).await?);
        Ok(())
    }

    /// AC-1: reading a file store with no sidecars, through every read, creates nothing.
    #[tokio::test]
    async fn read_only_file_store_creates_nothing() -> Result<(), Error> {
        let root = temp_root("nothing");
        std::fs::create_dir_all(root.join("dir"))
            .map_err(|e| Error::general_error(format!("setup: {e}")))?;
        std::fs::write(root.join("dir/a.txt"), b"alpha")
            .map_err(|e| Error::general_error(format!("setup: {e}")))?;
        std::fs::write(root.join("b.txt"), b"beta")
            .map_err(|e| Error::general_error(format!("setup: {e}")))?;
        let before = tree(&root);

        let file = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let store = ReadOnlyStore::new(Arc::new(file));
        let a = parse_key("dir/a.txt")?;
        let b = parse_key("b.txt")?;
        let dir = parse_key("dir")?;
        let mut listed = store.listdir(&Key::new()).await?;
        listed.sort();
        assert_eq!(listed, vec!["b.txt".to_string(), "dir".to_string()]);
        let infos = store.listdir_asset_info(&dir).await?;
        assert_eq!(infos.len(), 1);
        let (data, metadata) = store.get(&a).await?;
        assert_eq!(data, b"alpha".to_vec());
        assert_eq!(metadata.file_size(), Some(5));
        assert_eq!(store.get_metadata(&b).await?.file_size(), Some(4));
        assert!(store.get_metadata(&dir).await?.is_dir());
        let _ = store.get_asset_info(&b).await?;
        let _ = store.get_asset_info(&dir).await?;
        let _ = store.listdir_asset_info(&Key::new()).await?;

        assert_eq!(tree(&root), before);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// AC-1: the runtime accessor answers `true`, and a store without data access is forwarded.
    #[tokio::test]
    async fn read_only_reports_is_read_only() -> Result<(), Error> {
        let memory = AsyncMemoryStore::new(&Key::new());
        assert!(!memory.is_read_only());
        let store = ReadOnlyStore::new(Arc::new(memory));
        assert!(store.is_read_only());
        assert!(store.store_name().starts_with("read-only "));
        let forwarded = ReadOnlyStore::new(Arc::new(crate::store::NoAsyncStore));
        assert!(forwarded.is_read_only());
        assert!(forwarded.get(&parse_key("x")?).await.is_err());
        Ok(())
    }
}
