//! `LocalStorageStore` — the full `AsyncStore` contract over the browser's `localStorage`.
//!
//! # Layout
//!
//! Three entry kinds under one namespace. `namespace` is validated `/`-free at construction, so an
//! entry name parses back unambiguously.
//!
//! | Entry name | Holds |
//! |---|---|
//! | `{ns}/d/{key}` | the data envelope ([`crate::store::encoding`]) |
//! | `{ns}/m/{key}` | `Metadata::to_json()` |
//! | `{ns}/D/{key}` | an empty-directory marker (empty value) |
//!
//! The key is **not** stripped of the store's routing prefix: the namespace already separates this
//! store's entries from everything else in `localStorage`, and keeping the whole key is what makes
//! the entry name reversible — which is what lets the index be rebuilt by scanning.
//!
//! # The index is derived, never stored separately
//!
//! At construction the store enumerates `localStorage` and rebuilds `dirs`, `explicit_dirs` and
//! `used_bytes` from the entries themselves. There is deliberately no persisted index entry: a
//! second source of truth can disagree with the entries after a partial write, and reconciling
//! them is a bug farm. Empty directories are the one thing not derivable from data entries, which
//! is exactly why they get their own marker entry — and why `makedir` survives a reload.
//!
//! # Why `RefCell` is sound here
//!
//! The Web Storage API is **synchronous**, so none of the `AsyncStore` methods below contains an
//! `.await`. The crate's borrow rule — no `RefCell` borrow held across an `await` or a call into
//! JavaScript — is satisfied by construction for the first half, and by discipline for the second:
//! each method computes what it needs, drops the borrow, then touches storage. That is the single
//! easiest thing to get wrong in this file.
//!
//! It is also why `STORE06` has a real answer: there is no interleaving point, so concurrent
//! writes serialize and the last one is intact rather than torn.
//!
//! # Data-only access
//!
//! The data-only methods of `AsyncStore` (`STORE_SEMANTICS.md` §11) work on the `d` entries alone,
//! so a `localstorage` mount can take a `metadata` section: a `per_entry` or `grouped` record is
//! then simply another `d` entry, and `native` storage reads the `m` entry through
//! `get_stored_metadata`. No `m` entry is ever read, written or repaired by `get_data`, `set_data`,
//! `remove_data`, `rename_data`, `listdir_data` or `data_facts`.
//!
//! What [`AsyncStore::data_capabilities`] claims, and why each is honest:
//!
//! - **`atomic_write`** — one `setItem` replaces one entry; a reader sees the old string or the
//!   new one, never a mix.
//! - **`rename`** — the copy and the removal run synchronously with no `.await` between them, so
//!   no other code on the page can observe the store between the two steps. A browser crash
//!   between them leaves the object at *both* keys, never at neither. (Another tab is not
//!   observed at all; see the limitation below.)
//! - **`etag`** — a 64-bit FNV-1a hash of the stored envelope, so any change of the bytes changes
//!   it. This is what lets a metadata layer notice a same-size overwrite.
//! - **no `modified_time`** — the store keeps no write time, and inventing one at read time would
//!   make every read look like a change.
//!
//! # Known limitation
//!
//! A second tab writing the same namespace is not observed; this store's derived index goes stale
//! until the page reloads. Listening for `storage` events is the fix, and is deliberately not in
//! this design.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use liquers_core::error::Error;
use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, DataCapabilities, DataFacts};

use crate::store::encoding::{decode_envelope, encode_envelope};
use crate::store::key_guard::check_key;

/// What a `localStorage` entry belonging to this store holds.
///
/// An enum rather than bare `char` constants so the match in [`LocalStorageStore::rescan`] is
/// exhaustive without a default arm: adding a fourth entry kind is then a compile error there,
/// rather than an entry silently ignored by the index scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    /// The data envelope.
    Data,
    /// `Metadata::to_json()`.
    Metadata,
    /// An empty-directory marker.
    Dir,
}

impl EntryKind {
    fn tag(self) -> char {
        match self {
            EntryKind::Data => 'd',
            EntryKind::Metadata => 'm',
            EntryKind::Dir => 'D',
        }
    }

    fn from_tag(tag: char) -> Option<Self> {
        match tag {
            'd' => Some(EntryKind::Data),
            'm' => Some(EntryKind::Metadata),
            'D' => Some(EntryKind::Dir),
            _ => None,
        }
    }
}

/// Directory index and byte accounting, rebuilt at construction from the stored entries.
#[derive(Debug, Default)]
struct LocalState {
    /// parent → child names. Mirrors `AsyncMemoryStore::dir_index` in role.
    dirs: BTreeMap<Key, BTreeSet<String>>,
    /// Keys holding data.
    keys: BTreeSet<Key>,
    /// Directories created by `makedir`, which have no data entry to derive them from.
    explicit_dirs: BTreeSet<Key>,
    /// Total stored length, for quota enforcement.
    used_bytes: u64,
}

/// A store backed by the browser's `localStorage`.
pub struct LocalStorageStore {
    prefix: Key,
    namespace: String,
    /// Byte budget the store enforces itself. `None` is unlimited.
    quota_bytes: Option<u64>,
    state: RefCell<LocalState>,
}

impl LocalStorageStore {
    /// Opens the store, rebuilding its index from whatever is already in `localStorage`.
    pub fn new(prefix: &Key, namespace: &str, quota_bytes: Option<u64>) -> Result<Self, Error> {
        if namespace.is_empty() || namespace.contains('/') {
            return Err(Error::general_error(format!(
                "localStorage namespace must be non-empty and contain no '/', got {namespace:?}"
            )));
        }
        let store = Self {
            prefix: prefix.to_owned(),
            namespace: namespace.to_string(),
            quota_bytes,
            state: RefCell::new(LocalState::default()),
        };
        store.rescan()?;
        Ok(store)
    }

    /// Total bytes currently attributed to this store's namespace.
    pub fn used_bytes(&self) -> u64 {
        self.state.borrow().used_bytes
    }

    fn storage(&self) -> Result<web_sys::Storage, Error> {
        web_sys::window()
            .ok_or_else(|| {
                Error::general_error(
                    "localStorage needs a browser window; none is available here".to_string(),
                )
            })?
            .local_storage()
            .map_err(|_| Error::general_error("localStorage is not accessible".to_string()))?
            .ok_or_else(|| {
                Error::general_error("localStorage is disabled in this context".to_string())
            })
    }

    fn entry_name(&self, kind: EntryKind, key: &Key) -> String {
        format!("{}/{}/{}", self.namespace, kind.tag(), key.encode())
    }

    /// Recovers `(kind, key)` from an entry name belonging to this store's namespace.
    ///
    /// `None` for anything else in `localStorage` — the page owns that storage too, and this store
    /// must not claim entries it did not write.
    fn parse_entry_name(&self, name: &str) -> Option<(EntryKind, Key)> {
        let rest = name.strip_prefix(&format!("{}/", self.namespace))?;
        let (kind_field, key_text) = rest.split_once('/')?;
        let mut characters = kind_field.chars();
        let tag = characters.next()?;
        if characters.next().is_some() {
            return None;
        }
        let kind = EntryKind::from_tag(tag)?;
        parse_key(key_text).ok().map(|key| (kind, key))
    }

    /// Rebuilds the whole index by enumerating `localStorage`.
    fn rescan(&self) -> Result<(), Error> {
        let storage = self.storage()?;
        let length = storage
            .length()
            .map_err(|_| Error::general_error("could not read localStorage length".to_string()))?;

        let mut fresh = LocalState::default();
        for index in 0..length {
            let Ok(Some(name)) = storage.key(index) else {
                continue;
            };
            let Some((kind, key)) = self.parse_entry_name(&name) else {
                continue;
            };
            if let Ok(Some(value)) = storage.get_item(&name) {
                fresh.used_bytes = fresh.used_bytes.saturating_add(value.len() as u64);
            }
            match kind {
                // A metadata entry implies its data key (STORE_SEMANTICS §8): a key whose metadata
                // was written before, or without, its data is still enumerable. Both entries of an
                // ordinary key index the same key, which the sets absorb.
                EntryKind::Data | EntryKind::Metadata => {
                    fresh.keys.insert(key.clone());
                    index_key(&mut fresh.dirs, &key);
                }
                EntryKind::Dir => {
                    fresh.explicit_dirs.insert(key.clone());
                    fresh.dirs.entry(key.clone()).or_default();
                    index_key(&mut fresh.dirs, &key);
                }
            }
        }
        *self.state.borrow_mut() = fresh;
        Ok(())
    }

    fn entry_len(storage: &web_sys::Storage, name: &str) -> u64 {
        storage
            .get_item(name)
            .ok()
            .flatten()
            .map(|v| v.len() as u64)
            .unwrap_or(0)
    }

    /// Refuses a write whose entries would take the store past its byte budget.
    ///
    /// Takes **all** the entries a single operation will write, not one at a time. A `set` writes
    /// both metadata and data, and checking them separately lets the first succeed and the second
    /// fail — which leaves an orphan metadata entry occupying quota for a key that has no value.
    /// This covers the *configured* budget only. The browser's own limit cannot be checked in
    /// advance and is handled by the rollback in [`Self::put_all`]; between the two, an operation
    /// writes all of its entries or leaves storage as it found it.
    ///
    /// Note that metadata dominates the budget for small values: a `MetadataRecord` serializes to
    /// several hundred bytes regardless of payload size. That is honest accounting — the bytes are
    /// really in `localStorage` — but it means a quota below roughly a kilobyte cannot hold even
    /// one entry.
    fn ensure_budget(
        &self,
        storage: &web_sys::Storage,
        key: &Key,
        entries: &[(String, String)],
    ) -> Result<(), Error> {
        let Some(quota) = self.quota_bytes else {
            return Ok(());
        };
        let used = self.state.borrow().used_bytes;
        let mut projected = used;
        for (name, value) in entries {
            projected = projected
                .saturating_sub(Self::entry_len(storage, name))
                .saturating_add(value.len() as u64);
        }
        if projected > quota {
            return Err(Error::key_write_error(
                key,
                &self.store_name(),
                &format!("quota exceeded: {projected} bytes would exceed the {quota} byte budget"),
            ));
        }
        Ok(())
    }

    /// Writes one entry and updates the byte accounting. The budget is checked by
    /// [`Self::ensure_budget`] before any entry of an operation is written.
    fn write_entry(
        &self,
        storage: &web_sys::Storage,
        key: &Key,
        name: &str,
        value: &str,
    ) -> Result<(), Error> {
        let previous = Self::entry_len(storage, name);

        storage.set_item(name, value).map_err(|e| {
            // A browser may refuse below the configured budget; that is still a write error.
            Error::key_write_error(
                key,
                &self.store_name(),
                &format!("localStorage refused the write ({e:?})"),
            )
        })?;

        let mut state = self.state.borrow_mut();
        state.used_bytes = state
            .used_bytes
            .saturating_sub(previous)
            .saturating_add(value.len() as u64);
        Ok(())
    }

    /// Writes every entry of one operation, or leaves storage as it found it.
    ///
    /// Two failures are possible and they need different handling. The **configured** budget is
    /// checked for all entries together up front, so it refuses before anything is written. The
    /// **browser's own** limit cannot be checked in advance — `setItem` simply throws, and it can
    /// throw on the second entry after the first has been committed. Without a rollback, a `set`
    /// that hit the real quota would leave the new metadata beside the *old* data, and `get`
    /// would return a value paired with metadata describing a different one.
    ///
    /// So each write records what it replaced, and a failure restores those values in reverse.
    /// Restoring cannot itself hit the quota: every restored value is one storage already held,
    /// and a removal only frees space.
    fn put_all(
        &self,
        storage: &web_sys::Storage,
        key: &Key,
        entries: &[(String, String)],
    ) -> Result<(), Error> {
        self.ensure_budget(storage, key, entries)?;

        let mut written: Vec<(&str, Option<String>)> = Vec::with_capacity(entries.len());
        for (name, value) in entries {
            let replaced = storage.get_item(name).ok().flatten();
            match self.write_entry(storage, key, name, value) {
                Ok(()) => written.push((name.as_str(), replaced)),
                Err(e) => {
                    self.roll_back(storage, &written);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Undoes the writes of a failed [`Self::put_all`], newest first.
    ///
    /// The byte accounting is rebuilt by rescanning rather than by unwinding the increments:
    /// this is an error path, so the extra pass is free, and a scan cannot drift from what is
    /// actually stored the way a hand-maintained counter can.
    fn roll_back(&self, storage: &web_sys::Storage, written: &[(&str, Option<String>)]) {
        for (name, replaced) in written.iter().rev() {
            match replaced {
                Some(value) => {
                    let _ = storage.set_item(name, value);
                }
                None => {
                    let _ = storage.remove_item(name);
                }
            }
        }
        let _ = self.rescan();
    }

    fn drop_entry(&self, storage: &web_sys::Storage, name: &str) {
        let previous = storage
            .get_item(name)
            .ok()
            .flatten()
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        let _ = storage.remove_item(name);
        let mut state = self.state.borrow_mut();
        state.used_bytes = state.used_bytes.saturating_sub(previous);
    }

    fn check(&self, key: &Key) -> Result<(), Error> {
        check_key(key, &self.store_name())
    }
}

/// The number of bytes an envelope written by [`encode_envelope`] decodes to, without decoding it.
///
/// `None` for anything that is not a well-formed current envelope; the caller reports that as a
/// read error, as [`decode_envelope`] would.
fn envelope_data_len(envelope: &str) -> Option<u64> {
    let bytes = envelope.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'1' {
        return None;
    }
    let payload = &bytes[2..];
    match crate::store::encoding::ByteEncoding::from_tag(bytes[1])? {
        crate::store::encoding::ByteEncoding::Utf8 => Some(payload.len() as u64),
        crate::store::encoding::ByteEncoding::Base64 => {
            // Standard alphabet with padding: every 4 characters carry 3 bytes, less the padding.
            if payload.len() % 4 != 0 {
                return None;
            }
            let padding = payload.iter().rev().take_while(|b| **b == b'=').count();
            if padding > 2 {
                return None;
            }
            Some((payload.len() / 4 * 3 - padding) as u64)
        }
    }
}

/// A 64-bit FNV-1a hash of a stored entry, as a quoted hexadecimal ETag.
fn envelope_etag(envelope: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in envelope.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("\"{hash:016x}\"")
}

/// Records every ancestor edge of `key` in the directory index.
fn index_key(dirs: &mut BTreeMap<Key, BTreeSet<String>>, key: &Key) {
    for depth in 0..key.len() {
        let parent = key.prefix_of_size(depth).unwrap_or_default();
        if let Some(child) = key.iter().nth(depth) {
            dirs.entry(parent)
                .or_default()
                .insert(child.encode().to_string());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for LocalStorageStore {
    fn store_name(&self) -> String {
        format!("{} localStorage store ({})", self.prefix, self.namespace)
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        let data_name = self.entry_name(EntryKind::Data, key);
        let envelope = storage
            .get_item(&data_name)
            .ok()
            .flatten()
            .ok_or_else(|| Error::key_not_found(key))?;
        let data = decode_envelope(&envelope, key, &self.store_name())?;

        let metadata = match storage
            .get_item(&self.entry_name(EntryKind::Metadata, key))
            .ok()
            .flatten()
        {
            Some(json) => Metadata::from_json(&json).map_err(|e| {
                Error::key_read_error(key, &self.store_name(), &format!("corrupt metadata: {e}"))
            })?,
            // Data without metadata is not corruption: `set_metadata` may simply never have run.
            None => Metadata::MetadataRecord(self.default_metadata(key, false)),
        };
        Ok((data, metadata))
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.check(key)?;
        if self.is_dir(key).await? {
            let mut record = self.default_metadata(key, true);
            record.children = self.listdir_asset_info(key).await?;
            return Ok(Metadata::MetadataRecord(record));
        }
        let storage = self.storage()?;
        match storage
            .get_item(&self.entry_name(EntryKind::Metadata, key))
            .ok()
            .flatten()
        {
            Some(json) => Metadata::from_json(&json).map_err(|e| {
                Error::key_read_error(key, &self.store_name(), &format!("corrupt metadata: {e}"))
            }),
            None => {
                if storage
                    .get_item(&self.entry_name(EntryKind::Data, key))
                    .ok()
                    .flatten()
                    .is_some()
                {
                    Ok(Metadata::MetadataRecord(self.default_metadata(key, false)))
                } else {
                    Err(Error::key_not_found(key))
                }
            }
        }
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;

        let mut finalized = metadata.clone();
        self.finalize_metadata(&mut finalized, key, data, true);
        let json = finalized.to_json().map_err(|e| {
            Error::key_write_error(
                key,
                &self.store_name(),
                &format!("metadata is not serializable: {e}"),
            )
        })?;

        // Both entries are budgeted together, then written metadata-first so that a failure of
        // the second leaves no *readable* value — `get` keys off the data entry.
        self.put_all(
            &storage,
            key,
            &[
                (self.entry_name(EntryKind::Metadata, key), json),
                (self.entry_name(EntryKind::Data, key), encode_envelope(data)),
            ],
        )?;

        let mut state = self.state.borrow_mut();
        state.keys.insert(key.clone());
        index_key(&mut state.dirs, key);
        Ok(())
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        let json = metadata.to_json().map_err(|e| {
            Error::key_write_error(
                key,
                &self.store_name(),
                &format!("metadata is not serializable: {e}"),
            )
        })?;
        self.put_all(
            &storage,
            key,
            &[(self.entry_name(EntryKind::Metadata, key), json)],
        )?;
        // Indexed like `set`: a key holding only metadata is listed by its parent (§8).
        let mut state = self.state.borrow_mut();
        state.keys.insert(key.clone());
        index_key(&mut state.dirs, key);
        Ok(())
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        self.drop_entry(&storage, &self.entry_name(EntryKind::Data, key));
        self.drop_entry(&storage, &self.entry_name(EntryKind::Metadata, key));
        self.state.borrow_mut().keys.remove(key);
        self.rescan()
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        self.check(key)?;
        // Absent means the postcondition already holds (STORE_SEMANTICS §4, §5). The key-shape
        // check above stays first, so a refused key is still an error rather than a no-op.
        if !self.is_dir(key).await? {
            return Ok(());
        }
        let storage = self.storage()?;
        let doomed: Vec<Key> = {
            let state = self.state.borrow();
            state
                .keys
                .iter()
                .filter(|k| k.has_key_prefix(key))
                .cloned()
                .collect()
        };
        for victim in doomed {
            self.drop_entry(&storage, &self.entry_name(EntryKind::Data, &victim));
            self.drop_entry(&storage, &self.entry_name(EntryKind::Metadata, &victim));
        }
        let markers: Vec<Key> = {
            let state = self.state.borrow();
            state
                .explicit_dirs
                .iter()
                .filter(|k| k.has_key_prefix(key))
                .cloned()
                .collect()
        };
        for marker in markers {
            self.drop_entry(&storage, &self.entry_name(EntryKind::Dir, &marker));
        }
        self.rescan()
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        self.put_all(
            &storage,
            key,
            &[(self.entry_name(EntryKind::Dir, key), String::new())],
        )?;
        let mut state = self.state.borrow_mut();
        state.explicit_dirs.insert(key.clone());
        state.dirs.entry(key.clone()).or_default();
        index_key(&mut state.dirs, key);
        Ok(())
    }

    // The index lookups refuse a key the store would refuse to write (STORE_SEMANTICS §7): a store
    // never resolves a relative key, including when merely asked whether it holds one.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.check(key)?;
        let state = self.state.borrow();
        Ok(state.keys.contains(key) || state.dirs.contains_key(key))
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.check(key)?;
        Ok(self.state.borrow().dirs.contains_key(key))
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.check(key)?;
        Ok(self
            .state
            .borrow()
            .dirs
            .get(key)
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default())
    }

    /// Data keys, the directories above them, and the prefix (STORE_SEMANTICS §9).
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let state = self.state.borrow();
        let mut keys: BTreeSet<Key> = state.keys.iter().cloned().collect();
        keys.extend(
            state
                .dirs
                .keys()
                .filter(|dir| dir.has_key_prefix(&self.prefix))
                .cloned(),
        );
        keys.insert(self.prefix.clone());
        Ok(keys.into_iter().collect())
    }

    // --- Data-only access (module documentation, "Data-only access") ------------------------

    /// The `d` entry alone; `KeyNotFound` when there is none. The `m` entry is never read.
    async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.check(key)?;
        let storage = self.storage()?;
        let envelope = storage
            .get_item(&self.entry_name(EntryKind::Data, key))
            .ok()
            .flatten()
            .ok_or_else(|| Error::key_not_found(key))?;
        decode_envelope(&envelope, key, &self.store_name())
    }

    /// Writes the `d` entry alone, inside the configured budget.
    async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        self.put_all(
            &storage,
            key,
            &[(self.entry_name(EntryKind::Data, key), encode_envelope(data))],
        )?;
        let mut state = self.state.borrow_mut();
        state.keys.insert(key.clone());
        index_key(&mut state.dirs, key);
        Ok(())
    }

    /// Removes the `d` entry alone; absent is `Ok(())`. A key that keeps an `m` entry stays
    /// enumerable (STORE_SEMANTICS §8), which the rescan works out from the entries themselves.
    async fn remove_data(&self, key: &Key) -> Result<(), Error> {
        self.check(key)?;
        let storage = self.storage()?;
        let name = self.entry_name(EntryKind::Data, key);
        if storage.get_item(&name).ok().flatten().is_none() {
            return Ok(());
        }
        self.drop_entry(&storage, &name);
        self.rescan()
    }

    /// Copies the `d` entry verbatim (no re-encoding), then removes the source. Atomic as far as
    /// the page can observe: there is no `.await` between the two steps. Neither key's `m` entry
    /// moves.
    async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error> {
        self.check(from)?;
        self.check(to)?;
        let storage = self.storage()?;
        let from_name = self.entry_name(EntryKind::Data, from);
        let envelope = storage
            .get_item(&from_name)
            .ok()
            .flatten()
            .ok_or_else(|| Error::key_not_found(from))?;
        if from == to {
            return Ok(());
        }
        // A refused write (budget or browser quota) leaves both keys as they were.
        self.put_all(
            &storage,
            to,
            &[(self.entry_name(EntryKind::Data, to), envelope)],
        )?;
        self.drop_entry(&storage, &from_name);
        self.rescan()
    }

    /// The raw names under `key`: data objects (records a metadata layer keeps among them), keys
    /// holding only a native `m` entry, and directories. This store reserves no names of its own —
    /// its entry kinds are separated in the entry name, not in the key space.
    async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.listdir(key).await
    }

    /// Size and an ETag from the `d` entry; `Ok(None)` when there is none. No modification time:
    /// the store keeps none.
    async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
        self.check(key)?;
        let storage = self.storage()?;
        let Some(envelope) = storage
            .get_item(&self.entry_name(EntryKind::Data, key))
            .ok()
            .flatten()
        else {
            return Ok(None);
        };
        let size = envelope_data_len(&envelope).ok_or_else(|| {
            Error::key_read_error(
                key,
                &self.store_name(),
                "stored value is not a valid envelope",
            )
        })?;
        Ok(Some(DataFacts {
            size,
            modified: None,
            etag: Some(envelope_etag(&envelope)),
        }))
    }

    /// The `m` entry exactly as stored, never synthesized or repaired; `Ok(None)` when there is
    /// none. This is what `storage: native` reads.
    async fn get_stored_metadata(&self, key: &Key) -> Result<Option<Metadata>, Error> {
        self.check(key)?;
        let storage = self.storage()?;
        match storage
            .get_item(&self.entry_name(EntryKind::Metadata, key))
            .ok()
            .flatten()
        {
            Some(json) => Metadata::from_json(&json).map(Some).map_err(|e| {
                Error::key_read_error(key, &self.store_name(), &format!("corrupt metadata: {e}"))
            }),
            None => Ok(None),
        }
    }

    fn data_capabilities(&self) -> DataCapabilities {
        DataCapabilities {
            data_access: true,
            atomic_write: true,
            rename: true,
            modified_time: false,
            etag: true,
        }
    }

    /// Must be overridden: the trait default is `false`, and `AsyncStoreRouter` consults it.
    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix) && check_key(key, "localStorage store").is_ok()
    }
}
