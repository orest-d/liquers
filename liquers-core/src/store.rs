//! Key-value stores: the [`Store`] and [`AsyncStore`] traits, the routers that compose them, and
//! the in-memory and filesystem backends.
//!
//! # A store key is absolute
//!
//! Every key handed to a store must be absolute: no element may be `.` or `..`. Relative keys are
//! a *plan-level* construct — they are meaningful against a current working directory, and
//! [`Key::to_absolute`] resolves them while the plan is built. Nothing below that layer resolves
//! them, so a relative key arriving at a store is a malformed address, and the store refuses it
//! with [`crate::error::ErrorType::KeyNotAbsolute`].
//!
//! **Refusal, not normalization.** A key is an address, not a path. Silently rewriting `a/../b` to
//! `b` would make two distinct addresses alias one asset, which is worse than rejecting a key
//! nobody meant to write.
//!
//! This is a **well-formedness** rule, not authorization. It says nothing about who may read or
//! write a key; per-key permission is a separate, orthogonal question (see
//! `specs/issues/CORE-SESSION-AND-KEY-ACL.md`). That the rule also closes a path-traversal hole in
//! the filesystem backends is a consequence of it, not its definition.
//!
//! ## Implementing a store
//!
//! Call [`Key::as_absolute`] at the top of every fallible method that takes a key, shadowing the
//! parameter so the unchecked key cannot be used afterwards:
//!
//! ```ignore
//! async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
//!     let key = key.as_absolute()?;
//!     // … only the checked `key` is reachable from here
//! }
//! ```
//!
//! Writes as well as reads: a read-only guard leaves the write path open.
//!
//! [`Store::is_supported`] should *also* reject relative keys, but **it is not the enforcement
//! point.** Only [`StoreRouter`] and [`AsyncStoreRouter`] consult it, so a store held directly —
//! which is how an `Environment` is often configured, and how store unit tests construct one —
//! never runs it. A backend guarded only in `is_supported` therefore passes a routed test and is
//! wide open when used directly.
//!
//! A backend that maps keys onto backend paths gets the check structurally as well: the path
//! builders of [`FileStore`], [`AsyncFileStore`] and the OpenDAL store are fallible, so the
//! backend cannot be reached without passing.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::error::Error;
use crate::metadata::{self, AssetInfo, Metadata, MetadataRecord};
use crate::query::Key;
use crate::store_dir_index::DirectoryIndex;

#[cfg(not(target_arch = "wasm32"))]
use tokio::time::{sleep, Duration};

/// A synchronous key-value store.
///
/// **Every key must be absolute** — see the [module documentation](self#a-store-key-is-absolute).
/// Implementors call [`Key::as_absolute`] at the top of each fallible key-taking method;
/// [`Self::is_supported`] is consulted only by [`StoreRouter`] and is not sufficient on its own.
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
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Store metadata only
    fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove data and metadata associated with the key
    fn remove(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    fn removedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Returns true if store contains the key.
    fn contains(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
        Ok(false)
    }

    /// Returns true if key points to a directory.
    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
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
    fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        key.as_absolute()?;
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
            // See the async twin: the guard is about the child.
            if self.is_dir(&sub_key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    fn makedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns whether this store supports the supplied key.
    ///
    /// A supported key must be absolute, must start with [`Store::key_prefix`], and must pass any
    /// narrower backend-specific filter. For example, a single-file overlay may have an empty
    /// prefix but return `true` only for the one file it intercepts, allowing later stores in a
    /// router to handle every other key. Fallible operations must still enforce absolute keys
    /// themselves. See the
    /// [module documentation](self#a-store-key-is-absolute).
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
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
/// An asynchronous key-value store. This is the trait new backends implement.
///
/// **Every key must be absolute** — see the [module documentation](self#a-store-key-is-absolute).
/// Implementors call [`Key::as_absolute`] at the top of each fallible key-taking method;
/// [`Self::is_supported`] is consulted only by [`AsyncStoreRouter`] and is not sufficient on its
/// own. `openbin`, when it is implemented, is one more such method and needs the same check.
pub trait AsyncStore: crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync {
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
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error>;

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove a directory and everything under it.
    ///
    /// **Specified by its postcondition:** if this returns `Ok(())`, the directory does not exist
    /// afterwards. Failing to remove it is an error; what is forbidden is claiming success without
    /// the effect. Recursion follows rather than being stipulated separately — a directory derived
    /// from its children exists while any child remains, so a removal that left one and reported
    /// `Ok(())` would break the postcondition.
    ///
    /// On a directory that does not exist, `Ok(())` is correct: the postcondition already holds.
    /// This default returns `Err(KeyNotSupported)` instead, which is also correct — a store that
    /// has not implemented directory removal is *refusing*, not silently succeeding.
    ///
    /// Not atomic on any backend. See `specs/reference/STORE_SEMANTICS.md` §5.
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Returns true if store contains the key.
    ///
    /// Falls back to [`Self::is_dir`], so a store that answers for directories does not have to
    /// restate that a directory is contained. `AsyncMemoryStore` and `liquers-web`'s
    /// `LocalStorageStore` both wrote this by hand before it was a default; a store that overrode
    /// `is_dir` and not `contains` got the two disagreeing, silently.
    ///
    /// The absoluteness check stays first: the fallback must not weaken the refusal.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
        self.is_dir(key).await
    }

    /// Returns true if key points to a directory.
    ///
    /// `Ok(false)` for a key that is simply absent — **not** an error. A backend failure
    /// (permissions, network) is still an error; the two are different answers and callers rely on
    /// the distinction.
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        key.as_absolute()?;
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
        key.as_absolute()?;
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
            // `sub_key`, not `key`: the guard decides whether to descend into the *child*.
            // Testing the parent made it a constant, so every child including data keys was
            // recursed into. CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY.
            if self.is_dir(&sub_key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        key.as_absolute()?;
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns whether this store supports the supplied key.
    ///
    /// A supported key must be absolute, must start with [`AsyncStore::key_prefix`], and must pass
    /// any narrower backend-specific filter. For example, a single-file overlay may have an empty
    /// prefix but return `true` only for the one file it intercepts, allowing later stores in a
    /// router to handle every other key. Fallible operations must still enforce absolute keys
    /// themselves. See the
    /// [module documentation](self#a-store-key-is-absolute).
    fn is_supported(&self, _key: &Key) -> bool {
        false
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
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for NoAsyncStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        // The absoluteness check comes first even here. A store that holds nothing still has to say
        // *no* correctly: reporting a relative key as merely "not found" hides a malformed key
        // behind an ordinary miss, and `contains` on this same store already refuses it properly.
        // Caught by `keyshape01` — this store had never been run against the suite.
        key.as_absolute()?;
        Err(Error::key_not_found(key))
    }

    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, "NoAsyncStore"))
    }
}

/// Async-native in-memory store implementation.
pub struct AsyncMemoryStore {
    data: scc::HashMap<Key, (Arc<[u8]>, Metadata)>,
    /// Directory structure derived from the stored keys.
    ///
    /// The mechanism used to live here as a private field and a handful of private methods; it is
    /// now `store_dir_index::DirectoryIndex`, shared with every other store that has to derive
    /// directories from a flat key set. See `CORE-DIRECTORY-INDEX-NOT-SHARED`.
    dir_index: DirectoryIndex,
    prefix: Key,
}
impl AsyncMemoryStore {
    pub fn new(prefix: &Key) -> Self {
        Self {
            data: scc::HashMap::new(),
            dir_index: DirectoryIndex::new(),
            prefix: prefix.to_owned(),
        }
    }

    async fn add_key_to_index(&self, key: &Key) {
        self.dir_index.insert_key(key).await;
    }

    async fn remove_key_from_index(&self, key: &Key) {
        self.dir_index.remove_key(key).await;
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for AsyncMemoryStore {
    fn store_name(&self) -> String {
        format!("{} Async memory store", self.key_prefix())
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        let mut metadata = MetadataRecord::new();
        metadata.with_key(key.to_owned());
        metadata.is_dir = is_dir;
        metadata
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let key = key.as_absolute()?;
        if let Some((data, metadata)) = self
            .data
            .read_async(key, |_key, (data, metadata)| {
                (data.clone(), metadata.clone())
            })
            .await
        {
            return Ok((data.as_ref().to_vec(), metadata));
        }
        Err(Error::key_not_found(key))
    }

    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let key = key.as_absolute()?;
        if let Some(data) = self
            .data
            .read_async(key, |_key, (data, _metadata)| data.clone())
            .await
        {
            return Ok(data.as_ref().to_vec());
        }
        Err(Error::key_not_found(key))
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let key = key.as_absolute()?;
        if self.is_dir(key).await? {
            let mut metadata = self.default_metadata(key, true);
            metadata.children = self.listdir_asset_info(key).await?;
            return Ok(Metadata::MetadataRecord(metadata));
        }
        if let Some(metadata) = self
            .data
            .read_async(key, |_key, (_data, metadata)| metadata.clone())
            .await
        {
            return Ok(metadata);
        }
        Err(Error::key_not_found(key))
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        let was_new = self
            .data
            .upsert_async(
                key.to_owned(),
                (Arc::<[u8]>::from(data.to_vec()), metadata.clone()),
            )
            .await
            .is_none();
        if was_new {
            self.add_key_to_index(key).await;
        }
        Ok(())
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if self
            .data
            .update_async(key, |_k, (_data, current_metadata)| {
                *current_metadata = metadata.clone();
            })
            .await
            .is_some()
        {
            return Ok(());
        }

        let inserted = self
            .data
            .insert_async(
                key.to_owned(),
                (Arc::<[u8]>::from(Vec::<u8>::new()), metadata.clone()),
            )
            .await
            .is_ok();
        if inserted {
            self.add_key_to_index(key).await;
            return Ok(());
        }

        let _ = self
            .data
            .update_async(key, |_k, (_data, current_metadata)| {
                *current_metadata = metadata.clone();
            })
            .await;
        Ok(())
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if self.data.remove_async(key).await.is_some() {
            self.remove_key_from_index(key).await;
        }
        Ok(())
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        let mut keys_to_remove = Vec::new();
        let _ = self
            .data
            .iter_async(|stored_key, _| {
                if stored_key.has_key_prefix(key) {
                    keys_to_remove.push(stored_key.clone());
                }
                true
            })
            .await;
        for key_to_remove in keys_to_remove {
            if self.data.remove_async(&key_to_remove).await.is_some() {
                self.remove_key_from_index(&key_to_remove).await;
            }
        }
        // An explicitly created directory survives losing its children, so `removedir` is what
        // takes it away — and it must take explicit directories *beneath* it too, or a `makedir`
        // descendant would outlive the recursive removal that just succeeded.
        self.dir_index.remove_directory_tree(key).await;
        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
        if self.data.contains_async(key).await {
            return Ok(true);
        }
        Ok(self.dir_index.is_dir(key).await)
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
        Ok(self.dir_index.is_dir(key).await)
    }

    /// Data keys, the directories above them, and this store's own prefix.
    ///
    /// Inherits the `AsyncStore` default rather than enumerating the data map directly. Returning
    /// data keys alone was `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`: one stored object yielded
    /// one key here and four in every other store, and a router could return both shapes at once.
    /// `STORE_SEMANTICS.md` §9 settles it — and note the cost it names, that a key returned here is
    /// not necessarily one `get` will succeed on, because a directory is enumerated and cannot be
    /// read as data.
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        // Built here rather than from `listdir_keys_deep`, which for this store deliberately
        // enumerates the *data* map and so never yields a derived directory.
        let data_keys = self.listdir_keys_deep(&self.prefix).await?;
        let mut keys: Vec<Key> = vec![self.prefix.clone()];
        for key in data_keys {
            // Every proper prefix of a stored key is a directory (§2), and §9 says they are
            // enumerated alongside the data keys.
            let mut ancestor = key.parent();
            while ancestor.len() > self.prefix.len() {
                if !keys.contains(&ancestor) {
                    keys.push(ancestor.clone());
                }
                ancestor = ancestor.parent();
            }
            keys.push(key);
        }
        Ok(keys)
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let key = key.as_absolute()?;
        let keys = self.listdir_keys(key).await?;
        Ok(keys
            .iter()
            .filter_map(|k| k.filename().map(|f| f.to_string()))
            .collect())
    }

    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        Ok(self.dir_index.child_keys(key).await)
    }

    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        let mut keys = Vec::new();
        let _ = self
            .data
            .iter_async(|stored_key, _| {
                if stored_key.has_key_prefix(key) {
                    keys.push(stored_key.clone());
                }
                true
            })
            .await;
        Ok(keys)
    }

    /// Creates a directory that exists in its own right.
    ///
    /// This used to validate its key and return `Ok(())`, recording nothing: the caller was told a
    /// directory had been created and none existed. The cause was structural — a directory index
    /// derived from stored keys cannot represent a directory with no children — and it made
    /// `PUT /api/store/makedir/{*key}` a no-op against a memory-backed store.
    /// `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`.
    ///
    /// An explicitly created directory outlives its children: removing the last file from it
    /// leaves the directory the caller asked for, and only `removedir` takes it away.
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.dir_index.insert_directory(key).await;
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative() && key.has_key_prefix(&self.prefix)
    }
}

/// The metadata sidecar suffix: the metadata for `foo` lives at `foo.__metadata__`.
pub const METADATA_SUFFIX: &str = ".__metadata__";

/// The lock-file suffix [`AsyncFileStore`] takes while writing: the lock for `foo` is
/// `foo.__lock__`.
pub const LOCK_SUFFIX: &str = ".__lock__";

/// The legacy metadata *folder* name, reserved exactly rather than as a suffix.
///
/// This is not a layout any store in this repository uses. It is the predecessor Python
/// implementation's: in [`orest-d/liquer`](https://github.com/orest-d/liquer), `liquer/store.py`
/// (at `2eb4e64`) declares `METADATA = "__metadata__"` and puts the metadata for `sub/foo.txt` at
/// `sub/__metadata__/foo.txt.json`. That implementation refuses the name as a filename *and* in
/// any interior position, and filters it out of listings — the same three rules this module
/// applies.
///
/// It is reserved here so that layout stays readable if support for it is ever wanted, and the
/// citation is in this doc comment because **nothing in this repository evidences the layout**: a
/// reservation whose reason is invisible is the one a future reader deletes.
pub const METADATA_FOLDER: &str = "__metadata__";

/// The names a store's metadata layout reserves, and therefore the keys it must refuse.
///
/// A sidecar layout keeps the metadata for `foo` at `foo.__metadata__`, which makes the key
/// `foo.__metadata__` unaddressable — its *data* path is that same byte string. Such keys are
/// refused rather than silently colliding, because a store must not accept a key it cannot address
/// unambiguously (`STORE_SEMANTICS.md` §8).
///
/// Three kinds of caller ask the same question through this type, and all three must:
///
/// 1. [`AsyncStore::is_supported`], which is only a **routing hint** — `AsyncStoreRouter` consults
///    it and a direct caller need not.
/// 2. The path builders, so that every fallible method inherits the refusal. This is the half that
///    was missing, and `liquers-axum`'s store handlers are the callers that reached the filesystem
///    through it (`CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`).
/// 3. The listing filters. Not optional: `listdir_keys_deep` calls `is_dir` on every child, so an
///    unfiltered reserved name turns a refusal into a *failed enumeration* — the store stops being
///    listable at all. §8 requires listings to skip what they cannot address, not fail on it.
///
/// A predicate rather than a fallible function because [`AsyncStore::is_supported`] returns `bool`
/// and cannot carry an error; the *store* raises the refusal, because `Error::key_not_supported`
/// needs a store name this type cannot reach (`CORE-ERROR-STORE-NAME-NOT-STRUCTURED`).
///
/// Each store declares what its **own** layout uses. Over-reserving is a defect in the same family
/// as under-reserving: `x.__lock__` is a key `AsyncOpenDALStore` can address perfectly well,
/// because it takes no locks.
///
/// See `specs/design/sidecar-colliding-keys/` and `specs/reference/STORE_SEMANTICS.md` §8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedNames {
    /// Reserved as a suffix: any segment *ending* in one of these.
    suffixes: &'static [&'static str],
    /// Reserved exactly: a segment equal to one of these.
    ///
    /// Deliberately separate from `suffixes` rather than derived from it by stripping a leading
    /// dot. Deriving is shorter and wrong — it would reserve the bare name `__lock__`, and no
    /// layout has ever used a `__lock__` directory, so the store would refuse a key for no reason
    /// and the contract would claim a layout that does not exist.
    exact: &'static [&'static str],
}

impl ReservedNames {
    /// Declares what a layout reserves. `const` so a store holds the result as an associated
    /// constant rather than rebuilding it per call.
    pub const fn new(
        suffixes: &'static [&'static str],
        exact: &'static [&'static str],
    ) -> Self {
        Self { suffixes, exact }
    }

    /// True when one directory-entry name is reserved.
    ///
    /// Takes `&str` because the listing filters have a name and no key, and building a key per
    /// entry in order to ask would allocate for nothing.
    pub fn is_reserved_name(&self, name: &str) -> bool {
        self.suffixes.iter().any(|suffix| name.ends_with(suffix))
            || self.exact.iter().any(|reserved| name == *reserved)
    }

    /// True when **any** segment of the key is reserved.
    ///
    /// Any, not the last: `dir.__metadata__/child` needs `dir.__metadata__` to be a directory,
    /// while the metadata of `dir` needs it to be a file, so the key is unaddressable even though
    /// its filename is innocent. [`Key::filename`] is the last segment only, which is how the
    /// original check missed this shape.
    pub fn is_reserved_key(&self, key: &Key) -> bool {
        key.iter().any(|segment| self.is_reserved_name(&segment.name))
    }
}

/// Async-native file store implementation.
/// Uses `tokio::fs`, unavailable on wasm — excluded from `wasm32` targets.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct AsyncFileStore {
    pub path: PathBuf,
    pub prefix: Key,
}

#[cfg(not(target_arch = "wasm32"))]
impl AsyncFileStore {
    /// What this store's layout reserves: the metadata sidecar, the lock it takes while writing,
    /// and the legacy metadata folder name.
    const RESERVED: ReservedNames =
        ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX], &[METADATA_FOLDER]);

    pub fn new(path: &str, prefix: &Key) -> Self {
        Self {
            path: PathBuf::from(path),
            prefix: prefix.to_owned(),
        }
    }

    /// Raises the refusal for a key this store's layout cannot represent.
    ///
    /// The rule lives in [`ReservedNames`]; the error is raised here because
    /// `Error::key_not_supported` needs this store's name, which the predicate cannot reach.
    fn reject_reserved(&self, key: &Key) -> Result<(), Error> {
        if Self::RESERVED.is_reserved_key(key) {
            return Err(Error::key_not_supported(key, &self.store_name()));
        }
        Ok(())
    }

    /// Maps a key onto a filesystem path under the store root.
    ///
    /// Fallible because this is where a relative key would become a real path traversal:
    /// `PathBuf::push` appends the key verbatim, and the operating system then resolves `..`
    /// against the root. Checking here means no method can reach the filesystem without the key
    /// having passed. See the [module documentation](self#a-store-key-is-absolute).
    pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error> {
        let key = key.as_absolute()?;
        self.reject_reserved(&key)?;
        let mut path = self.path.clone();
        path.push(key.to_string());
        Ok(path)
    }

    /// Maps a key onto the path of its metadata file. Fallible for the same reason as
    /// [`Self::key_to_path`].
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error> {
        let key = key.as_absolute()?;
        self.reject_reserved(&key)?;
        let mut path = self.path.clone();
        path.push(format!("{}{}", key, METADATA_SUFFIX));
        Ok(path)
    }

    fn key_to_lock_path(&self, key: &Key) -> Result<PathBuf, Error> {
        let key = key.as_absolute()?;
        self.reject_reserved(&key)?;
        let mut path = self.path.clone();
        path.push(format!("{}{}", key, LOCK_SUFFIX));
        Ok(path)
    }

    async fn write_metadata_file(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let path = self.key_to_path_metadata(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        let bytes = match metadata {
            Metadata::MetadataRecord(record) => serde_json::to_vec_pretty(record)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(record) => serde_json::to_vec_pretty(record)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        tokio::fs::write(path, bytes)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))
    }

    async fn acquire_lock(&self, key: &Key) -> Result<FileLockGuard, Error> {
        let lock_path = self.key_to_lock_path(key)?;
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        let mut retries = 0usize;
        loop {
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
                .await
            {
                Ok(_) => return Ok(FileLockGuard { path: lock_path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    retries += 1;
                    if retries > 300 {
                        return Err(Error::key_write_error(
                            key,
                            &self.store_name(),
                            format!("Timed out acquiring lock for {}", key).as_str(),
                        ));
                    }
                    sleep(Duration::from_millis(10)).await;
                }
                Err(e) => {
                    return Err(Error::key_write_error(key, &self.store_name(), &e));
                }
            }
        }
    }
}
struct FileLockGuard {
    path: PathBuf,
}
impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl AsyncStore for AsyncFileStore {
    fn store_name(&self) -> String {
        format!(
            "{} Async file store in {}",
            self.key_prefix(),
            self.path.display()
        )
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let data = self.get_bytes(key).await?;
        match self.get_metadata(key).await {
            Ok(metadata) => Ok((data, metadata)),
            Err(error) => {
                let mut metadata = self.default_metadata(key, false);
                metadata.warning(&format!("Can't read metadata: {}", error));
                metadata.warning("New metadata has been created. (get)");
                let mut metadata = Metadata::MetadataRecord(metadata);
                self.finalize_metadata(&mut metadata, key, &data, false);
                self.set_metadata(key, &metadata).await?;
                Ok((data, metadata))
            }
        }
    }

    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            return Err(Error::key_not_found(key));
        }
        if tokio::fs::metadata(&path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
            .is_dir()
        {
            return Err(Error::key_not_found(key));
        }
        tokio::fs::read(path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let metadata_path = self.key_to_path_metadata(key)?;
        if tokio::fs::try_exists(&metadata_path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            if tokio::fs::metadata(&metadata_path)
                .await
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
                .is_dir()
            {
                let mut metadata = self.default_metadata(key, true);
                metadata.children = self.listdir_asset_info(key).await?;
                return Ok(Metadata::MetadataRecord(metadata));
            }
            let buffer = tokio::fs::read(metadata_path)
                .await
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            if let Ok(metadata) = serde_json::from_slice(&buffer) {
                return Ok(Metadata::MetadataRecord(metadata));
            }
            if let Ok(metadata) = serde_json::from_slice(&buffer) {
                return Ok(Metadata::LegacyMetadata(metadata));
            }
            return Err(Error::key_read_error(
                key,
                &self.store_name(),
                "Metadata parsing error",
            ));
        }

        let data_path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&data_path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            return Err(Error::key_not_found(key));
        }
        if tokio::fs::metadata(&data_path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
            .is_dir()
        {
            let mut metadata = self.default_metadata(key, true);
            metadata.children = self.listdir_asset_info(key).await?;
            return Ok(Metadata::MetadataRecord(metadata));
        }
        let mut metadata = self.default_metadata(key, false);
        metadata.warning(&format!(
            "Metadata file {} does not exist.",
            self.key_to_path_metadata(key)?.display()
        ));
        metadata.warning("New metadata has been created. (get_metadata)");
        let mut metadata = Metadata::MetadataRecord(metadata);
        let data = self.get_bytes(key).await?;
        self.finalize_metadata(&mut metadata, key, &data, false);
        self.set_metadata(key, &metadata).await?;
        Ok(metadata)
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let _lock = self.acquire_lock(key).await?;
        let path = self.key_to_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }

        let mut tmp_metadata = metadata.clone();
        self.finalize_metadata(&mut tmp_metadata, key, data, true);
        tmp_metadata.set_status(metadata::Status::Storing)?;
        self.write_metadata_file(key, &tmp_metadata).await?;

        tokio::fs::write(&path, data)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;

        self.write_metadata_file(key, metadata).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let _lock = self.acquire_lock(key).await?;
        self.write_metadata_file(key, metadata).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        let _lock = self.acquire_lock(key).await?;
        let data_path = self.key_to_path(key)?;
        if tokio::fs::try_exists(&data_path)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?
        {
            tokio::fs::remove_file(&data_path)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        let metadata_path = self.key_to_path_metadata(key)?;
        if tokio::fs::try_exists(&metadata_path)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?
        {
            tokio::fs::remove_file(&metadata_path)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        let _lock = self.acquire_lock(key).await?;
        let path = self.key_to_path(key)?;
        if tokio::fs::try_exists(&path)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?
        {
            tokio::fs::remove_dir_all(path)
                .await
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        if tokio::fs::try_exists(path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key)?;
        tokio::fs::try_exists(metadata_path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            return Ok(false);
        }
        tokio::fs::metadata(path)
            .await
            .map(|m| m.is_dir())
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let path = self.key_to_path(key)?;
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            // An absent addressable directory is an empty namespace. Keep failures from
            // `try_exists` distinct: callers must not mistake a permission or I/O error for
            // absence. CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER.
            return Ok(vec![]);
        }
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
        if !metadata.is_dir() {
            return Ok(vec![]);
        }
        let mut dir = tokio::fs::read_dir(path)
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
        let mut names = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            // The same predicate the path builders use. Skipping rather than failing is what
            // §8 requires, and it is not optional: `listdir_keys_deep` calls `is_dir` on every
            // child, so a reserved name left in a listing would make `keys()` fail outright.
            if !Self::RESERVED.is_reserved_name(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key)?;
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative()
            && key.has_key_prefix(&self.prefix)
            && !Self::RESERVED.is_reserved_key(key)
    }
}

#[derive(Debug, Clone)]
pub struct FileStore {
    pub path: PathBuf,
    pub prefix: Key,
}

impl FileStore {
    /// What this store's layout reserves. Unlike [`AsyncFileStore`] it takes **no lock files**, so
    /// `x.__lock__` is a key it can address and must not refuse — over-reserving is a defect in
    /// the same family as under-reserving. `reserved04` pins the difference.
    const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX], &[METADATA_FOLDER]);

    pub fn new(path: &str, prefix: &Key) -> FileStore {
        FileStore {
            path: PathBuf::from(path),
            prefix: prefix.to_owned(),
        }
    }

    /// Raises the refusal for a key this store's layout cannot represent. See
    /// [`AsyncFileStore::reject_reserved`].
    fn reject_reserved(&self, key: &Key) -> Result<(), Error> {
        if Self::RESERVED.is_reserved_key(key) {
            return Err(Error::key_not_supported(key, &self.store_name()));
        }
        Ok(())
    }

    /// Maps a key onto a filesystem path under the store root.
    ///
    /// Fallible because this is where a relative key would become a real path traversal — see
    /// [`AsyncFileStore::key_to_path`] and the
    /// [module documentation](self#a-store-key-is-absolute).
    pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error> {
        let key = key.as_absolute()?;
        self.reject_reserved(&key)?;
        let mut path = self.path.clone();
        path.push(key.to_string());
        Ok(path)
    }

    /// Maps a key onto the path of its metadata file. Fallible for the same reason as
    /// [`Self::key_to_path`].
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error> {
        let key = key.as_absolute()?;
        self.reject_reserved(&key)?;
        let mut path = self.path.clone();
        path.push(format!("{}{}", key, METADATA_SUFFIX));
        Ok(path)
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
        let path = self.key_to_path(key)?;
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
        let path = self.key_to_path_metadata(key)?;
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
            let path = self.key_to_path(key)?;
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
        let path = self.key_to_path(key)?;
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
        let path = self.key_to_path_metadata(key)?;
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
        let path = self.key_to_path(key)?;
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        let matadata_path = self.key_to_path_metadata(key)?;
        if matadata_path.exists() {
            std::fs::remove_file(matadata_path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key)?;
        if path.exists() {
            std::fs::remove_dir_all(path)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        if path.exists() {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key)?;
        if metadata_path.exists() {
            return Ok(true);
        }
        Ok(false)
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        Ok(path.is_dir())
    }

    fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let path = self.key_to_path(key)?;
        if !path
            .try_exists()
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
        {
            // Match AsyncFileStore: absence is empty; a failed existence check is still an error.
            return Ok(vec![]);
        }
        if path
            .metadata()
            .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?
            .is_dir()
        {
            let dir = path
                .read_dir()
                .map_err(|e| Error::key_read_error(key, &self.store_name(), &e))?;
            let names = dir
                .flat_map(|entry| {
                    entry
                        .ok()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                })
                .filter(|name| !Self::RESERVED.is_reserved_name(name))
                .collect();
            Ok(names)
        } else {
            Ok(vec![])
        }
    }

    fn makedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key)?;
        std::fs::create_dir_all(path)
            .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?;
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative()
            && key.has_key_prefix(&self.prefix)
            && !Self::RESERVED.is_reserved_key(key)
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
        let key = key.as_absolute()?;
        let mem = self.data.read().unwrap();
        match mem.get(key) {
            Some((data, metadata)) => Ok((data.to_owned(), metadata.to_owned())),
            None => Err(Error::key_not_found(key)),
        }
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let key = key.as_absolute()?;
        let mem = self.data.read().unwrap();
        match mem.get(key) {
            Some((data, _)) => Ok(data.to_owned()),
            None => Err(Error::key_not_found(key)),
        }
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let mut mem = self.data.write().unwrap();

        mem.insert(key.to_owned(), (data.to_owned(), metadata.to_owned()));
        Ok(())
    }

    fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        let res = self.get(key)?;
        let mut mem = self.data.write().unwrap();
        mem.insert(key.to_owned(), (res.0, metadata.to_owned()));
        Ok(())
    }

    fn remove(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        let mut mem = self.data.write().unwrap();
        mem.remove(key);
        Ok(())
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let mem = self.data.read().unwrap();
        if mem.contains_key(key) {
            return Ok(true);
        }
        Ok(self.is_dir(key)?)
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let keys = self.listdir_keys(key)?;
        Ok(keys
            .iter()
            .filter_map(|x| x.filename().map(|xx| xx.to_string()))
            .collect())
    }

    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let mem = self.data.read().unwrap();
        let keys = mem
            .keys()
            .filter(|k| k.has_key_prefix(key))
            .cloned()
            .collect::<Vec<_>>();
        Ok(keys)
    }

    fn makedir(&self, key: &Key) -> Result<(), Error> {
        let _ = key.as_absolute()?;
        // TODO: implement correct makedir
        Ok(())
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative() && key.has_key_prefix(&self.prefix)
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
        let key = key.as_absolute()?;
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| store.get(key))
    }

    fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let key = key.as_absolute()?;
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| store.get_bytes(key))
    }

    fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let key = key.as_absolute()?;
        self.find_store(key)
            .map_or(Err(Error::key_not_found(key)), |store| {
                store.get_metadata(key)
            })
    }

    fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.set(key, data, metadata),
        )
    }

    fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.set_metadata(key, _metadata),
        )
    }

    fn remove(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.remove(key),
        )
    }

    fn removedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.removedir(key),
        )
    }

    fn contains(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
        self.find_store(key)
            .map_or(Ok(false), |store| store.contains(key))
    }

    fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let mut list = Vec::new();
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                let names = store.listdir(key)?;
                list.extend(names);
            }
            // `key` is a prefix of the store's prefix *and strictly shorter* — so the next
            // segment of the store's prefix is a directory inside `key`.
            //
            // The length check is what makes this safe: `has_key_prefix` is also true when the two
            // are equal, and indexing `key_prefix[key.len()]` would then be out of bounds. Listing
            // a store's own prefix — `listdir("data")` for a store mounted at `data` — is the most
            // ordinary call there is, and it panicked.
            if store.key_prefix().len() > key.len() && store.key_prefix().has_key_prefix(key) {
                list.push(store.key_prefix()[key.len()].to_string());
            }
        }

        Ok(list)
    }

    fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        let names = self.listdir(key)?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        let keys = self.listdir_keys(key)?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            // See the async twin: the guard is about the child.
            if self.is_dir(&sub_key)? {
                let sub = self.listdir_keys_deep(&sub_key)?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    fn makedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        self.find_store(key).map_or(
            Err(Error::key_not_supported(key, "store router")),
            |store| store.makedir(key),
        )
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative()
            && self
                .find_store(key)
                .is_some_and(|store| store.is_supported(key))
    }
}

/// Asunchronous store that routes requests to multiple (asynchronous) stores.
pub struct AsyncStoreRouter {
    stores: Vec<Box<dyn AsyncStore>>,
}
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.get(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.get_bytes(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.get_metadata(key).await
        } else {
            Err(Error::key_not_found(key))
        }
    }

    /// Store data and metadata.
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.set(key, data, metadata).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.set_metadata(key, metadata).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.remove(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Remove a directory and everything under it.
    ///
    /// **Specified by its postcondition:** if this returns `Ok(())`, the directory does not exist
    /// afterwards. Failing to remove it is an error; what is forbidden is claiming success without
    /// the effect. Recursion follows rather than being stipulated separately — a directory derived
    /// from its children exists while any child remains, so a removal that left one and reported
    /// `Ok(())` would break the postcondition.
    ///
    /// On a directory that does not exist, `Ok(())` is correct: the postcondition already holds.
    /// This default returns `Err(KeyNotSupported)` instead, which is also correct — a store that
    /// has not implemented directory removal is *refusing*, not silently succeeding.
    ///
    /// Not atomic on any backend. See `specs/reference/STORE_SEMANTICS.md` §5.
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.removedir(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
        if let Some(store) = self.find_store(key) {
            store.contains(key).await
        } else {
            Err(Error::key_not_supported(key, "store router"))
        }
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let key = key.as_absolute()?;
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
        let key = key.as_absolute()?;
        let mut list = Vec::new();
        for store in &self.stores {
            if key.has_key_prefix(&store.key_prefix()) {
                let names = store.listdir(key).await?;
                list.extend(names);
            }
            // `key` is a prefix of the store's prefix *and strictly shorter* — so the next
            // segment of the store's prefix is a directory inside `key`.
            //
            // The length check is what makes this safe: `has_key_prefix` is also true when the two
            // are equal, and indexing `key_prefix[key.len()]` would then be out of bounds. Listing
            // a store's own prefix — `listdir("data")` for a store mounted at `data` — is the most
            // ordinary call there is, and it panicked.
            if store.key_prefix().len() > key.len() && store.key_prefix().has_key_prefix(key) {
                list.push(store.key_prefix()[key.len()].to_string());
            }
        }

        Ok(list)
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        let names = self.listdir(key).await?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let key = key.as_absolute()?;
        let keys = self.listdir_keys(key).await?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            // `sub_key`, not `key`: the guard decides whether to descend into the *child*.
            // Testing the parent made it a constant, so every child including data keys was
            // recursed into. CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY.
            if self.is_dir(&sub_key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let key = key.as_absolute()?;
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
    fn is_supported(&self, key: &Key) -> bool {
        if key.is_relative() {
            return false;
        }
        if let Some(store) = self.find_store(key) {
            store.is_supported(key)
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
    #[tokio::test]
    async fn test_async_memory_store_basic() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        let key = parse_key("a/b/c").unwrap();
        let data = b"test data".to_vec();
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());

        assert!(!store.contains(&key).await?);
        // `keys()` returns data keys, the directories above them, **and the store's own prefix**
        // (`STORE_SEMANTICS.md` §9), so an empty store still reports one key: the prefix itself.
        assert_eq!(store.keys().await?, vec![Key::new()]);
        assert!(!store.is_dir(&parse_key("a/b")?).await?);

        store.set(&key, &data, &metadata).await?;
        assert!(store.contains(&key).await?);
        assert!(store.keys().await?.contains(&key));
        assert!(store.is_dir(&parse_key("a/b")?).await?);
        // One data key `a/b/c`, the two directories above it, and the prefix: four, not one.
        let mut keys = store.keys().await?;
        keys.sort_by_key(|k| k.encode());
        assert_eq!(
            keys,
            vec![
                Key::new(),
                parse_key("a")?,
                parse_key("a/b")?,
                parse_key("a/b/c")?,
            ]
        );

        let (data2, metadata2) = store.get(&key).await?;
        assert_eq!(data, data2);
        assert!(metadata2.filename().is_none());

        let mut updated_metadata = metadata.clone();
        updated_metadata.set_filename("c.bin")?;
        store.set_metadata(&key, &updated_metadata).await?;
        let persisted_metadata = store.get_metadata(&key).await?;
        assert_eq!(persisted_metadata.filename(), Some("c.bin".to_string()));

        store.remove(&key).await?;
        assert!(!store.contains(&key).await?);

        let metadata_only_key = parse_key("meta/only.json").unwrap();
        let mut metadata_only = Metadata::MetadataRecord(MetadataRecord::new());
        metadata_only.set_filename("only.json")?;
        store
            .set_metadata(&metadata_only_key, &metadata_only)
            .await?;
        assert!(store.contains(&metadata_only_key).await?);
        assert_eq!(
            store.get_metadata(&metadata_only_key).await?.filename(),
            Some("only.json".to_string())
        );
        assert_eq!(store.get_bytes(&metadata_only_key).await?, Vec::<u8>::new());
        Ok(())
    }

    fn memory_store_support(prefix: &Key, key: &Key) -> (bool, bool) {
        let sync_store = MemoryStore::new(prefix);
        let async_store = AsyncMemoryStore::new(prefix);
        (sync_store.is_supported(key), async_store.is_supported(key))
    }

    #[test]
    fn memsupport01_absolute_key_inside_prefix_is_supported() -> Result<(), Error> {
        let prefix = parse_key("data")?;
        let key = parse_key("data/report.txt")?;

        assert_eq!(memory_store_support(&prefix, &key), (true, true));
        Ok(())
    }

    #[test]
    fn memsupport02_absolute_key_outside_prefix_is_not_supported() -> Result<(), Error> {
        let prefix = parse_key("data")?;
        let key = parse_key("other/report.txt")?;

        assert_eq!(memory_store_support(&prefix, &key), (false, false));
        Ok(())
    }

    #[test]
    fn memsupport03_relative_key_with_matching_prefix_is_not_supported() -> Result<(), Error> {
        let prefix = parse_key("data")?;
        let key = parse_key("data/../secret")?;

        assert!(key.has_key_prefix(&prefix));
        assert!(key.is_relative());
        assert_eq!(memory_store_support(&prefix, &key), (false, false));
        Ok(())
    }

    #[test]
    fn memsupport04_root_store_supports_absolute_key() -> Result<(), Error> {
        let key = parse_key("any/report.txt")?;

        assert_eq!(memory_store_support(&Key::new(), &key), (true, true));
        Ok(())
    }

    #[test]
    fn memsupport05_key_equal_to_prefix_is_supported() -> Result<(), Error> {
        let prefix = parse_key("data")?;

        assert_eq!(memory_store_support(&prefix, &prefix), (true, true));
        Ok(())
    }

    #[test]
    fn memsupport06_similar_segment_is_not_supported() -> Result<(), Error> {
        let prefix = parse_key("data")?;
        let key = parse_key("database/report.txt")?;

        assert_eq!(memory_store_support(&prefix, &key), (false, false));
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // `MEMDIR01`-`MEMDIR05` — characterization of `AsyncMemoryStore`'s directory behaviour.
    //
    // These pin what the store does **today**, before its directory index is extracted into
    // `store_dir_index::DirectoryIndex`. They exist because the extraction's safety was argued
    // from "the existing tests pass unchanged", and the existing tests were one: a single key,
    // one directory level, and no check of `is_dir` after a removal — none of the refcount
    // behaviour an extraction is most likely to break.
    //
    // They must therefore pass against the *unextracted* store first, and pass **unchanged**
    // afterwards. A test that needed editing during the extraction is a signal that behaviour
    // moved, not that the test was wrong.
    //
    // See specs/design/opendal-path-mapping/phase3-examples.md, Finding 1.
    // ---------------------------------------------------------------------------------------

    /// `MEMDIR01` — storing a key makes every proper ancestor a directory, and the key itself not.
    #[tokio::test]
    async fn memdir01_ancestors_of_a_stored_key_are_directories() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        let key = parse_key("a/b/c.txt")?;
        assert!(!store.is_dir(&parse_key("a")?).await?);

        store.set(&key, b"data", &Metadata::new()).await?;

        assert!(store.is_dir(&parse_key("a")?).await?);
        assert!(store.is_dir(&parse_key("a/b")?).await?);
        assert!(!store.is_dir(&key).await?, "the key itself holds data, it is not a directory");
        assert!(!store.is_dir(&parse_key("a/z")?).await?, "an unrelated name is not a directory");
        Ok(())
    }

    /// `MEMDIR02` — a directory outlives all but its last child.
    ///
    /// This is what the index's reference counts are for, and the case no existing test reached.
    #[tokio::test]
    async fn memdir02_directory_retires_only_with_its_last_child() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        let (first, second) = (parse_key("a/b/c.txt")?, parse_key("a/b/d.txt")?);
        store.set(&first, b"1", &Metadata::new()).await?;
        store.set(&second, b"2", &Metadata::new()).await?;
        let dir = parse_key("a/b")?;
        assert!(store.is_dir(&dir).await?);

        store.remove(&first).await?;
        assert!(store.is_dir(&dir).await?, "one child remains, so a/b is still a directory");

        store.remove(&second).await?;
        assert!(!store.is_dir(&dir).await?, "no children remain");
        assert!(!store.is_dir(&parse_key("a")?).await?, "and the retirement propagates upward");
        Ok(())
    }

    /// `MEMDIR03` — `listdir` reports direct children only, at every depth.
    #[tokio::test]
    async fn memdir03_listdir_reports_direct_children_only() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        for text in ["a/b/c.txt", "a/b/d.txt", "a/e.txt", "f.txt"] {
            store.set(&parse_key(text)?, b"x", &Metadata::new()).await?;
        }

        let mut root = store.listdir(&Key::new()).await?;
        root.sort();
        assert_eq!(root, vec!["a".to_string(), "f.txt".to_string()]);

        let mut a = store.listdir(&parse_key("a")?).await?;
        a.sort();
        assert_eq!(a, vec!["b".to_string(), "e.txt".to_string()], "c.txt is not a child of a");

        let mut b = store.listdir(&parse_key("a/b")?).await?;
        b.sort();
        assert_eq!(b, vec!["c.txt".to_string(), "d.txt".to_string()]);
        Ok(())
    }

    /// `MEMDIR04` — `makedir` records an empty directory, and `removedir` takes it away.
    ///
    /// This test was written asserting the opposite — that `makedir` recorded nothing — because
    /// that was the behaviour it had to characterize before the directory index moved into
    /// `store_dir_index`. `DirectoryIndex::explicit` is what a derived index could not express,
    /// and this is the assertion flipping to match. `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`.
    #[tokio::test]
    async fn memdir04_makedir_records_an_empty_directory() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        let dir = parse_key("empty/folder")?;

        store.makedir(&dir).await?;

        assert!(store.is_dir(&dir).await?);
        assert!(store.contains(&dir).await?);
        assert!(
            store.is_dir(&parse_key("empty")?).await?,
            "its parent is a directory too"
        );
        assert_eq!(
            store.listdir(&parse_key("empty")?).await?,
            vec!["folder".to_string()]
        );

        // An explicitly created directory outlives its children.
        let child = dir.join("f.txt");
        store.set(&child, b"x", &Metadata::new()).await?;
        store.remove(&child).await?;
        assert!(store.is_dir(&dir).await?, "explicit, so it survives losing its child");

        store.removedir(&dir).await?;
        assert!(!store.is_dir(&dir).await?);

        // A `makedir` descendant must not outlive a recursive removal of its parent.
        // Raised in review of PR #58.
        store.makedir(&parse_key("tree/inner")?).await?;
        store.removedir(&parse_key("tree")?).await?;
        assert!(!store.is_dir(&parse_key("tree/inner")?).await?);
        assert!(!store.is_dir(&parse_key("tree")?).await?);
        Ok(())
    }

    /// `MEMDIR05` — `removedir` clears the subtree and the index with it.
    #[tokio::test]
    async fn memdir05_removedir_clears_the_subtree_and_the_index() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        for text in ["a/b/c.txt", "a/e.txt", "f.txt"] {
            store.set(&parse_key(text)?, b"x", &Metadata::new()).await?;
        }

        store.removedir(&parse_key("a")?).await?;

        assert!(!store.is_dir(&parse_key("a")?).await?);
        assert!(!store.is_dir(&parse_key("a/b")?).await?);
        assert!(!store.contains(&parse_key("a/b/c.txt")?).await?);
        assert!(store.contains(&parse_key("f.txt")?).await?, "an unrelated key survives");
        assert_eq!(store.listdir(&Key::new()).await?, vec!["f.txt".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn test_async_file_store_basic() -> Result<(), Error> {
        let unique = format!(
            "liquers_async_file_store_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        tokio::fs::create_dir_all(&root).await.unwrap();

        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let key = parse_key("dir/test.txt").unwrap();
        let data = b"hello async file store".to_vec();
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());

        store.makedir(&parse_key("dir")?).await?;
        store.set(&key, &data, &metadata).await?;
        assert!(store.contains(&key).await?);

        let bytes = store.get_bytes(&key).await?;
        assert_eq!(bytes, data);

        let (stored_data, stored_metadata) = store.get(&key).await?;
        assert_eq!(stored_data, data);
        assert!(stored_metadata.filename().is_none());

        let dir_names = store.listdir(&parse_key("dir")?).await?;
        assert!(dir_names.contains(&"test.txt".to_string()));

        store.remove(&key).await?;
        assert!(!store.contains(&key).await?);

        tokio::fs::remove_dir_all(root).await.unwrap();
        Ok(())
    }

    /// `filestore01` — an absent addressable directory is an empty namespace.
    #[tokio::test]
    async fn filestore01_async_missing_directory_lists_empty() -> Result<(), Error> {
        let root = std::env::temp_dir().join(format!(
            "liquers_filestore01_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let prefix = parse_key("files")?;
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &prefix);

        assert!(store.listdir(&prefix).await?.is_empty());
        assert!(!store.is_dir(&prefix).await?);
        assert_eq!(store.keys().await?, vec![prefix]);

        tokio::fs::remove_dir_all(&root).await.expect("remove root");
        Ok(())
    }

    /// `filestore02` — the synchronous file store shares the absence contract.
    #[test]
    fn filestore02_sync_missing_directory_lists_empty() -> Result<(), Error> {
        let root = std::env::temp_dir().join(format!(
            "liquers_filestore02_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create root");
        let prefix = parse_key("files")?;
        let store = FileStore::new(root.to_string_lossy().as_ref(), &prefix);

        assert!(store.listdir(&prefix)?.is_empty());
        assert!(!store.is_dir(&prefix)?);
        assert_eq!(store.keys()?, vec![prefix]);

        std::fs::remove_dir_all(&root).expect("remove root");
        Ok(())
    }

    /// `listdir` on a key that exactly equals a store's prefix must not panic.
    ///
    /// `has_key_prefix` is true for equal keys, so the "next segment of the store's prefix"
    /// lookup used to index one past the end. Listing a store's own root is the most ordinary
    /// call there is, and it aborted — in wasm, where a panic kills the instance, that surfaced as
    /// a hung `Promise` rather than an error.
    #[tokio::test]
    async fn async_router_listdir_at_store_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let mut router = AsyncStoreRouter::new();
        router.add_store(Box::new(AsyncMemoryStore::new(&parse_key("data")?)));

        let at_prefix = router.listdir(&parse_key("data")?).await?;
        assert!(
            at_prefix.is_empty(),
            "an empty store lists nothing at its own prefix, got {at_prefix:?}"
        );

        // Above the prefix, the store's own first segment is the directory that shows up.
        let at_root = router.listdir(&Key::new()).await?;
        assert_eq!(at_root, vec!["data".to_string()]);

        // And with content, the store's own listing comes through unchanged.
        router
            .set(&parse_key("data/a.txt")?, b"x", &Metadata::new())
            .await?;
        let with_content = router.listdir(&parse_key("data")?).await?;
        assert_eq!(with_content, vec!["a.txt".to_string()]);
        Ok(())
    }

    /// The same guard on the synchronous router, which carries the identical code.
    #[test]
    fn sync_router_listdir_at_store_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let mut router = StoreRouter::new();
        router.add_store(Box::new(MemoryStore::new(&parse_key("data")?)));
        assert!(router.listdir(&parse_key("data")?)?.is_empty());
        assert_eq!(router.listdir(&Key::new())?, vec!["data".to_string()]);
        Ok(())
    }
}

/// Tests for the absolute-key precondition (`specs/design/store-key-guard/`).
///
/// Every test asserts the *error type*, never merely that an error occurred. `KeyNotAbsolute`,
/// `KeyNotSupported` and `KeyReadError` are three different refusals here, and asserting `is_err()`
/// would conflate them — which is how the deep-traversal case below can appear covered while the
/// guard is absent.
#[cfg(test)]
mod key_absolute_tests {
    use super::*;
    use crate::error::ErrorType;
    use crate::parse::parse_key;

    /// The key shapes a store must refuse. `a/./b` and `a/../../etc` are the ones a guard that
    /// inspects only the leading element would let through.
    const RELATIVE: [&str; 5] = ["../escape", "a/../../etc/passwd", "a/./b", "..", "./x"];

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let unique = format!(
            "liquers_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    /// `keyabs17` — the *trait defaults* refuse relative keys, so the contract holds for a backend
    /// that overrides nothing.
    ///
    /// Raised in review of PR #36: `contains`, `is_dir` and `listdir` default to `Ok(false)` and
    /// `Ok(vec![])`, permissive values that answered a relative key as though it were an ordinary
    /// absent one. A backend inheriting them satisfied the documented "every store refuses" contract
    /// only by accident of which methods it happened to override.
    ///
    /// `MinimalStore` implements exactly the two methods that have no default, so every other
    /// method exercised here is the trait's own body.
    #[tokio::test]
    async fn keyabs17_trait_defaults_refuse_relative_keys() -> Result<(), Error> {
        struct MinimalStore;

        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        impl AsyncStore for MinimalStore {
            async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
                key.as_absolute()?;
                Err(Error::key_not_found(key))
            }
            async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
                key.as_absolute()?;
                Ok(())
            }
        }

        let store = MinimalStore;
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());
        for text in RELATIVE {
            let key = parse_key(text)?;
            for (label, error) in [
                ("contains", store.contains(&key).await.err()),
                ("is_dir", store.is_dir(&key).await.err()),
                ("listdir", store.listdir(&key).await.err()),
                ("set", store.set(&key, b"x", &metadata).await.err()),
                ("remove", store.remove(&key).await.err()),
                ("removedir", store.removedir(&key).await.err()),
                ("makedir", store.makedir(&key).await.err()),
            ] {
                let error = error.unwrap_or_else(|| panic!("{label} default must refuse {text}"));
                assert_eq!(
                    error.error_type,
                    ErrorType::KeyNotAbsolute,
                    "{label} {text}"
                );
            }
        }

        // The defaults stay permissive for an ordinary key — the guard must not turn them into
        // blanket refusals.
        let ok = parse_key("data/report.txt")?;
        assert!(!store.contains(&ok).await?);
        assert!(!store.is_dir(&ok).await?);
        assert!(store.listdir(&ok).await?.is_empty());
        Ok(())
    }

    /// `TRAITDEF01` — the default `contains` falls back to `is_dir`.
    ///
    /// `DirOnlyStore` implements the two methods that have no default, plus `is_dir`. Everything
    /// else exercised here is the trait's own body, so this checks the default and not an override.
    #[tokio::test]
    async fn traitdef01_default_contains_falls_back_to_is_dir() -> Result<(), Error> {
        struct DirOnlyStore;

        #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
        impl AsyncStore for DirOnlyStore {
            async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
                key.as_absolute()?;
                Err(Error::key_not_found(key))
            }
            async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
                key.as_absolute()?;
                Ok(())
            }
            async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
                Ok(key.as_absolute()?.encode() == "a/b")
            }
        }

        let store = DirOnlyStore;
        assert!(
            store.contains(&parse_key("a/b")?).await?,
            "a directory is contained, without the store restating it"
        );
        assert!(!store.contains(&parse_key("a/c")?).await?);

        // The fallback must not weaken the relative-key refusal.
        use crate::error::ErrorType;
        let error = store
            .contains(&parse_key("a/../b")?)
            .await
            .err()
            .unwrap_or_else(|| panic!("a relative key must be refused"));
        assert_eq!(error.error_type, ErrorType::KeyNotAbsolute);
        Ok(())
    }

    /// `keyabs07` — the in-memory stores refuse relative keys too.
    ///
    /// A map-backed store cannot traverse anything, so this is uniformity rather than safety: a key
    /// that one store refuses and another serves would be a worse rule than one that holds
    /// everywhere, and the memory stores are what most tests are written against.
    #[tokio::test]
    async fn keyabs07_memory_stores_refuse_relative_keys() -> Result<(), Error> {
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());
        let store = AsyncMemoryStore::new(&Key::new());
        for text in RELATIVE {
            let key = parse_key(text)?;
            for error in [
                store.get(&key).await.err(),
                store.get_bytes(&key).await.err(),
                store.get_metadata(&key).await.err(),
                store.set(&key, b"x", &metadata).await.err(),
                store.set_metadata(&key, &metadata).await.err(),
                store.remove(&key).await.err(),
                store.contains(&key).await.err(),
                store.is_dir(&key).await.err(),
                store.listdir(&key).await.err(),
                store.makedir(&key).await.err(),
            ] {
                let error = error.unwrap_or_else(|| panic!("{text} must be refused"));
                assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            }
        }

        let sync_store = MemoryStore::new(&Key::new());
        for text in RELATIVE {
            let key = parse_key(text)?;
            let error = sync_store.get(&key).expect_err("must refuse");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            let error = sync_store
                .set(&key, b"x", &metadata)
                .expect_err("must refuse");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
        }
        Ok(())
    }

    /// `keyabs08` — `AsyncFileStore` refuses the traversal, and the file outside the root is
    /// untouched.
    ///
    /// This is the reproduction of `STORE-FILESTORE-PATH-TRAVERSAL`, and two details are what make
    /// it one rather than a restatement of the guard:
    ///
    /// 1. **The intermediate directory is created.** The kernel resolves `..` by walking *real*
    ///    directories, so `a/../../SECRET.txt` against an unguarded store fails with `ENOENT` when
    ///    `a/` does not exist. Without the `makedir` below, this test passes on unfixed code — for
    ///    the wrong reason — and the deep-traversal case would look covered while it is not.
    /// 2. **`get_bytes` is asserted, not only `get`.** `get` reads the data *first* and only then
    ///    touches metadata, so with the guard removed from `key_to_path` alone it still returns
    ///    `KeyNotAbsolute` — raised by the metadata write, after the secret has already been read.
    ///    Asserting `get` alone therefore passes while the escape happens. `get_bytes` is the
    ///    direct read path and is what makes the mutation visible.
    /// 3. **The outside file is byte-compared after an attempted write.** Asserting only the error
    ///    type would still pass if the store failed for some unrelated reason; what actually pins
    ///    the write closed is that the file did not change.
    #[tokio::test]
    async fn keyabs08_async_file_store_refuses_traversal() -> Result<(), Error> {
        let sandbox = unique_temp_dir("keyabs08");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let secret = sandbox.join("SECRET.txt");
        let original = b"outside the store root".to_vec();
        tokio::fs::write(&secret, &original)
            .await
            .expect("write secret");

        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        // Detail 1: without this, the deep case below fails with ENOENT rather than the guard.
        store.makedir(&parse_key("a")?).await?;

        let metadata = Metadata::MetadataRecord(MetadataRecord::new());
        for text in ["../SECRET.txt", "a/../../SECRET.txt", "a/./b.txt"] {
            let key = parse_key(text)?;

            // Detail 2: the direct read path. `get` would mask a successful read behind a
            // metadata error, so it is asserted too but is not what pins the escape closed.
            let read = store.get_bytes(&key).await;
            if let Ok(data) = &read {
                assert_ne!(data, &original, "{text} READ THE FILE OUTSIDE THE ROOT");
            }
            let error = read.expect_err("read must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "read {text}");

            let error = store.get(&key).await.expect_err("get must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "get {text}");

            let error = store
                .set(&key, b"owned", &metadata)
                .await
                .expect_err("write must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "write {text}");

            let error = store
                .remove(&key)
                .await
                .expect_err("remove must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "remove {text}");

            assert!(!store.is_supported(&key), "{text} must not route here");
            assert!(store.key_to_path(&key).is_err(), "path builder {text}");
        }

        // Detail 2: nothing outside the root was read, written or created.
        assert_eq!(
            tokio::fs::read(&secret).await.expect("secret still there"),
            original
        );
        assert!(!sandbox.join("owned").exists());

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `keyabs09` — the synchronous `FileStore` refuses the same shapes.
    #[test]
    fn keyabs09_file_store_refuses_traversal() -> Result<(), Error> {
        let sandbox = unique_temp_dir("keyabs09");
        let root = sandbox.join("root");
        std::fs::create_dir_all(root.join("a")).expect("create root");
        let secret = sandbox.join("SECRET.txt");
        let original = b"outside the store root".to_vec();
        std::fs::write(&secret, &original).expect("write secret");

        let store = FileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let metadata = Metadata::MetadataRecord(MetadataRecord::new());
        for text in ["../SECRET.txt", "a/../../SECRET.txt"] {
            let key = parse_key(text)?;
            let error = store.get(&key).expect_err("read must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "read {text}");
            let error = store
                .set(&key, b"owned", &metadata)
                .expect_err("write must be refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "write {text}");
            assert!(!store.is_supported(&key), "{text}");
            assert!(store.key_to_path(&key).is_err(), "path builder {text}");
        }
        assert_eq!(
            std::fs::read(&secret).expect("secret still there"),
            original
        );

        std::fs::remove_dir_all(&sandbox).expect("cleanup");
        Ok(())
    }

    /// `keyabs10` — a router reports the malformed key, not "no store matched".
    ///
    /// Without the check ahead of `find_store`, no store would claim the key and the router would
    /// answer `KeyNotSupported` — which says the key was not routed, when in fact it is not an
    /// address at all.
    #[tokio::test]
    async fn keyabs10_routers_report_key_not_absolute() -> Result<(), Error> {
        let mut router = AsyncStoreRouter::new();
        router.add_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        for text in RELATIVE {
            let key = parse_key(text)?;
            let error = router.get(&key).await.expect_err("must refuse");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            assert!(!router.is_supported(&key), "{text}");
        }

        let mut sync_router = StoreRouter::new();
        sync_router.add_store(Box::new(MemoryStore::new(&Key::new())));
        for text in RELATIVE {
            let key = parse_key(text)?;
            let error = sync_router.get(&key).expect_err("must refuse");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            assert!(!sync_router.is_supported(&key), "{text}");
        }
        Ok(())
    }

    /// `keyabs11` — `is_supported` is false on a **directly held** store.
    ///
    /// Deliberately not through a router. Routing is the one configuration in which a store that
    /// guards only `is_supported` behaves correctly, so a test that always routes would pass
    /// against an implementation whose `get` and `set` are wide open. An `Environment` is commonly
    /// configured with a store held directly.
    #[tokio::test]
    async fn keyabs11_is_supported_false_on_directly_held_store() -> Result<(), Error> {
        let root = unique_temp_dir("keyabs11");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let file_store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let memory_store = AsyncMemoryStore::new(&Key::new());

        for text in RELATIVE {
            let key = parse_key(text)?;
            assert!(!file_store.is_supported(&key), "file store {text}");
            assert!(!memory_store.is_supported(&key), "memory store {text}");
        }
        for text in ["data/report.txt", "a/.hidden", "a..b/c"] {
            let key = parse_key(text)?;
            assert!(file_store.is_supported(&key), "file store {text}");
            assert!(memory_store.is_supported(&key), "memory store {text}");
        }

        tokio::fs::remove_dir_all(&root).await.expect("cleanup");
        Ok(())
    }
}

/// Tests for the reserved-name rule (`specs/design/sidecar-colliding-keys/`).
///
/// A sibling of `key_absolute_tests`, and deliberately not part of it. That module asks whether a
/// key is an *address* at all; this one asks whether a given store can represent it. They are two
/// refusals with two error types, and they meet in exactly one place — the order the two checks
/// run in, which `reserved05` pins.
#[cfg(test)]
mod reserved_name_tests {
    use super::*;
    use crate::error::ErrorType;
    use crate::parse::parse_key;

    /// As `key_absolute_tests` does it — nanosecond-stamped, because `cargo test` runs these in
    /// parallel.
    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let unique = format!(
            "liquers_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    /// Assert that a store refused a key as unrepresentable, rather than failing some other way.
    ///
    /// Generic over the success type so one helper covers `PathBuf`, `Vec<u8>`, `bool`, `Metadata`,
    /// `Vec<String>` and `()`. Asserting the *type* matters: `KeyNotAbsolute`, `KeyNotSupported`
    /// and `KeyReadError` are three different refusals, and `is_err()` would conflate them.
    fn assert_not_supported<T>(result: Result<T, Error>, what: &str) {
        match result {
            Ok(_) => panic!("{what} must be refused"),
            Err(error) => assert_eq!(error.error_type, ErrorType::KeyNotSupported, "{what}"),
        }
    }

    /// `reserved01` — `ReservedNames` recognises both forms of a reserved name, and nothing else.
    ///
    /// The negatives are the half that matters. A predicate written as
    /// `name.contains("__metadata__")` passes every positive below and refuses five keys a store
    /// can address perfectly well, so the positives alone would not distinguish a correct
    /// implementation from a destructive one.
    #[test]
    fn reserved01_reserved_names_recognises_both_forms() -> Result<(), Error> {
        let file_store = ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX], &[METADATA_FOLDER]);

        for name in [
            "collide.__metadata__", // the sidecar of `collide`
            "__metadata__",         // the legacy metadata folder
            "collide.__lock__",     // the lock taken while writing `collide`
        ] {
            assert!(file_store.is_reserved_name(name), "{name} must be reserved");
        }

        for name in [
            "metadata",           // not the reserved name at all
            "x.__metadata__.txt", // the suffix is not final — an ordinary file
            "__metadata__x",      // the bare form is a prefix here, not the whole name
            "x.__metadata",       // truncated
            "x.__lock",
            // Reserved *exactly* is declared per name, not derived from every suffix: no layout
            // has ever used a `__lock__` directory, so this is an ordinary name.
            "__lock__",
        ] {
            assert!(
                !file_store.is_reserved_name(name),
                "{name} must NOT be reserved"
            );
        }

        // A key is reserved when ANY segment is — the filename is not privileged.
        for text in [
            "collide.__metadata__",
            "data/collide.__metadata__",
            "data/__metadata__/x.json",
        ] {
            assert!(file_store.is_reserved_key(&parse_key(text)?), "{text}");
        }
        for text in [
            "data/report.txt",
            "metadata/report.txt",
            "data/x.__metadata__.txt",
        ] {
            assert!(!file_store.is_reserved_key(&parse_key(text)?), "{text}");
        }

        // The root must never be reserved: an empty key has no segments, and a store whose root
        // is refused is a store that refuses everything.
        assert!(!file_store.is_reserved_key(&Key::new()));
        Ok(())
    }

    /// `reserved02` — `AsyncFileStore` refuses a sidecar-colliding key from every fallible method,
    /// and the metadata it collides with is still intact afterwards.
    ///
    /// This is the reproduction of `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`, and three
    /// details are what make it one rather than a restatement of the guard:
    ///
    /// 1. **Every fallible method, not a representative sample.** The bug was precisely that
    ///    `is_supported` and the operations disagreed; a test checking two operations could pick
    ///    the two that happened to be guarded.
    /// 2. **The metadata of `collide` is read back after the refused write.** Asserting only
    ///    `KeyNotSupported` would still pass a fix that wrote the bytes and then returned an
    ///    error — the exact failure being fixed, merely with a better return value.
    /// 3. **`get_metadata` is asserted, not `get`.** `get` *repairs* metadata it cannot parse, by
    ///    synthesizing a fresh record and writing it back, so against unfixed code it returns `Ok`
    ///    and hides the corruption.
    #[tokio::test]
    async fn reserved02_async_file_store_refuses_a_colliding_key_uniformly() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved02");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

        // An ordinary asset whose metadata is worth protecting.
        let victim = parse_key("collide")?;
        let mut record = MetadataRecord::new();
        record
            .with_key(victim.clone())
            .with_title("do not lose me".to_owned());
        store
            .set(&victim, b"body", &Metadata::MetadataRecord(record))
            .await?;

        // Its data path is byte-identical to the metadata path of `collide`.
        let collide = parse_key("collide.__metadata__")?;
        assert!(
            !store.is_supported(&collide),
            "the routing hint already refused it before the fix"
        );

        let blank = Metadata::MetadataRecord(MetadataRecord::new());
        // Detail 1: every fallible method.
        assert_not_supported(store.key_to_path(&collide), "key_to_path");
        assert_not_supported(store.key_to_path_metadata(&collide), "key_to_path_metadata");
        assert_not_supported(store.get_bytes(&collide).await, "get_bytes");
        assert_not_supported(store.get(&collide).await, "get");
        assert_not_supported(store.get_metadata(&collide).await, "get_metadata");
        assert_not_supported(store.contains(&collide).await, "contains");
        assert_not_supported(store.is_dir(&collide).await, "is_dir");
        assert_not_supported(store.listdir(&collide).await, "listdir");
        assert_not_supported(store.set(&collide, b"corrupt", &blank).await, "set");
        assert_not_supported(store.set_metadata(&collide, &blank).await, "set_metadata");
        assert_not_supported(store.remove(&collide).await, "remove");
        assert_not_supported(store.makedir(&collide).await, "makedir");
        assert_not_supported(store.removedir(&collide).await, "removedir");

        // Details 2 and 3: the write did not happen, and the victim can still be described.
        match store.get_metadata(&victim).await? {
            Metadata::MetadataRecord(record) => assert_eq!(record.title, "do not lose me"),
            Metadata::LegacyMetadata(_) => panic!("the sidecar was overwritten"),
        }
        assert_eq!(store.get_bytes(&victim).await?, b"body".to_vec());

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `reserved03` — a reserved name anywhere in the key is refused, not only as the filename.
    ///
    /// `dir.__metadata__/child` has an innocent filename. It is still unaddressable: this key needs
    /// `dir.__metadata__` to be a directory, while the metadata of `dir` needs it to be a file, and
    /// a filesystem will not be both. [`Key::filename`] returns the last segment only, which is how
    /// the original check missed this shape.
    #[tokio::test]
    async fn reserved03_a_reserved_segment_anywhere_is_refused() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved03");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let blank = Metadata::MetadataRecord(MetadataRecord::new());

        for text in [
            "dir.__metadata__/child",  // interior sidecar name
            "a/__metadata__/b.json",   // the legacy metadata folder
            "a/x.__lock__/b",          // interior lock name
            "__metadata__",            // the folder itself, as a key
        ] {
            let key = parse_key(text)?;
            assert!(!store.is_supported(&key), "{text} must not route here");
            assert_not_supported(store.key_to_path(&key), text);
            assert_not_supported(store.key_to_path_metadata(&key), text);
            assert_not_supported(store.get_bytes(&key).await, text);
            assert_not_supported(store.set(&key, b"x", &blank).await, text);
        }

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `reserved05` — a key that is both relative and reserved reports `KeyNotAbsolute`.
    ///
    /// `as_absolute()?` runs before the reserved-name check in every path builder, and this pins
    /// that order. A relative key is not a store address at all, so it is the more fundamental
    /// answer; `keyabs08` and `keyabs09` assert `KeyNotAbsolute` for traversal shapes and would
    /// start failing if someone reordered the two checks while tidying them into one guard.
    #[tokio::test]
    async fn reserved05_relative_and_reserved_reports_key_not_absolute() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved05");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

        for text in [
            "../x.__metadata__",
            "a/../x.__metadata__",
            "a/./x.__metadata__",
        ] {
            let key = parse_key(text)?;
            assert!(
                key.is_relative(),
                "{text} must be relative for this test to mean anything"
            );
            let error = store.key_to_path(&key).expect_err("refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            let error = store.get_bytes(&key).await.expect_err("refused");
            assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
        }

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `reserved06` — a real `__metadata__` directory is skipped by `keys()`, not fallen over.
    ///
    /// The test for the half a partial fix forgets. The chain is `keys()` → `listdir_keys_deep` →
    /// `listdir_keys` → `listdir`; `listdir_keys_deep` calls `is_dir` on every child it was handed,
    /// and `is_dir` goes through `key_to_path`. So an unfiltered reserved name turns a refusal into
    /// a **failed enumeration** — the store stops being listable at all.
    ///
    /// The directory is created on the filesystem directly rather than through `makedir`, because
    /// after the fix `makedir` refuses it: this state is left behind by an older Liquers or an
    /// outside process, not reachable through the API.
    #[tokio::test]
    async fn reserved06_a_reserved_directory_is_skipped_by_listings() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved06");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(root.join("__metadata__"))
            .await
            .expect("legacy metadata folder");
        tokio::fs::write(root.join("__metadata__").join("report.txt.json"), b"{}")
            .await
            .expect("legacy sidecar");
        // An ordinary asset beside it, so a pass means "skipped", not "listed nothing".
        tokio::fs::write(root.join("report.txt"), b"body")
            .await
            .expect("data file");

        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

        let names = store.listdir(&Key::new()).await?;
        assert!(!names.contains(&"__metadata__".to_owned()), "{names:?}");
        assert!(names.contains(&"report.txt".to_owned()), "{names:?}");

        // The enumeration must succeed. Against a path-builders-only fix this line is where it
        // fails.
        let keys = store.keys().await?;
        let encoded: Vec<String> = keys.iter().map(|k| k.encode()).collect();
        assert!(
            !encoded.iter().any(|k| k.starts_with("__metadata__")),
            "keys() must skip the reserved subtree: {encoded:?}"
        );
        assert!(encoded.iter().any(|k| k == "report.txt"), "{encoded:?}");

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `reserved08` — a store that already holds a corrupted sidecar can still be repaired.
    ///
    /// The fix refuses the colliding key, which also means the orphan can no longer be addressed
    /// *as a key* in order to clean it up. Three routes out remain, and this is what keeps them
    /// honest: a documented upgrade path nobody exercises is a rumour.
    ///
    /// The corruption is written with `tokio::fs` rather than through the store, because after the
    /// fix no API can produce it. That is the point, and it is also why this cannot be a
    /// conformance rule: the suite only ever reaches a store through the trait.
    #[tokio::test]
    async fn reserved08_an_existing_corruption_can_still_be_repaired() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved08");
        let root = sandbox.join("root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

        let report = parse_key("report.txt")?;
        let mut record = MetadataRecord::new();
        record
            .with_key(report.clone())
            .with_title("before".to_owned());
        store
            .set(&report, b"body", &Metadata::MetadataRecord(record))
            .await?;

        // Exactly what a pre-fix `set("report.txt.__metadata__", …)` left behind.
        let sidecar = root.join("report.txt.__metadata__");
        tokio::fs::write(&sidecar, b"not json at all")
            .await
            .expect("corrupt the sidecar");
        assert!(
            store.get_metadata(&report).await.is_err(),
            "the corruption must be real"
        );

        // Route 1 — `get` repairs metadata it cannot parse, and returns the data intact.
        let (data, _) = store.get(&report).await?;
        assert_eq!(data, b"body".to_vec());
        match store.get_metadata(&report).await? {
            Metadata::MetadataRecord(_) => {}
            Metadata::LegacyMetadata(_) => panic!("repaired into legacy metadata"),
        }

        // Route 2 — replace it deliberately.
        let mut good = MetadataRecord::new();
        good.with_key(report.clone()).with_title("after".to_owned());
        store
            .set_metadata(&report, &Metadata::MetadataRecord(good))
            .await?;
        match store.get_metadata(&report).await? {
            Metadata::MetadataRecord(record) => assert_eq!(record.title, "after"),
            Metadata::LegacyMetadata(_) => panic!("unexpected legacy metadata"),
        }

        // Route 3 — `remove` unlinks the data path *and* the metadata path, so the orphan goes too.
        store.remove(&report).await?;
        assert!(!sidecar.exists(), "remove must unlink the sidecar");
        assert!(!root.join("report.txt").exists());

        tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
        Ok(())
    }

    /// `reserved04` — the synchronous `FileStore` reserves the metadata name and **not** the lock.
    ///
    /// This is the test that pins the reserved set to the store rather than to the crate.
    /// `FileStore` takes no lock files, so `x.__lock__` is a key it can address, and a single
    /// global reserved list would refuse it for nothing.
    #[test]
    fn reserved04_file_store_reserves_metadata_but_not_lock() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved04");
        let root = sandbox.join("root");
        std::fs::create_dir_all(&root).expect("create root");
        let store = FileStore::new(root.to_string_lossy().as_ref(), &Key::new());

        // Both forms, and an interior segment — `FileStore` is `AsyncFileStore` minus the lock, so
        // the segment rule has to hold here too and not only in the async twin.
        for text in [
            "file.__metadata__",
            "__metadata__",
            "data/__metadata__/file.json",
        ] {
            let key = parse_key(text)?;
            assert!(!store.is_supported(&key), "{text}");
            assert_not_supported(store.key_to_path(&key), text);
            assert_not_supported(store.key_to_path_metadata(&key), text);
        }

        // The lock suffix belongs to `AsyncFileStore`'s layout, not this one.
        let lock_shaped = parse_key("file.__lock__")?;
        assert!(
            store.is_supported(&lock_shaped),
            "FileStore takes no locks — this key is addressable"
        );
        assert!(store.key_to_path(&lock_shaped).is_ok());

        std::fs::remove_dir_all(&sandbox).expect("cleanup");
        Ok(())
    }

    /// `reserved07` — `FileStore` filters its listing by the same predicate, and by *its own* set.
    ///
    /// The synchronous store is obsolete and unreachable (`CORE-SYNC-STORE-TRAIT-OBSOLETE`), and
    /// that is precisely why it needs its own test rather than being trusted to follow
    /// `AsyncFileStore`: nothing else exercises it, so a filter updated in one store and forgotten
    /// in the other would stay invisible until the trait is revived or deleted.
    ///
    /// The last assertion is the per-store half: a file genuinely named `x.__lock__` is an ordinary
    /// asset here and must still be listed.
    #[test]
    fn reserved07_file_store_listing_uses_its_own_reserved_set() -> Result<(), Error> {
        let sandbox = unique_temp_dir("reserved07");
        let root = sandbox.join("root");
        std::fs::create_dir_all(root.join("__metadata__")).expect("legacy metadata folder");
        std::fs::write(root.join("__metadata__").join("report.txt.json"), b"{}")
            .expect("legacy sidecar");
        std::fs::write(root.join("report.txt"), b"body").expect("data file");
        std::fs::write(root.join("report.txt.__metadata__"), b"{}").expect("sidecar");
        std::fs::write(root.join("notes.__lock__"), b"not a lock here").expect("lock-shaped file");

        let store = FileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        let names = store.listdir(&Key::new())?;

        // Reserved by this store's layout — dropped.
        assert!(!names.contains(&"__metadata__".to_owned()), "{names:?}");
        assert!(
            !names.contains(&"report.txt.__metadata__".to_owned()),
            "{names:?}"
        );
        // Not reserved by this store's layout — listed.
        assert!(names.contains(&"report.txt".to_owned()), "{names:?}");
        assert!(names.contains(&"notes.__lock__".to_owned()), "{names:?}");

        std::fs::remove_dir_all(&sandbox).expect("cleanup");
        Ok(())
    }
}
