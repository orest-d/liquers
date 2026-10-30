//! [`MetadataPipelineStore`]: metadata handling as a layer above a data backend
//! (`specs/design/store-adapters/`, Phase 2 §6).
//!
//! The backend stores data objects; the pipeline decides what a key's metadata record is. A record
//! is read from the [`MetadataStorage`] model, or synthesized by the `on_default` chain when there
//! is none, and shaped by the `on_read` chain. A write is checked by `on_validate`, shaped by
//! `on_write`, planned by the storage model and committed, data and metadata together, by the
//! [`WriteStrategy`].
//!
//! The pipeline is not a data backend: it reports no data-only access, so a [`super::ReadOnlyStore`]
//! above it forwards every read to it and the configured chains run.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{Error, ErrorType};
use crate::metadata::{AssetInfo, BackendTime, Metadata, MetadataRecord, TimePrecision};
use crate::query::Key;

use super::realm::{CompiledChain, FactsIn, StepOut};
use super::storage::MetadataStorage;
use super::writes::{ObjectWrite, TargetStores, WriteSet, WriteStrategy, WriteTarget};
use super::{AsyncStore, DataFacts, ReservedNameSet, SyncReport};

/// A compiled chain for each stage.
pub struct StageChains {
    pub default: CompiledChain,
    pub validate: CompiledChain,
    pub write: CompiledChain,
    pub read: CompiledChain,
}

impl StageChains {
    /// No steps in any stage.
    pub fn empty() -> Self {
        StageChains {
            default: CompiledChain::empty(),
            validate: CompiledChain::empty(),
            write: CompiledChain::empty(),
            read: CompiledChain::empty(),
        }
    }
}

/// A member's own storage model under central storage: the record copy it keeps as a backup.
pub struct Backup {
    /// The member's prefix; the backup holds the keys under it.
    pub prefix: Key,
    pub storage: Arc<dyn MetadataStorage>,
}

/// What a pipeline is built from.
pub struct PipelineParts {
    pub backend: Arc<dyn AsyncStore>,
    pub storage: Arc<dyn MetadataStorage>,
    pub chains: StageChains,
    pub writes: Arc<dyn WriteStrategy>,
    /// The mount is read-only: never write on read (no recovery, no
    /// backup copy). The caller's writes are refused by the decorator above.
    pub read_only: bool,
    /// Members' backups, under central storage.
    pub backups: Vec<Backup>,
    /// The version-verification policy the mount configured, if any.
    pub version_verification: Option<crate::environment_builder::VersionVerification>,
}

impl PipelineParts {
    /// The parts with today's behaviour: native storage, direct writes, no chains.
    pub fn new(
        backend: Arc<dyn AsyncStore>,
        storage: Arc<dyn MetadataStorage>,
        writes: Arc<dyn WriteStrategy>,
    ) -> Self {
        PipelineParts {
            backend,
            storage,
            chains: StageChains::empty(),
            writes,
            read_only: false,
            backups: Vec::new(),
            version_verification: None,
        }
    }
}

/// A record read, with the facts it was shaped for.
#[derive(Clone)]
struct CacheEntry {
    metadata: Metadata,
    facts: DataFacts,
}

/// The metadata layer above a data backend.
pub struct MetadataPipelineStore {
    backend: Arc<dyn AsyncStore>,
    storage: Arc<dyn MetadataStorage>,
    chains: StageChains,
    writes: Arc<dyn WriteStrategy>,
    read_only: bool,
    backups: Vec<Backup>,
    version_verification: Option<crate::environment_builder::VersionVerification>,
    /// What the storage model and the write strategy reserve.
    reserved: ReservedNameSet,
    /// Key → the record last read, valid while the data facts are unchanged.
    cache: scc::HashMap<Key, CacheEntry>,
    /// Group → its write lock. An entry is evicted when nobody else holds it.
    locks: scc::HashMap<Key, Arc<tokio::sync::Mutex<()>>>,
}

impl MetadataPipelineStore {
    /// Builds a pipeline, refusing a backend the write strategy cannot use and a reserved-name
    /// clash between the parts.
    pub fn new(parts: PipelineParts) -> Result<Self, Error> {
        let capabilities = parts.backend.data_capabilities();
        if !capabilities.data_access {
            return Err(Error::configuration_error(format!(
                "Store '{}' does not offer data-only access, so its metadata cannot be configured",
                parts.backend.store_name()
            )));
        }
        parts.writes.check(&capabilities)?;
        let storage_names = parts.storage.reserved_names();
        let writes_names = parts.writes.reserved_names().to_set();
        let mut reserved = storage_names.union(&writes_names);
        for backup in &parts.backups {
            reserved = reserved.union(&backup.storage.reserved_names());
        }
        Ok(MetadataPipelineStore {
            backend: parts.backend,
            storage: parts.storage,
            chains: parts.chains,
            writes: parts.writes,
            read_only: parts.read_only,
            backups: parts.backups,
            version_verification: parts.version_verification,
            reserved,
            cache: scc::HashMap::new(),
            locks: scc::HashMap::new(),
        })
    }

    /// The backend under the pipeline.
    pub fn backend(&self) -> &Arc<dyn AsyncStore> {
        &self.backend
    }

    /// The names the pipeline refuses.
    pub fn reserved_names(&self) -> &ReservedNameSet {
        &self.reserved
    }

    fn metadata_store(&self) -> Arc<dyn AsyncStore> {
        self.storage
            .object_store()
            .unwrap_or_else(|| self.backend.clone())
    }

    fn backup_store(&self, backup: &Backup) -> Arc<dyn AsyncStore> {
        backup
            .storage
            .object_store()
            .unwrap_or_else(|| self.backend.clone())
    }

    /// The backup that holds `key`, with its index.
    fn backup_for(&self, key: &Key) -> Option<(usize, &Backup)> {
        self.backups
            .iter()
            .enumerate()
            .find(|(_, backup)| key.has_key_prefix(&backup.prefix))
    }

    /// Refuses a relative key and a name the parts reserve.
    fn check_key<'k>(&self, key: &'k Key) -> Result<&'k Key, Error> {
        let key = key.as_absolute()?;
        if self.reserved.is_reserved_key(key) {
            return Err(Error::key_not_supported(key, &self.store_name()));
        }
        Ok(key)
    }

    /// The target stores of a write, resolved.
    fn with_stores<'a>(
        &'a self,
        metadata: &'a Arc<dyn AsyncStore>,
        backups: &'a [Arc<dyn AsyncStore>],
    ) -> TargetStores<'a> {
        TargetStores {
            data: self.backend.as_ref(),
            metadata: metadata.as_ref(),
            backups: backups.iter().map(|b| b.as_ref()).collect(),
        }
    }

    fn backup_stores(&self) -> Vec<Arc<dyn AsyncStore>> {
        self.backups.iter().map(|b| self.backup_store(b)).collect()
    }

    /// The objects an interrupted write about `key` may have left.
    fn recovery_candidates(&self, key: &Key) -> Vec<(WriteTarget, Key)> {
        let mut candidates = vec![
            (WriteTarget::Data, key.clone()),
            (WriteTarget::Native, key.clone()),
        ];
        candidates.extend(
            self.storage
                .object_keys(key)
                .into_iter()
                .map(|k| (WriteTarget::Metadata, k)),
        );
        if let Some((index, backup)) = self.backup_for(key) {
            candidates.extend(
                backup
                    .storage
                    .object_keys(key)
                    .into_iter()
                    .map(|k| (WriteTarget::Backup(index), k)),
            );
        }
        candidates
    }

    /// Settles an interrupted write about `key`; the warning to log, if there was one.
    async fn recover(&self, key: &Key) -> Result<Option<String>, Error> {
        if self.read_only {
            return Ok(None);
        }
        let metadata = self.metadata_store();
        let backups = self.backup_stores();
        let stores = self.with_stores(&metadata, &backups);
        let candidates = self.recovery_candidates(key);
        let recovery = self.writes.recover(&stores, key, &candidates).await?;
        Ok(recovery.map(|r| {
            self.cache_invalidate_sync(key);
            format!(
                "An interrupted write of '{}' was {} on read.",
                r.key,
                match r.outcome {
                    super::RecoveryOutcome::RolledForward => "completed",
                    super::RecoveryOutcome::RolledBack => "rolled back",
                }
            )
        }))
    }

    fn cache_invalidate_sync(&self, key: &Key) {
        let _ = self.cache.remove_sync(key);
    }

    async fn cache_invalidate(&self, key: &Key) {
        let _ = self.cache.remove_async(key).await;
    }

    /// The groups a write of `key` must lock, sorted: the group of every model that holds its
    /// record (the storage and the backup covering it), or `key` itself when none groups it.
    fn lock_groups(&self, key: &Key) -> Vec<Key> {
        let mut groups: Vec<Key> = self
            .storage
            .group_of(key)
            .into_iter()
            .chain(
                self.backup_for(key)
                    .and_then(|(_, backup)| backup.storage.group_of(key)),
            )
            .collect();
        if groups.is_empty() {
            groups.push(key.clone());
        }
        groups.sort();
        groups.dedup();
        groups
    }

    /// The lock of `group`, created when absent.
    async fn lock(&self, group: &Key) -> Arc<tokio::sync::Mutex<()>> {
        self.locks
            .entry_async(group.clone())
            .await
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .get()
            .clone()
    }

    /// Evicts a group's lock when nobody else holds it.
    async fn release(&self, group: &Key, lock: Arc<tokio::sync::Mutex<()>>) {
        drop(lock);
        let _ = self
            .locks
            .remove_if_async(group, |l| Arc::strong_count(l) == 1)
            .await;
    }

    /// A backend folder, or a folder holding only records kept outside the data tree.
    async fn is_folder(&self, key: &Key) -> Result<bool, Error> {
        if self.backend.is_dir(key).await? {
            return Ok(true);
        }
        match self.storage.record_folder(key) {
            Some(folder) => self.metadata_store().is_dir(&folder).await,
            None => Ok(false),
        }
    }

    /// Plans and commits `data` (`Some(None)` deletes it, `None` leaves it) with `record` for
    /// `key`, under the group lock.
    /// Removes every entry under `folder`, records included, leaving the folders themselves.
    async fn remove_entries(&self, folder: &Key) -> Result<(), Error> {
        let mut pending = vec![folder.clone()];
        while let Some(current) = pending.pop() {
            let names = match self.listdir(&current).await {
                Ok(names) => names,
                Err(error) if error.error_type == ErrorType::KeyNotFound => continue,
                Err(error) => return Err(error),
            };
            for name in names {
                let child = current.join(&name);
                if self.is_folder(&child).await? {
                    pending.push(child);
                } else {
                    match self.commit(&child, Some(None), None).await {
                        Ok(()) => {}
                        Err(error) if error.error_type == ErrorType::KeyNotFound => {}
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Ok(())
    }

    async fn commit(
        &self,
        key: &Key,
        data: Option<Option<&[u8]>>,
        record: Option<&Metadata>,
    ) -> Result<(), Error> {
        // Taken in sorted order, so two writes sharing groups cannot deadlock.
        let groups = self.lock_groups(key);
        let mut locks = Vec::with_capacity(groups.len());
        for group in &groups {
            locks.push(self.lock(group).await);
        }
        let result = {
            let mut guards = Vec::with_capacity(locks.len());
            for lock in &locks {
                guards.push(lock.lock().await);
            }
            self.commit_locked(key, data, record).await
        };
        for (group, lock) in groups.iter().zip(locks) {
            self.release(group, lock).await;
        }
        self.cache_invalidate(key).await;
        result
    }

    async fn commit_locked(
        &self,
        key: &Key,
        data: Option<Option<&[u8]>>,
        record: Option<&Metadata>,
    ) -> Result<(), Error> {
        let metadata = self.metadata_store();
        let backups = self.backup_stores();
        let mut objects = Vec::new();
        match data {
            Some(Some(bytes)) => objects.push(ObjectWrite::put(
                WriteTarget::Data,
                key.clone(),
                bytes.to_vec(),
            )),
            Some(None) => objects.push(ObjectWrite::delete(WriteTarget::Data, key.clone())),
            None => {}
        }
        objects.extend(self.storage.plan(metadata.as_ref(), key, record).await?);
        if let Some((index, backup)) = self.backup_for(key) {
            let store = self.backup_store(backup);
            objects.extend(
                backup
                    .storage
                    .plan(store.as_ref(), key, record)
                    .await?
                    .into_iter()
                    .map(|o| ObjectWrite {
                        target: WriteTarget::Backup(index),
                        ..o
                    }),
            );
        }
        // A native delete removes the backend's whole entry, data included: when the data is
        // deleted in the same write, the native object is what deletes it.
        let stores = self.with_stores(&metadata, &backups);
        self.writes
            .commit(
                &stores,
                WriteSet {
                    primary: key.clone(),
                    objects,
                },
            )
            .await
    }

    /// The stored record of `key`, falling back to the member's backup under central storage
    /// (copied to central storage, with a warning).
    async fn stored_record(&self, key: &Key) -> Result<Option<Metadata>, Error> {
        let metadata = self.metadata_store();
        if let Some(stored) = self.storage.get(metadata.as_ref(), key).await? {
            return Ok(Some(stored));
        }
        let Some((_, backup)) = self.backup_for(key) else {
            return Ok(None);
        };
        let store = self.backup_store(backup);
        let Some(mut found) = backup.storage.get(store.as_ref(), key).await? else {
            return Ok(None);
        };
        if !self.read_only {
            let objects = self
                .storage
                .plan(metadata.as_ref(), key, Some(&found))
                .await?;
            let stores = TargetStores::single(metadata.as_ref());
            self.writes
                .commit(
                    &stores,
                    WriteSet {
                        primary: key.clone(),
                        objects,
                    },
                )
                .await?;
        }
        if let Metadata::MetadataRecord(record) = &mut found {
            record.warning("Metadata restored from the member's backup.");
        }
        Ok(Some(found))
    }

    /// The record of a data object: stored or synthesized, shaped by the read chain.
    ///
    /// `stored` is the record when the caller already has it (a grouped listing), `bytes` the
    /// data when the caller read it (`get`).
    async fn read_record(
        &self,
        key: &Key,
        stored: Option<Option<Metadata>>,
        bytes: Option<&[u8]>,
        operation: &str,
        warning: Option<String>,
    ) -> Result<Metadata, Error> {
        let facts = self.backend.data_facts(key).await?;
        if warning.is_none() {
            if let Some(facts) = &facts {
                if let Some(hit) = self
                    .cache
                    .read_async(key, |_, entry| {
                        (entry.facts == *facts).then(|| entry.metadata.clone())
                    })
                    .await
                    .flatten()
                {
                    return Ok(hit);
                }
            }
        }
        let stored = match stored {
            Some(stored) => stored,
            None => self.stored_record(key).await?,
        };
        // Bytes fetched for the on_default chain, reused by on_read.
        let mut fetched_default: Option<Vec<u8>> = None;
        let (mut record, previous) = match stored {
            Some(Metadata::LegacyMetadata(value)) => {
                // No lossless conversion exists: returned untouched, chains bypassed.
                return Ok(Metadata::LegacyMetadata(value));
            }
            Some(Metadata::MetadataRecord(record)) => (record.clone(), Some(record)),
            None => {
                if facts.is_none() {
                    return Err(Error::key_not_found(key));
                }
                let mut record = MetadataRecord::new();
                let probe = FactsIn {
                    key,
                    facts: facts.as_ref(),
                    bytes,
                    previous: None,
                    operation,
                };
                if bytes.is_none() && self.chains.default.needs_bytes(&probe) {
                    fetched_default = Some(self.backend.get_data(key).await?);
                }
                let facts_in = FactsIn {
                    key,
                    facts: facts.as_ref(),
                    bytes: bytes.or(fetched_default.as_deref()),
                    previous: None,
                    operation,
                };
                let mut out = StepOut::default();
                self.chains.default.run(&mut record, &facts_in, &mut out)?;
                (record, None)
            }
        };
        let fetched;
        let mut facts_in = FactsIn {
            key,
            facts: facts.as_ref(),
            bytes: bytes.or(fetched_default.as_deref()),
            previous: previous.as_ref(),
            operation,
        };
        if facts_in.bytes.is_none() && facts.is_some() && self.chains.read.needs_bytes(&facts_in) {
            fetched = self.backend.get_data(key).await?;
            facts_in.bytes = Some(&fetched);
        }
        let mut out = StepOut::default();
        self.chains.read.run(&mut record, &facts_in, &mut out)?;
        // A fact, set after the read chain so no step can change it: a record read from storage is
        // never inferred, whatever its bytes say; one the default chain built always is.
        record.inferred = previous.is_none();
        if let Some(warning) = &warning {
            record.warning(warning);
        }
        let metadata = Metadata::MetadataRecord(record);
        if let (Some(facts), None) = (facts, &warning) {
            let _ = self
                .cache
                .upsert_async(
                    key.clone(),
                    CacheEntry {
                        metadata: metadata.clone(),
                        facts,
                    },
                )
                .await;
        }
        Ok(metadata)
    }

    async fn directory_record(&self, key: &Key) -> Result<Metadata, Error> {
        let mut record = self.backend.default_metadata(key, true);
        record.children = self.listdir_asset_info(key).await?;
        Ok(Metadata::MetadataRecord(record))
    }

    /// Validates and shapes a record for a write; the replacement data, if a step produced one.
    fn prepare_record(
        &self,
        key: &Key,
        record: &mut MetadataRecord,
        facts: Option<&DataFacts>,
        bytes: Option<&[u8]>,
        previous: Option<&MetadataRecord>,
        operation: &str,
    ) -> Result<Option<Vec<u8>>, Error> {
        let facts_in = FactsIn {
            key,
            facts,
            bytes,
            previous,
            operation,
        };
        // The flag is the store's fact, never a caller's request: a record read and sent back
        // is stored as on any mount. Only an `on_write` step (`infer`) may set it again.
        record.inferred = false;
        let mut out = StepOut::default();
        self.chains.validate.run(record, &facts_in, &mut out)?;
        self.chains.write.run(record, &facts_in, &mut out)?;
        Ok(out.data)
    }

    /// The record to store for a prepared `record`: `None` when it is inferred, so that the key
    /// has no stored record (an existing one is deleted in the same write).
    fn record_to_store(&self, key: &Key, record: &MetadataRecord) -> Result<Option<Metadata>, Error> {
        if !record.inferred {
            return Ok(Some(Metadata::MetadataRecord(record.clone())));
        }
        if !self.storage.records_apart_from_data() {
            return Err(Error::not_supported(format!(
                "The record of '{key}' is marked inferred, but storage '{}' keeps a record with \
                 its data and cannot leave it unstored; use a storage model that keeps records \
                 apart (per_entry, grouped, none)",
                self.storage.name()
            )));
        }
        Ok(None)
    }

    /// Sets `created` and `etag` from the backend's facts after a data commit, rewriting the
    /// record when they moved. A backend that reports no modification time (some OpenDAL
    /// services, a JavaScript store without one) gets the time of this write as Liquers saw it,
    /// so `created` is never left unset or stale after new data; change detection does not use it
    /// for such a backend, since there is no backend time to compare.
    async fn record_backend_facts(
        &self,
        key: &Key,
        record: &mut MetadataRecord,
    ) -> Result<(), Error> {
        let Some(facts) = self.backend.data_facts(key).await? else {
            return Ok(());
        };
        let created = Some(
            facts
                .modified
                .clone()
                .unwrap_or_else(|| BackendTime::now(TimePrecision::Millis)),
        );
        let etag = facts.etag.clone().or_else(|| record.etag.clone());
        if created == record.created && etag == record.etag {
            return Ok(());
        }
        record.created = created;
        record.etag = etag;
        let metadata = Metadata::MetadataRecord(record.clone());
        self.commit(key, None, Some(&metadata)).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for MetadataPipelineStore {
    fn store_name(&self) -> String {
        format!(
            "{} with {} metadata",
            self.backend.store_name(),
            self.storage.name()
        )
    }

    fn key_prefix(&self) -> Key {
        self.backend.key_prefix()
    }

    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.backend.default_metadata(key, is_dir)
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let key = self.check_key(key)?;
        let warning = self.recover(key).await?;
        let data = self.backend.get_data(key).await?;
        let metadata = self
            .read_record(key, None, Some(&data), "get", warning)
            .await?;
        Ok((data, metadata))
    }

    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let key = self.check_key(key)?;
        self.recover(key).await?;
        self.backend.get_data(key).await
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        let key = self.check_key(key)?;
        let warning = self.recover(key).await?;
        if self.backend.is_dir(key).await? {
            return self.directory_record(key).await;
        }
        self.read_record(key, None, None, "get_metadata", warning)
            .await
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let key = self.check_key(key)?;
        let mut record = match metadata {
            Metadata::LegacyMetadata(_) => {
                return self.commit(key, Some(Some(data)), Some(metadata)).await;
            }
            Metadata::MetadataRecord(record) => record.clone(),
        };
        let previous = match self
            .storage
            .get(self.metadata_store().as_ref(), key)
            .await?
        {
            Some(Metadata::MetadataRecord(previous)) => Some(previous),
            Some(Metadata::LegacyMetadata(_)) | None => None,
        };
        let replaced =
            self.prepare_record(key, &mut record, None, Some(data), previous.as_ref(), "set")?;
        let data = replaced.as_deref().unwrap_or(data);
        let stored = self.record_to_store(key, &record)?;
        self.commit(key, Some(Some(data)), stored.as_ref()).await?;
        if stored.is_none() {
            return Ok(());
        }
        self.record_backend_facts(key, &mut record).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        let key = self.check_key(key)?;
        let mut record = match metadata {
            Metadata::LegacyMetadata(_) => return self.commit(key, None, Some(metadata)).await,
            Metadata::MetadataRecord(record) => record.clone(),
        };
        let facts = self.backend.data_facts(key).await?;
        let previous = match self
            .storage
            .get(self.metadata_store().as_ref(), key)
            .await?
        {
            Some(Metadata::MetadataRecord(previous)) => Some(previous),
            Some(Metadata::LegacyMetadata(_)) | None => None,
        };
        let fetched;
        let mut bytes = None;
        {
            let facts_in = FactsIn {
                key,
                facts: facts.as_ref(),
                bytes: None,
                previous: previous.as_ref(),
                operation: "set_metadata",
            };
            if facts.is_some()
                && (self.chains.validate.needs_bytes(&facts_in)
                    || self.chains.write.needs_bytes(&facts_in))
            {
                fetched = self.backend.get_data(key).await?;
                bytes = Some(fetched.as_slice());
            }
        }
        let replaced = self.prepare_record(
            key,
            &mut record,
            facts.as_ref(),
            bytes,
            previous.as_ref(),
            "set_metadata",
        )?;
        let stored = self.record_to_store(key, &record)?;
        match &replaced {
            Some(data) => {
                self.commit(key, Some(Some(data)), stored.as_ref()).await?;
                if stored.is_none() {
                    return Ok(());
                }
                self.record_backend_facts(key, &mut record).await
            }
            None => self.commit(key, None, stored.as_ref()).await,
        }
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        let key = self.check_key(key)?;
        self.commit(key, Some(None), None).await
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        let key = self.check_key(key)?;
        // Records kept outside the folder (the `root` layout, central storage) or of keys with no
        // data go with it: each entry is removed through the write strategy first.
        let result = match self.remove_entries(key).await {
            Ok(()) => self.backend.removedir(key).await,
            Err(error) => Err(error),
        };
        // The folder that mirrored it in the object store, now holding no records.
        let result = match (result, self.storage.record_folder(key)) {
            (Ok(()), Some(folder)) => self.metadata_store().removedir(&folder).await,
            (result, _) => result,
        };
        let mut stale = Vec::new();
        let _ = self
            .cache
            .iter_async(|k, _| {
                if k.has_key_prefix(key) {
                    stale.push(k.clone());
                }
                true
            })
            .await;
        for k in stale {
            self.cache_invalidate(&k).await;
        }
        result
    }

    /// A key with a record and no data is held too (`STORE_SEMANTICS.md` §8).
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let key = self.check_key(key)?;
        if self.backend.contains(key).await? {
            return Ok(true);
        }
        Ok(self
            .storage
            .get(self.metadata_store().as_ref(), key)
            .await?
            .is_some())
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        let key = self.check_key(key)?;
        self.is_folder(key).await
    }

    /// The backend's names, without the ones the parts reserve, and the names that have only a
    /// record (`STORE_SEMANTICS.md` §8).
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let key = self.check_key(key)?;
        let recorded = self
            .storage
            .record_names(self.metadata_store().as_ref(), key)
            .await?;
        let mut names: Vec<String> = match self.backend.listdir(key).await {
            Ok(names) => names,
            // A folder holding only records has no data objects for the backend to list.
            Err(error) if error.error_type == ErrorType::KeyNotFound && !recorded.is_empty() => {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        names.retain(|name| !self.reserved.is_reserved_name(name));
        for name in recorded {
            if !self.reserved.is_reserved_name(&name) && !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }

    /// One read of the group file for a grouped model; one record per entry otherwise.
    async fn listdir_asset_info(&self, key: &Key) -> Result<Vec<AssetInfo>, Error> {
        let key = self.check_key(key)?;
        let folder_records = self
            .storage
            .get_folder(self.metadata_store().as_ref(), key)
            .await?;
        let mut infos = Vec::new();
        for name in self.listdir(key).await? {
            let child = key.join(&name);
            if self.is_folder(&child).await? {
                let mut info = self.backend.default_metadata(&child, true).get_asset_info();
                info.with_key(child.clone());
                info.is_dir = true;
                infos.push(info);
                continue;
            }
            let warning = self.recover(&child).await?;
            let stored = folder_records.as_ref().map(|records| {
                records
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, metadata)| metadata.clone())
            });
            let metadata = match self
                .read_record(&child, stored, None, "listdir", warning)
                .await
            {
                Ok(metadata) => metadata,
                Err(error) if error.error_type == ErrorType::KeyNotFound => continue,
                Err(error) => return Err(error),
            };
            let mut info = metadata
                .get_asset_info()
                .unwrap_or_else(|_| AssetInfo::new());
            info.with_key(child);
            info.is_dir = false;
            infos.push(info);
        }
        super::sort_asset_info(&mut infos);
        Ok(infos)
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        let key = self.check_key(key)?;
        self.backend.makedir(key).await
    }

    fn is_supported(&self, key: &Key) -> bool {
        self.backend.is_supported(key) && !self.reserved.is_reserved_key(key)
    }

    async fn sync(&self) -> Result<SyncReport, Error> {
        if self.read_only {
            return Ok(SyncReport::default());
        }
        let metadata = self.metadata_store();
        let backups = self.backup_stores();
        let stores = self.with_stores(&metadata, &backups);
        let report = self.writes.sync(&stores).await?;
        if !report.repaired.is_empty() {
            self.cache.clear_async().await;
        }
        Ok(report)
    }

    fn version_verification(
        &self,
        _key: &Key,
    ) -> Option<crate::environment_builder::VersionVerification> {
        self.version_verification
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::CommandMetadataRegistry;
    use crate::metadata::{Status, Version};
    use crate::store::storage::encode_metadata;
    use crate::parse::parse_key;
    use crate::store::realm::{
        register_builtin_store_config, register_test_store_config, ComponentContext, ComponentKind,
        Stage, StoreComponent, StoreConfigFunctions,
    };
    use crate::store::test_support::{FailingStore, RecordingStore};
    use crate::store::{AsyncFileStore, AsyncMemoryStore};

    /// A pipeline's configuration, in specs.
    struct Spec<'a> {
        storage: &'a str,
        writes: &'a str,
        on_default: &'a str,
        on_validate: &'a str,
        on_write: &'a str,
        on_read: &'a str,
        read_only: bool,
    }

    impl Default for Spec<'_> {
        fn default() -> Self {
            Spec {
                storage: "per_entry",
                writes: "direct",
                on_default: "defaults/media_type-fill/file_facts",
                on_validate: "",
                on_write: "defaults/media_type-fill/file_facts",
                on_read: "media_type-fill/file_facts",
                read_only: false,
            }
        }
    }

    fn registry() -> Result<(CommandMetadataRegistry, StoreConfigFunctions), Error> {
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        register_builtin_store_config(&mut cmr, &mut functions)?;
        register_test_store_config(&mut cmr, &mut functions)?;
        Ok((cmr, functions))
    }

    fn pipeline(
        backend: Arc<dyn AsyncStore>,
        spec: &Spec<'_>,
    ) -> Result<MetadataPipelineStore, Error> {
        let (cmr, f) = registry()?;
        let storage = match f.compile_component(
            spec.storage,
            ComponentKind::Storage,
            &cmr,
            &ComponentContext::none(),
            &mut Vec::new(),
        )? {
            StoreComponent::Storage(storage) => storage,
            StoreComponent::Writes(_) => return Err(Error::general_error("storage".into())),
        };
        let writes = match f.compile_component(
            spec.writes,
            ComponentKind::Writes,
            &cmr,
            &ComponentContext::none(),
            &mut Vec::new(),
        )? {
            StoreComponent::Writes(writes) => writes,
            StoreComponent::Storage(_) => return Err(Error::general_error("writes".into())),
        };
        let mut parts = PipelineParts::new(backend, storage, writes);
        parts.chains = StageChains {
            default: f.compile(spec.on_default, Stage::Default, &cmr)?,
            validate: f.compile(spec.on_validate, Stage::Validate, &cmr)?,
            write: f.compile(spec.on_write, Stage::Write, &cmr)?,
            read: f.compile(spec.on_read, Stage::Read, &cmr)?,
        };
        parts.read_only = spec.read_only;
        MetadataPipelineStore::new(parts)
    }

    fn memory() -> Arc<dyn AsyncStore> {
        Arc::new(AsyncMemoryStore::new(&Key::new()))
    }

    fn record(title: &str) -> Metadata {
        let mut record = MetadataRecord::new();
        record.title = title.to_string();
        Metadata::MetadataRecord(record)
    }

    fn as_record(metadata: Metadata) -> Result<MetadataRecord, Error> {
        match metadata {
            Metadata::MetadataRecord(record) => Ok(record),
            Metadata::LegacyMetadata(_) => Err(Error::general_error("legacy record".into())),
        }
    }

    /// Every data-object name in the store, recursively.
    async fn objects(store: &dyn AsyncStore) -> Result<Vec<String>, Error> {
        async fn walk(
            store: &dyn AsyncStore,
            folder: Key,
            out: &mut Vec<String>,
        ) -> Result<(), Error> {
            for name in store.listdir_data(&folder).await? {
                let key = folder.join(&name);
                if store.data_facts(&key).await?.is_some() {
                    out.push(key.encode());
                } else {
                    Box::pin(walk(store, key, out)).await?;
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(store, Key::new(), &mut out).await?;
        out.sort();
        Ok(out)
    }

    fn writes_of(recording: &RecordingStore) -> Vec<&'static str> {
        recording
            .methods()
            .into_iter()
            .filter(|m| {
                matches!(
                    *m,
                    "set_data" | "remove_data" | "rename_data" | "set_metadata" | "set" | "remove"
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn per_entry_reserved_names_are_refused_by_the_pipeline() -> Result<(), Error> {
        let store = pipeline(
            memory(),
            &Spec {
                writes: "transactional",
                ..Spec::default()
            },
        )?;
        for name in ["a.md.__metadata__", "a.md.__txn__", "a.md.__staged__"] {
            let key = parse_key(name)?;
            assert!(!store.is_supported(&key), "{name}");
            let refused = store.set(&key, b"x", &record("x")).await;
            assert!(
                matches!(refused, Err(e) if e.error_type == ErrorType::KeyNotSupported),
                "{name}"
            );
        }
        assert!(store.is_supported(&parse_key("a.md")?));
        Ok(())
    }

    #[tokio::test]
    async fn recovery_on_read_and_listing_completes_the_write() -> Result<(), Error> {
        let key = parse_key("docs/a.md")?;
        for (fail_at, rolled_forward) in [(5, true), (3, false)] {
            let raw = memory();
            raw.set_data(&key, b"old").await?;
            let spec = Spec {
                writes: "transactional",
                ..Spec::default()
            };
            let interrupted = pipeline(Arc::new(FailingStore::new(raw.clone(), fail_at)), &spec)?;
            assert!(interrupted.set(&key, b"new", &record("new")).await.is_err());
            assert!(!objects(raw.as_ref())
                .await?
                .iter()
                .all(|o| !o.contains("__")));

            let store = pipeline(raw.clone(), &spec)?;
            let (data, metadata) = store.get(&key).await?;
            let expected: &[u8] = if rolled_forward { b"new" } else { b"old" };
            assert_eq!(data, expected);
            let recovered = as_record(metadata)?;
            assert!(recovered
                .log
                .iter()
                .any(|e| e.message.contains("interrupted write")));
            let leftovers: Vec<String> = objects(raw.as_ref())
                .await?
                .into_iter()
                .filter(|o| o.contains("__staged__") || o.contains("__txn__"))
                .collect();
            assert!(leftovers.is_empty(), "{leftovers:?}");

            // A listing recovers as well.
            let raw = memory();
            raw.set_data(&key, b"old").await?;
            let interrupted = pipeline(Arc::new(FailingStore::new(raw.clone(), fail_at)), &spec)?;
            assert!(interrupted.set(&key, b"new", &record("new")).await.is_err());
            let store = pipeline(raw.clone(), &spec)?;
            let infos = store.listdir_asset_info(&parse_key("docs")?).await?;
            assert_eq!(infos.len(), 1);
            assert_eq!(store.get_bytes(&key).await?, expected);
        }
        Ok(())
    }

    #[tokio::test]
    async fn storage_none_never_writes() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("docs/a.md")?;
        raw.set_data(&key, b"# A").await?;
        let recording = Arc::new(RecordingStore::new(raw.clone()));
        let store = pipeline(
            recording.clone(),
            &Spec {
                storage: "none",
                ..Spec::default()
            },
        )?;
        let _ = store.get(&key).await?;
        let _ = store.get_metadata(&key).await?;
        let _ = store.listdir_asset_info(&parse_key("docs")?).await?;
        assert!(
            writes_of(&recording).is_empty(),
            "{:?}",
            writes_of(&recording)
        );
        store.set(&key, b"# B", &record("B")).await?;
        assert_eq!(objects(raw.as_ref()).await?, vec!["docs/a.md".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn default_records_write_nothing_and_are_stable() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.md")?;
        raw.set_data(&key, b"# A").await?;
        let recording = Arc::new(RecordingStore::new(raw.clone()));
        let store = pipeline(recording.clone(), &Spec::default())?;
        let first = as_record(store.get_metadata(&key).await?)?;
        let second = as_record(store.get_metadata(&key).await?)?;
        assert_eq!(first.updated, second.updated);
        assert_eq!(first.media_type.as_deref(), Some("text/markdown"));
        assert!(writes_of(&recording).is_empty());
        assert_eq!(objects(raw.as_ref()).await?, vec!["a.md".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn write_chain_overwrites_and_fills() -> Result<(), Error> {
        let store = pipeline(
            memory(),
            &Spec {
                on_write: "set-title-T/set-description-D-fill",
                ..Spec::default()
            },
        )?;
        let key = parse_key("a.txt")?;
        let mut supplied = MetadataRecord::new();
        supplied.title = "x".to_string();
        supplied.description = "keep".to_string();
        store
            .set(&key, b"x", &Metadata::MetadataRecord(supplied))
            .await?;
        let stored = as_record(store.get_metadata(&key).await?)?;
        assert_eq!(stored.title, "T");
        assert_eq!(stored.description, "keep");
        Ok(())
    }

    #[tokio::test]
    async fn read_chain_fills_a_file_written_outside() -> Result<(), Error> {
        let root = std::env::temp_dir().join(format!(
            "lq-pipeline-outside-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).map_err(|e| Error::general_error(e.to_string()))?;
        std::fs::write(root.join("notes.md"), b"# Notes")
            .map_err(|e| Error::general_error(e.to_string()))?;
        let backend: Arc<dyn AsyncStore> = Arc::new(AsyncFileStore::new(
            root.to_string_lossy().as_ref(),
            &Key::new(),
        ));
        let store = pipeline(backend, &Spec::default())?;
        let stored = as_record(store.get_metadata(&parse_key("notes.md")?).await?)?;
        assert_eq!(stored.media_type.as_deref(), Some("text/markdown"));
        assert_eq!(stored.file_size, Some(7));
        assert!(stored.created.is_some());
        // Nothing was written next to it.
        let names: Vec<String> = std::fs::read_dir(&root)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(names, vec!["notes.md".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[tokio::test]
    async fn validator_rejects_and_nothing_is_written() -> Result<(), Error> {
        let raw = memory();
        let store = pipeline(
            raw.clone(),
            &Spec {
                on_validate: "require-title",
                ..Spec::default()
            },
        )?;
        let key = parse_key("a.txt")?;
        let refused = store
            .set(&key, b"x", &Metadata::MetadataRecord(MetadataRecord::new()))
            .await;
        assert!(refused
            .err()
            .map(|e| e.message.contains("title"))
            .unwrap_or(false));
        assert!(objects(raw.as_ref()).await?.is_empty());
        store.set(&key, b"x", &record("T")).await?;
        assert_eq!(objects(raw.as_ref()).await?.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn media_type_in_write_chain_corrects_the_field() -> Result<(), Error> {
        let store = pipeline(
            memory(),
            &Spec {
                on_write: "defaults/media_type",
                ..Spec::default()
            },
        )?;
        let key = parse_key("a.md")?;
        let mut supplied = MetadataRecord::new();
        supplied.media_type = Some("text/plain".to_string());
        store
            .set(&key, b"# A", &Metadata::MetadataRecord(supplied))
            .await?;
        let stored = as_record(store.get_metadata(&key).await?)?;
        assert_eq!(stored.media_type.as_deref(), Some("text/markdown"));
        Ok(())
    }

    #[tokio::test]
    async fn created_comes_from_backend_facts_after_commit() -> Result<(), Error> {
        let raw = memory();
        let store = pipeline(raw.clone(), &Spec::default())?;
        let key = parse_key("a.txt")?;
        store.set(&key, b"one", &record("A")).await?;
        let facts = raw
            .data_facts(&key)
            .await?
            .ok_or_else(|| Error::general_error("facts".into()))?;
        let stored = as_record(
            crate::store::storage::PerEntryStorage::new(crate::store::storage::Layout::Sidecar)
                .get(raw.as_ref(), &key)
                .await?
                .ok_or_else(|| Error::general_error("stored".into()))?,
        )?;
        assert_eq!(stored.created, facts.modified);
        assert_eq!(stored.etag, facts.etag);
        // A second write moves it.
        store.set(&key, b"two", &record("A")).await?;
        let moved = as_record(store.get_metadata(&key).await?)?;
        assert_ne!(moved.etag, stored.etag);
        Ok(())
    }

    #[tokio::test]
    async fn listing_rehashes_after_an_outside_edit() -> Result<(), Error> {
        for when in ["always", "on_change"] {
            let raw = memory();
            let on_read = format!("media_type-fill/file_facts/content_hash-{when}");
            let store = pipeline(
                raw.clone(),
                &Spec {
                    on_write: "defaults/file_facts/content_hash",
                    on_read: &on_read,
                    ..Spec::default()
                },
            )?;
            let key = parse_key("docs/a.txt")?;
            store.set(&key, b"one", &record("A")).await?;
            let folder = parse_key("docs")?;
            // The listing shapes the record; the version is read back from it (cached).
            assert_eq!(store.listdir_asset_info(&folder).await?.len(), 1);
            let version = as_record(store.get_metadata(&key).await?)?.version;
            assert_eq!(version, Some(Version::from_bytes(b"one")));
            raw.set_data(&key, b"two!").await?;
            assert_eq!(store.listdir_asset_info(&folder).await?.len(), 1);
            let version = as_record(store.get_metadata(&key).await?)?.version;
            assert_eq!(version, Some(Version::from_bytes(b"two!")), "{when}");
        }
        Ok(())
    }

    /// An on_default step that needs bytes gets them through `get_metadata` too, so the default
    /// record does not depend on which call came first.
    #[tokio::test]
    async fn on_default_gets_bytes_through_get_metadata() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.txt")?;
        raw.set_data(&key, b"abc").await?;
        let store = pipeline(
            raw,
            &Spec {
                on_default: "defaults/content_hash-always",
                on_read: "",
                ..Spec::default()
            },
        )?;
        let record = as_record(store.get_metadata(&key).await?)?;
        assert_eq!(record.version, Some(Version::from_bytes(b"abc")));
        Ok(())
    }

    #[tokio::test]
    async fn listing_reads_no_bytes_for_unchanged_entries() -> Result<(), Error> {
        let raw = memory();
        let spec = Spec {
            on_write: "defaults/file_facts/content_hash",
            on_read: "media_type-fill/file_facts/content_hash",
            ..Spec::default()
        };
        let writer = pipeline(raw.clone(), &spec)?;
        for name in ["a.txt", "b.txt", "c.txt"] {
            writer
                .set(
                    &parse_key(&format!("docs/{name}"))?,
                    name.as_bytes(),
                    &record(name),
                )
                .await?;
        }
        let recording = Arc::new(RecordingStore::new(raw.clone()));
        let reader = pipeline(recording.clone(), &spec)?;
        let infos = reader.listdir_asset_info(&parse_key("docs")?).await?;
        assert_eq!(infos.len(), 3);
        let data_reads: Vec<Key> = recording
            .calls()
            .into_iter()
            .filter(|c| c.method == "get_data")
            .flat_map(|c| c.keys)
            .filter(|k| !k.encode().ends_with("__metadata__"))
            .collect();
        assert!(data_reads.is_empty(), "{data_reads:?}");
        Ok(())
    }

    #[tokio::test]
    async fn grouped_sibling_writes_are_serialized() -> Result<(), Error> {
        let store = Arc::new(pipeline(
            memory(),
            &Spec {
                storage: "grouped",
                ..Spec::default()
            },
        )?);
        let mut tasks = tokio::task::JoinSet::new();
        for i in 0..20 {
            let store = store.clone();
            tasks.spawn(async move {
                let key = parse_key(&format!("docs/f{i}.txt"))?;
                store.set(&key, b"x", &record(&format!("T{i}"))).await
            });
        }
        while let Some(result) = tasks.join_next().await {
            result.map_err(|e| Error::general_error(e.to_string()))??;
        }
        let infos = store.listdir_asset_info(&parse_key("docs")?).await?;
        assert_eq!(infos.len(), 20);
        for i in 0..20 {
            let stored = as_record(
                store
                    .get_metadata(&parse_key(&format!("docs/f{i}.txt"))?)
                    .await?,
            )?;
            assert_eq!(stored.title, format!("T{i}"));
        }
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn grouped_backup_sibling_writes_are_serialized() -> Result<(), Error> {
        let (cmr, f) = registry()?;
        let raw = memory();
        let mut parts = PipelineParts::new(
            raw.clone(),
            Arc::new(crate::store::storage::PerEntryStorage::new(
                crate::store::storage::Layout::Sidecar,
            )),
            match f.compile_component(
                "direct",
                ComponentKind::Writes,
                &cmr,
                &ComponentContext::none(),
                &mut Vec::new(),
            )? {
                StoreComponent::Writes(writes) => writes,
                StoreComponent::Storage(_) => return Err(Error::general_error("writes".into())),
            },
        );
        parts.backups = vec![Backup {
            prefix: Key::new(),
            storage: Arc::new(crate::store::storage::GroupedStorage::new(
                crate::store::storage::DEFAULT_GROUP_FILE,
            )?),
        }];
        let store = Arc::new(MetadataPipelineStore::new(parts)?);
        let mut tasks = tokio::task::JoinSet::new();
        for i in 0..20 {
            let store = store.clone();
            tasks.spawn(async move {
                let key = parse_key(&format!("docs/f{i}.txt"))?;
                store.set(&key, b"x", &record(&format!("T{i}"))).await
            });
        }
        while let Some(result) = tasks.join_next().await {
            result.map_err(|e| Error::general_error(e.to_string()))??;
        }
        let backup =
            crate::store::storage::GroupedStorage::new(crate::store::storage::DEFAULT_GROUP_FILE)?;
        let mut names = backup
            .record_names(raw.as_ref(), &parse_key("docs")?)
            .await?;
        names.sort();
        assert_eq!(names.len(), 20, "{names:?}");
        Ok(())
    }

    #[tokio::test]
    async fn removedir_descends_into_folders_holding_only_records() -> Result<(), Error> {
        for storage in ["per_entry-root", "memory_central"] {
            let raw = memory();
            let store = pipeline(
                raw.clone(),
                &Spec {
                    storage,
                    ..Spec::default()
                },
            )?;
            let nested = parse_key("folder/sub/a.txt")?;
            store.set_metadata(&nested, &record("A")).await?;
            assert!(store.is_dir(&parse_key("folder/sub")?).await?, "{storage}");
            let infos = store.listdir_asset_info(&parse_key("folder")?).await?;
            assert!(infos.iter().all(|i| i.is_dir), "{storage}: {infos:?}");
            store.removedir(&parse_key("folder")?).await?;
            assert!(!store.contains(&nested).await?, "{storage}");
            assert!(!store.is_dir(&parse_key("folder/sub")?).await?, "{storage}");
            assert!(objects(raw.as_ref()).await?.is_empty(), "{storage}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn legacy_record_bypasses_chains() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.txt")?;
        raw.set_data(&key, b"x").await?;
        let legacy = serde_json::json!({"custom": "kept", "title": 3});
        raw.set_data(
            &parse_key("a.txt.__metadata__")?,
            legacy.to_string().as_bytes(),
        )
        .await?;
        let store = pipeline(
            raw.clone(),
            &Spec {
                on_read: "set-title-X",
                ..Spec::default()
            },
        )?;
        match store.get_metadata(&key).await? {
            Metadata::LegacyMetadata(value) => assert_eq!(value, legacy),
            Metadata::MetadataRecord(_) => return Err(Error::general_error("not legacy".into())),
        }
        Ok(())
    }

    /// Under `native` storage a key colliding with the backend's sidecar is refused before any
    /// byte is written (the file store's data paths refuse it), so the colliding key's record
    /// survives.
    #[tokio::test]
    async fn native_storage_refuses_a_key_the_backend_reserves() -> Result<(), Error> {
        let root = std::env::temp_dir().join(format!(
            "lq-pipeline-native-reserved-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).map_err(|e| Error::general_error(e.to_string()))?;
        let backend: Arc<dyn AsyncStore> = Arc::new(AsyncFileStore::new(
            root.to_string_lossy().as_ref(),
            &Key::new(),
        ));
        let store = pipeline(
            backend,
            &Spec {
                storage: "native",
                ..Spec::default()
            },
        )?;
        let foo = parse_key("foo")?;
        store.set(&foo, b"data", &record("x")).await?;
        let before = std::fs::read(root.join("foo.__metadata__"))
            .map_err(|e| Error::general_error(e.to_string()))?;
        let colliding = parse_key("foo.__metadata__")?;
        assert!(!store.is_supported(&colliding));
        let refused = store.set(&colliding, b"garbage", &record("x")).await;
        assert!(matches!(refused, Err(e) if e.error_type == ErrorType::KeyNotSupported));
        let after = std::fs::read(root.join("foo.__metadata__"))
            .map_err(|e| Error::general_error(e.to_string()))?;
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// A backend whose data facts carry no modification time (some OpenDAL services).
    struct NoTimes(Arc<dyn AsyncStore>);

    #[async_trait]
    impl AsyncStore for NoTimes {
        fn store_name(&self) -> String {
            "no-times".to_string()
        }
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            self.0.get(key).await
        }
        async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
            self.0.set_metadata(key, metadata).await
        }
        async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
            self.0.get_data(key).await
        }
        async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error> {
            self.0.set_data(key, data).await
        }
        async fn remove_data(&self, key: &Key) -> Result<(), Error> {
            self.0.remove_data(key).await
        }
        async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
            self.0.listdir_data(key).await
        }
        async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
            Ok(self.0.data_facts(key).await?.map(|facts| DataFacts {
                modified: None,
                ..facts
            }))
        }
        fn data_capabilities(&self) -> super::super::DataCapabilities {
            super::super::DataCapabilities {
                modified_time: false,
                ..self.0.data_capabilities()
            }
        }
    }

    /// Without a backend time, `created` is the time of the write, and a later write moves it.
    #[tokio::test]
    async fn created_falls_back_to_the_write_time() -> Result<(), Error> {
        let store = pipeline(Arc::new(NoTimes(memory())), &Spec::default())?;
        let key = parse_key("a.txt")?;
        store.set(&key, b"one", &record("x")).await?;
        let first = as_record(store.get_metadata(&key).await?)?.created;
        assert!(first.is_some());
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        store.set(&key, b"two", &record("x")).await?;
        let second = as_record(store.get_metadata(&key).await?)?.created;
        assert!(second.is_some());
        assert_ne!(first, second);
        Ok(())
    }

    #[tokio::test]
    async fn read_only_pipeline_never_persists() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.md")?;
        raw.set_data(&key, b"# A").await?;
        let recording = Arc::new(RecordingStore::new(raw.clone()));
        let store = pipeline(
            recording.clone(),
            &Spec {
                read_only: true,
                writes: "transactional",
                ..Spec::default()
            },
        )?;
        let _ = store.get_metadata(&key).await?;
        let _ = store.get(&key).await?;
        let _ = store.listdir_asset_info(&Key::new()).await?;
        let _ = store.sync().await?;
        assert!(
            writes_of(&recording).is_empty(),
            "{:?}",
            writes_of(&recording)
        );
        Ok(())
    }

    #[tokio::test]
    async fn the_pipeline_reports_no_data_access() -> Result<(), Error> {
        let store = pipeline(memory(), &Spec::default())?;
        assert!(!store.data_capabilities().data_access);
        assert!(store.get_data(&parse_key("a")?).await.is_err());
        // A backend without data access is refused at build.
        let refused = pipeline(Arc::new(crate::store::NoAsyncStore), &Spec::default());
        assert!(matches!(refused, Err(e) if e.error_type == ErrorType::ConfigurationError));
        Ok(())
    }

    /// AC-17: what each strategy does to the backend, for set, set_metadata and remove.
    #[tokio::test]
    async fn each_strategy_follows_its_protocol() -> Result<(), Error> {
        let root = std::env::temp_dir().join(format!(
            "lq-pipeline-protocol-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        for writes in ["direct", "atomic", "transactional"] {
            let file: Arc<dyn AsyncStore> = Arc::new(AsyncFileStore::new(
                root.join(writes).to_string_lossy().as_ref(),
                &Key::new(),
            ));
            let recording = Arc::new(RecordingStore::new(file));
            let store = pipeline(
                recording.clone(),
                &Spec {
                    writes,
                    ..Spec::default()
                },
            )?;
            let key = parse_key("a.txt")?;
            for operation in ["set", "set_metadata", "remove"] {
                recording.clear();
                match operation {
                    "set" => store.set(&key, b"x", &record("A")).await?,
                    "set_metadata" => store.set_metadata(&key, &record("B")).await?,
                    _ => store.remove(&key).await?,
                }
                let calls: Vec<String> = recording
                    .calls()
                    .into_iter()
                    .filter(|c| matches!(c.method, "set_data" | "remove_data" | "rename_data"))
                    .map(|c| {
                        let keys: Vec<String> = c.keys.iter().map(|k| k.encode()).collect();
                        format!("{} {}", c.method, keys.join(" "))
                    })
                    .collect();
                let staged = calls.iter().any(|c| c.contains("__staged__"));
                let journal = calls.iter().any(|c| c.contains("__txn__"));
                match writes {
                    "direct" => assert!(!staged && !journal, "{writes} {operation}: {calls:?}"),
                    "atomic" => {
                        assert!(!journal, "{writes} {operation}: {calls:?}");
                        if operation != "remove" {
                            assert!(staged, "{writes} {operation}: {calls:?}");
                            assert!(
                                calls.iter().any(|c| c.starts_with("rename_data")),
                                "{calls:?}"
                            );
                        }
                    }
                    _ => {
                        assert!(journal, "{writes} {operation}: {calls:?}");
                        let first_journal = calls.iter().position(|c| c.contains("__txn__"));
                        let first_target = calls.iter().position(|c| {
                            c.starts_with("rename_data") || c.starts_with("remove_data a.txt")
                        });
                        assert!(
                            first_journal < first_target,
                            "{writes} {operation}: {calls:?}"
                        );
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    // --- `design/storage-none-computed-values/`: inferred records ---

    fn ready_record(title: &str) -> Metadata {
        let mut record = MetadataRecord::new();
        record.title = title.to_string();
        record.status = Status::Ready;
        Metadata::MetadataRecord(record)
    }

    /// AC-1: a record the default chain built is inferred; one read from storage is not, even when
    /// its stored bytes claim otherwise.
    #[tokio::test]
    async fn default_record_is_inferred_and_stored_record_is_not() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.md")?;
        raw.set_data(&key, b"# A").await?;
        let store = pipeline(raw.clone(), &Spec::default())?;
        assert!(store.get_metadata(&key).await?.is_inferred());
        assert!(store.get(&key).await?.1.is_inferred());
        store.set(&key, b"# A", &record("A")).await?;
        assert!(!store.get_metadata(&key).await?.is_inferred());
        let listed = store.listdir_asset_info(&Key::new()).await?;
        assert_eq!(listed.len(), 1);
        // A record stored by hand with the flag set still reads back as stored.
        let other = parse_key("b.md")?;
        raw.set_data(&other, b"# B").await?;
        let mut forged = MetadataRecord::new();
        forged.inferred = true;
        let forged = serde_json::to_vec(&forged)
            .map_err(|e| Error::from_error(crate::error::ErrorType::SerializationError, e))?;
        raw.set_data(&parse_key("b.md.__metadata__")?, &forged).await?;
        assert!(!store.get_metadata(&other).await?.is_inferred());
        Ok(())
    }

    /// AC-2: no storage model keeps the flag.
    #[tokio::test]
    async fn every_storage_model_drops_the_flag() -> Result<(), Error> {
        for storage in ["per_entry", "per_entry-root", "per_entry-prefix", "per_entry-folder", "grouped"] {
            let raw = memory();
            let key = parse_key("docs/a.md")?;
            let store = pipeline(
                raw.clone(),
                &Spec {
                    storage,
                    on_write: "",
                    ..Spec::default()
                },
            )?;
            // `on_write: ""` stores the caller's record as it is, flag cleared on the way in.
            let mut sent = MetadataRecord::new();
            sent.inferred = true;
            store.set(&key, b"# A", &Metadata::MetadataRecord(sent)).await?;
            for name in objects(raw.as_ref()).await? {
                if name == "docs/a.md" {
                    continue;
                }
                let bytes = raw.get_data(&parse_key(&name)?).await?;
                let text = String::from_utf8_lossy(&bytes);
                assert!(!text.contains("inferred"), "{storage}: {name}: {text}");
            }
            assert!(!store.get_metadata(&key).await?.is_inferred(), "{storage}");
        }
        let mut record = MetadataRecord::new();
        record.inferred = true;
        let encoded = encode_metadata(&Metadata::MetadataRecord(record))?;
        assert!(!String::from_utf8_lossy(&encoded).contains("inferred"));
        Ok(())
    }

    /// AC-3: a record read and sent back is stored, as on any mount.
    #[tokio::test]
    async fn incoming_inferred_flag_is_cleared() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.md")?;
        raw.set_data(&key, b"# A").await?;
        let store = pipeline(raw.clone(), &Spec::default())?;
        let mut read = as_record(store.get_metadata(&key).await?)?;
        assert!(read.inferred);
        read.title = "Edited".to_string();
        store
            .set_metadata(&key, &Metadata::MetadataRecord(read))
            .await?;
        assert!(objects(raw.as_ref())
            .await?
            .contains(&"a.md.__metadata__".to_string()));
        let fresh = pipeline(raw.clone(), &Spec::default())?;
        let reread = as_record(fresh.get_metadata(&key).await?)?;
        assert_eq!(reread.title, "Edited");
        assert!(!reread.inferred);
        Ok(())
    }

    /// AC-4, AC-5: `infer-Source` keeps no record for a user value and deletes the record a
    /// computed value had; a computed value keeps its record.
    #[tokio::test]
    async fn inferred_record_is_not_stored_and_deletes_the_old_one() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("docs/index.md")?;
        let spec = Spec {
            storage: "per_entry-root",
            on_write: "defaults/media_type-fill/file_facts/infer-Source",
            ..Spec::default()
        };
        let store = pipeline(raw.clone(), &spec)?;
        store.set(&key, b"# Index", &ready_record("Index")).await?;
        assert!(objects(raw.as_ref())
            .await?
            .contains(&"__metadata__/docs/index.md".to_string()));
        let stored = as_record(store.get_metadata(&key).await?)?;
        assert_eq!(stored.status, Status::Ready);
        assert!(!stored.inferred);
        // The user writes the key: no record is kept, and the computed one is gone.
        store.set(&key, b"# Mine", &record("Mine")).await?;
        assert_eq!(objects(raw.as_ref()).await?, vec!["docs/index.md".to_string()]);
        let read = as_record(store.get_metadata(&key).await?)?;
        assert!(read.inferred);
        assert_eq!(read.status, Status::Source);
        assert_eq!(store.get_bytes(&key).await?, b"# Mine".to_vec());
        // set_metadata of a user value under the same policy keeps nothing either.
        store.set_metadata(&key, &record("Title")).await?;
        assert_eq!(objects(raw.as_ref()).await?, vec!["docs/index.md".to_string()]);
        Ok(())
    }

    /// AC-5: `infer` with no status marks every record, `storage: none` included.
    #[tokio::test]
    async fn infer_without_statuses_marks_all() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.txt")?;
        let store = pipeline(
            raw.clone(),
            &Spec {
                on_write: "defaults/infer",
                ..Spec::default()
            },
        )?;
        store.set(&key, b"x", &ready_record("R")).await?;
        assert_eq!(objects(raw.as_ref()).await?, vec!["a.txt".to_string()]);
        let none = pipeline(
            memory(),
            &Spec {
                storage: "none",
                on_write: "defaults/infer-Ready",
                ..Spec::default()
            },
        )?;
        none.set(&key, b"x", &ready_record("R")).await?;
        assert!(none.get_metadata(&key).await?.is_inferred());
        Ok(())
    }

    /// AC-5: an unknown status, or `infer` outside `on_write`, fails at build.
    #[test]
    fn infer_refuses_unknown_status_and_other_stages() -> Result<(), Error> {
        let bad_status = pipeline(
            memory(),
            &Spec {
                on_write: "infer-Sourcey",
                ..Spec::default()
            },
        );
        assert!(matches!(&bad_status, Err(e) if e.message.contains("Sourcey")));
        let bad_stage = pipeline(
            memory(),
            &Spec {
                on_read: "infer",
                ..Spec::default()
            },
        );
        assert!(bad_stage.is_err());
        Ok(())
    }

    /// A model that keeps the record with the data cannot leave it unstored: the write is refused,
    /// nothing is deleted.
    #[tokio::test]
    async fn native_storage_refuses_an_inferred_record() -> Result<(), Error> {
        let raw = memory();
        let key = parse_key("a.txt")?;
        let store = pipeline(
            raw.clone(),
            &Spec {
                storage: "native",
                on_write: "defaults/infer",
                ..Spec::default()
            },
        )?;
        let refused = store.set(&key, b"x", &record("A")).await;
        assert!(matches!(&refused, Err(e) if e.message.contains("native")), "{refused:?}");
        Ok(())
    }
}
