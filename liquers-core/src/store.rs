use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use thiserror::Error;

use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Key not found: {0}")]
    KeyNotFound(Key),
    #[error("Key {0} not supported by store {1}")]
    KeyNotSupported(Key, String),
    #[error("Error reading key {0}, store {1}")]
    KeyReadError(Key, String),
    #[error("Error writing key {0}, store {1}")]
    KeyWriteError(Key, String),
}

pub trait Store {
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

    /// Get data and metadata
    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), StoreError> {
        Err(StoreError::KeyNotFound(key.to_owned()))
    }

    /// Get data as bytes
    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::KeyNotFound(key.to_owned()))
    }

    /// Get metadata
    fn get_metadata(&self, key: &Key) -> Result<Metadata, StoreError> {
        Err(StoreError::KeyNotFound(key.to_owned()))
    }

    /// Store data and metadata.
    fn set(&mut self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), StoreError> {
        Err(StoreError::KeyNotSupported(
            key.to_owned(),
            self.store_name(),
        ))
    }

    /// Store metadata only
    fn set_metadata(&mut self, key: &Key, _metadata: &Metadata) -> Result<(), StoreError> {
        Err(StoreError::KeyNotSupported(
            key.to_owned(),
            self.store_name(),
        ))
    }

    /// Remove data and metadata associated with the key
    fn remove(&mut self, key: &Key) -> Result<(), StoreError> {
        Err(StoreError::KeyNotSupported(
            key.to_owned(),
            self.store_name(),
        ))
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    fn removedir(&mut self, key: &Key) -> Result<(), StoreError> {
        Err(StoreError::KeyNotSupported(
            key.to_owned(),
            self.store_name(),
        ))
    }

    /// Returns true if store contains the key.
    fn contains(&self, _key: &Key) -> bool {
        false
    }

    /// Returns true if key points to a directory.
    fn is_dir(&self, _key: &Key) -> bool {
        false
    }

    /// List or iterator of all keys
    fn keys(&self) -> Result<Vec<Key>, StoreError> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix())?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    fn listdir(&self, _key: &Key) -> Result<Vec<String>, StoreError> {
        Ok(vec![])
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, StoreError> {
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, StoreError> {
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(&key) {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    fn makedir(&self, key: &Key) -> Result<(), StoreError> {
        Err(StoreError::KeyNotSupported(
            key.to_owned(),
            self.store_name(),
        ))
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

pub struct NoStore;

impl Store for NoStore {}


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
    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), StoreError> {
        let data = self.get_bytes(key)?;
        match self.get_metadata(key) {
            Ok(metadata) => Ok((data, metadata)),
            Err(_) => Ok((data, Metadata::MetadataRecord(MetadataRecord::new()))),
        }
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, StoreError> {
        let path = self.key_to_path(key);
        if path.exists() {
            let mut file = File::open(path)
                .map_err(|_| StoreError::KeyReadError(key.to_owned(), self.store_name()))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|_| StoreError::KeyReadError(key.to_owned(), self.store_name()))?;
            Ok(buffer)
        } else {
            Err(StoreError::KeyNotFound(key.to_owned()))
        }
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, StoreError> {
        let path = self.key_to_path_metadata(key);
        if path.exists() {
            let mut file = File::open(path)
                .map_err(|_| StoreError::KeyReadError(key.to_owned(), self.store_name()))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|_| StoreError::KeyReadError(key.to_owned(), self.store_name()))?;
            if let Ok(metadata) = serde_json::from_reader(&buffer[..]) {
                return Ok(Metadata::MetadataRecord(metadata));
            }
            if let Ok(metadata) = serde_json::from_reader(&buffer[..]) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            Err(StoreError::KeyReadError(key.to_owned(), self.store_name()))
        } else {
            Err(StoreError::KeyNotFound(key.to_owned()))
        }
    }

    fn set(&mut self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), StoreError> {
        let path = self.key_to_path(key);
        let mut file = File::create(path)
            .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        file.write_all(data)
            .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        self.set_metadata(key, metadata)?;
        Ok(())
    }

    fn set_metadata(&mut self, key: &Key, metadata: &Metadata) -> Result<(), StoreError> {
        let path = self.key_to_path_metadata(key);
        let file = File::create(path)
            .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_writer_pretty(file, metadata)
                .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?,
        };
        Ok(())
    }

    fn remove(&mut self, key: &Key) -> Result<(), StoreError> {
        let path = self.key_to_path(key);
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        }
        let matadata_path = self.key_to_path_metadata(key);
        if matadata_path.exists() {
            std::fs::remove_file(matadata_path)
                .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        }
        Ok(())
    }

    fn removedir(&mut self, key: &Key) -> Result<(), StoreError> {
        let path = self.key_to_path(key);
        if path.exists() {
            std::fs::remove_dir_all(path)
                .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> bool {
        let path = self.key_to_path(key);
        if path.exists() {
            return true;
        }
        let metadata_path = self.key_to_path_metadata(key);
        if metadata_path.exists() {
            return true;
        }
        false
    }

    fn is_dir(&self, key: &Key) -> bool {
        let path = self.key_to_path(key);
        if path.exists() {
            return path.is_dir();
        }
        false
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, StoreError> {
        let path = self.key_to_path(key);
        if path.exists() {
            let dir = path
                .read_dir()
                .map_err(|_| StoreError::KeyReadError(key.to_owned(), self.store_name()))?;
            let names = dir
                .flat_map(|entry| {
                    entry
                        .ok()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                })
                .filter(|name| !name.ends_with(Self::METADATA))
                .collect();
            return Ok(names);
        }
        Err(StoreError::KeyNotFound(key.to_owned()))
    }

    fn makedir(&self, key: &Key) -> Result<(), StoreError> {
        let path = self.key_to_path(key);
        std::fs::create_dir_all(path)
            .map_err(|_| StoreError::KeyWriteError(key.to_owned(), self.store_name()))?;
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix)
    }
}


pub struct MemoryStore {
    data: std::collections::HashMap<Key, (Vec<u8>, Metadata)>,
    prefix: Key,
}

impl MemoryStore {
    pub fn new(prefix: &Key) -> MemoryStore {
        MemoryStore {
            data: std::collections::HashMap::new(),
            prefix: prefix.to_owned(),
        }
    }
}

impl Store for MemoryStore {
    fn store_name(&self) -> String {
        format!(
            "{} Memory store",
            self.key_prefix()
        )
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
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

    fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), StoreError> {
        match self.data.get(key) {
            Some((data, metadata)) => Ok((data.to_owned(), metadata.to_owned())),
            None => Err(StoreError::KeyNotFound(key.to_owned())),
        }
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, StoreError> {
        match self.data.get(key) {
            Some((data, _)) => Ok(data.to_owned()),
            None => Err(StoreError::KeyNotFound(key.to_owned())),
        }
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, StoreError> {
        match self.data.get(key) {
            Some((_, metadata)) => Ok(metadata.to_owned()),
            None => Err(StoreError::KeyNotFound(key.to_owned())),
        }
    }

    fn set(&mut self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), StoreError> {
        self.data.insert(key.to_owned(), (data.to_owned(), metadata.to_owned()));
        Ok(())
    }

    fn set_metadata(&mut self, key: &Key, metadata: &Metadata) -> Result<(), StoreError> {
        if let Some((data, _)) = self.data.get(key) {
            self.data.insert(key.to_owned(), (data.to_owned(), metadata.to_owned()));
            Ok(())
        } else {
            Err(StoreError::KeyNotFound(key.to_owned()))
        }
    }

    fn remove(&mut self, key: &Key) -> Result<(), StoreError> {
        self.data.remove(key);
        Ok(())
    }

    fn removedir(&mut self, key: &Key) -> Result<(), StoreError> {
        let keys = self.data.keys().filter(|k| k.has_key_prefix(key)).cloned().collect::<Vec<_>>();
        for k in keys {
            self.data.remove(&k);
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> bool {
        self.data.contains_key(key)
    }

    fn is_dir(&self, key: &Key) -> bool {

        let keys = self.data.keys().filter(|k| k.has_key_prefix(key)).cloned().collect::<Vec<_>>();
        for k in keys {
            if k.len() > key.len() {
                return true;
            }
        }
        false
    }

    fn keys(&self) -> Result<Vec<Key>, StoreError> {
        let keys = self.data.keys().cloned().collect::<Vec<_>>();
        Ok(keys)
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, StoreError> {
        let keys = self.listdir_keys(key)?;
        Ok(keys.iter().map(|x| x.to_string()).collect())
    }

    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, StoreError> {
        let n = key.len() + 1;
        let keys = self.data.keys().filter(|k| k.has_key_prefix(key) && k.len()==n).cloned().collect::<Vec<_>>();
        Ok(keys)
    }

    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, StoreError> {
        let keys = self.data.keys().filter(|k| k.has_key_prefix(key)).cloned().collect::<Vec<_>>();
        Ok(keys)
    }

    fn makedir(&self, key: &Key) -> Result<(), StoreError> {
        // TODO: implement correct makedir
        Ok(())
    }

    fn is_supported(&self, _key: &Key) -> bool {
        true
    }
    
}

// Unittests
#[cfg(test)]
mod tests {
    //    use crate::query::Key;

    use super::*;

    use crate::parse::parse_key;

    #[test]
    fn test_simple_store() {
        let mut store = MemoryStore::new(&Key::new());
        let key = parse_key("a/b/c").unwrap();
        let data = b"test data".to_vec();
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());

        assert!(!store.contains(&key));
        assert!(store.keys().unwrap().is_empty());
        assert!(!store.is_dir(&parse_key("a/b").unwrap()));

        store.set(&key, &data, &metadata).unwrap();
        assert!(store.contains(&key));
        assert!(store.keys().unwrap().contains(&key));
        assert!(store.is_dir(&parse_key("a/b").unwrap()));
        assert_eq!(store.keys().unwrap().len(), 1);

        let (data2, metadata2) = store.get(&key).unwrap();
        assert_eq!(data, data2);
        store.remove(&key).unwrap();
        assert!(!store.contains(&key));
    }
}
