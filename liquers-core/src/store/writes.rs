//! Write strategies of the metadata pipeline (`specs/design/store-adapters/`, Phase 2 §5).
//!
//! A write is a [`WriteSet`]: the data object and the objects the storage model planned. A
//! [`WriteStrategy`] commits it — in order, atomically per object, or all-or-nothing through a
//! journal — and recovers what an interrupted commit left behind.

use std::sync::Arc;

use async_trait::async_trait;

use crate::command_metadata::{CommandMetadata, CommandMetadataRegistry};
use crate::error::{Error, ErrorType};
use crate::plan::ResolvedParameterValues;
use crate::query::Key;

use super::realm::{ComponentContext, ComponentKind, StoreComponent, StoreConfigFunctions};
use super::storage::decode_metadata;
use super::{AsyncStore, DataCapabilities, Recovery, RecoveryOutcome, ReservedNames, SyncReport};

/// Which store an [`ObjectWrite`] goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteTarget {
    /// The backend's data object.
    Data,
    /// An object of the storage model, in the store the model names (the backend unless the model
    /// keeps its own).
    Metadata,
    /// The backend's own metadata of the key, through `set_metadata` (bytes are the JSON record)
    /// or, for a delete, `remove`.
    Native,
    /// An object of a member's backup storage model, by its index in [`TargetStores::backups`].
    Backup(usize),
}

/// One object to write; `bytes: None` deletes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectWrite {
    pub target: WriteTarget,
    pub key: Key,
    pub bytes: Option<Vec<u8>>,
}

impl ObjectWrite {
    pub fn put(target: WriteTarget, key: Key, bytes: Vec<u8>) -> Self {
        ObjectWrite {
            target,
            key,
            bytes: Some(bytes),
        }
    }

    pub fn delete(target: WriteTarget, key: Key) -> Self {
        ObjectWrite {
            target,
            key,
            bytes: None,
        }
    }
}

/// The stores the targets of a [`WriteSet`] resolve to.
pub struct TargetStores<'a> {
    /// The backend: data objects, native metadata, and the strategy's own staging and journal.
    pub data: &'a dyn AsyncStore,
    /// Where the storage model keeps its objects.
    pub metadata: &'a dyn AsyncStore,
    /// The members' backup stores, under central storage.
    pub backups: Vec<&'a dyn AsyncStore>,
}

impl<'a> TargetStores<'a> {
    /// Every target in one store.
    pub fn single(store: &'a dyn AsyncStore) -> Self {
        TargetStores {
            data: store,
            metadata: store,
            backups: Vec::new(),
        }
    }

    /// The store a target resolves to.
    pub fn store_for(&self, target: WriteTarget) -> Result<&'a dyn AsyncStore, Error> {
        match target {
            WriteTarget::Data | WriteTarget::Native => Ok(self.data),
            WriteTarget::Metadata => Ok(self.metadata),
            WriteTarget::Backup(index) => self.backups.get(index).copied().ok_or_else(|| {
                Error::unexpected_error(format!("No backup store with index {index}"))
            }),
        }
    }
}

/// Everything one store operation writes.
#[derive(Debug, Clone, Default)]
pub struct WriteSet {
    /// The key the operation is about; names the journal.
    pub primary: Key,
    pub objects: Vec<ObjectWrite>,
}

/// The suffix of a staged object.
pub const STAGED_SUFFIX: &str = ".__staged__";
/// The suffix of a write journal.
pub const JOURNAL_SUFFIX: &str = ".__txn__";
/// The extra suffix that keeps a staged native record apart from the staged data object.
const NATIVE_STAGE: &str = ".__native__";

/// The staged key of an object.
pub fn staged_key(target: WriteTarget, key: &Key) -> Key {
    let name = key.filename().map(|n| n.name.clone()).unwrap_or_default();
    let suffix = match target {
        WriteTarget::Native => format!("{NATIVE_STAGE}{STAGED_SUFFIX}"),
        WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_) => {
            STAGED_SUFFIX.to_string()
        }
    };
    key.parent().join(format!("{name}{suffix}"))
}

/// The journal key of a write about `primary`.
pub fn journal_key(primary: &Key) -> Key {
    let name = primary
        .filename()
        .map(|n| n.name.clone())
        .unwrap_or_default();
    primary.parent().join(format!("{name}{JOURNAL_SUFFIX}"))
}

/// How a write set is committed, and how an interrupted commit is settled.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WriteStrategy: crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync {
    /// The strategy's name, as a spec spells it.
    fn name(&self) -> String;

    /// Refuses a backend that cannot carry the strategy, at build (AC-17).
    fn check(&self, capabilities: &DataCapabilities) -> Result<(), Error>;

    /// The names the strategy's staging and journal objects occupy.
    fn reserved_names(&self) -> ReservedNames;

    /// Writes every object of `writes`.
    async fn commit(&self, stores: &TargetStores<'_>, writes: WriteSet) -> Result<(), Error>;

    /// Settles an interrupted write about `primary`, whose objects would be `objects` (the data
    /// object and the storage model's objects of that key). `None` when there was nothing to do.
    async fn recover(
        &self,
        _stores: &TargetStores<'_>,
        _primary: &Key,
        _objects: &[(WriteTarget, Key)],
    ) -> Result<Option<Recovery>, Error> {
        Ok(None)
    }

    /// Settles every interrupted write in the stores.
    async fn sync(&self, _stores: &TargetStores<'_>) -> Result<SyncReport, Error> {
        Ok(SyncReport::default())
    }
}

/// Writes one object in place.
async fn write_in_place(stores: &TargetStores<'_>, object: &ObjectWrite) -> Result<(), Error> {
    let store = stores.store_for(object.target)?;
    match (object.target, &object.bytes) {
        (WriteTarget::Native, Some(bytes)) => {
            let metadata = decode_metadata(bytes)?;
            store.set_metadata(&object.key, &metadata).await
        }
        (WriteTarget::Native, None) => store.remove(&object.key).await,
        (WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_), Some(bytes)) => {
            store.set_data(&object.key, bytes).await
        }
        (WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_), None) => {
            store.remove_data(&object.key).await
        }
    }
}

/// The bytes of a data object, or `None` when there is none.
async fn read_object(store: &dyn AsyncStore, key: &Key) -> Result<Option<Vec<u8>>, Error> {
    match store.get_data(key).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.error_type == ErrorType::KeyNotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Moves a staged object into place: a rename where the store has one, else copy then delete.
/// A native record is read back and written with `set_metadata`.
async fn move_into_place(
    stores: &TargetStores<'_>,
    target: WriteTarget,
    staged: &Key,
    key: &Key,
) -> Result<(), Error> {
    let store = stores.store_for(target)?;
    match target {
        WriteTarget::Native => {
            let Some(bytes) = read_object(store, staged).await? else {
                return Ok(());
            };
            store.set_metadata(key, &decode_metadata(&bytes)?).await?;
            store.remove_data(staged).await
        }
        WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_) => {
            if store.data_capabilities().rename {
                return store.rename_data(staged, key).await;
            }
            let Some(bytes) = read_object(store, staged).await? else {
                return Ok(());
            };
            store.set_data(key, &bytes).await?;
            store.remove_data(staged).await
        }
    }
}

/// Removes the staged objects of `objects` that exist; true when any did.
async fn remove_staged(
    stores: &TargetStores<'_>,
    objects: &[(WriteTarget, Key)],
) -> Result<bool, Error> {
    let mut removed = false;
    for (target, key) in objects {
        let store = stores.store_for(*target)?;
        let staged = staged_key(*target, key);
        if store.data_facts(&staged).await?.is_some() {
            store.remove_data(&staged).await?;
            removed = true;
        }
    }
    Ok(removed)
}

/// Every key under `folder` whose name ends with one of `suffixes`, through the data-only
/// listing.
async fn find_by_suffix(
    store: &dyn AsyncStore,
    folder: &Key,
    suffixes: &[&str],
    found: &mut Vec<Key>,
) -> Result<(), Error> {
    for name in store.listdir_data(folder).await? {
        let key = folder.join(&name);
        if suffixes.iter().any(|suffix| name.ends_with(suffix)) {
            found.push(key);
        } else if store.data_facts(&key).await?.is_none() {
            // Not a data object, so a folder: `is_dir` is an ordinary method and may refuse a
            // name the data-only methods accept (a file store's sidecar).
            Box::pin(find_by_suffix(store, &key, suffixes, found)).await?;
        }
    }
    Ok(())
}

/// Writes every object in place, in order: today's behaviour.
pub struct DirectWrite;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl WriteStrategy for DirectWrite {
    fn name(&self) -> String {
        "direct".to_string()
    }

    fn check(&self, capabilities: &DataCapabilities) -> Result<(), Error> {
        require_data_access(&self.name(), capabilities)
    }

    fn reserved_names(&self) -> ReservedNames {
        ReservedNames::new(&[], &[])
    }

    async fn commit(&self, stores: &TargetStores<'_>, writes: WriteSet) -> Result<(), Error> {
        for object in &writes.objects {
            write_in_place(stores, object).await?;
        }
        Ok(())
    }
}

fn require_data_access(name: &str, capabilities: &DataCapabilities) -> Result<(), Error> {
    if capabilities.data_access {
        Ok(())
    } else {
        Err(Error::configuration_error(format!(
            "Write strategy '{name}' needs a backend with data-only access"
        )))
    }
}

/// Writes each object atomically: in place where the store writes atomically, otherwise staged
/// beside it and renamed over it. A reader sees each object old or new, never torn; the objects of
/// one write are not atomic together.
pub struct AtomicWrite;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl WriteStrategy for AtomicWrite {
    fn name(&self) -> String {
        "atomic".to_string()
    }

    fn check(&self, capabilities: &DataCapabilities) -> Result<(), Error> {
        require_data_access(&self.name(), capabilities)?;
        if capabilities.atomic_write || capabilities.rename {
            Ok(())
        } else {
            Err(Error::configuration_error(
                "Write strategy 'atomic' needs a backend that writes atomically or renames \
                 atomically; this one does neither"
                    .to_string(),
            ))
        }
    }

    fn reserved_names(&self) -> ReservedNames {
        ReservedNames::new(&[STAGED_SUFFIX], &[])
    }

    async fn commit(&self, stores: &TargetStores<'_>, writes: WriteSet) -> Result<(), Error> {
        for object in &writes.objects {
            let store = stores.store_for(object.target)?;
            match (object.target, &object.bytes) {
                (
                    WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_),
                    Some(bytes),
                ) if !store.data_capabilities().atomic_write => {
                    let staged = staged_key(object.target, &object.key);
                    store.set_data(&staged, bytes).await?;
                    store.rename_data(&staged, &object.key).await?;
                }
                (WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_), Some(_))
                | (WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_), None)
                | (WriteTarget::Native, Some(_))
                | (WriteTarget::Native, None) => write_in_place(stores, object).await?,
            }
        }
        Ok(())
    }

    /// A staged object left behind is an interrupted write that never replaced anything: removed.
    async fn recover(
        &self,
        stores: &TargetStores<'_>,
        primary: &Key,
        objects: &[(WriteTarget, Key)],
    ) -> Result<Option<Recovery>, Error> {
        if remove_staged(stores, objects).await? {
            Ok(Some(Recovery {
                key: primary.clone(),
                outcome: RecoveryOutcome::RolledBack,
            }))
        } else {
            Ok(None)
        }
    }

    async fn sync(&self, stores: &TargetStores<'_>) -> Result<SyncReport, Error> {
        let mut report = SyncReport::default();
        for store in distinct_stores(stores) {
            let mut staged = Vec::new();
            find_by_suffix(store, &store.key_prefix(), &[STAGED_SUFFIX], &mut staged).await?;
            for key in staged {
                match store.remove_data(&key).await {
                    Ok(()) => report.repaired.push(Recovery {
                        key,
                        outcome: RecoveryOutcome::RolledBack,
                    }),
                    Err(error) => report.errors.push((key, error)),
                }
            }
        }
        Ok(report)
    }
}

/// The stores of `stores`, each once.
fn distinct_stores<'a>(stores: &TargetStores<'a>) -> Vec<&'a dyn AsyncStore> {
    let mut distinct: Vec<&'a dyn AsyncStore> = Vec::new();
    for store in std::iter::once(stores.data)
        .chain(std::iter::once(stores.metadata))
        .chain(stores.backups.iter().copied())
    {
        if !distinct.iter().any(|s| std::ptr::addr_eq(*s, store)) {
            distinct.push(store);
        }
    }
    distinct
}

/// One entry of a write journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct JournalEntry {
    target: WriteTarget,
    key: Key,
    /// The staged object to move into place; `None` deletes `key`.
    staged: Option<Key>,
}

/// All or nothing: every object staged, then a journal listing them, then the moves, then the
/// journal deleted. Recovery rolls forward when the journal exists and back when it does not.
pub struct JournalWrite;

impl JournalWrite {
    async fn roll_forward(
        &self,
        stores: &TargetStores<'_>,
        journal: &Key,
        entries: &[JournalEntry],
    ) -> Result<(), Error> {
        for entry in entries {
            let store = stores.store_for(entry.target)?;
            match &entry.staged {
                Some(staged) => move_into_place(stores, entry.target, staged, &entry.key).await?,
                None => match entry.target {
                    WriteTarget::Native => store.remove(&entry.key).await?,
                    WriteTarget::Data | WriteTarget::Metadata | WriteTarget::Backup(_) => {
                        store.remove_data(&entry.key).await?
                    }
                },
            }
        }
        stores.data.remove_data(journal).await
    }

    async fn read_journal(
        &self,
        stores: &TargetStores<'_>,
        journal: &Key,
    ) -> Result<Option<Vec<JournalEntry>>, Error> {
        let Some(bytes) = read_object(stores.data, journal).await? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl WriteStrategy for JournalWrite {
    fn name(&self) -> String {
        "transactional".to_string()
    }

    fn check(&self, capabilities: &DataCapabilities) -> Result<(), Error> {
        require_data_access(&self.name(), capabilities)
    }

    fn reserved_names(&self) -> ReservedNames {
        ReservedNames::new(&[STAGED_SUFFIX, JOURNAL_SUFFIX], &[])
    }

    async fn commit(&self, stores: &TargetStores<'_>, writes: WriteSet) -> Result<(), Error> {
        let mut entries = Vec::with_capacity(writes.objects.len());
        for object in &writes.objects {
            let staged = match &object.bytes {
                Some(bytes) => {
                    let staged = staged_key(object.target, &object.key);
                    stores
                        .store_for(object.target)?
                        .set_data(&staged, bytes)
                        .await?;
                    Some(staged)
                }
                None => None,
            };
            entries.push(JournalEntry {
                target: object.target,
                key: object.key.clone(),
                staged,
            });
        }
        let journal = journal_key(&writes.primary);
        let bytes = serde_json::to_vec_pretty(&entries)
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
        stores.data.set_data(&journal, &bytes).await?;
        self.roll_forward(stores, &journal, &entries).await
    }

    async fn recover(
        &self,
        stores: &TargetStores<'_>,
        primary: &Key,
        objects: &[(WriteTarget, Key)],
    ) -> Result<Option<Recovery>, Error> {
        let journal = journal_key(primary);
        if let Some(entries) = self.read_journal(stores, &journal).await? {
            self.roll_forward(stores, &journal, &entries).await?;
            return Ok(Some(Recovery {
                key: primary.clone(),
                outcome: RecoveryOutcome::RolledForward,
            }));
        }
        if remove_staged(stores, objects).await? {
            return Ok(Some(Recovery {
                key: primary.clone(),
                outcome: RecoveryOutcome::RolledBack,
            }));
        }
        Ok(None)
    }

    async fn sync(&self, stores: &TargetStores<'_>) -> Result<SyncReport, Error> {
        let mut report = SyncReport::default();
        let mut journals = Vec::new();
        find_by_suffix(
            stores.data,
            &stores.data.key_prefix(),
            &[JOURNAL_SUFFIX],
            &mut journals,
        )
        .await?;
        for journal in journals {
            let result = match self.read_journal(stores, &journal).await {
                Ok(Some(entries)) => self.roll_forward(stores, &journal, &entries).await,
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => report.repaired.push(Recovery {
                    key: journal,
                    outcome: RecoveryOutcome::RolledForward,
                }),
                Err(error) => report.errors.push((journal, error)),
            }
        }
        // Staged objects no journal names: writes interrupted before they were journalled.
        for store in distinct_stores(stores) {
            let mut staged = Vec::new();
            find_by_suffix(store, &store.key_prefix(), &[STAGED_SUFFIX], &mut staged).await?;
            for key in staged {
                match store.remove_data(&key).await {
                    Ok(()) => report.repaired.push(Recovery {
                        key,
                        outcome: RecoveryOutcome::RolledBack,
                    }),
                    Err(error) => report.errors.push((key, error)),
                }
            }
        }
        Ok(report)
    }
}

/// A write strategy's metadata in realm `store_config`.
fn writes_entry(name: &str, label: &str, doc: &str) -> CommandMetadata {
    let mut metadata = CommandMetadata::new(name);
    metadata
        .with_namespace("")
        .with_label(label)
        .with_doc(doc)
        .with_hint("store_config", serde_json::json!("writes"));
    metadata
}

fn build_direct(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(StoreComponent::Writes(Arc::new(DirectWrite)))
}

fn build_atomic(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(StoreComponent::Writes(Arc::new(AtomicWrite)))
}

fn build_transactional(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(StoreComponent::Writes(Arc::new(JournalWrite)))
}

/// Registers the built-in write strategies.
pub fn register_builtin_writes(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    functions.register_component(
        cmr,
        writes_entry(
            "direct",
            "Direct",
            "Every object written in place, in order: today's behaviour.",
        ),
        ComponentKind::Writes,
        build_direct,
    )?;
    functions.register_component(
        cmr,
        writes_entry(
            "atomic",
            "Atomic",
            "Each object written atomically: in place on a backend that writes atomically, else \
             staged as <key>.__staged__ and renamed. Needs atomic writes or rename.",
        ),
        ComponentKind::Writes,
        build_atomic,
    )?;
    functions.register_component(
        cmr,
        writes_entry(
            "transactional",
            "Transactional",
            "All or nothing: objects staged, a journal <key>.__txn__ written, then the moves. \
             Interrupted writes are rolled forward or back on the next read or sync.",
        ),
        ComponentKind::Writes,
        build_transactional,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Metadata, MetadataRecord};
    use crate::parse::parse_key;
    use crate::store::storage::{
        GroupedStorage, Layout, MemoryCentralStorage, MetadataStorage, PerEntryStorage,
        DEFAULT_GROUP_FILE,
    };
    use crate::store::test_support::FailingStore;
    use crate::store::{AsyncFileStore, AsyncMemoryStore};
    use std::path::PathBuf;

    fn temp_file_store(label: &str) -> (Arc<dyn AsyncStore>, PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("lq-writes-{label}-{nanos}"));
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        (Arc::new(store), root)
    }

    fn record(title: &str) -> Metadata {
        let mut record = MetadataRecord::new();
        record.title = title.to_string();
        Metadata::MetadataRecord(record)
    }

    async fn data_of(store: &dyn AsyncStore, key: &Key) -> Result<Option<Vec<u8>>, Error> {
        read_object(store, key).await
    }

    /// Every name in the store, recursively, that a staging or journal object would carry.
    async fn leftovers(store: &dyn AsyncStore) -> Result<Vec<Key>, Error> {
        let mut found = Vec::new();
        find_by_suffix(
            store,
            &Key::new(),
            &[STAGED_SUFFIX, JOURNAL_SUFFIX],
            &mut found,
        )
        .await?;
        Ok(found)
    }

    #[test]
    fn atomic_refused_without_rename_or_atomic_write() {
        let neither = DataCapabilities {
            data_access: true,
            ..DataCapabilities::default()
        };
        assert!(
            matches!(AtomicWrite.check(&neither), Err(e) if e.error_type == ErrorType::ConfigurationError)
        );
        assert!(AtomicWrite
            .check(&DataCapabilities {
                rename: true,
                ..neither
            })
            .is_ok());
        assert!(AtomicWrite
            .check(&DataCapabilities {
                atomic_write: true,
                ..neither
            })
            .is_ok());
        assert!(DirectWrite.check(&neither).is_ok());
        assert!(JournalWrite.check(&neither).is_ok());
        let none = DataCapabilities::default();
        assert!(DirectWrite.check(&none).is_err());
        assert!(JournalWrite.check(&none).is_err());
    }

    #[tokio::test]
    async fn atomic_failure_at_each_step_leaves_old_or_new() -> Result<(), Error> {
        let key = parse_key("docs/a.txt")?;
        let model = PerEntryStorage::new(Layout::Sidecar);
        let metadata_key = model.metadata_key(&key)?;
        let mut n = 1;
        loop {
            let (raw, root) = temp_file_store("atomic");
            raw.set_data(&key, b"old").await?;
            raw.set_data(&metadata_key, b"{\"old\":1}").await?;
            let failing = FailingStore::new(raw.clone(), n);
            let writes = WriteSet {
                primary: key.clone(),
                objects: vec![
                    ObjectWrite::put(WriteTarget::Data, key.clone(), b"new".to_vec()),
                    ObjectWrite::put(
                        WriteTarget::Metadata,
                        metadata_key.clone(),
                        b"{\"new\":1}".to_vec(),
                    ),
                ],
            };
            let committed = AtomicWrite
                .commit(&TargetStores::single(&failing), writes)
                .await;
            let data = data_of(raw.as_ref(), &key).await?;
            assert!(
                data == Some(b"old".to_vec()) || data == Some(b"new".to_vec()),
                "n={n}"
            );
            let meta = data_of(raw.as_ref(), &metadata_key).await?;
            assert!(
                meta == Some(b"{\"old\":1}".to_vec()) || meta == Some(b"{\"new\":1}".to_vec()),
                "n={n}"
            );
            let objects = [
                (WriteTarget::Data, key.clone()),
                (WriteTarget::Metadata, metadata_key.clone()),
            ];
            AtomicWrite
                .recover(&TargetStores::single(raw.as_ref()), &key, &objects)
                .await?;
            assert!(leftovers(raw.as_ref()).await?.is_empty(), "n={n}");
            let _ = std::fs::remove_dir_all(&root);
            if committed.is_ok() {
                assert_eq!(data, Some(b"new".to_vec()));
                assert!(n > 2, "the staged path makes several calls");
                break;
            }
            n += 1;
        }
        Ok(())
    }

    #[tokio::test]
    async fn atomic_staging_key_is_never_listed() -> Result<(), Error> {
        assert!(AtomicWrite
            .reserved_names()
            .is_reserved_name("a.txt.__staged__"));
        let (raw, root) = temp_file_store("staging");
        let key = parse_key("a.txt")?;
        let writes = WriteSet {
            primary: key.clone(),
            objects: vec![ObjectWrite::put(
                WriteTarget::Data,
                key.clone(),
                b"x".to_vec(),
            )],
        };
        AtomicWrite
            .commit(&TargetStores::single(raw.as_ref()), writes)
            .await?;
        assert_eq!(
            raw.listdir_data(&Key::new()).await?,
            vec!["a.txt".to_string()]
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn transactional_reserved_names_refused() {
        let names = JournalWrite.reserved_names();
        assert!(names.is_reserved_name("a.txt.__staged__"));
        assert!(names.is_reserved_name("a.txt.__txn__"));
        assert!(!names.is_reserved_name("a.txt"));
        assert!(!DirectWrite
            .reserved_names()
            .is_reserved_name("a.txt.__txn__"));
    }

    /// What one key looks like across the data, its record and its sibling or backup.
    type Snapshot = (Option<Vec<u8>>, Option<String>, Option<String>);

    async fn snapshot(
        data: &dyn AsyncStore,
        model: &dyn MetadataStorage,
        metadata_store: &dyn AsyncStore,
        key: &Key,
        other: &Key,
        backup: Option<(&dyn MetadataStorage, &dyn AsyncStore)>,
    ) -> Result<Snapshot, Error> {
        let title = |m: Option<Metadata>| m.map(|m| m.title().to_string());
        let record = title(model.get(metadata_store, key).await?);
        let second = match backup {
            Some((backup_model, backup_store)) => title(backup_model.get(backup_store, key).await?),
            None => title(model.get(metadata_store, other).await?),
        };
        Ok((data_of(data, key).await?, record, second))
    }

    /// Runs a commit interrupted at every data-only call in turn, recovering after each, and
    /// requires the result to be the whole old state or the whole new one.
    async fn interrupted_commits_recover(
        model: Arc<dyn MetadataStorage>,
        with_backup: bool,
        remove: bool,
    ) -> Result<(), Error> {
        let key = parse_key("docs/a.md")?;
        let sibling = parse_key("docs/b.md")?;
        let mut n = 1;
        loop {
            let raw: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
            let backup_model = PerEntryStorage::new(Layout::Folder);
            let backup_store: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
            let model_store: Arc<dyn AsyncStore> =
                model.object_store().unwrap_or_else(|| raw.clone());
            // The old state: data, a record, a sibling record and a backup record.
            let mut setup = vec![ObjectWrite::put(
                WriteTarget::Data,
                key.clone(),
                b"old".to_vec(),
            )];
            setup.extend(
                model
                    .plan(model_store.as_ref(), &key, Some(&record("old")))
                    .await?,
            );
            let setup_stores = TargetStores {
                data: raw.as_ref(),
                metadata: model_store.as_ref(),
                backups: vec![backup_store.as_ref()],
            };
            DirectWrite
                .commit(
                    &setup_stores,
                    WriteSet {
                        primary: key.clone(),
                        objects: setup,
                    },
                )
                .await?;
            let sibling_objects = model
                .plan(model_store.as_ref(), &sibling, Some(&record("sibling")))
                .await?;
            DirectWrite
                .commit(
                    &setup_stores,
                    WriteSet {
                        primary: sibling.clone(),
                        objects: sibling_objects,
                    },
                )
                .await?;
            if with_backup {
                let objects = backup_model
                    .plan(backup_store.as_ref(), &key, Some(&record("old")))
                    .await?
                    .into_iter()
                    .map(|o| ObjectWrite {
                        target: WriteTarget::Backup(0),
                        ..o
                    })
                    .collect();
                DirectWrite
                    .commit(
                        &setup_stores,
                        WriteSet {
                            primary: key.clone(),
                            objects,
                        },
                    )
                    .await?;
            }
            let backup = if with_backup {
                Some((&backup_model as &dyn MetadataStorage, backup_store.as_ref()))
            } else {
                None
            };
            let old = snapshot(
                raw.as_ref(),
                model.as_ref(),
                model_store.as_ref(),
                &key,
                &sibling,
                backup,
            )
            .await?;

            // The write.
            let (data_object, new_record) = if remove {
                (ObjectWrite::delete(WriteTarget::Data, key.clone()), None)
            } else {
                (
                    ObjectWrite::put(WriteTarget::Data, key.clone(), b"new".to_vec()),
                    Some(record("new")),
                )
            };
            let mut objects = vec![data_object];
            objects.extend(
                model
                    .plan(model_store.as_ref(), &key, new_record.as_ref())
                    .await?,
            );
            if with_backup {
                objects.extend(
                    backup_model
                        .plan(backup_store.as_ref(), &key, new_record.as_ref())
                        .await?
                        .into_iter()
                        .map(|o| ObjectWrite {
                            target: WriteTarget::Backup(0),
                            ..o
                        }),
                );
            }
            let failing = FailingStore::new(raw.clone(), n);
            let failing_stores = TargetStores {
                data: &failing,
                metadata: if model.object_store().is_some() {
                    model_store.as_ref()
                } else {
                    &failing
                },
                backups: vec![backup_store.as_ref()],
            };
            let committed = JournalWrite
                .commit(
                    &failing_stores,
                    WriteSet {
                        primary: key.clone(),
                        objects,
                    },
                )
                .await;

            let mut candidates = vec![(WriteTarget::Data, key.clone())];
            candidates.extend(
                model
                    .object_keys(&key)
                    .into_iter()
                    .map(|k| (WriteTarget::Metadata, k)),
            );
            candidates.extend(
                backup_model
                    .object_keys(&key)
                    .into_iter()
                    .map(|k| (WriteTarget::Backup(0), k)),
            );
            JournalWrite
                .recover(&setup_stores, &key, &candidates)
                .await?;

            let after = snapshot(
                raw.as_ref(),
                model.as_ref(),
                model_store.as_ref(),
                &key,
                &sibling,
                backup,
            )
            .await?;
            let new: Snapshot = if remove {
                (
                    None,
                    None,
                    if with_backup {
                        None
                    } else {
                        Some("sibling".to_string())
                    },
                )
            } else {
                (
                    Some(b"new".to_vec()),
                    Some("new".to_string()),
                    Some(if with_backup { "new" } else { "sibling" }.to_string()),
                )
            };
            assert!(
                after == old || after == new,
                "{} n={n}: {after:?}",
                model.name()
            );
            assert!(leftovers(raw.as_ref()).await?.is_empty(), "n={n}");
            assert!(leftovers(model_store.as_ref()).await?.is_empty(), "n={n}");
            if committed.is_ok() {
                assert_eq!(after, new);
                break;
            }
            n += 1;
            assert!(n < 64, "the commit never completed");
        }
        Ok(())
    }

    #[tokio::test]
    async fn transactional_interrupted_at_each_step_recovers_consistently() -> Result<(), Error> {
        for remove in [false, true] {
            interrupted_commits_recover(
                Arc::new(PerEntryStorage::new(Layout::Sidecar)),
                false,
                remove,
            )
            .await?;
            interrupted_commits_recover(
                Arc::new(GroupedStorage::new(DEFAULT_GROUP_FILE)?),
                false,
                remove,
            )
            .await?;
            interrupted_commits_recover(Arc::new(MemoryCentralStorage::new()), true, remove)
                .await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn sync_reports_repairs() -> Result<(), Error> {
        let raw: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
        let a = parse_key("docs/a.txt")?;
        // A write interrupted after its journal: rolled forward.
        let failing = FailingStore::new(raw.clone(), 3);
        let writes = WriteSet {
            primary: a.clone(),
            objects: vec![ObjectWrite::put(
                WriteTarget::Data,
                a.clone(),
                b"new".to_vec(),
            )],
        };
        assert!(JournalWrite
            .commit(&TargetStores::single(&failing), writes)
            .await
            .is_err());
        // A staged object no journal names: rolled back.
        raw.set_data(&parse_key("docs/b.txt.__staged__")?, b"partial")
            .await?;

        let report = JournalWrite
            .sync(&TargetStores::single(raw.as_ref()))
            .await?;
        let outcomes: Vec<RecoveryOutcome> = report.repaired.iter().map(|r| r.outcome).collect();
        assert!(
            outcomes.contains(&RecoveryOutcome::RolledForward),
            "{outcomes:?}"
        );
        assert!(
            outcomes.contains(&RecoveryOutcome::RolledBack),
            "{outcomes:?}"
        );
        assert!(report.errors.is_empty());
        assert_eq!(data_of(raw.as_ref(), &a).await?, Some(b"new".to_vec()));
        assert!(leftovers(raw.as_ref()).await?.is_empty());

        // Atomic: a staged object is removed.
        raw.set_data(&parse_key("c.txt.__staged__")?, b"partial")
            .await?;
        let report = AtomicWrite
            .sync(&TargetStores::single(raw.as_ref()))
            .await?;
        assert_eq!(report.repaired.len(), 1);
        Ok(())
    }
}
