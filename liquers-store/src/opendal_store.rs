use std::collections::BTreeSet;

use bytes::Buf;
use liquers_core::{
    error::Error,
    metadata::{Metadata, MetadataRecord},
    query::Key,
    store::AsyncStore,
};
use opendal::{Buffer, Operator};

use async_trait::async_trait;
use liquers_core::metadata::Status;


/// What a backend path denotes.
///
/// Explicit, so a caller decoding a listing cannot forget that it yields metadata sidecars and
/// directory entries alongside data entries.
#[cfg(feature = "async_store")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedPath {
    /// A data object.
    Data(Key),
    /// A metadata sidecar; the key is the *data* it describes.
    Metadata(Key),
    /// A directory — the backend path carried a trailing `/`.
    Directory(Key),
}

#[cfg(feature = "async_store")]
impl DecodedPath {
    /// The key this path denotes, whichever kind it is.
    pub fn key(&self) -> &Key {
        match self {
            DecodedPath::Data(key) => key,
            DecodedPath::Metadata(key) => key,
            DecodedPath::Directory(key) => key,
        }
    }
}

/// The one place that maps a [`Key`] onto a backend path and back.
///
/// A store key is absolute (see `liquers_core::store`), so every fallible entry point starts with
/// [`Key::as_absolute`]. A data path is the key's `encode()` form. A **directory path additionally
/// carries a trailing `/`**, which OpenDAL requires and which is not cosmetic: without it,
/// `list`, `remove_all` and `create_dir` treat the path as a *prefix*, so `"sub"` also matches
/// `"subway/…"`. That is how `removedir("data")` came to delete `database/`, and why the trailing
/// slash is this type's business rather than each call site's.
///
/// # The metadata suffix, and what this type does not enforce
///
/// The metadata for `foo` lives at `foo.__metadata__`, so the *data* path of the key
/// `foo.__metadata__` is byte-identical to the *metadata* path of the key `foo`. No decoder can be
/// injective over both while preserving the on-disk layout, so such keys are excluded:
/// [`PathMap::is_suffix_ambiguous`] is the rule, and both [`AsyncOpenDALStore::is_supported`] and
/// the store's path entry points consult it.
///
/// The refusal is raised by the *store*, not here, because `Error::key_not_supported` needs a
/// store name that an associated function cannot reach. So `PathMap::data` does **not** itself
/// refuse an ambiguous key — a new call site using it directly would bypass the rule.
/// `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` is what would close that: with a `store` field on
/// `ErrorPayload` and a `with_store_name` builder, these functions could refuse directly and the
/// store would keep only the enrichment.
#[cfg(feature = "async_store")]
pub struct PathMap;

#[cfg(feature = "async_store")]
impl PathMap {
    const METADATA: &'static str = ".__metadata__";

    /// True when the key's filename ends in the metadata suffix, so its data path would collide
    /// with another key's metadata path.
    ///
    /// A predicate rather than a `Result` because [`AsyncStore::is_supported`] returns `bool` and
    /// cannot use an error at all; keeping the rule in one predicate is what lets both it and the
    /// path entry points ask the same question.
    pub fn is_suffix_ambiguous(key: &Key) -> bool {
        key.filename()
            .is_some_and(|file_name| file_name.name.ends_with(Self::METADATA))
    }

    /// The data path: `"sub/foo.txt"`. Fallible via [`Key::as_absolute`].
    pub fn data(key: &Key) -> Result<String, Error> {
        Ok(key.as_absolute()?.encode())
    }

    /// The metadata sidecar path: `"sub/foo.txt.__metadata__"`.
    pub fn metadata(key: &Key) -> Result<String, Error> {
        Ok(format!("{}{}", key.as_absolute()?.encode(), Self::METADATA))
    }

    /// The directory path: `"sub/"`, and `""` for the root key.
    ///
    /// The trailing slash is the whole point; the root maps to the empty string because OpenDAL
    /// spells the backend root that way and `"/"` would name a directory called nothing.
    pub fn directory(key: &Key) -> Result<String, Error> {
        let encoded = key.as_absolute()?.encode();
        if encoded.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("{}/", encoded.trim_end_matches('/')))
        }
    }

    /// Decodes a path the backend returned into what it denotes.
    ///
    /// Order matters and is asserted by the tests: the trailing `/` is stripped **before** the
    /// metadata suffix, and the suffix is stripped from the final segment **once**. Stripping it
    /// repeatedly — which `trim_end_matches` does — would decode `x.__metadata__.__metadata__` to
    /// `x` rather than to `x.__metadata__`.
    pub fn decode(path: &str) -> Result<DecodedPath, Error> {
        use liquers_core::parse;
        let trimmed = path.trim_matches('/');
        let is_directory = path.ends_with('/') && !trimmed.is_empty();
        if is_directory {
            return Ok(DecodedPath::Directory(parse::parse_key(trimmed)?));
        }
        match trimmed.strip_suffix(Self::METADATA) {
            Some(data) => Ok(DecodedPath::Metadata(parse::parse_key(data)?)),
            None => Ok(DecodedPath::Data(parse::parse_key(trimmed)?)),
        }
    }
}

#[cfg(feature = "async_store")]
pub struct AsyncOpenDALStore {
    op: Operator,
    prefix: Key,
}

#[cfg(feature = "async_store")]
impl AsyncOpenDALStore {
    pub fn new(op: Operator, prefix: Key) -> Self {
        AsyncOpenDALStore { op, prefix }
    }

    /// Refuses a key whose filename ends in the metadata suffix.
    ///
    /// The rule lives in [`PathMap::is_suffix_ambiguous`]; the error is raised here because
    /// `Error::key_not_supported` needs this store's name.
    fn reject_ambiguous(&self, key: &Key) -> Result<(), Error> {
        if PathMap::is_suffix_ambiguous(key) {
            return Err(Error::key_not_supported(key, &self.store_name()));
        }
        Ok(())
    }

    /// Maps a key onto a backend path.
    ///
    /// Fallible because a store requires an absolute key: a `.` or `..` element would address
    /// something outside the intended namespace. See `liquers_core::store` for the rule.
    pub fn key_to_path(&self, key: &Key) -> Result<String, Error> {
        self.reject_ambiguous(key)?;
        PathMap::data(key)
    }

    /// Maps a key onto its directory path, with the trailing `/` OpenDAL requires for `list`,
    /// `remove_all` and `create_dir`.
    ///
    /// Refuses a suffix-ambiguous key like the data and metadata forms do. Without the check,
    /// `makedir`, `removedir` and `listdir` accepted a key that `is_supported`, `get` and `set`
    /// all reject — so a reserved-name subtree could be created or deleted through the directory
    /// path while every other entry point refused it. Raised in review of PR #58.
    pub fn key_to_path_dir(&self, key: &Key) -> Result<String, Error> {
        self.reject_ambiguous(key)?;
        PathMap::directory(key)
    }

    /// Decodes a backend path back into a key, whatever kind of path it is.
    pub fn path_to_key(&self, path: &str) -> Result<Key, Error> {
        Ok(PathMap::decode(path)?.key().to_owned())
    }

    /// Maps a key onto its metadata path. Fallible for the same reason as [`Self::key_to_path`].
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<String, Error> {
        self.reject_ambiguous(key)?;
        PathMap::metadata(key)
    }
    fn map_read_error<T>(
        &self,
        key: &Key,
        res: opendal::Result<T>,
    ) -> Result<T, liquers_core::error::Error> {
        res.map_err(|e| {
            liquers_core::error::Error::key_read_error(
                key,
                &self.store_name(),
                &format!("{e} (OpenDAL Read Error)"),
            )
        })
    }
    fn map_write_error<T>(
        &self,
        key: &Key,
        res: opendal::Result<T>,
    ) -> Result<T, liquers_core::error::Error> {
        res.map_err(|e| {
            liquers_core::error::Error::key_write_error(
                key,
                &self.store_name(),
                &format!("{e} (OpenDAL Write Error)"),
            )
        })
    }
    /// True when the backend holds anything under this key's directory path.
    ///
    /// This store's source of directory truth on a backend that has no directory objects — most of
    /// them, `s3`, `gcs`, `azblob` and the SQL backends included. It asks the backend rather than
    /// keeping a `DirectoryIndex`, because the backend is authoritative and may be written by
    /// another process: an index would go stale, and rebuilding it means listing the whole bucket.
    ///
    /// `limit(1)` is a page-size **hint**, not a cap — the memory backend returns two entries for
    /// it — so this tests for non-emptiness and never for a count.
    async fn has_children(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path_dir(key)?;
        let entries = self.map_read_error(key, self.op.list_with(&path).limit(1).await)?;
        Ok(!entries.is_empty())
    }

}

#[cfg(feature = "async_store")]
#[async_trait]
impl AsyncStore for AsyncOpenDALStore {
    /// Get store name
    fn store_name(&self) -> String {
        format!("{} OpenDAL Store", self.key_prefix())
    }

    /// Key prefix common to all keys in this store.
    ///
    /// The configured prefix, matching `AsyncFileStore` and `FileStore`. It used to return the
    /// root key, which made `AsyncStoreRouter::is_dir` and `listdir` answer from this store for
    /// *every* key in the router — `find_store` was unaffected because it also consults
    /// `is_supported`, which does check the real prefix.
    ///
    /// The prefix is part of the path under the backend root, as it is for the file stores, so
    /// `key_to_path` needs no adjustment: only the advertised value was wrong.
    fn key_prefix(&self) -> Key {
        self.prefix.clone()
    }

    /// Create default metadata object for a given key
    /// Records the key and whether it is a directory, as `AsyncMemoryStore` does.
    ///
    /// This ignored both arguments and returned an empty record, so `get_metadata` on a directory
    /// produced a record with `is_dir == false` and no key — a file-shaped answer for a directory.
    /// It mattered little while directory keys were unaddressable on flat backends; now that they
    /// are addressable it is the record a caller actually receives. Raised in review of PR #58.
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        let mut metadata = MetadataRecord::new();
        metadata.with_key(key.to_owned());
        metadata.is_dir = is_dir;
        metadata
    }

    /// Get data asynchronously
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        Ok((self.get_bytes(key).await?, self.get_metadata(key).await?))
    }

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let path = self.key_to_path(key)?;
        let buf = self.map_read_error(key, self.op.read(&path).await)?;
        Ok(buf.to_vec())
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let path = self.key_to_path_metadata(key)?;
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
            let path = self.key_to_path(key)?;
            if self.map_read_error(key, self.op.exists(&path).await)? {
                let stat = self.map_read_error(key, self.op.stat(&path).await)?;
                if stat.is_dir() {
                    // Directory children are deliberately not populated here: `listdir_asset_info`
                    // calls `get_asset_info` per child, which calls `get_metadata` per child
                    // directory — a full recursive walk of the subtree for one directory read.
                    let metadata = self.default_metadata(key, true);
                    return Ok(Metadata::MetadataRecord(metadata));
                } else {
                    let mut metadata = self.default_metadata(key, false);
                    metadata.warning(&format!("Metadata file {} does not exist.", path));
                    metadata.warning("New metadata has been created. (get_metadata)");
                    let mut metadata = Metadata::MetadataRecord(metadata);
                    let data = self.get_bytes(key).await?;
                    self.finalize_metadata(&mut metadata, key, &data, false);
                    //self.set_metadata(key, &metadata)?;
                    return Ok(metadata);
                }
            } else {
                // Neither the data object nor its sidecar exists. On a backend with no directory
                // objects that is also what a *directory* looks like, so ask whether anything is
                // stored under it before reporting the key absent. Without this,
                // `get_metadata("sub")` — and `get_asset_info`, which is built on it — failed for
                // a directory that `listdir` could see, and fixing `is_dir` alone would not have
                // helped: this method overrides the trait default and never calls `is_dir`.
                //
                // The value returned is the one the `stat().is_dir()` branch above returns, so the
                // two paths cannot diverge.
                if self.has_children(key).await? {
                    return Ok(Metadata::MetadataRecord(self.default_metadata(key, true)));
                }
                Err(Error::key_not_found(key))
            }
        }
    }

    /// Store data and metadata.
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let mut tmp_metadata = metadata.clone();
        self.finalize_metadata(&mut tmp_metadata, key, data, true);
        tmp_metadata.set_status(Status::Storing)?;
        self.set_metadata(key, &tmp_metadata).await?;
        let path = self.key_to_path(key)?;
        let buffer = Buffer::from_iter(data.iter().copied());
        self.map_write_error(key, self.op.write(&path, buffer).await)?;
        let mut tmp_metadata = metadata.clone();
        self.finalize_metadata(&mut tmp_metadata, key, data, true);
        self.set_metadata(key, &tmp_metadata).await?;
        Ok(())
    }

    /// Store metadata only
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let metadata_str = match metadata {
            Metadata::MetadataRecord(metadata) => serde_json::to_string_pretty(metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
            Metadata::LegacyMetadata(metadata) => serde_json::to_string_pretty(metadata)
                .map_err(|e| Error::key_write_error(key, &self.store_name(), &e))?,
        };
        let path = self.key_to_path_metadata(key)?;
        self.map_write_error(key, self.op.write(&path, metadata_str).await)?;
        Ok(())
    }

    /// Remove data and metadata associated with the key
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path(key)?;
        if self.map_read_error(key, self.op.exists(&path).await)? {
            self.map_write_error(key, self.op.delete(&path).await)?;
        }
        let matadata_path = self.key_to_path_metadata(key)?;
        if self.map_read_error(key, self.op.exists(&matadata_path).await)? {
            self.map_write_error(key, self.op.delete(&matadata_path).await)?;
        }
        Ok(())
    }

    /// Removes a directory and everything under it.
    ///
    /// Recursive, matching `AsyncMemoryStore` and `AsyncFileStore` — the previous doc comment said
    /// "Files are not removed recursively", which was true of no implementation.
    ///
    /// The path **must** carry a trailing `/`. Without it `remove_all` deletes by *prefix*, so
    /// `removedir("data")` also destroyed `database/`. That was `STORE-OPENDAL-SLASH-HANDLING`
    /// defect 1, and `PathMap::directory` is what prevents it recurring.
    ///
    /// Removing a directory that does not exist is `Ok(())`, as it is for `AsyncFileStore`.
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path_dir(key)?;
        self.map_write_error(key, self.op.remove_all(&path).await)
    }

    /// Returns true if store contains the key.
    ///
    /// Data, else the metadata sidecar, else a directory — the same order `AsyncMemoryStore` uses.
    /// Without the last step a directory visible to `listdir` was reported as not contained.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        if self.map_read_error(key, self.op.exists(&path).await)? {
            return Ok(true);
        }
        let metadata_path = self.key_to_path_metadata(key)?;
        if self.map_read_error(key, self.op.exists(&metadata_path).await)? {
            return Ok(true);
        }
        self.is_dir(key).await
    }

    /// Returns true if key points to a directory.
    ///
    /// `stat` first, because a backend that has real directories answers in O(1). When it reports
    /// the path absent, fall back to asking whether anything is stored under it: on an object
    /// store a directory has no object of its own, so `stat` finding nothing says nothing about
    /// whether the directory exists.
    ///
    /// An absent key is `Ok(false)`, matching every other store. Any **other** backend error still
    /// propagates — an S3 403 must not be reported as "not a directory", which is why this matches
    /// `NotFound` specifically rather than testing `is_err()`. The catch-all arm is over
    /// `opendal::ErrorKind`, a foreign `#[non_exhaustive]` enum, and is the one place this file
    /// needs one.
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let path = self.key_to_path(key)?;
        match self.op.stat(&path).await {
            Ok(stat) => Ok(stat.is_dir()),
            Err(e) if e.kind() == opendal::ErrorKind::NotFound => self.has_children(key).await,
            Err(e) => Err(Error::key_read_error(
                key,
                &self.store_name(),
                &format!("{e} (OpenDAL Read Error)"),
            )),
        }
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix()).await?;
        if !keys.contains(&self.key_prefix()) {
            keys.push(self.key_prefix().to_owned());
        }
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        /*
        if !self.is_dir(key).await? {
            return Err(Error::general_error(format!("Key {} is not a directory", key)).with_key(key));
        }
        */
        let mut list = BTreeSet::new();
        let path = self.key_to_path_dir(key)?;
        let entries = self.map_read_error(key, self.op.list(&path).await)?;
        for entry in entries {
            // A path the backend returned that this store cannot decode is skipped rather than
            // failing the listing: one unexpected object in a shared bucket must not make a
            // directory unlistable.
            let Ok(decoded) = PathMap::decode(entry.path()) else {
                continue;
            };
            if decoded.key() == key {
                continue;
            }
            // A metadata sidecar implies its data key, so both yield the same name.
            let Some(name) = decoded.key().filename() else {
                continue;
            };
            list.insert(name.encode().to_string());
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
        let mut list = BTreeSet::new();
        // The trailing slash is required: `list_with` on a path without one matches by *prefix*,
        // so listing `sub` also returned everything under `subway/`.
        let path = self.key_to_path_dir(key)?;
        let entries = self.map_read_error(key, self.op.list_with(&path).recursive(true).await)?;
        for entry in entries {
            if let Ok(decoded) = PathMap::decode(entry.path()) {
                let sub = decoded.key();
                list.extend(((key.len() + 1)..=sub.len()).filter_map(|i| sub.prefix_of_size(i)));
                list.insert(sub.to_owned());
            }
        }
        Ok(list.into_iter().collect())
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let path = self.key_to_path_dir(key)?;
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
        !key.is_relative()
            && key.has_key_prefix(&self.prefix)
            && !PathMap::is_suffix_ambiguous(key)
    }
}

#[cfg(feature = "async_store")]
#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::context::{EnvRef, Environment, SimpleEnvironment};
    use liquers_core::metadata::{Metadata, MetadataRecord};
    use liquers_core::parse::parse_key;
    use liquers_core::value::Value;
    use opendal::services::Memory;
    use opendal::Operator;

    /// `keyabs16` — the OpenDAL store refuses relative keys.
    ///
    /// A `.` or `..` element here is a namespace escape rather than a filesystem one, but the rule
    /// is the same everywhere: a store requires an absolute key. Asserts the error *type*, so a
    /// backend that happened to fail for an unrelated reason would not satisfy it.
    #[tokio::test]
    async fn keyabs16_opendal_store_refuses_relative_keys() {
        use liquers_core::error::ErrorType;

        let op = Operator::new(Memory::default())
            .expect("memory operator")
            .finish();
        let store = AsyncOpenDALStore::new(op, Key::new());
        let metadata = Metadata::new();

        for text in ["../escape", "a/../../etc/passwd", "a/./b", ".."] {
            let key = parse_key(text).expect("key parses");
            for error in [
                store.get(&key).await.err(),
                store.get_bytes(&key).await.err(),
                store.set(&key, b"x", &metadata).await.err(),
                store.set_metadata(&key, &metadata).await.err(),
                store.remove(&key).await.err(),
                store.contains(&key).await.err(),
                store.makedir(&key).await.err(),
            ] {
                let error = error.unwrap_or_else(|| panic!("{text} must be refused"));
                assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
            }
            assert!(!store.is_supported(&key), "{text} must not route here");
            assert!(store.key_to_path(&key).is_err(), "path builder {text}");
        }

        let ok = parse_key("data/report.txt").expect("key parses");
        assert!(store.is_supported(&ok));
    }

    /// A memory-backed store: no directory objects, which is the object-store shape.
    fn memory_store() -> AsyncOpenDALStore {
        let op = Operator::new(Memory::default())
            .expect("memory operator")
            .finish();
        AsyncOpenDALStore::new(op, Key::new())
    }

    /// A filesystem-backed store in a uniquely named temp directory, removed on drop.
    ///
    /// The two backends differ in exactly the way that matters — one has directory objects and the
    /// other does not — so a fix verified on only one proves nothing about the other.
    #[cfg(feature = "services-fs")]
    struct FsStore {
        store: AsyncOpenDALStore,
        root: std::path::PathBuf,
    }

    #[cfg(feature = "services-fs")]
    impl Drop for FsStore {
        fn drop(&mut self) {
            let _ignore = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(feature = "services-fs")]
    fn fs_store(label: &str) -> FsStore {
        let root = std::env::temp_dir().join(format!(
            "liquers_opendal_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let _ignore = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        let op = Operator::new(
            opendal::services::Fs::default().root(root.to_str().expect("utf-8 temp path")),
        )
        .expect("fs operator")
        .finish();
        FsStore {
            store: AsyncOpenDALStore::new(op, Key::new()),
            root,
        }
    }

    /// `PATHMAP01` — every supported key round-trips: `decode(data(k))` is `Data(k)`.
    ///
    /// The corpus is deliberately adversarial: multi-segment keys, dots inside names, a name
    /// containing the metadata suffix without ending in it, the root, and — the case that
    /// motivated all of this — two names where one is a prefix of the other.
    ///
    /// **Not covered: non-ASCII names.** `parse_key("données/rapport.csv")` fails at HEAD —
    /// `resource_name` accepts only alphanumerics, `_`, `.` and `-`, so such a key cannot reach a
    /// store to be mapped. That is `RESOURCE-NAME-ASCII-ONLY`, not a gap in this mapping; the
    /// corpus records the boundary rather than pretending to test past it.
    #[test]
    fn pathmap01_data_paths_round_trip() -> Result<(), Error> {
        for text in [
            "",
            "a",
            "a/b",
            "a/b/c.txt",
            "data/input.csv",
            "sub",
            "subway",
            "sub/deeper/foo.txt",
            "a.b.c/d.e.f",
            "x.__metadata__.txt",
            "one/two/three/four/five/six.bin",
            "UPPER/MixedCase/file-name_v2.tar.gz",
        ] {
            let key = parse_key(text)?;
            let path = PathMap::data(&key)?;
            assert_eq!(
                PathMap::decode(&path)?,
                DecodedPath::Data(key.clone()),
                "round trip {text} via {path}"
            );
        }
        Ok(())
    }

    /// `PATHMAP02` — a metadata path decodes to the key of the **data** it describes.
    #[test]
    fn pathmap02_metadata_paths_decode_to_their_data_key() -> Result<(), Error> {
        for text in ["a", "a/b/c.txt", "sub/deeper/report.csv"] {
            let key = parse_key(text)?;
            let path = PathMap::metadata(&key)?;
            assert_eq!(PathMap::decode(&path)?, DecodedPath::Metadata(key.clone()), "{text}");
        }
        Ok(())
    }

    /// `PATHMAP03` — a key whose filename ends in the suffix is refused, in one rule.
    ///
    /// Its data path would be byte-identical to another key's metadata path, so no decoder can be
    /// injective over both. The exclusion is asserted rather than left implicit: `is_supported`,
    /// `key_to_path` and `key_to_path_metadata` must all agree.
    #[test]
    fn pathmap03_suffix_ambiguous_keys_are_refused_everywhere() -> Result<(), Error> {
        use liquers_core::error::ErrorType;
        let store = memory_store();
        for text in ["a.__metadata__", "sub/a.__metadata__"] {
            let key = parse_key(text)?;
            assert!(PathMap::is_suffix_ambiguous(&key), "{text} is ambiguous");
            assert!(!store.is_supported(&key), "{text} must not route here");
            for error in [
                store.key_to_path(&key).err(),
                store.key_to_path_metadata(&key).err(),
            ] {
                let error = error.unwrap_or_else(|| panic!("{text} must be refused"));
                assert_eq!(error.error_type, ErrorType::KeyNotSupported, "{text}");
            }
        }
        // A name that merely *contains* the suffix is fine.
        let ok = parse_key("x.__metadata__.txt")?;
        assert!(!PathMap::is_suffix_ambiguous(&ok));
        assert!(store.is_supported(&ok));
        Ok(())
    }

    /// `PATHMAP04` — the directory form carries exactly one trailing slash; the root is empty.
    #[test]
    fn pathmap04_directory_form_carries_one_trailing_slash() -> Result<(), Error> {
        assert_eq!(PathMap::directory(&Key::new())?, "");
        assert_eq!(PathMap::directory(&parse_key("sub")?)?, "sub/");
        assert_eq!(PathMap::directory(&parse_key("a/b/c")?)?, "a/b/c/");
        assert_eq!(PathMap::data(&parse_key("sub")?)?, "sub", "data form has none");
        Ok(())
    }

    /// `PATHMAP05` — decode order: the trailing slash first, then the suffix, once.
    ///
    /// Stripping the suffix repeatedly — what `trim_end_matches` did — decoded
    /// `x.__metadata__.__metadata__` to `x` instead of to `x.__metadata__`.
    #[test]
    fn pathmap05_decode_order_and_single_strip() -> Result<(), Error> {
        assert_eq!(PathMap::decode("sub/")?, DecodedPath::Directory(parse_key("sub")?));
        assert_eq!(PathMap::decode("sub/f.txt")?, DecodedPath::Data(parse_key("sub/f.txt")?));
        assert_eq!(
            PathMap::decode("sub/f.txt.__metadata__")?,
            DecodedPath::Metadata(parse_key("sub/f.txt")?)
        );
        assert_eq!(
            PathMap::decode("x.__metadata__.__metadata__")?,
            DecodedPath::Metadata(parse_key("x.__metadata__")?),
            "the suffix is stripped once, not repeatedly"
        );
        assert_eq!(PathMap::decode("")?, DecodedPath::Data(Key::new()));
        Ok(())
    }

    /// `PATHMAP06` — a listing entry that cannot be decoded is skipped, not fatal.
    ///
    /// One unexpected object in a shared bucket must not make a directory unlistable. A stray
    /// sidecar is reported under its data name, which is what a sidecar implies.
    #[tokio::test]
    async fn pathmap06_undecodable_listing_entries_are_skipped() -> Result<(), Error> {
        let op = Operator::new(Memory::default()).expect("memory operator").finish();
        op.write("sub/orphan.__metadata__", "{}")
            .await
            .map_err(|e| Error::general_error(e.to_string()))?;
        let store = AsyncOpenDALStore::new(op, Key::new());

        assert_eq!(
            store.listdir(&parse_key("sub")?).await?,
            vec!["orphan".to_string()],
            "a sidecar implies its data key"
        );
        Ok(())
    }

    /// `SIBLING01` — `removedir` must not reach a directory whose name shares its prefix.
    ///
    /// The P0. `remove_all` on a path with no trailing slash deletes by *prefix*, so
    /// `removedir("data")` destroyed `database/`. Reachable through
    /// `DELETE /api/store/removedir/{*key}`.
    #[tokio::test]
    async fn sibling01_removedir_leaves_a_prefix_sharing_sibling() -> Result<(), Error> {
        let fs = fs_store("sibling01");
        for store in [&memory_store(), &fs.store] {
            let inside = parse_key("data/input.csv")?;
            let sibling = parse_key("database/export.csv")?;
            store.set(&inside, b"in", &Metadata::new()).await?;
            store.set(&sibling, b"out", &Metadata::new()).await?;

            store.removedir(&parse_key("data")?).await?;

            assert!(!store.contains(&inside).await?, "the named directory is gone");
            assert!(store.contains(&sibling).await?, "the sibling survives");
            assert_eq!(store.get_bytes(&sibling).await?, b"out", "and is intact");
        }
        Ok(())
    }

    /// `SIBLING02` — the same, one level down.
    #[tokio::test]
    async fn sibling02_removedir_is_scoped_at_depth() -> Result<(), Error> {
        let fs = fs_store("sibling02");
        for store in [&memory_store(), &fs.store] {
            store.set(&parse_key("p/sub/a.txt")?, b"a", &Metadata::new()).await?;
            store.set(&parse_key("p/subway/b.txt")?, b"b", &Metadata::new()).await?;

            store.removedir(&parse_key("p/sub")?).await?;

            assert!(!store.contains(&parse_key("p/sub/a.txt")?).await?);
            assert!(store.contains(&parse_key("p/subway/b.txt")?).await?);
        }
        Ok(())
    }

    /// `SIBLING03` — a recursive listing must not return keys from a prefix-sharing sibling.
    #[tokio::test]
    async fn sibling03_listdir_keys_deep_excludes_siblings() -> Result<(), Error> {
        let fs = fs_store("sibling03");
        for store in [&memory_store(), &fs.store] {
            store.set(&parse_key("sub/a.txt")?, b"a", &Metadata::new()).await?;
            store.set(&parse_key("subway/b.txt")?, b"b", &Metadata::new()).await?;

            let deep = store.listdir_keys_deep(&parse_key("sub")?).await?;
            let encoded: Vec<String> = deep.iter().map(|k| k.encode()).collect();

            assert!(encoded.contains(&"sub/a.txt".to_string()), "got {encoded:?}");
            assert!(
                !encoded.iter().any(|k| k.starts_with("subway")),
                "a sibling leaked into the listing: {encoded:?}"
            );
        }
        Ok(())
    }

    /// `REMOVE01` — removing a directory that does not exist is a no-op, as in `AsyncFileStore`.
    #[tokio::test]
    async fn remove01_removedir_on_an_absent_directory_is_ok() -> Result<(), Error> {
        let fs = fs_store("remove01");
        for store in [&memory_store(), &fs.store] {
            store.removedir(&parse_key("never/existed")?).await?;
        }
        Ok(())
    }

    /// `REMOVE02` — `removedir` on the root key empties the store. Deliberate, and asserted so.
    ///
    /// This is the one case where scoping the delete to a directory narrows nothing: the root
    /// directory *is* everything. `AsyncFileStore` would do the same.
    #[tokio::test]
    async fn remove02_removedir_on_the_root_empties_the_store() -> Result<(), Error> {
        let store = memory_store();
        store.set(&parse_key("a/b.txt")?, b"x", &Metadata::new()).await?;
        store.set(&parse_key("c.txt")?, b"y", &Metadata::new()).await?;

        store.removedir(&Key::new()).await?;

        assert!(!store.contains(&parse_key("a/b.txt")?).await?);
        assert!(!store.contains(&parse_key("c.txt")?).await?);
        Ok(())
    }

    /// `FSREG01` — the filesystem behaviour that already worked, kept working.
    ///
    /// This is the 2026-08-29 reproduction that concluded the store was correct, turned from
    /// printed output into assertions. It was right about everything it tested; the defects lived
    /// in cases it did not reach.
    #[cfg(feature = "services-fs")]
    #[tokio::test]
    async fn fsreg01_nested_key_round_trip_on_the_filesystem() -> Result<(), Error> {
        let fs = fs_store("fsreg01");
        let store = &fs.store;
        let key = parse_key("sub/deeper/foo.txt")?;
        store.set(&key, b"hello", &Metadata::new()).await?;

        assert_eq!(store.get_bytes(&key).await?, b"hello");
        assert!(store.contains(&key).await?);
        assert!(store.is_dir(&parse_key("sub")?).await?);
        assert!(!store.is_dir(&key).await?);
        assert_eq!(store.listdir(&Key::new()).await?, vec!["sub".to_string()]);
        assert_eq!(store.listdir(&parse_key("sub")?).await?, vec!["deeper".to_string()]);
        assert_eq!(
            store.listdir(&parse_key("sub/deeper")?).await?,
            vec!["foo.txt".to_string()]
        );
        assert_eq!(
            store
                .listdir_keys(&parse_key("sub/deeper")?)
                .await?
                .iter()
                .map(|k| k.encode())
                .collect::<Vec<_>>(),
            vec!["sub/deeper/foo.txt".to_string()]
        );
        assert!(store.get_metadata(&parse_key("sub")?).await.is_ok());
        Ok(())
    }

    /// `PREFIX01` — a prefixed store advertises its prefix and enumerates only within it.
    ///
    /// Both halves matter: `key_prefix()` reports `data`, and the backend path still *contains*
    /// `data`, because the prefix is part of the path under the backend root rather than a mount
    /// point that gets stripped. `liquers-web`'s `FetchStore` is the documented exception.
    #[tokio::test]
    async fn prefix01_a_prefixed_store_reports_and_respects_its_prefix() -> Result<(), Error> {
        let op = Operator::new(Memory::default()).expect("memory operator").finish();
        let store = AsyncOpenDALStore::new(op, parse_key("data")?);
        let key = parse_key("data/input.csv")?;
        store.set(&key, b"rows", &Metadata::new()).await?;

        assert_eq!(store.key_prefix(), parse_key("data")?);
        assert!(store.store_name().starts_with("data"), "got {}", store.store_name());
        assert_eq!(store.key_to_path(&key)?, "data/input.csv", "the prefix is part of the path");
        assert!(store.is_supported(&key));
        assert!(!store.is_supported(&parse_key("other/input.csv")?));
        Ok(())
    }

    /// `SIBLING04` — a prefixed store does not enumerate a prefix-sharing directory beside it.
    ///
    /// **This test needs both fixes.** Without the trailing slash, `keys()` lists `database/…`
    /// because `list_with("data")` matches by prefix. Without `key_prefix()`, it enumerates from
    /// the backend root and reaches `database/` that way. Remove either and it fails.
    #[tokio::test]
    async fn sibling04_a_prefixed_store_enumerates_only_its_own_subtree() -> Result<(), Error> {
        let op = Operator::new(Memory::default()).expect("memory operator").finish();
        // Written through an unprefixed view, so both directories exist in one backend root.
        let root = AsyncOpenDALStore::new(op.clone(), Key::new());
        root.set(&parse_key("data/input.csv")?, b"in", &Metadata::new()).await?;
        root.set(&parse_key("database/export.csv")?, b"out", &Metadata::new()).await?;

        let store = AsyncOpenDALStore::new(op, parse_key("data")?);
        let keys: Vec<String> = store.keys().await?.iter().map(|k| k.encode()).collect();

        assert!(keys.contains(&"data/input.csv".to_string()), "got {keys:?}");
        assert!(
            !keys.iter().any(|k| k.starts_with("database")),
            "a prefix-sharing sibling leaked into keys(): {keys:?}"
        );
        Ok(())
    }

    /// `ROUTER01` — a prefixed OpenDAL store no longer answers for keys outside its prefix.
    ///
    /// `AsyncStoreRouter::is_dir` consults **only** `key_prefix()`, unlike `find_store`, which also
    /// requires `is_supported`. So a store claiming the root prefix answered `is_dir` for every key
    /// in the router, including keys belonging to a store listed after it.
    #[tokio::test]
    async fn router01_a_prefixed_store_does_not_claim_the_whole_router() -> Result<(), Error> {
        use liquers_core::store::{AsyncMemoryStore, AsyncStoreRouter};

        let op = Operator::new(Memory::default()).expect("memory operator").finish();
        let mut router = AsyncStoreRouter::new();
        router.add_store(Box::new(AsyncOpenDALStore::new(op, parse_key("data")?)));
        router.add_store(Box::new(AsyncMemoryStore::new(&parse_key("other")?)));

        router
            .set(&parse_key("data/in.csv")?, b"in", &Metadata::new())
            .await?;
        router
            .set(&parse_key("other/out.csv")?, b"out", &Metadata::new())
            .await?;

        assert_eq!(router.get_bytes(&parse_key("data/in.csv")?).await?, b"in");
        assert_eq!(router.get_bytes(&parse_key("other/out.csv")?).await?, b"out");
        assert!(
            router.is_dir(&parse_key("other")?).await?,
            "the second store's directory is answered by the second store, not claimed by the first"
        );
        // `router.is_dir("data")` is asserted by `DIR04`, which delegates to the OpenDAL store and
        // therefore needs the directory fallback this commit's successor adds.
        Ok(())
    }

    #[tokio::test]
    async fn test_async_opendal_store_memory_basic() {
        // Create a memory operator
        let memory = Memory::default();
        let op = Operator::new(memory).unwrap().finish();
        let store = AsyncOpenDALStore::new(op, Key::new());
        let key = parse_key("foo.txt").unwrap();
        let data = b"hello world";

        // Write data
        store.set(&key, data, &Metadata::new()).await.unwrap();

        // Read data
        let (read_data, read_metadata) = store.get(&key).await.unwrap();
        assert_eq!(read_data, data);
        assert!(matches!(read_metadata, Metadata::MetadataRecord(_)));

        // Remove data
        store.remove(&key).await.unwrap();
        let result = store.get(&key).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_opendal_store_metadata() {
        let memory = Memory::default();
        let op = Operator::new(memory).unwrap().finish();
        let store = AsyncOpenDALStore::new(op, parse_key("").unwrap());
        let key = parse_key("bar.txt").unwrap();
        let data = b"testdata";
        let mut metadata = MetadataRecord::new();
        metadata
            .with_title("Test Title".into())
            .with_filename("bar.txt".into());

        // Write data and metadata
        store.set(&key, data, &metadata.into()).await.unwrap();

        // Read metadata
        let read_metadata = store.get_metadata(&key).await.unwrap();
        if let Metadata::MetadataRecord(m) = read_metadata {
            assert_eq!(m.title, "Test Title");
            assert_eq!(m.filename(), Some("bar.txt".to_string()));
        } else {
            panic!("Expected MetadataRecord");
        }
    }

    #[tokio::test]
    async fn test_opendal_dir() {
        // Create a memory operator
        let memory = Memory::default();
        let op = Operator::new(memory).unwrap().finish();
        let store = AsyncOpenDALStore::new(op, Key::new());

        assert_eq!(store.keys().await.unwrap().len(), 1);
        assert!(store.listdir(&Key::new()).await.unwrap().is_empty());
        assert!(store.listdir_keys(&Key::new()).await.unwrap().is_empty());

        let key = parse_key("foo.txt").unwrap();
        let data = b"hello world";

        // Write data
        store.set(&key, data, &Metadata::new()).await.unwrap();

        assert_eq!(store.keys().await.unwrap().len(), 2);
        assert!(store
            .keys()
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.encode())
            .collect::<Vec<_>>()
            .contains(&"foo.txt".to_string()));
        assert!(store.listdir(&Key::new()).await.unwrap().len() == 1);
        assert!(store.listdir(&Key::new()).await.unwrap()[0] == "foo.txt");
        assert!(store.listdir_keys(&Key::new()).await.unwrap().len() == 1);
        assert!(store.listdir_keys(&Key::new()).await.unwrap()[0].encode() == "foo.txt");
        assert!(store.listdir_keys_deep(&Key::new()).await.unwrap().len() == 1);
        assert!(store.listdir_keys_deep(&Key::new()).await.unwrap()[0].encode() == "foo.txt");

        // Remove data
        store.remove(&key).await.unwrap();
        let result = store.get(&key).await;
        assert!(result.is_err());

        assert_eq!(store.keys().await.unwrap().len(), 1);
        assert!(store.listdir(&Key::new()).await.unwrap().is_empty());
        assert!(store.listdir_keys(&Key::new()).await.unwrap().is_empty());
    }

    /// The memory backend has no directory objects, which is the object-store shape.
    ///
    /// This test used to carry the note "memory backend does not support directories explicitly,
    /// so not everything works as it should" and a commented-out block of assertions. That was the
    /// bug, written down and then tolerated: `listdir` could see a directory that `is_dir`,
    /// `contains` and `get_metadata` all denied. The assertions are live now.
    #[tokio::test]
    async fn test_opendal_subdir() -> Result<(), Error> {
        let store = memory_store();

        assert_eq!(store.keys().await?.len(), 1, "the root key alone");
        assert!(store.listdir(&Key::new()).await?.is_empty());
        assert!(store.listdir_keys(&Key::new()).await?.is_empty());

        let key = parse_key("sub/foo.txt")?;
        let subkey = parse_key("sub")?;

        store.set(&key, b"hello world", &Metadata::new()).await?;
        assert!(store.contains(&key).await?);
        assert!(store.is_dir(&subkey).await?);

        // Root, the directory, and the data key.
        let encoded: Vec<String> = store.keys().await?.iter().map(|k| k.encode()).collect();
        assert_eq!(encoded.len(), 3, "got {encoded:?}");
        assert!(encoded.contains(&"sub/foo.txt".to_string()));

        assert_eq!(store.listdir(&subkey).await?, vec!["foo.txt".to_string()]);
        assert_eq!(
            store.listdir_keys(&subkey).await?.iter().map(|k| k.encode()).collect::<Vec<_>>(),
            vec!["sub/foo.txt".to_string()]
        );
        assert_eq!(
            store.listdir_keys_deep(&subkey).await?.iter().map(|k| k.encode()).collect::<Vec<_>>(),
            vec!["sub/foo.txt".to_string()]
        );
        Ok(())
    }

    /// `DIR01` — on a backend with no directory objects, addressing agrees with listing.
    #[tokio::test]
    async fn dir01_directory_key_is_addressable_without_directory_objects() -> Result<(), Error> {
        let store = memory_store();
        let key = parse_key("data/reports/q3.csv")?;
        let dir = parse_key("data/reports")?;
        store.set(&key, b"rows", &Metadata::new()).await?;

        assert_eq!(store.listdir(&dir).await?, vec!["q3.csv".to_string()]);
        assert!(store.is_dir(&dir).await?, "listing sees it, so addressing must too");
        assert!(store.contains(&dir).await?);
        assert!(store.get_metadata(&dir).await.is_ok(), "directory metadata");
        assert!(store.get_asset_info(&dir).await.is_ok(), "and asset info built on it");
        Ok(())
    }

    /// `DIR05` — directory metadata says it is a directory, and names its key.
    ///
    /// Raised in review of PR #58: `default_metadata` ignored both arguments, so the record a
    /// caller got for a directory was indistinguishable from one for a file.
    #[tokio::test]
    async fn dir05_directory_metadata_is_marked_as_a_directory() -> Result<(), Error> {
        let store = memory_store();
        let dir = parse_key("data/reports")?;
        store
            .set(&parse_key("data/reports/q3.csv")?, b"rows", &Metadata::new())
            .await?;

        let Metadata::MetadataRecord(record) = store.get_metadata(&dir).await? else {
            panic!("expected a MetadataRecord for a directory");
        };
        assert!(record.is_dir, "a directory must be marked as one");
        assert_eq!(record.key, Some(dir), "and must name its key");
        Ok(())
    }

    /// `PATHMAP07` — the directory form refuses a suffix-ambiguous key, like the other forms.
    ///
    /// Raised in review of PR #58: `makedir`, `removedir` and `listdir` reached the directory
    /// mapper without the ambiguity check, so they accepted a key every other entry point refused.
    #[tokio::test]
    async fn pathmap07_directory_form_refuses_suffix_ambiguous_keys() -> Result<(), Error> {
        use liquers_core::error::ErrorType;
        let store = memory_store();
        let key = parse_key("reserved.__metadata__")?;

        assert!(store.key_to_path_dir(&key).is_err());
        for error in [
            store.makedir(&key).await.err(),
            store.removedir(&key).await.err(),
            store.listdir(&key).await.err(),
        ] {
            let error = error.unwrap_or_else(|| panic!("directory ops must refuse the key"));
            assert_eq!(error.error_type, ErrorType::KeyNotSupported);
        }
        Ok(())
    }

    /// `DIR02` — an absent key is `Ok(false)`, not an error.
    ///
    /// Every other store answers this way: `AsyncFileStore`, `AsyncMemoryStore`, and the trait
    /// default. This store returning `Err` was the divergence.
    #[tokio::test]
    async fn dir02_is_dir_on_an_absent_key_is_false_not_an_error() -> Result<(), Error> {
        let fs = fs_store("dir02");
        for store in [&memory_store(), &fs.store] {
            assert!(!store.is_dir(&parse_key("nothing/here")?).await?);
            assert!(!store.contains(&parse_key("nothing/here")?).await?);
        }
        Ok(())
    }

    /// `DIR03` — `has_children` is non-emptiness, never a count.
    ///
    /// `limit(1)` is a page-size hint: the memory backend returns two entries for it, because a
    /// data object and its sidecar arrive together. A test asserting a count would pass on one
    /// backend and fail on another.
    #[tokio::test]
    async fn dir03_directory_detection_does_not_depend_on_a_count() -> Result<(), Error> {
        let store = memory_store();
        for i in 0..5 {
            store
                .set(&parse_key(&format!("many/f{i}.txt"))?, b"x", &Metadata::new())
                .await?;
        }
        assert!(store.is_dir(&parse_key("many")?).await?);
        assert_eq!(store.listdir(&parse_key("many")?).await?.len(), 5);
        Ok(())
    }

    /// `DIR04` — the router's `is_dir` reaches a prefixed OpenDAL store's directory.
    ///
    /// The half of `ROUTER01` that had to wait for this commit: `AsyncStoreRouter::is_dir`
    /// delegates on `key_prefix()`, and the store it delegates to could not answer for a directory
    /// with no directory object.
    #[tokio::test]
    async fn dir04_router_is_dir_reaches_a_prefixed_opendal_store() -> Result<(), Error> {
        use liquers_core::store::{AsyncMemoryStore, AsyncStoreRouter};

        let op = Operator::new(Memory::default()).expect("memory operator").finish();
        let mut router = AsyncStoreRouter::new();
        router.add_store(Box::new(AsyncOpenDALStore::new(op, parse_key("data")?)));
        router.add_store(Box::new(AsyncMemoryStore::new(&parse_key("other")?)));

        router.set(&parse_key("data/in.csv")?, b"in", &Metadata::new()).await?;

        assert!(router.is_dir(&parse_key("data")?).await?);
        Ok(())
    }
    /// Names `opendal::services::Fs`, which exists only behind the service feature — a build
    /// with OpenDAL linked but `services-fs` off must skip this rather than fail to compile.
    #[cfg(feature = "services-fs")]
    #[tokio::test]
    async fn test_opendal_localfs() {
        let op = opendal::Operator::new(opendal::services::Fs::default().root("."))
            .expect("OpenDAL FS store")
            .finish();
        let store: Box<dyn AsyncStore> = Box::new(AsyncOpenDALStore::new(op, Key::new()));
        for k in store.keys().await.unwrap() {
            assert!(store.contains(&k).await.unwrap());
            store.get_asset_info(&k).await.unwrap();
        }
        store.listdir(&parse_key("src").unwrap()).await.unwrap();
        store
            .listdir_keys(&parse_key("src").unwrap())
            .await
            .unwrap();
        let mut env = SimpleEnvironment::<Value>::new();
        env.with_async_store(store);

        let envref: EnvRef<SimpleEnvironment<Value>> = env.to_ref();

        let a = envref.evaluate("-R-dir/src").await.unwrap();
        let s = a.get().await.expect("Failed to get asset state");
        // Both branches used to `eprintln!`, so this test reported `ok` whether or not `-R-dir/src`
        // returned an `AssetInfo` — and it is the only end-to-end coverage of `get_asset_info`
        // through the interpreter. `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`.
        let Value::AssetInfo(info) = s.data_unchecked().as_ref() else {
            panic!(
                "-R-dir/src must evaluate to AssetInfo, got {:?}",
                s.data_unchecked()
            );
        };
        let names: std::collections::HashSet<String> = info
            .iter()
            .filter_map(|x| x.filename.clone())
            .collect();
        assert!(
            names.contains("opendal_store.rs"),
            "the listing of src/ must contain this file, got {names:?}"
        );
    }
}
