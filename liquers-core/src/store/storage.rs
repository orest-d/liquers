//! Storage models of the metadata pipeline (`specs/design/store-adapters/`, Phase 2 §3).
//!
//! A [`MetadataStorage`] says where a key's metadata record lives. It reads records through the
//! backend's data-only methods and, for a write, plans the objects to write rather than writing
//! them, so the pipeline's write strategy commits the data and the metadata together. Built:
//! [`NativeStorage`] (the backend's own record, today's behaviour), [`PerEntryStorage`] (one object
//! per key, in one of four layouts), [`GroupedStorage`] (one JSON map per folder) and
//! [`NoStorage`]; [`MemoryCentralStorage`] is a test implementation of central storage.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::command_metadata::{
    ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry, EnumArgument,
};
use crate::error::{Error, ErrorType};
use crate::metadata::{Metadata, MetadataRecord};
use crate::plan::ResolvedParameterValues;
use crate::query::Key;

use super::realm::{
    string_parameter, ComponentContext, ComponentKind, StoreComponent, StoreConfigFunctions,
};
use super::writes::{ObjectWrite, WriteTarget};
use super::{AsyncStore, ReservedNameSet};

/// Where a key's metadata lives.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MetadataStorage: crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync {
    /// The model's name, as a spec spells it.
    fn name(&self) -> String;

    /// The names this model's objects occupy in the backend. Owned: they can come from
    /// configuration (`grouped-index.json`).
    fn reserved_names(&self) -> ReservedNameSet;

    /// The stored record of `key`, or `None` when there is none.
    async fn get(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<Metadata>, Error>;

    /// Every record of a folder in one read, when the model keeps them together; `None` when it
    /// does not (the pipeline then asks for each key).
    async fn get_folder(
        &self,
        _backend: &dyn AsyncStore,
        _folder: &Key,
    ) -> Result<Option<Vec<(String, Metadata)>>, Error> {
        Ok(None)
    }

    /// The objects to write so that `key` has `record` (`None`: so that it has none). A grouped
    /// model reads and patches its group file here, under the group lock the pipeline holds.
    async fn plan(
        &self,
        backend: &dyn AsyncStore,
        key: &Key,
        record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error>;

    /// The names in `folder` that have a record in this model, data or not: what makes a key with
    /// metadata and no data enumerable (`STORE_SEMANTICS.md` §8). Empty for a model whose records
    /// the backend already lists (`native`) or that keeps none (`none`).
    async fn record_names(
        &self,
        _backend: &dyn AsyncStore,
        _folder: &Key,
    ) -> Result<Vec<String>, Error> {
        Ok(Vec::new())
    }

    /// Whether a key's record can be absent while its data is present. `false` for a model whose
    /// record is the backend's own entry (`native`), where deleting the record deletes the data:
    /// such a model cannot leave an inferred record unstored.
    fn records_apart_from_data(&self) -> bool {
        true
    }

    /// The unit of locking and caching a key belongs to: its folder for a grouped model, `None`
    /// when each key stands alone.
    fn group_of(&self, _key: &Key) -> Option<Key> {
        None
    }

    /// The folder of the model's object store that mirrors `folder`, when records live outside
    /// the data tree (the `root` layout, central storage): what makes a folder that holds only
    /// records a folder. `None` when records live beside the data, where the backend sees them.
    fn record_folder(&self, _folder: &Key) -> Option<Key> {
        None
    }

    /// The keys of the objects that hold `key`'s record, in the model's object store; what a write
    /// strategy looks for when it recovers an interrupted write. Empty for a model with no
    /// objects of its own.
    fn object_keys(&self, _key: &Key) -> Vec<Key> {
        Vec::new()
    }

    /// The store this model's objects live in when it is not the backend (central storage).
    fn object_store(&self) -> Option<Arc<dyn AsyncStore>> {
        None
    }
}

/// `record` as it is stored: never marked inferred, since a stored record is not inferred by
/// definition (`MetadataRecord::inferred`). Borrowed when there is nothing to clear.
fn stored_form(record: &MetadataRecord) -> std::borrow::Cow<'_, MetadataRecord> {
    if record.inferred {
        let mut stored = record.clone();
        stored.inferred = false;
        std::borrow::Cow::Owned(stored)
    } else {
        std::borrow::Cow::Borrowed(record)
    }
}

/// A record as JSON, the way the file store writes its sidecars.
pub fn encode_metadata(metadata: &Metadata) -> Result<Vec<u8>, Error> {
    match metadata {
        Metadata::MetadataRecord(record) => serde_json::to_vec_pretty(stored_form(record).as_ref()),
        Metadata::LegacyMetadata(value) => serde_json::to_vec_pretty(value),
    }
    .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
}

/// A record read back: a [`crate::metadata::MetadataRecord`] when it parses as one, otherwise kept
/// as legacy metadata, losslessly. A record read back is never inferred.
pub fn decode_metadata(bytes: &[u8]) -> Result<Metadata, Error> {
    if let Ok(mut record) = serde_json::from_slice::<MetadataRecord>(bytes) {
        record.inferred = false;
        return Ok(Metadata::MetadataRecord(record));
    }
    serde_json::from_slice(bytes)
        .map(Metadata::LegacyMetadata)
        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
}

/// A record as a JSON value, for a group file.
fn metadata_to_value(metadata: &Metadata) -> Result<serde_json::Value, Error> {
    match metadata {
        Metadata::MetadataRecord(record) => serde_json::to_value(stored_form(record).as_ref())
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e)),
        Metadata::LegacyMetadata(value) => Ok(value.clone()),
    }
}

fn metadata_from_value(value: serde_json::Value) -> Metadata {
    match serde_json::from_value::<MetadataRecord>(value.clone()) {
        Ok(mut record) => {
            record.inferred = false;
            Metadata::MetadataRecord(record)
        }
        Err(_) => Metadata::LegacyMetadata(value),
    }
}

/// The bytes of a data object, or `None` when the backend holds none.
async fn read_object(backend: &dyn AsyncStore, key: &Key) -> Result<Option<Vec<u8>>, Error> {
    match backend.get_data(key).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.error_type == ErrorType::KeyNotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The data-only listing of `folder`, empty when it does not exist.
async fn list_objects(backend: &dyn AsyncStore, folder: &Key) -> Result<Vec<String>, Error> {
    match backend.listdir_data(folder).await {
        Ok(names) => Ok(names),
        Err(error) if error.error_type == ErrorType::KeyNotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// The backend's own record, through `get_stored_metadata` and `set_metadata`: today's sidecars,
/// byte for byte.
pub struct NativeStorage;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MetadataStorage for NativeStorage {
    fn name(&self) -> String {
        "native".to_string()
    }

    fn reserved_names(&self) -> ReservedNameSet {
        // The backend reserves its own names.
        ReservedNameSet::default()
    }

    async fn get(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<Metadata>, Error> {
        backend.get_stored_metadata(key).await
    }

    fn records_apart_from_data(&self) -> bool {
        false
    }

    async fn plan(
        &self,
        _backend: &dyn AsyncStore,
        key: &Key,
        record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error> {
        Ok(match record {
            Some(record) => vec![ObjectWrite::put(
                WriteTarget::Native,
                key.clone(),
                encode_metadata(record)?,
            )],
            None => vec![ObjectWrite::delete(WriteTarget::Native, key.clone())],
        })
    }
}

/// Where a per-entry model keeps the record of `docs/a.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `docs/a.md.__metadata__` — today's sidecar.
    Sidecar,
    /// `__metadata__/docs/a.md`.
    Root,
    /// `docs/__metadata__a.md.json`.
    Prefix,
    /// `docs/__metadata__/a.md.json` — the Python predecessor's layout.
    Folder,
}

impl Layout {
    pub fn name(&self) -> &'static str {
        match self {
            Layout::Sidecar => "sidecar",
            Layout::Root => "root",
            Layout::Prefix => "prefix",
            Layout::Folder => "folder",
        }
    }

    pub fn from_name(name: &str) -> Result<Layout, Error> {
        match name {
            "sidecar" => Ok(Layout::Sidecar),
            "root" => Ok(Layout::Root),
            "prefix" => Ok(Layout::Prefix),
            "folder" => Ok(Layout::Folder),
            other => Err(Error::configuration_error(format!(
                "layout '{other}' is not one of sidecar, root, prefix, folder"
            ))),
        }
    }
}

/// The reserved segment of the per-entry layouts.
pub const METADATA_SEGMENT: &str = "__metadata__";

/// One metadata object per key, under a [`Layout`].
pub struct PerEntryStorage {
    pub layout: Layout,
}

impl PerEntryStorage {
    pub fn new(layout: Layout) -> Self {
        PerEntryStorage { layout }
    }

    /// The key of `key`'s metadata object.
    pub fn metadata_key(&self, key: &Key) -> Result<Key, Error> {
        let Some(name) = key.filename().map(|n| n.name.clone()) else {
            return Err(Error::key_not_supported(key, &self.name()));
        };
        let folder = key.parent();
        Ok(match self.layout {
            Layout::Sidecar => folder.join(format!("{name}{}", super::METADATA_SUFFIX)),
            Layout::Root => {
                let mut root = Key::new().join(METADATA_SEGMENT);
                for segment in key.iter() {
                    root = root.join(&segment.name);
                }
                root
            }
            Layout::Prefix => folder.join(format!("{METADATA_SEGMENT}{name}.json")),
            Layout::Folder => folder.join(METADATA_SEGMENT).join(format!("{name}.json")),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MetadataStorage for PerEntryStorage {
    fn name(&self) -> String {
        format!("per_entry-{}", self.layout.name())
    }

    fn reserved_names(&self) -> ReservedNameSet {
        match self.layout {
            Layout::Sidecar => ReservedNameSet {
                suffixes: vec![super::METADATA_SUFFIX.to_string()],
                exact: Vec::new(),
                prefixes: Vec::new(),
            },
            Layout::Root | Layout::Folder => ReservedNameSet {
                suffixes: Vec::new(),
                exact: vec![METADATA_SEGMENT.to_string()],
                prefixes: Vec::new(),
            },
            Layout::Prefix => ReservedNameSet {
                suffixes: Vec::new(),
                exact: Vec::new(),
                prefixes: vec![METADATA_SEGMENT.to_string()],
            },
        }
    }

    async fn get(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<Metadata>, Error> {
        let Some(bytes) = read_object(backend, &self.metadata_key(key)?).await? else {
            return Ok(None);
        };
        decode_metadata(&bytes).map(Some)
    }

    async fn plan(
        &self,
        _backend: &dyn AsyncStore,
        key: &Key,
        record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error> {
        let metadata_key = self.metadata_key(key)?;
        Ok(match record {
            Some(record) => vec![ObjectWrite::put(
                WriteTarget::Metadata,
                metadata_key,
                encode_metadata(record)?,
            )],
            None => vec![ObjectWrite::delete(WriteTarget::Metadata, metadata_key)],
        })
    }

    fn object_keys(&self, key: &Key) -> Vec<Key> {
        self.metadata_key(key).into_iter().collect()
    }

    fn record_folder(&self, folder: &Key) -> Option<Key> {
        match self.layout {
            Layout::Root => {
                let mut root = Key::new().join(METADATA_SEGMENT);
                for segment in folder.iter() {
                    root = root.join(&segment.name);
                }
                Some(root)
            }
            Layout::Sidecar | Layout::Prefix | Layout::Folder => None,
        }
    }

    async fn record_names(
        &self,
        backend: &dyn AsyncStore,
        folder: &Key,
    ) -> Result<Vec<String>, Error> {
        let suffix = super::METADATA_SUFFIX;
        Ok(match self.layout {
            Layout::Sidecar => list_objects(backend, folder)
                .await?
                .into_iter()
                .filter_map(|name| name.strip_suffix(suffix).map(str::to_string))
                .filter(|name| !name.is_empty())
                .collect(),
            Layout::Root => {
                let mut root = Key::new().join(METADATA_SEGMENT);
                for segment in folder.iter() {
                    root = root.join(&segment.name);
                }
                list_objects(backend, &root).await?
            }
            Layout::Prefix => list_objects(backend, folder)
                .await?
                .into_iter()
                .filter_map(|name| {
                    name.strip_prefix(METADATA_SEGMENT)
                        .and_then(|n| n.strip_suffix(".json"))
                        .map(str::to_string)
                })
                .filter(|name| !name.is_empty())
                .collect(),
            Layout::Folder => list_objects(backend, &folder.join(METADATA_SEGMENT))
                .await?
                .into_iter()
                .filter_map(|name| name.strip_suffix(".json").map(str::to_string))
                .filter(|name| !name.is_empty())
                .collect(),
        })
    }
}

/// One JSON map, name → record, per folder.
pub struct GroupedStorage {
    pub file: String,
}

/// The default group file name.
pub const DEFAULT_GROUP_FILE: &str = "__metadata__.json";

impl GroupedStorage {
    pub fn new(file: &str) -> Result<Self, Error> {
        if file.is_empty() || file.contains('/') {
            return Err(Error::configuration_error(format!(
                "group file '{file}' must be a plain file name"
            )));
        }
        Ok(GroupedStorage {
            file: file.to_string(),
        })
    }

    fn group_key(&self, folder: &Key) -> Key {
        folder.join(&self.file)
    }

    /// The group file of `folder`, or an empty map when there is none.
    async fn read_group(
        &self,
        backend: &dyn AsyncStore,
        folder: &Key,
    ) -> Result<Option<BTreeMap<String, serde_json::Value>>, Error> {
        let Some(bytes) = read_object(backend, &self.group_key(folder)).await? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MetadataStorage for GroupedStorage {
    fn name(&self) -> String {
        format!("grouped-{}", self.file)
    }

    fn reserved_names(&self) -> ReservedNameSet {
        ReservedNameSet {
            suffixes: Vec::new(),
            exact: vec![self.file.clone()],
            prefixes: Vec::new(),
        }
    }

    async fn get(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<Metadata>, Error> {
        let Some(name) = key.filename().map(|n| n.name.clone()) else {
            return Ok(None);
        };
        let Some(mut group) = self.read_group(backend, &key.parent()).await? else {
            return Ok(None);
        };
        Ok(group.remove(&name).map(metadata_from_value))
    }

    async fn get_folder(
        &self,
        backend: &dyn AsyncStore,
        folder: &Key,
    ) -> Result<Option<Vec<(String, Metadata)>>, Error> {
        let group = self.read_group(backend, folder).await?.unwrap_or_default();
        Ok(Some(
            group
                .into_iter()
                .map(|(name, value)| (name, metadata_from_value(value)))
                .collect(),
        ))
    }

    async fn plan(
        &self,
        backend: &dyn AsyncStore,
        key: &Key,
        record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error> {
        let Some(name) = key.filename().map(|n| n.name.clone()) else {
            return Err(Error::key_not_supported(key, &self.name()));
        };
        let folder = key.parent();
        let mut group = self.read_group(backend, &folder).await?.unwrap_or_default();
        match record {
            Some(record) => {
                group.insert(name, metadata_to_value(record)?);
            }
            None => {
                if group.remove(&name).is_none() {
                    return Ok(Vec::new());
                }
            }
        }
        let group_key = self.group_key(&folder);
        if group.is_empty() {
            return Ok(vec![ObjectWrite::delete(WriteTarget::Metadata, group_key)]);
        }
        let bytes = serde_json::to_vec_pretty(&group)
            .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
        Ok(vec![ObjectWrite::put(
            WriteTarget::Metadata,
            group_key,
            bytes,
        )])
    }

    fn group_of(&self, key: &Key) -> Option<Key> {
        Some(key.parent())
    }

    async fn record_names(
        &self,
        backend: &dyn AsyncStore,
        folder: &Key,
    ) -> Result<Vec<String>, Error> {
        Ok(self
            .read_group(backend, folder)
            .await?
            .unwrap_or_default()
            .into_keys()
            .collect())
    }

    fn object_keys(&self, key: &Key) -> Vec<Key> {
        vec![self.group_key(&key.parent())]
    }
}

/// Nothing is stored; the handlers synthesize every record.
pub struct NoStorage;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MetadataStorage for NoStorage {
    fn name(&self) -> String {
        "none".to_string()
    }

    fn reserved_names(&self) -> ReservedNameSet {
        ReservedNameSet::default()
    }

    async fn get(&self, _backend: &dyn AsyncStore, _key: &Key) -> Result<Option<Metadata>, Error> {
        Ok(None)
    }

    async fn plan(
        &self,
        _backend: &dyn AsyncStore,
        _key: &Key,
        _record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error> {
        Ok(Vec::new())
    }
}

/// Central storage kept in a store of its own, by key: the test implementation of `central`.
#[cfg(any(test, feature = "store-conformance"))]
pub struct MemoryCentralStorage {
    store: Arc<dyn AsyncStore>,
}

#[cfg(any(test, feature = "store-conformance"))]
impl MemoryCentralStorage {
    pub fn new() -> Self {
        MemoryCentralStorage {
            store: Arc::new(super::AsyncMemoryStore::new(&Key::new())),
        }
    }
}

#[cfg(any(test, feature = "store-conformance"))]
impl Default for MemoryCentralStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "store-conformance"))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MetadataStorage for MemoryCentralStorage {
    fn name(&self) -> String {
        "memory_central".to_string()
    }

    fn reserved_names(&self) -> ReservedNameSet {
        ReservedNameSet::default()
    }

    async fn get(&self, _backend: &dyn AsyncStore, key: &Key) -> Result<Option<Metadata>, Error> {
        let Some(bytes) = read_object(self.store.as_ref(), key).await? else {
            return Ok(None);
        };
        decode_metadata(&bytes).map(Some)
    }

    async fn plan(
        &self,
        _backend: &dyn AsyncStore,
        key: &Key,
        record: Option<&Metadata>,
    ) -> Result<Vec<ObjectWrite>, Error> {
        Ok(match record {
            Some(record) => vec![ObjectWrite::put(
                WriteTarget::Metadata,
                key.clone(),
                encode_metadata(record)?,
            )],
            None => vec![ObjectWrite::delete(WriteTarget::Metadata, key.clone())],
        })
    }

    fn object_keys(&self, key: &Key) -> Vec<Key> {
        vec![key.clone()]
    }

    fn record_folder(&self, folder: &Key) -> Option<Key> {
        Some(folder.clone())
    }

    async fn record_names(
        &self,
        _backend: &dyn AsyncStore,
        folder: &Key,
    ) -> Result<Vec<String>, Error> {
        list_objects(self.store.as_ref(), folder).await
    }

    fn object_store(&self) -> Option<Arc<dyn AsyncStore>> {
        Some(self.store.clone())
    }
}

/// A storage model's metadata in realm `store_config`.
fn storage_entry(name: &str, label: &str, doc: &str) -> CommandMetadata {
    let mut metadata = CommandMetadata::new(name);
    metadata
        .with_namespace("")
        .with_label(label)
        .with_doc(doc)
        .with_hint("store_config", serde_json::json!("storage"));
    metadata
}

fn storage(model: impl MetadataStorage + 'static) -> StoreComponent {
    StoreComponent::Storage(Arc::new(model))
}

fn build_native(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(storage(NativeStorage))
}

fn build_per_entry(
    parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    let layout = Layout::from_name(&string_parameter(parameters, "layout")?)?;
    Ok(storage(PerEntryStorage::new(layout)))
}

fn build_grouped(
    parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(storage(GroupedStorage::new(&string_parameter(
        parameters, "file",
    )?)?))
}

fn build_none(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(storage(NoStorage))
}

/// `central-<mount>`: records kept in another member of the router. The builder resolves the mount
/// through the [`ComponentContext`], which only a router-level section has, so the interface a
/// real implementation needs is in place; the model itself (key mapping into the mount, its
/// reserved names, listing) is not implemented yet (`STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED`).
fn build_central(
    parameters: &ResolvedParameterValues,
    components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    let mount = string_parameter(parameters, "mount")?;
    if !components.has_mounts() {
        return Err(Error::configuration_error(format!(
            "central storage in mount '{mount}' belongs in the router-level metadata section, \
             where the router's mounts are known"
        )));
    }
    let prefix = crate::parse::parse_key(&mount)?;
    if components.mount(&prefix).is_none() {
        return Err(Error::configuration_error(format!(
            "central storage names mount '{mount}', which the router does not have"
        )));
    }
    Err(Error::not_supported(format!(
        "central storage in mount '{mount}' is declared but not implemented yet"
    )))
}

#[cfg(any(test, feature = "store-conformance"))]
fn build_memory_central(
    _parameters: &ResolvedParameterValues,
    _components: &ComponentContext<'_>,
) -> Result<StoreComponent, Error> {
    Ok(storage(MemoryCentralStorage::new()))
}

/// Registers the built-in storage models.
pub fn register_builtin_storage(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    functions.register_component(
        cmr,
        storage_entry(
            "native",
            "Native",
            "The backend's own metadata (today's sidecars), through get_stored_metadata and \
             set_metadata.",
        ),
        ComponentKind::Storage,
        build_native,
    )?;

    let mut per_entry = storage_entry(
        "per_entry",
        "Per entry",
        "One metadata object per key: sidecar {key}.__metadata__, root __metadata__/{key}, \
         prefix {folder}/__metadata__{name}.json, folder {folder}/__metadata__/{name}.json.",
    );
    per_entry.with_argument(
        ArgumentInfo::argument("layout")
            .with_type(ArgumentType::Enum(
                EnumArgument::new("layout")
                    .with_alternative("sidecar")
                    .with_alternative("root")
                    .with_alternative("prefix")
                    .with_alternative("folder"),
            ))
            .with_default("sidecar"),
    );
    functions.register_component(cmr, per_entry, ComponentKind::Storage, build_per_entry)?;

    let mut grouped = storage_entry(
        "grouped",
        "Grouped",
        "One JSON map, name to record, per folder, in the named file.",
    );
    grouped.with_argument(ArgumentInfo::string_argument("file").with_default(DEFAULT_GROUP_FILE));
    functions.register_component(cmr, grouped, ComponentKind::Storage, build_grouped)?;

    functions.register_component(
        cmr,
        storage_entry(
            "none",
            "None",
            "Nothing stored: the handlers synthesize every record.",
        ),
        ComponentKind::Storage,
        build_none,
    )?;

    let mut central = storage_entry(
        "central",
        "Central",
        "Records kept in another mount of the router, by key. Declared; not implemented yet.",
    );
    central.with_argument(ArgumentInfo::string_argument("mount"));
    functions.register_component(cmr, central, ComponentKind::Storage, build_central)?;
    Ok(())
}

/// Registers the test-only storage model `memory_central`.
#[cfg(any(test, feature = "store-conformance"))]
pub fn register_test_storage(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    functions.register_component(
        cmr,
        storage_entry(
            "memory_central",
            "Memory central",
            "Test-only central storage, kept in memory.",
        ),
        ComponentKind::Storage,
        build_memory_central,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::MetadataRecord;
    use crate::parse::parse_key;
    use crate::store::realm::{register_builtin_store_config, register_test_store_config};
    use crate::store::test_support::RecordingStore;
    use crate::store::AsyncMemoryStore;

    fn record(title: &str) -> Metadata {
        let mut record = MetadataRecord::new();
        record.title = title.to_string();
        Metadata::MetadataRecord(record)
    }

    fn title(metadata: &Option<Metadata>) -> Option<String> {
        metadata.as_ref().map(|m| m.title().to_string())
    }

    /// Applies planned objects the way a direct write would.
    async fn apply(backend: &dyn AsyncStore, objects: Vec<ObjectWrite>) -> Result<(), Error> {
        for object in objects {
            match object.bytes {
                Some(bytes) => backend.set_data(&object.key, &bytes).await?,
                None => backend.remove_data(&object.key).await?,
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn per_entry_layouts_write_under_their_template() -> Result<(), Error> {
        let key = parse_key("docs/a.md")?;
        for (layout, expected) in [
            (Layout::Sidecar, "docs/a.md.__metadata__"),
            (Layout::Root, "__metadata__/docs/a.md"),
            (Layout::Prefix, "docs/__metadata__a.md.json"),
            (Layout::Folder, "docs/__metadata__/a.md.json"),
        ] {
            let backend = AsyncMemoryStore::new(&Key::new());
            let model = PerEntryStorage::new(layout);
            let objects = model.plan(&backend, &key, Some(&record("T"))).await?;
            assert_eq!(objects.len(), 1);
            assert_eq!(objects[0].key.encode(), expected, "{layout:?}");
            assert_eq!(objects[0].target, WriteTarget::Metadata);
            apply(&backend, objects).await?;
            assert_eq!(
                title(&model.get(&backend, &key).await?).as_deref(),
                Some("T")
            );
            apply(&backend, model.plan(&backend, &key, None).await?).await?;
            assert!(model.get(&backend, &key).await?.is_none());
        }
        Ok(())
    }

    #[tokio::test]
    async fn per_entry_reserved_names_refused_by_is_supported_and_set() -> Result<(), Error> {
        // The model declares the names its objects occupy; the pipeline refuses keys that use
        // them (its half of the test is in `pipeline.rs`).
        for (layout, reserved, free) in [
            (
                Layout::Sidecar,
                "docs/a.md.__metadata__",
                "docs/__metadata__x",
            ),
            (
                Layout::Root,
                "__metadata__/docs/a.md",
                "docs/a.md.__metadata__",
            ),
            (
                Layout::Prefix,
                "docs/__metadata__a.md.json",
                "docs/a.md.__metadata__",
            ),
            (
                Layout::Folder,
                "docs/__metadata__/a.md.json",
                "docs/a.md.__metadata__",
            ),
        ] {
            let names = PerEntryStorage::new(layout).reserved_names();
            assert!(
                names.is_reserved_key(&parse_key(reserved)?),
                "{layout:?} {reserved}"
            );
            assert!(
                !names.is_reserved_key(&parse_key(free)?),
                "{layout:?} {free}"
            );
            assert!(!names.is_reserved_key(&parse_key("docs/a.md")?));
        }
        Ok(())
    }

    #[tokio::test]
    async fn grouped_change_keeps_sibling_records() -> Result<(), Error> {
        let backend = AsyncMemoryStore::new(&Key::new());
        let model = GroupedStorage::new(DEFAULT_GROUP_FILE)?;
        let a = parse_key("docs/a.md")?;
        let b = parse_key("docs/b.md")?;
        apply(
            &backend,
            model.plan(&backend, &a, Some(&record("A"))).await?,
        )
        .await?;
        apply(
            &backend,
            model.plan(&backend, &b, Some(&record("B"))).await?,
        )
        .await?;
        apply(
            &backend,
            model.plan(&backend, &a, Some(&record("A2"))).await?,
        )
        .await?;
        assert_eq!(
            title(&model.get(&backend, &a).await?).as_deref(),
            Some("A2")
        );
        assert_eq!(title(&model.get(&backend, &b).await?).as_deref(), Some("B"));
        assert_eq!(
            backend.listdir_data(&parse_key("docs")?).await?,
            vec!["__metadata__.json"]
        );
        Ok(())
    }

    #[tokio::test]
    async fn grouped_remove_drops_only_its_record() -> Result<(), Error> {
        let backend = AsyncMemoryStore::new(&Key::new());
        let model = GroupedStorage::new(DEFAULT_GROUP_FILE)?;
        let a = parse_key("docs/a.md")?;
        let b = parse_key("docs/b.md")?;
        apply(
            &backend,
            model.plan(&backend, &a, Some(&record("A"))).await?,
        )
        .await?;
        apply(
            &backend,
            model.plan(&backend, &b, Some(&record("B"))).await?,
        )
        .await?;
        apply(&backend, model.plan(&backend, &a, None).await?).await?;
        assert!(model.get(&backend, &a).await?.is_none());
        assert_eq!(title(&model.get(&backend, &b).await?).as_deref(), Some("B"));
        // The last record gone, the group file goes too.
        apply(&backend, model.plan(&backend, &b, None).await?).await?;
        assert!(backend.listdir_data(&parse_key("docs")?).await?.is_empty());
        // Removing a record that is not there plans nothing.
        assert!(model.plan(&backend, &a, None).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn grouped_listing_reads_the_group_file_once() -> Result<(), Error> {
        let memory: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
        let model = GroupedStorage::new(DEFAULT_GROUP_FILE)?;
        for name in ["a.md", "b.md", "c.md"] {
            let key = parse_key(&format!("docs/{name}"))?;
            apply(
                memory.as_ref(),
                model
                    .plan(memory.as_ref(), &key, Some(&record(name)))
                    .await?,
            )
            .await?;
        }
        let recording = RecordingStore::new(memory);
        let folder = model.get_folder(&recording, &parse_key("docs")?).await?;
        assert_eq!(folder.map(|f| f.len()), Some(3));
        let reads = recording
            .methods()
            .into_iter()
            .filter(|m| *m == "get_data")
            .count();
        assert_eq!(reads, 1);
        Ok(())
    }

    #[tokio::test]
    async fn grouped_reserves_file_name_in_every_folder() -> Result<(), Error> {
        let names = GroupedStorage::new("index.json")?.reserved_names();
        assert!(names.is_reserved_key(&parse_key("index.json")?));
        assert!(names.is_reserved_key(&parse_key("a/b/index.json")?));
        assert!(!names.is_reserved_key(&parse_key("a/index.json.bak")?));
        assert!(GroupedStorage::new("a/b.json").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn native_reads_and_plans_the_backend_record() -> Result<(), Error> {
        let backend = AsyncMemoryStore::new(&Key::new());
        let key = parse_key("a.txt")?;
        backend.set(&key, b"x", &record("N")).await?;
        let model = NativeStorage;
        assert_eq!(
            title(&model.get(&backend, &key).await?).as_deref(),
            Some("N")
        );
        let objects = model.plan(&backend, &key, Some(&record("M"))).await?;
        assert_eq!(objects[0].target, WriteTarget::Native);
        assert_eq!(
            decode_metadata(objects[0].bytes.as_deref().unwrap_or_default())?.title(),
            "M"
        );
        Ok(())
    }

    #[tokio::test]
    async fn memory_central_keeps_records_in_its_own_store() -> Result<(), Error> {
        let backend = AsyncMemoryStore::new(&Key::new());
        let model = MemoryCentralStorage::new();
        let key = parse_key("a.txt")?;
        let store = model
            .object_store()
            .ok_or_else(|| Error::general_error("no store".into()))?;
        apply(
            store.as_ref(),
            model.plan(&backend, &key, Some(&record("C"))).await?,
        )
        .await?;
        assert_eq!(
            title(&model.get(&backend, &key).await?).as_deref(),
            Some("C")
        );
        assert!(backend.listdir_data(&Key::new()).await?.is_empty());
        Ok(())
    }

    fn compile_storage(spec: &str, test_entries: bool) -> Result<StoreComponent, Error> {
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        register_builtin_store_config(&mut cmr, &mut functions)?;
        if test_entries {
            register_test_store_config(&mut cmr, &mut functions)?;
        }
        functions.compile_component(
            spec,
            ComponentKind::Storage,
            &cmr,
            &ComponentContext::none(),
            &mut Vec::new(),
        )
    }

    fn storage_name(component: Result<StoreComponent, Error>) -> Result<String, Error> {
        match component? {
            StoreComponent::Storage(model) => Ok(model.name()),
            StoreComponent::Writes(strategy) => Ok(format!("writes {}", strategy.name())),
        }
    }

    #[test]
    fn storage_specs_compile_to_their_models() -> Result<(), Error> {
        assert_eq!(storage_name(compile_storage("native", false))?, "native");
        assert_eq!(
            storage_name(compile_storage("per_entry", false))?,
            "per_entry-sidecar"
        );
        assert_eq!(
            storage_name(compile_storage("per_entry-folder", false))?,
            "per_entry-folder"
        );
        assert_eq!(
            storage_name(compile_storage("grouped", false))?,
            "grouped-__metadata__.json"
        );
        assert_eq!(
            storage_name(compile_storage("grouped-index.json", false))?,
            "grouped-index.json"
        );
        assert_eq!(storage_name(compile_storage("none", false))?, "none");
        assert!(compile_storage("memory_central", false).is_err());
        assert_eq!(
            storage_name(compile_storage("memory_central", true))?,
            "memory_central"
        );
        assert!(compile_storage("per_entry-diagonal", false).is_err());
        Ok(())
    }

    #[test]
    fn compile_refuses_component_of_the_wrong_kind() {
        let handler = compile_storage("defaults", false);
        assert!(matches!(&handler, Err(e) if e.error_type == ErrorType::ConfigurationError));
        assert!(handler
            .err()
            .map(|e| e.message.clone())
            .unwrap_or_default()
            .contains("handler"));
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        let _ = register_builtin_store_config(&mut cmr, &mut functions);
        let in_a_chain = functions.compile("native", crate::store::realm::Stage::Read, &cmr);
        assert!(matches!(in_a_chain, Err(e) if e.message.contains("storage")));
    }

    #[test]
    fn component_spec_with_two_steps_is_refused() {
        let two = compile_storage("native/none", false);
        assert!(matches!(&two, Err(e) if e.error_type == ErrorType::ConfigurationError));
        assert!(compile_storage("", false).is_err());
    }

    #[test]
    fn registered_storage_model_is_named_like_a_builtin() -> Result<(), Error> {
        let mut cmr = CommandMetadataRegistry::new();
        let mut functions = StoreConfigFunctions::new();
        register_builtin_store_config(&mut cmr, &mut functions)?;
        fn build(
            _p: &ResolvedParameterValues,
            _c: &ComponentContext<'_>,
        ) -> Result<StoreComponent, Error> {
            Ok(StoreComponent::Storage(Arc::new(NoStorage)))
        }
        let mut metadata = CommandMetadata::new("mine");
        metadata.with_namespace("");
        functions.register_component(&mut cmr, metadata, ComponentKind::Storage, build)?;
        assert_eq!(
            storage_name(functions.compile_component(
                "mine",
                ComponentKind::Storage,
                &cmr,
                &ComponentContext::none(),
                &mut Vec::new()
            ))?,
            "none"
        );
        Ok(())
    }
}
