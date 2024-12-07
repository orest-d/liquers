
use std::collections::BTreeSet;

use liquers_core::{error::Error, metadata::{Metadata, MetadataRecord}, query::Key, store::{AsyncStore, Store}};
use opendal::{BlockingOperator, Operator, Buffer};
use bytes::Buf;

use async_trait::async_trait;

pub struct OpenDALStore {
    op: BlockingOperator,
    prefix: Key,
}

impl OpenDALStore {
    const METADATA: &'static str = ".__metadata__";

    pub fn new(op: BlockingOperator, prefix: Key) -> Self {
        OpenDALStore { op, prefix }
    }

    pub fn key_to_path(&self, key: &Key) -> String {
        key.encode()
    }

    pub fn key_to_path_metadata(&self, key: &Key) -> String {
        format!("{}{}", key.encode(), Self::METADATA)
    }
    fn map_read_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_read_error(key, &self.store_name(), &format!("{e} (OpenDAL Read Error)")))
    }
    fn map_write_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_write_error(key, &self.store_name(), &format!("{e} (OpenDAL Write Error)")))
    }

}

impl Store for OpenDALStore {
    fn store_name(&self) -> String {
        "OpenDALStore".to_string()
    }

    fn key_prefix(&self) -> Key {
        self.prefix.clone()
    }
    
    fn default_metadata(&self, _key: &Key, _is_dir: bool) -> MetadataRecord {
        MetadataRecord::new()
    }
    
    fn finalize_metadata(
        &self,
        metadata: Metadata,
        _key: &Key,
        _data: &[u8],
        _update: bool,
    ) -> Metadata {
        metadata
    }
    
    fn finalize_metadata_empty(
        &self,
        metadata: Metadata,
        _key: &Key,
        _is_dir: bool,
        _update: bool,
    ) -> Metadata {
        metadata
    }
    
    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), liquers_core::error::Error> {
        Ok((self.get_bytes(key)?, self.get_metadata(key)?))
    }
    
    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, liquers_core::error::Error> {
        let path = self.key_to_path(key);
        let buf = self.map_read_error(key, self.op.read(&path))?;
        Ok(buf.to_vec())
    }
    
    fn get_metadata(&self, key: &Key) -> Result<Metadata, liquers_core::error::Error> {
        let path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&path))? {
            let buffer = self.map_read_error(key, self.op.read(&path))?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::MetadataRecord(metadata));
            }
            let buffer = self.map_read_error(key, self.op.read(&path))?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            Err(Error::key_read_error(
                key,
                &self.store_name(),
                "Metadata parsing error",
            ))
        } else {
            Err(Error::key_not_found(key))
        }
    }
    
    fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), liquers_core::error::Error> {
        //TODO: create_dir
        let path = self.key_to_path(key);
        let buffer = Buffer::from_iter(data.iter().copied());
        self.map_write_error(key, self.op.write(&path, buffer))?;
        self.set_metadata(key, metadata)?;
        Ok(())
    }
    
    fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), liquers_core::error::Error> {
        //TODO: create_dir
        let path = self.key_to_path_metadata(key);
        let file = self.map_write_error(key, self.op.writer(&path))?.into_std_write();
        match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        Ok(())
    }
    
    fn remove(&self, key: &Key) -> Result<(), liquers_core::error::Error> {
        let path = self.key_to_path(key);
        if self.map_read_error(key, self.op.exists(&path))? {
            self.map_write_error(key, self.op.delete(&path))?;
        }
        let matadata_path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&matadata_path))? {
            self.map_write_error(key, self.op.delete(&matadata_path))?;
        }
        Ok(())
    }
    
    /// Remove directory.
    /// The key must be a directory.
    /// Files are not removed recursively.
    fn removedir(&self, key: &Key) -> Result<(), liquers_core::error::Error> {
        let path = self.key_to_path(key);
        self.map_write_error(key, self.op.remove_all(&path))
    }
    
    fn contains(&self, key: &Key) -> Result<bool, liquers_core::error::Error> {
        let path = self.key_to_path(key);
        if self.map_read_error(key, self.op.exists(&path))? {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&metadata_path))? {
            return Ok(true);
        }
        Ok(false)
    }
    
    fn is_dir(&self, key: &Key) -> Result<bool, liquers_core::error::Error> {
        let path = self.key_to_path(key);
        let stat = self.map_read_error(key, self.op.stat(&path))?;
        Ok(stat.is_dir())
    }
    
    fn keys(&self) -> Result<Vec<Key>, liquers_core::error::Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix())?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }
    
    fn listdir(&self, key: &Key) -> Result<Vec<String>, liquers_core::error::Error> {
        let mut list = BTreeSet::new();
        let path = self.key_to_path(key);
        let entries = self.map_read_error(key, self.op.list(&path))?;
        for entry in entries {
            let mut name = entry.name().to_string();
            if name.ends_with(Self::METADATA) {
                name = name.trim_end_matches(Self::METADATA).to_string();
            }            
            list.insert(name);
        }
        Ok(list.into_iter().collect())
    }
    
    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, liquers_core::error::Error> {
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }
    
    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, liquers_core::error::Error> {
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(&key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }
    
    fn makedir(&self, key: &Key) -> Result<(), liquers_core::error::Error> {
        let path = format!("{}/",self.key_to_path(key));
        self.map_write_error(key, self.op.create_dir(&path))
    }
    
    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix)
            && (!key
                .filename()
                .is_some_and(|file_name| file_name.name.ends_with(Self::METADATA)))
    }
}

#[cfg(feature = "async_store")]
pub struct AsyncOpenDALStore {
    op: Operator,
    prefix: Key,
}


#[cfg(feature = "async_store")]
impl AsyncOpenDALStore {
    const METADATA: &'static str = ".__metadata__";

    pub fn new(op: Operator, prefix: Key) -> Self {
        AsyncOpenDALStore { op, prefix }
    }

    pub fn key_to_path(&self, key: &Key) -> String {
        key.encode()
    }

    pub fn key_to_path_metadata(&self, key: &Key) -> String {
        format!("{}{}", key.encode(), Self::METADATA)
    }
    fn map_read_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_read_error(key, &self.store_name(), &format!("{e} (OpenDAL Read Error)")))
    }
    fn map_write_error<T>(&self, key:&Key, res:opendal::Result<T>)->Result<T, liquers_core::error::Error> {
        res.map_err(|e| liquers_core::error::Error::key_write_error(key, &self.store_name(), &format!("{e} (OpenDAL Write Error)")))
    }
}

#[cfg(feature = "async_store")]
#[async_trait(?Send)]
impl AsyncStore for AsyncOpenDALStore{
    /// Get store name
    fn store_name(&self) -> String {
        format!("{} Store", self.key_prefix())
    }

    /// Key prefix common to all keys in this store.
    fn key_prefix(&self) -> Key {
        Key::new()
    }

    /// Create default metadata object for a given key
    fn default_metadata(&self, _key: &Key, _is_dir: bool) -> MetadataRecord {
        MetadataRecord::new()
    }

    /// Finalize metadata before storing - when data is available
    /// This can't be a directory
    fn finalize_metadata(
        &self,
        metadata: Metadata,
        _key: &Key,
        _data: &[u8],
        _update: bool,
    ) -> Metadata {
        metadata
    }

    /// Finalize metadata before storing - when data is not available
    fn finalize_metadata_empty(
        &self,
        metadata: Metadata,
        _key: &Key,
        _is_dir: bool,
        _update: bool,
    ) -> Metadata {
        metadata
    }

    /// Get data asynchronously
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>{
        Ok((self.get_bytes(key).await?, self.get_metadata(key).await?))
    }

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let path = self.key_to_path(key);
        let buf = self.map_read_error(key, self.op.read(&path).await)?;
        Ok(buf.to_vec())
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&path).await)? {
            let buffer = self.map_read_error(key, self.op.read(&path).await)?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::MetadataRecord(metadata));
            }
            let buffer = self.map_read_error(key, self.op.read(&path).await)?;
            if let Ok(metadata) = serde_json::from_reader(buffer.reader()) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            Err(Error::key_read_error(
                key,
                &self.store_name(),
                "Metadata parsing error",
            ))
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Store data and metadata.
    async fn set(&mut self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        //TODO: create_dir
        let path = self.key_to_path(key);
        let buffer = Buffer::from_iter(data.iter().copied());
        self.map_write_error(key, self.op.write(&path, buffer).await)?;
        self.set_metadata(key, metadata).await?;
        Ok(())
    }

    /// Store metadata only
    async fn set_metadata(&mut self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        //TODO: create_dir
        let metadata_str = match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_string_pretty(metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_string_pretty(metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        let path = self.key_to_path_metadata(key);
        self.map_write_error(key, self.op.write(&path, metadata_str).await)
    }

    /// Remove data and metadata associated with the key
    async fn remove(&mut self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key);
        if self.map_read_error(key, self.op.exists(&path).await)? {
            self.map_write_error(key, self.op.delete(&path).await)?;
        }
        let matadata_path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&matadata_path).await)? {
            self.map_write_error(key, self.op.delete(&matadata_path).await)?;
        }
        Ok(())
    }

    /// Remove directory.
    /// The key must be a directory.
    /// Files are not removed recursively.
    async fn removedir(&mut self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key);
        self.map_write_error(key, self.op.remove_all(&path).await)
    }

    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key);
        if self.map_read_error(key, self.op.exists(&path).await)? {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key);
        if self.map_read_error(key, self.op.exists(&metadata_path).await)? {
            return Ok(true);
        }
        Ok(false)
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key);
        let stat = self.map_read_error(key, self.op.stat(&path).await)?;
        Ok(stat.is_dir())
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
        let mut list = BTreeSet::new();
        let path = self.key_to_path(key);
        let entries = self.map_read_error(key, self.op.list(&path).await)?;
        for entry in entries {
            let mut name = entry.name().to_string();
            if name.ends_with(Self::METADATA) {
                name = name.trim_end_matches(Self::METADATA).to_string();
            }            
            list.insert(name);
        }
        Ok(list.into_iter().collect())
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
            if self.is_dir(&key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let path = format!("{}/",self.key_to_path(key));
        self.map_write_error(key, self.op.create_dir(&path).await)
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
        key.has_key_prefix(&self.prefix)
            && (!key
                .filename()
                .is_some_and(|file_name| file_name.name.ends_with(Self::METADATA)))
    }

}