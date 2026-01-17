use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::error::Error;
use crate::metadata::{self, AssetInfo, Metadata, MetadataRecord};
use crate::query::Key;

pub trait Store: Send + Sync {
    /// Get store name
    fn store_name(&self) -> String {
        format!("{} Store", self.key_prefix())
    }

    /// Key prefix common to all keys in this store.
    fn key_prefix(&self) -> Key {
        Key::new()
    }

    /// Create default metadata object for a given key
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        let mut metadata = MetadataRecord::new();
        metadata.with_key(key.to_owned());
        let _ = metadata.set_updated_now();
        metadata.is_dir = is_dir;
        if is_dir {
            metadata.children = self.listdir_asset_info(key).unwrap_or_default();
        }
        metadata
    }

    /// Finalize metadata before storing - when data is available
    /// This can't be a directory
    /// If update is true, it is considered a real update of the data,
    /// not just fixing the metadata - the time of the update gets actualized too
    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
        if update {
            let _ = metadata.set_updated_now();
        }
        let _ = metadata.with_key(key.clone());
        metadata.with_file_size(data.len() as u64);
        match metadata.status() {
            metadata::Status::None => {
                // If there is data, then the status can't be None - It could be only some state that has data.
                // Source is the least assuming, but it can create inconsistency if there is a recipe.
                let _ = metadata.set_status(metadata::Status::Source);
            }
            _ => {}
        }
    }

    /// Finalize metadata before storing - when data is not available
    fn finalize_metadata_empty(
        &self,
        metadata: &mut Metadata,
        key: &Key,
        is_dir: bool,
        update: bool,
    ) {
        if update {
            let _ = metadata.set_updated_now();
        }
        metadata.with_is_dir(is_dir);
        let _ = metadata.with_key(key.clone());
        if is_dir {
            let _ = metadata.set_status(metadata::Status::Directory);
        }
    }

    /// Get data and metadata
    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        Err(Error::key_not_found(key))
    }

    /// Get data as bytes
    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        Err(Error::key_not_found(key))
    }

    /// Get metadata
    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.is_dir(key)? {
            let metadata = self.default_metadata(key, true);
            return Ok(Metadata::MetadataRecord(metadata));
        }
        Err(Error::key_not_found(key))
    }

    /// Get asset info
    fn get_asset_info(&self, key: &Key) -> Result<metadata::AssetInfo, Error> {
        let mut info = self
            .get_metadata(key)?
            .get_asset_info()
            .unwrap_or_else(|_e| AssetInfo::new());
        info.with_key(key.to_owned());
        info.is_dir = self.is_dir(key)?;
        Ok(info)
    }

    /// Store data and metadata.
    fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Store metadata only
    fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove data and metadata associated with the key
    fn remove(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    fn removedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Returns true if store contains the key.
    fn contains(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// Returns true if key points to a directory.
    fn is_dir(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// List or iterator of all keys
    fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix())?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error> {
        Ok(vec![])
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return asset info of assets inside a directory specified by key.
    /// Only info of assets present directly in the directory are returned,
    /// subdirectories are not traversed.
    fn listdir_asset_info(&self, key: &Key) -> Result<Vec<AssetInfo>, Error> {
        let keys = self.listdir_keys(key)?;
        let mut asset_info = Vec::new();
        for k in keys {
            let info = self.get_asset_info(&k)?;
            asset_info.push(info);
        }
        asset_info.sort_by(|a, b| {
            if a.is_dir {
                if b.is_dir {
                    a.filename.cmp(&b.filename)
                } else {
                    std::cmp::Ordering::Less
                }
            } else if b.is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.filename.cmp(&b.filename)
            }
        });
        Ok(asset_info)
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    fn makedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, _key: &Key) -> bool {
        false
    }

    /*
        def on_data_changed(self, key):
            """Event handler called when the data is changed."""
            pass

        def on_metadata_changed(self, key):
            """Event handler called when the metadata is changed."""
            pass

        def on_removed(self, key):
            """Event handler called when the data or directory is removed."""
            pass

        def to_root_key(self, key):
            """Convert local store key to a key in a root store.
            This is can be used e.g. to convert a key valid in a mounted (child) store to
            a key of a root store.
            The to_root_key(key) in the root_store() should point to the same object as key in self.
            """
            if self.parent_store is None:
                return key
            return self.parent_store.to_root_key(key)

        def root_store(self):
            """Get the root store.
            Root store is the highest level store in the store system.
            The to_root_key(key) in the root_store() should point to the same object as key in self.
            """
            if self.parent_store is None:
                return self
            return self.parent_store.root_store()

        def sync(self):
            pass

        def __str__(self):
            return f"Empty store"

        def __repr__(self):
            return f"Store()"
    */
}

#[cfg(feature = "async_store")]
#[async_trait]
pub trait AsyncStore: Send + Sync {
    /// Get store name
    fn store_name(&self) -> String {
        format!("{} Store", self.key_prefix())
    }

    /// Key prefix common to all keys in this store.
    fn key_prefix(&self) -> Key {
        Key::new()
    }

    /// Create default metadata object for a given key
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        let mut m = MetadataRecord::new();
        m.set_updated_now().with_key(key.to_owned()).is_dir = is_dir;
        m
    }

    /// Finalize metadata before storing - when data is available
    /// This can't be a directory
    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
        if update {
            let _ = metadata.set_updated_now();
        }
        let _ = metadata.with_key(key.clone());
        metadata.with_file_size(data.len() as u64);
        match metadata.status() {
            metadata::Status::None => {
                // If there is data, then the status can't be None - It could be only some state that has data.
                // Source is the least assuming, but it can create inconsistency if there is a recipe.
                let _ = metadata.set_status(metadata::Status::Source);
            }
            _ => {}
        }
    }

    /// Finalize metadata before storing - when data is not available
    fn finalize_metadata_empty(
        &self,
        metadata: &mut Metadata,
        key: &Key,
        is_dir: bool,
        update: bool,
    ) {
        if update {
            let _ = metadata.set_updated_now();
        }
        metadata.with_is_dir(is_dir);
        let _ = metadata.with_key(key.clone());
        if is_dir {
            let _ = metadata.set_status(metadata::Status::Directory);
        }
    }

    /// Get data asynchronously
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>;

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.get(key).await.map(|(data, _)| data)
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.is_dir(key).await? {
            let mut metadata = self.default_metadata(key, true);
            metadata.children = self.listdir_asset_info(key).await?;
            return Ok(Metadata::MetadataRecord(metadata));
        }
        self.get(key).await.map(|(_, metadata)| metadata)
    }

    /// Get asset info
    async fn get_asset_info(&self, key: &Key) -> Result<metadata::AssetInfo, Error> {
        let mut info = self
            .get_metadata(key)
            .await?
            .get_asset_info()
            .unwrap_or_else(|_e| AssetInfo::new());
        info.with_key(key.to_owned());
        info.is_dir = self.is_dir(key).await?;
        Ok(info)
    }

    /// Store data and metadata.
    async fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error>;

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Returns true if store contains the key.
    async fn contains(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix()).await?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error> {
        Ok(vec![])
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let names = self.listdir(key).await?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return asset info of assets inside a directory specified by key.
    /// Only info of assets present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_asset_info(&self, key: &Key) -> Result<Vec<AssetInfo>, Error> {
        let keys = self.listdir_keys(key).await?;
        let mut asset_info = Vec::new();
        for k in keys {
            let info = self.get_asset_info(&k).await?;
            asset_info.push(info);
        }
        asset_info.sort_by(|a, b| {
            if a.is_dir {
                if b.is_dir {
                    a.filename.cmp(&b.filename)
                } else {
                    std::cmp::Ordering::Less
                }
            } else if b.is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.filename.cmp(&b.filename)
            }
        });
        Ok(asset_info)
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let keys = self.listdir_keys(key).await?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, _key: &Key) -> bool {
        false
    }
}

#[cfg(feature = "async_store")]
pub struct AsyncStoreWrapper<T: Store>(pub T);

impl<T: Store + Clone> Clone for AsyncStoreWrapper<T> {
    fn clone(&self) -> Self {
        AsyncStoreWrapper(self.0.clone())
    }
}

#[cfg(feature = "async_store")]
#[async_trait]
impl<T: Store + std::marker::Sync> AsyncStore for AsyncStoreWrapper<T> {
    /// Get store name
    fn store_name(&self) -> String {
        self.0.store_name()
    }

    /// Key prefix common to all keys in this store.
    fn key_prefix(&self) -> Key {
        self.0.key_prefix()
    }

    /// Create default metadata object for a given key
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.0.default_metadata(key, is_dir)
    }

    /// Finalize metadata before storing - when data is available
    /// This can't be a directory
    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
        self.0.finalize_metadata(metadata, key, data, update)
    }

    /// Finalize metadata before storing - when data is not available
    fn finalize_metadata_empty(
        &self,
        metadata: &mut Metadata,
        key: &Key,
        is_dir: bool,
        update: bool,
    ) {
        self.0
            .finalize_metadata_empty(metadata, key, is_dir, update)
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.0.get(key)
    }

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.0.get_bytes(key)
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.0.get_metadata(key)
    }

    /// Store data and metadata.
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.0.set(key, data, metadata)
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.0.set_metadata(key, metadata)
    }

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.0.remove(key)
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        self.0.removedir(key)
    }

    /// Returns true if store contains the key.
    async fn contains(&self, _key: &Key) -> Result<bool, Error> {
        self.0.contains(_key)
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, _key: &Key) -> Result<bool, Error> {
        self.0.is_dir(_key)
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.0.keys()
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error> {
        self.0.listdir(_key)
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.0.listdir_keys(key)
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.0.listdir_keys_deep(key)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, key: &Key) -> bool {
        self.0.is_supported(key)
    }
}

/// Trivial store unable to store anything.
/// Used e.g. in the environment as a default value when the store is not available.
pub struct NoStore;

impl Clone for NoStore {
    fn clone(&self) -> Self {
        NoStore
    }
}

impl Store for NoStore {}

/// Trivial store unable to store anything.
/// Used e.g. in the environment as a default value when the store is not available.
pub struct NoAsyncStore;

impl Clone for NoAsyncStore {
    fn clone(&self) -> Self {
        NoAsyncStore
    }
}

#[cfg(feature = "async_store")]
#[async_trait]
impl AsyncStore for NoAsyncStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        Err(Error::key_not_found(key))
    }

    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, "NoAsyncStore"))
    }
}

#[derive(Debug, Clone)]
pub struct FileStore {
    pub path: PathBuf,
    pub prefix: Key,
}

impl FileStore {
    const METADATA: &'static str = ".__metadata__";
    pub fn new(path: &str, prefix: &Key) -> FileStore {
        FileStore {
            path: PathBuf::from(path),
            prefix: prefix.to_owned(),
        }
    }

    pub fn key_to_path(&self, key: &Key) -> PathBuf {
        let mut path = self.path.clone();
        path.push(key.to_string());
        path
    }

    pub fn key_to_path_metadata(&self, key: &Key) -> PathBuf {
        let mut path = self.path.clone();
        path.push(format!("{}{}", key, Self::METADATA));
        path
    }
}

impl Store for FileStore {
    fn store_name(&self) -> String {
        format!(
            "{} File store in {}",
            self.key_prefix(),
            self.path.display()
        )
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let data = self.get_bytes(key)?;
        match self.get_metadata(key) {
            Ok(metadata) => Ok((data, metadata)),
            Err(error) => {
                let mut metadata = self.default_metadata(key, false);
                metadata.warning(&format!("Can't read metadata: {}", error));
                metadata.warning("New metadata has been created. (get)");
                let mut metadata = Metadata::MetadataRecord(metadata);
                self.finalize_metadata(&mut metadata, key, &data, false);
                self.set_metadata(key, &metadata)?;
                Ok((data, metadata))
            }
        }
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let path = self.key_to_path(key);
        if path.exists() {
            let mut file =
                File::open(path).map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            Ok(buffer)
        } else {
            Err(Error::key_not_found(key))
        }
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let path = self.key_to_path_metadata(key);
        if path.exists() {
            if path.is_dir() {
                let mut metadata = self.default_metadata(key, true);
                metadata.children = self.listdir_asset_info(key).unwrap_or_default();
                return Ok(Metadata::MetadataRecord(metadata));
            }
            let mut file =
                File::open(path).map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            if let Ok(metadata) = serde_json::from_reader(&buffer[..]) {
                // TODO: fix metadata, e.g. add the key
                return Ok(Metadata::MetadataRecord(metadata));
            }
            if let Ok(metadata) = serde_json::from_reader(&buffer[..]) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            Err(Error::key_read_error(
                key,
                &self.store_name(),
                "Metadata parsing error",
            ))
        } else {
            let path = self.key_to_path(key);
            if path.exists() {
                if path.is_dir() {
                    let mut metadata = self.default_metadata(key, true);
                    metadata.children = self.listdir_asset_info(key).unwrap_or_default();
                    return Ok(Metadata::MetadataRecord(metadata));
                } else {
                    let mut metadata = self.default_metadata(key, false);
                    metadata.warning(&format!("Metadata file {} does not exist.", path.display()));
                    metadata.warning("New metadata has been created. (get_metadata)");
                    let mut metadata = Metadata::MetadataRecord(metadata);
                    let data = self.get_bytes(key)?;
                    self.finalize_metadata(&mut metadata, key, &data, false);
                    self.set_metadata(key, &metadata)?;
                    return Ok(metadata);
                }
            } else {
                Err(Error::key_not_found(key))
            }
        }
    }

    fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let path = self.key_to_path(key);
        let mut tmp_metadata = metadata.clone();
        self.finalize_metadata(&mut tmp_metadata, key, data, true);
        tmp_metadata.set_status(metadata::Status::Storing)?;
        self.set_metadata(key, &tmp_metadata)?;

        let mut file =
            File::create(path).map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        file.write_all(data)
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        self.finalize_metadata(&mut tmp_metadata, key, data, true);
        self.set_metadata(key, metadata)?;
        Ok(())
    }

    fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let path = self.key_to_path_metadata(key);
        let file =
            File::create(path).map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        Ok(())
    }

    fn remove(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key);
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        let matadata_path = self.key_to_path_metadata(key);
        if matadata_path.exists() {
            std::fs::remove_file(matadata_path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key);
        if path.exists() {
            std::fs::remove_dir_all(path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key);
        if path.exists() {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key);
        if metadata_path.exists() {
            return Ok(true);
        }
        Ok(false)
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key);
        Ok(path.is_dir())
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let path = self.key_to_path(key);
        if path.is_dir() {
            let dir = path
                .read_dir()
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            let names = dir
                .flat_map(|entry| {
                    entry
                        .ok()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                })
                .filter(|name| !name.ends_with(Self::METADATA))
                .collect();
            Ok(names)
        } else if path.exists() {
            Ok(vec![])
        } else {
            Err(Error::key_not_found(key))
        }
    }

    fn makedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key);
        std::fs::create_dir_all(path)
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix)
            && (!key
                .filename()
                .is_some_and(|file_name| file_name.name.ends_with(Self::METADATA)))
    }
}

pub struct MemoryStore {
    data: Arc<RwLock<std::collections::HashMap<Key, (Vec<u8>, Metadata)>>>,
    prefix: Key,
}

impl MemoryStore {
    pub fn new(prefix: &Key) -> MemoryStore {
        MemoryStore {
            data: Arc::new(RwLock::new(std::collections::HashMap::new())),
            prefix: prefix.to_owned(),
        }
    }
}

impl Store for MemoryStore {
    fn store_name(&self) -> String {
        format!("{} Memory store", self.key_prefix())
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    fn default_metadata(&self, _key: &Key, is_dir: bool) -> MetadataRecord {
        let mut metadata = MetadataRecord::new();
        metadata.with_key(_key.to_owned());
        metadata.is_dir = is_dir;
        metadata
    }

    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let mem = self.data.read().unwrap();
        match mem.get(key) {
            Some((data, metadata)) => Ok((data.to_owned(), metadata.to_owned())),
            None => Err(Error::key_not_found(key)),
        }
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let mem = self.data.read().unwrap();
        match mem.get(key) {
            Some((data, _)) => Ok(data.to_owned()),
            None => Err(Error::key_not_found(key)),
        }
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let mem = self.data.read().unwrap();
        if self.is_dir(key)? {
            let mut metadata = self.default_metadata(key, true);
            metadata.children = self.listdir_asset_info(key)?;
            return Ok(Metadata::MetadataRecord(metadata));
        }
        match mem.get(key) {
            Some((_, metadata)) => Ok(metadata.to_owned()),
            None => Err(Error::key_not_found(key)),
        }
    }

    fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let mut mem = self.data.write().unwrap();

        mem.insert(key.to_owned(), (data.to_owned(), metadata.to_owned()));
        Ok(())
    }

    fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let res = self.get(key)?;
        let mut mem = self.data.write().unwrap();
        mem.insert(key.to_owned(), (res.0, metadata.to_owned()));
        Ok(())
    }

    fn remove(&self, key: &Key) -> Result<(), Error> {
        let mut mem = self.data.write().unwrap();
        mem.remove(key);
        Ok(())
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        let mut mem = self.data.write().unwrap();
        let keys = mem
            .keys()
            .filter(|k| k.has_key_prefix(key))
            .cloned()
            .collect::<Vec<_>>();
        for k in keys {
            mem.remove(&k);
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> Result<bool, Error> {
        let mem = self.data.read().unwrap();
        if mem.contains_key(key){
            return Ok(true);
        }
        Ok(self.is_dir(key)?)
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let mem = self.data.read().unwrap();
        let keys = mem
            .keys()
            .filter(|k| k.has_key_prefix(key))
            .cloned()
            .collect::<Vec<_>>();
        for k in keys {
            if k.len() > key.len() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn keys(&self) -> Result<Vec<Key>, Error> {
        let mem = self.data.read().unwrap();
        let keys = mem.keys().cloned().collect::<Vec<_>>();
        Ok(keys)
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let keys = self.listdir_keys(key)?;
        Ok(keys.iter().filter_map(|x| x.filename().map(|xx| xx.to_string())).collect())
    }

    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let mem = self.data.read().unwrap();
        let n = key.len() + 1;
        let keys = mem
            .keys()
            .filter(|k| k.has_key_prefix(key))
            .filter_map(|k| k.prefix_of_size(n))
            .collect::<BTreeSet<_>>();
        Ok(keys.into_iter().collect())
    }

    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let mem = self.data.read().unwrap();
        let keys = mem
            .keys()
            .filter(|k| k.has_key_prefix(key))
            .cloned()
            .collect::<Vec<_>>();
        Ok(keys)
    }

    fn makedir(&self, _key: &Key) -> Result<(), Error> {
        // TODO: implement correct makedir
        Ok(())
    }

    fn is_supported(&self, _key: &Key) -> bool {
        true
    }
}

/// Store that routes requests to multiple stores.
/// Ideally there should only be one router in the system, therefore the StoreRouter has no key prefix (key prefix is empty).
/// Stores are evaluated in sequence until the first store that supports the key is found - i.e. prefix is matching and is_supported returns true.
pub struct StoreRouter {
    stores: Vec<Box<dyn Store>>,
}

impl Default for StoreRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreRouter {
    pub fn new() -> StoreRouter {
        StoreRouter { stores: Vec::new() }
    }

    pub fn add_store(&mut self, store: Box<dyn Store>) {
        self.stores.push(store);
    }

    pub fn find_store(&self, key: &Key) -> Option<&dyn Store> {
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) && store.is_supported(key) {
                return Some(store.as_ref());
            }
        }
        None
    }

    pub fn find_store_mut(&mut self, key: &Key) -> Option<&mut dyn Store> {
        for store in &mut self.stores {
            if key.has_key_prefix(&store.key_prefix()) && store.is_supported(key) {
                return Some(store.as_mut());
            }
        }
        None
    }
}

impl Store for StoreRouter {
    fn store_name(&self) -> String {
        "Store router".to_string()
    }

    fn key_prefix(&self) -> Key {
        Key::new()
    }

    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.find_store(key).map_or(MetadataRecord::new(), |store| {
            store.default_metadata(key, is_dir)
        })
    }

    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
        self.find_store(key)
            .iter()
            .for_each(|store| store.finalize_metadata(metadata, key, data, update));
        if update {
            let _ = metadata.set_updated_now();
        }
        let _ = metadata.with_key(key.clone());
        metadata.with_file_size(data.len() as u64);
        match metadata.status() {
            metadata::Status::None => {
                // If there is data, then the status can't be None - It could be only some state that has data.
                // Source is the least assuming, but it can create inconsistency if there is a recipe.
                let _ = metadata.set_status(metadata::Status::Source);
            }
            _ => {}
        }
    }

    fn finalize_metadata_empty(
        &self,
        metadata: &mut Metadata,
        key: &Key,
        is_dir: bool,
        update: bool,
    ) {
        self.find_store(key)
            .iter()
            .for_each(|store| store.finalize_metadata_empty(metadata, key, is_dir, update));
        if update {
            let _ = metadata.set_updated_now();
        }
        metadata.with_is_dir(is_dir);
        let _ = metadata.with_key(key.clone());
        if is_dir {
            let _ = metadata.set_status(metadata::Status::Directory);
        }
    }

    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| store.get(key))
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| store.get_bytes(key))
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| {
                store.get_metadata(key)
            })
    }

    fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.set(key, data, metadata),
        )
    }

    fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.set_metadata(key, _metadata),
        )
    }

    fn remove(&self, key: &Key) -> Result<(), Error> {
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.remove(key),
        )
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.removedir(key),
        )
    }

    fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.find_store(key)
            .map_or(Ok(false), |store| store.contains(key))
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                return store.is_dir(key);
            }
            if store.key_prefix().has_key_prefix(key) {
                // key is a prefix of store prefix, but smaller - hence it is a directory
                return Ok(true);
            }
        }
        if key.is_empty() {
            return Ok(true);
        }
        Ok(false)
    }

    fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix())?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let mut list = Vec::new();
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                let names = store.listdir(key)?;
                list.extend(names);
            }
            if store.key_prefix().has_key_prefix(key) {
                // key is a prefix of store prefix, but smaller - hence it is a directory
                list.push(store.key_prefix()[key.len()].to_string());
            }
        }

        Ok(list)
    }

    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    fn makedir(&self, key: &Key) -> Result<(), Error> {
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.makedir(key),
        )
    }

    fn is_supported(&self, key: &Key) -> bool {
        self.find_store(key)
            .is_some_and(|store| store.is_supported(key))
    }
}

/// Asunchronous store that routes requests to multiple (asynchronous) stores.
#[cfg(feature = "async_store")]
pub struct AsyncStoreRouter {
    stores: Vec<Box<dyn AsyncStore>>,
}

#[cfg(feature = "async_store")]
impl AsyncStoreRouter {
    pub fn new() -> AsyncStoreRouter {
        AsyncStoreRouter { stores: Vec::new() }
    }

    pub fn add_store(&mut self, store: Box<dyn AsyncStore>) {
        self.stores.push(store);
    }

    fn find_store(&self, key: &Key) -> Option<&Box<dyn AsyncStore>> {
        self.stores
            .iter()
            .find(|&store| key.has_key_prefix(&store.key_prefix()) && store.is_supported(key))
    }
}

#[async_trait]
#[cfg(feature = "async_store")]
impl AsyncStore for AsyncStoreRouter {
    fn store_name(&self) -> String {
        "Store router".to_string()
    }

    fn key_prefix(&self) -> Key {
        Key::new()
    }

    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.find_store(key).map_or(MetadataRecord::new(), |store| {
            store.default_metadata(key, is_dir)
        })
    }

    fn finalize_metadata(&self, metadata: &mut Metadata, key: &Key, data: &[u8], update: bool) {
        self.find_store(key).iter().for_each(|store| {
            store.finalize_metadata(metadata, key, data, update);
        });
    }

    fn finalize_metadata_empty(
        &self,
        metadata: &mut Metadata,
        key: &Key,
        is_dir: bool,
        update: bool,
    ) {
        self.find_store(key).iter().for_each(|store| {
            store.finalize_metadata_empty(metadata, key, is_dir, update);
        });
        if update {
            let _ = metadata.set_updated_now();
        }
        metadata.with_is_dir(is_dir);
        let _ = metadata.with_key(key.clone());
        if is_dir {
            let _ = metadata.set_status(metadata::Status::Directory);
        }
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        if let Some(store) = self.find_store(key) {
            store.get(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        if let Some(store) = self.find_store(key) {
            store.get_bytes(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if let Some(store) = self.find_store(key) {
            store.get_metadata(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Store data and metadata.
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        if let Some(store) = self.find_store(key) {
            store.set(key, data, metadata).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        if let Some(store) = self.find_store(key) {
            store.set_metadata(key, metadata).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        if let Some(store) = self.find_store(key) {
            store.remove(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        if let Some(store) = self.find_store(key) {
            store.removedir(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        if let Some(store) = self.find_store(key) {
            store.contains(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                return store.is_dir(key).await;
            }
            if store.key_prefix().has_key_prefix(key) {
                // key is a prefix of store prefix, but smaller - hence it is a directory
                return Ok(true);
            }
        }
        if key.is_empty() {
            return Ok(true);
        }
        Ok(false)
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix()).await?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let mut list = Vec::new();
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                let names = store.listdir(key).await?;
                list.extend(names);
            }
            if store.key_prefix().has_key_prefix(key) {
                // key is a prefix of store prefix, but smaller - hence it is a directory
                list.push(store.key_prefix()[key.len()].to_string());
            }
        }

        Ok(list)
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let names = self.listdir(key).await?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let keys = self.listdir_keys(key).await?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        if let Some(store) = self.find_store(key) {
            store.makedir(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, _key: &Key) -> bool {
        if let Some(store) = self.find_store(_key) {
            store.is_supported(_key)
        } else {
            false
        }
    }
}

// Unittests
#[cfg(test)]
mod tests {
    //    use crate::query::Key;

    use super::*;

    use crate::parse::parse_key;

    #[test]
    fn test_simple_store() -> Result<(), Error> {
        let store = MemoryStore::new(&Key::new());
        let key = parse_key("a/b/c").unwrap();
        let data = b"test data".to_vec();
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());

        assert!(!store.contains(&key)?);
        assert!(store.keys().unwrap().is_empty());
        assert!(!store.is_dir(&parse_key("a/b")?)?);

        store.set(&key, &data, &metadata)?;
        assert!(store.contains(&key)?);
        assert!(store.keys()?.contains(&key));
        assert!(store.is_dir(&parse_key("a/b")?)?);
        assert_eq!(store.keys().unwrap().len(), 1);

        let (data2, _metadata2) = store.get(&key).unwrap();
        assert_eq!(data, data2);
        store.remove(&key).unwrap();
        assert!(!store.contains(&key)?);
        Ok(())
    }
}
