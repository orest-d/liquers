//! The first-class roots: the places a value accessor reads and writes.
//!
//! A root does the I/O of an accessor; [`AccessorHub`] turns it into a full accessor with
//! fingerprints, conflicts, events and projections. Environment-backed roots hold the environment
//! weakly, so an accessor kept in a cached asset does not keep the environment alive.
//!
//! # Main components
//!
//! - [`MemoryRoot`] / [`memory_accessor`]: a variable; share it by cloning the accessor.
//! - [`StoreRoot`] / [`store_accessor`]: data in the store under a key, through a [`StoreView`]
//!   (bytes, metadata or the deserialized value); never evaluates a recipe.
//! - [`KeyRoot`] / [`key_accessor`]: a keyed asset, evaluated when read; a set is an `Override`
//!   with a recipe and a `Source` without one; status, progress and log are relayed.
//! - [`AssetRefRoot`] / [`asset_accessor`]: an asset the holder already has, read-only.
//!
//! # Where it is used
//!
//! - The `acc` commands of `liquers-lib` build these roots from queries.
//! - The `/api/acc` routes of `liquers-axum` wrap a query's own asset with [`asset_accessor`].
//!
//! # Related concepts
//!
//! - [`crate::assets::AssetManager`]: `set_binary` and `set_state` are the writes of the store and
//!   keyed-asset roots, so a live asset of the key is replaced and its dependents expire.
//! - [`crate::assets::AssetNotificationMessage`]: what the asset-backed roots relay.
//!
//! # Examples
//!
//! ```
//! # futures::executor::block_on(async {
//! use liquers_core::accessor::{memory_accessor, ValueAccessor};
//! use liquers_core::metadata::Status;
//! use liquers_core::{query::Query, value::Value};
//! let empty = memory_accessor::<Value>(None, Query::new());
//! assert_eq!(empty.info().await.unwrap().status, Status::None);
//! assert!(empty.get().await.is_err());
//! # });
//! ```
//!
//! - [Tests](https://github.com/orest-d/liquers/blob/main/liquers-core/tests/value_accessor.rs):
//!   every root in an environment: outside writers, expiry, removal, the store views, progress.

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::watch;

use super::{
    AccessorHub, AccessorInfo, AccessorRoot, Capabilities, ExpiryPolicy, RootEvent, ValueAccessor,
};
use crate::assets::{AssetManager, AssetNotificationMessage, AssetRef};
use crate::context::{EnvRef, Environment};
use crate::error::{Error, ErrorType};
use crate::maybe_send::BoxFuture;
use crate::metadata::{AssetInfo, LogEntry, Metadata, MetadataRecord, ProgressEntry, Status, Version};
use crate::query::{Key, Query};
use crate::state::State;
use crate::value::{DefaultValueSerializer, ValueInterface};

/// A variable in memory: a value, or none, and a counter version. Share it by cloning the
/// accessor [`memory_accessor`] returns.
#[derive(Debug)]
pub struct MemoryRoot<V: ValueInterface> {
    value: Mutex<Option<(State<V>, Version)>>,
}

impl<V: ValueInterface> MemoryRoot<V> {
    pub fn new(initial: Option<State<V>>) -> Self {
        MemoryRoot {
            value: Mutex::new(initial.map(|s| (s, Version::new_unique()))),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<V: ValueInterface> AccessorRoot<V> for MemoryRoot<V> {
    async fn read(&self) -> Result<Option<(State<V>, Version)>, Error> {
        self.value
            .lock()
            .map(|v| v.clone())
            .map_err(|_| Error::unexpected_error("memory accessor lock poisoned".into()))
    }
    async fn write(&self, mut state: State<V>) -> Result<(State<V>, Version), Error> {
        state.set_status(Status::Source)?;
        let version = Version::new_unique();
        let mut value = self
            .value
            .lock()
            .map_err(|_| Error::unexpected_error("memory accessor lock poisoned".into()))?;
        *value = Some((state.clone(), version));
        Ok((state, version))
    }
    async fn info(&self) -> Result<AccessorInfo, Error> {
        let present = self.value.lock().map(|v| v.is_some()).unwrap_or(false);
        Ok(AccessorInfo::with_status(if present {
            Status::Source
        } else {
            Status::None
        }))
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: true,
            notify: true,
        }
    }
    fn not_found(&self) -> Error {
        let mut error = Error::not_available();
        error.message = "the memory accessor holds no value".to_string();
        error
    }
}

/// A variable in memory, empty when `initial` is `None` (status `None` until the first set).
/// Clones of the returned `Arc` share it.
pub fn memory_accessor<V: ValueInterface>(
    initial: Option<State<V>>,
    address: Query,
) -> Arc<dyn ValueAccessor<V>> {
    AccessorHub::new(
        Arc::new(MemoryRoot::new(initial)),
        address,
        ExpiryPolicy::Relay,
    )
}

// ---------------------------------------------------------------------------------------------
// Environment-backed roots
// ---------------------------------------------------------------------------------------------

/// Upgrades the weak environment of a root, or reports that it was dropped.
fn upgrade<E: Environment>(env: &Weak<E>) -> Result<EnvRef<E>, Error> {
    env.upgrade().map(EnvRef).ok_or_else(|| {
        Error::general_error(
            "the environment was dropped; this accessor outlived it".to_string(),
        )
    })
}

/// The record of a [`Metadata`], or a conversion error for legacy metadata.
fn record_of(metadata: &Metadata) -> Result<MetadataRecord, Error> {
    match metadata {
        Metadata::MetadataRecord(record) => Ok(record.clone()),
        Metadata::LegacyMetadata(_) => Err(Error::conversion_error("legacy metadata", "metadata record")),
    }
}

/// The log of a [`Metadata`]; legacy metadata has none.
fn log_of(metadata: &Metadata) -> Vec<LogEntry> {
    match metadata {
        Metadata::MetadataRecord(record) => record.log.clone(),
        Metadata::LegacyMetadata(_) => Vec::new(),
    }
}

/// Which side of a store entry a [`store_accessor`] reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreView {
    /// The stored bytes, as a bytes value.
    Binary,
    /// The stored metadata record, as a metadata value.
    Metadata,
    /// The stored bytes deserialized with the metadata's type identifier and data format.
    Value,
}

impl StoreView {
    /// Parses `binary`, `metadata` or `value`.
    pub fn parse(view: &str) -> Result<StoreView, Error> {
        match view {
            "binary" => Ok(StoreView::Binary),
            "metadata" => Ok(StoreView::Metadata),
            "value" => Ok(StoreView::Value),
            other => Err(Error::general_error(format!(
                "unknown store view '{other}': expected binary, metadata or value"
            ))),
        }
    }
}

/// Data in the store under one key. It never evaluates anything: a key only a recipe could make
/// does not exist for it. Writes go through [`AssetManager::set_binary`], so a live asset of the
/// key is replaced and its dependents expire as for any write of the key.
pub struct StoreRoot<E: Environment> {
    env: Weak<E>,
    key: Key,
    view: StoreView,
}

impl<E: Environment> Debug for StoreRoot<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreRoot")
            .field("key", &self.key.encode())
            .field("view", &self.view)
            .finish()
    }
}

impl<E: Environment> StoreRoot<E> {
    pub fn new(envref: &EnvRef<E>, key: Key, view: StoreView) -> Self {
        StoreRoot {
            env: Arc::downgrade(&envref.0),
            key,
            view,
        }
    }

    /// The data format of the value view: the metadata's, else the key's extension.
    fn data_format(&self, record: Option<&MetadataRecord>, value: Option<&E::Value>) -> String {
        record
            .and_then(|r| r.data_format.clone())
            .or_else(|| self.key.extension())
            .or_else(|| value.map(|v| v.default_data_format().to_string()))
            .unwrap_or_else(|| "bin".to_string())
    }

    async fn stored_record(&self, envref: &EnvRef<E>) -> Result<Option<MetadataRecord>, Error> {
        let store = envref.get_async_store();
        if !store.contains(&self.key).await? {
            return Ok(None);
        }
        Ok(Some(record_of(&store.get_metadata(&self.key).await?)?))
    }

    /// A record for a key that does not exist yet, describing `value`.
    fn new_record(&self, value: &E::Value, data_format: String) -> MetadataRecord {
        let mut record = MetadataRecord::new();
        record.key = Some(self.key.clone());
        record.type_identifier = value.identifier().to_string();
        record.type_name = value.type_name().to_string();
        record.data_format = Some(data_format);
        record
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment> AccessorRoot<E::Value> for StoreRoot<E> {
    async fn read(&self) -> Result<Option<(State<E::Value>, Version)>, Error> {
        let envref = upgrade(&self.env)?;
        let store = envref.get_async_store();
        if !store.contains(&self.key).await? {
            return Ok(None);
        }
        match self.view {
            StoreView::Binary | StoreView::Value => {
                let (bytes, metadata) = store.get(&self.key).await?;
                let version = metadata
                    .version()
                    .unwrap_or_else(|| Version::from_content(&bytes));
                let value = match self.view {
                    StoreView::Binary => E::Value::from_bytes(bytes),
                    StoreView::Value | StoreView::Metadata => {
                        let record = metadata.metadata_record();
                        let format = self.data_format(record.as_ref(), None);
                        let type_identifier = metadata.type_identifier().unwrap_or_default();
                        E::Value::deserialize_from_bytes(&bytes, &type_identifier, &format)?
                    }
                };
                Ok(Some((
                    State::from_value_and_metadata(value, Arc::new(metadata)),
                    version,
                )))
            }
            StoreView::Metadata => {
                let metadata = store.get_metadata(&self.key).await?;
                let record = record_of(&metadata)?;
                let json = serde_json::to_vec(&record).map_err(|e| {
                    Error::general_error(format!("cannot serialize metadata: {e}"))
                })?;
                let version = Version::from_bytes(&json);
                let value = E::Value::from_metadata(record);
                Ok(Some((
                    State::from_value_and_metadata(value, Arc::new(metadata)),
                    version,
                )))
            }
        }
    }

    async fn write(&self, state: State<E::Value>) -> Result<(State<E::Value>, Version), Error> {
        let envref = upgrade(&self.env)?;
        let value = state.value()?;
        let current = self.stored_record(&envref).await?;
        let (bytes, mut record) = match self.view {
            StoreView::Binary => {
                let bytes = value.try_into_bytes()?;
                let record = match current {
                    Some(record) => record,
                    // Opaque bytes: their own type and format, not the key's extension.
                    None => self.new_record(&value, value.default_data_format().to_string()),
                };
                (bytes, record)
            }
            StoreView::Value => {
                let format = self.data_format(current.as_ref(), Some(&value));
                let bytes = value.as_bytes(&format)?;
                let mut record = match current {
                    Some(record) => record,
                    None => self.new_record(&value, format.clone()),
                };
                record.type_identifier = value.identifier().to_string();
                record.type_name = value.type_name().to_string();
                record.data_format = Some(format);
                (bytes, record)
            }
            StoreView::Metadata => {
                if current.is_none() {
                    return Err(Error::key_not_found(&self.key));
                }
                let bytes = envref.get_async_store().get_bytes(&self.key).await?;
                let json = value.try_into_json_value()?;
                let record: MetadataRecord = serde_json::from_value(json).map_err(|e| {
                    Error::conversion_error_with_message(
                        "value",
                        "metadata record",
                        &e.to_string(),
                    )
                })?;
                (bytes, record)
            }
        };
        // `set_binary` decides the status (Source, or Override when the key has a recipe).
        record.status = Status::None;
        record.key = Some(self.key.clone());
        envref
            .get_asset_manager()
            .set_binary(&self.key, &bytes, record)
            .await?;
        self.read().await?.ok_or_else(|| Error::key_not_found(&self.key))
    }

    async fn info(&self) -> Result<AccessorInfo, Error> {
        let envref = upgrade(&self.env)?;
        let store = envref.get_async_store();
        if !store.contains(&self.key).await? {
            return Ok(AccessorInfo::with_status(Status::None));
        }
        let metadata = store.get_metadata(&self.key).await?;
        let mut info = AccessorInfo::with_status(metadata.status());
        info.message = metadata.message().to_string();
        info.log_len = log_of(&metadata).len();
        Ok(info)
    }

    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        let envref = upgrade(&self.env)?;
        let store = envref.get_async_store();
        if !store.contains(&self.key).await? {
            return Ok(Vec::new());
        }
        let metadata = store.get_metadata(&self.key).await?;
        Ok(log_of(&metadata).into_iter().skip(since).collect())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: true,
            notify: false,
        }
    }

    fn not_found(&self) -> Error {
        Error::key_not_found(&self.key)
    }
}

/// An accessor of the data stored under `key`, seen through `view`. A key the store does not
/// contain has status `None`: `get` fails and `set` creates it (except through the metadata view,
/// which has no data to describe). Other writers of the key are seen on `get` and `refresh`.
pub fn store_accessor<E: Environment>(
    envref: EnvRef<E>,
    key: Key,
    view: StoreView,
    address: Query,
) -> Arc<dyn ValueAccessor<E::Value>> {
    AccessorHub::new(
        Arc::new(StoreRoot::new(&envref, key, view)),
        address,
        ExpiryPolicy::Relay,
    )
}

/// What an asset's notification means for an accessor root.
enum Relayed {
    Event(RootEvent),
    Removed,
    Nothing,
}

/// The live asset an asset-backed root relays, and its notification receiver.
struct AssetRelay<E: Environment> {
    live: Mutex<Option<(AssetRef<E>, watch::Receiver<AssetNotificationMessage>)>>,
    /// Bumped when the live asset changes, so a pending watch re-subscribes.
    generation: watch::Sender<u64>,
    secondary: Mutex<Option<ProgressEntry>>,
}

impl<E: Environment> AssetRelay<E> {
    fn new() -> Self {
        AssetRelay {
            live: Mutex::new(None),
            generation: watch::Sender::new(0),
            secondary: Mutex::new(None),
        }
    }

    fn current(&self) -> Option<AssetRef<E>> {
        self.live
            .lock()
            .ok()
            .and_then(|live| live.as_ref().map(|(asset, _)| asset.clone()))
    }

    /// Relays `asset` from now on, unless it is already the one relayed.
    async fn attach(&self, asset: &AssetRef<E>) {
        let same = self.current().map(|a| a.id() == asset.id()).unwrap_or(false);
        if same {
            return;
        }
        let mut receiver = asset.subscribe_to_notifications().await;
        // Relay the latest message too: the asset may have moved on before this subscription.
        receiver.mark_changed();
        if let Ok(mut live) = self.live.lock() {
            *live = Some((asset.clone(), receiver));
        }
        self.generation.send_modify(|g| *g = g.wrapping_add(1));
    }

    fn detach(&self) {
        let had = match self.live.lock() {
            Ok(mut live) => live.take().is_some(),
            Err(_) => false,
        };
        if had {
            self.generation.send_modify(|g| *g = g.wrapping_add(1));
        }
    }

    fn secondary(&self) -> Option<ProgressEntry> {
        self.secondary.lock().ok().and_then(|s| s.clone())
    }

    /// The pending notification of the live asset, translated.
    async fn poll(&self) -> Relayed {
        let pending = match self.live.lock() {
            Ok(mut live) => match live.as_mut() {
                Some((asset, receiver)) => match receiver.has_changed() {
                    Ok(true) => Some((receiver.borrow_and_update().clone(), asset.clone())),
                    Ok(false) | Err(_) => None,
                },
                None => None,
            },
            Err(_) => None,
        };
        let Some((message, asset)) = pending else {
            return Relayed::Nothing;
        };
        match message {
            AssetNotificationMessage::Initial => Relayed::Nothing,
            AssetNotificationMessage::JobSubmitted
            | AssetNotificationMessage::JobStarted
            | AssetNotificationMessage::JobFinished => {
                Relayed::Event(RootEvent::Status(asset.status().await))
            }
            AssetNotificationMessage::StatusChanged(status) => {
                Relayed::Event(RootEvent::Status(status))
            }
            AssetNotificationMessage::ErrorOccurred(_) => {
                Relayed::Event(RootEvent::Status(Status::Error))
            }
            AssetNotificationMessage::Expired => Relayed::Event(RootEvent::Status(Status::Expired)),
            AssetNotificationMessage::ValueProduced => {
                let version = match asset.get_metadata().await {
                    Ok(metadata) => metadata.version().unwrap_or_else(Version::unknown),
                    Err(_) => Version::unknown(),
                };
                Relayed::Event(RootEvent::Changed(version))
            }
            AssetNotificationMessage::LogMessage => {
                let len = match asset.get_metadata().await {
                    Ok(metadata) => log_of(&metadata).len(),
                    Err(_) => 0,
                };
                Relayed::Event(RootEvent::Log(len))
            }
            AssetNotificationMessage::PrimaryProgressUpdated(progress) => {
                Relayed::Event(RootEvent::PrimaryProgress(progress))
            }
            AssetNotificationMessage::SecondaryProgressUpdated(progress) => {
                if let Ok(mut secondary) = self.secondary.lock() {
                    *secondary = Some(progress.clone());
                }
                Relayed::Event(RootEvent::SecondaryProgress(progress))
            }
            AssetNotificationMessage::Removed => Relayed::Removed,
        }
    }

    /// Completes when the live asset sends something or the live asset changes.
    fn watch(&self) -> BoxFuture<'static, ()> {
        let receiver = self
            .live
            .lock()
            .ok()
            .and_then(|live| live.as_ref().map(|(_, r)| r.clone()));
        let mut generation = self.generation.subscribe();
        Box::pin(async move {
            let generation_changed = async move {
                if generation.changed().await.is_err() {
                    futures::future::pending::<()>().await;
                }
            };
            match receiver {
                Some(mut receiver) => {
                    let message = async move {
                        if receiver.changed().await.is_err() {
                            // The asset is gone; only a new one can wake us.
                            futures::future::pending::<()>().await;
                        }
                    };
                    futures::future::select(Box::pin(message), Box::pin(generation_changed)).await;
                }
                None => generation_changed.await,
            }
        })
    }

    /// Status, message and progress of `asset_info`; the secondary progress and the log length
    /// from the live asset's `metadata`, or the last relayed secondary progress without one.
    fn info(&self, asset_info: AssetInfo, metadata: Option<&Metadata>) -> AccessorInfo {
        let mut info = AccessorInfo::with_status(asset_info.status);
        info.message = asset_info.message;
        info.primary_progress = Some(asset_info.progress);
        info.secondary_progress = metadata
            .map(|m| m.secondary_progress())
            .filter(|p| !p.is_off())
            .or_else(|| self.secondary());
        info.log_len = metadata.map(|m| log_of(m).len()).unwrap_or(0);
        info
    }
}

/// A keyed asset: read through the asset manager (which evaluates it when needed), written with
/// [`AssetManager::set_state`] (an `Override` when the key has a recipe, else a `Source`, created
/// when missing). It relays the live asset's status, progress and log.
pub struct KeyRoot<E: Environment> {
    env: Weak<E>,
    key: Key,
    relay: AssetRelay<E>,
}

impl<E: Environment> Debug for KeyRoot<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyRoot")
            .field("key", &self.key.encode())
            .finish()
    }
}

impl<E: Environment> KeyRoot<E> {
    pub fn new(envref: &EnvRef<E>, key: Key) -> Self {
        KeyRoot {
            env: Arc::downgrade(&envref.0),
            key,
            relay: AssetRelay::new(),
        }
    }

    async fn read_asset(
        &self,
        asset: &AssetRef<E>,
    ) -> Result<(State<E::Value>, Version), Error> {
        self.relay.attach(asset).await;
        let state = asset.get().await?;
        let version = asset
            .get_metadata()
            .await?
            .version()
            .unwrap_or_else(Version::unknown);
        Ok((state, version))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment> AccessorRoot<E::Value> for KeyRoot<E> {
    async fn read(&self) -> Result<Option<(State<E::Value>, Version)>, Error> {
        let envref = upgrade(&self.env)?;
        let manager = envref.get_asset_manager();
        if !manager.can_make(&self.key).await? {
            self.relay.detach();
            return Ok(None);
        }
        let asset = manager.get(&self.key).await?;
        Ok(Some(self.read_asset(&asset).await?))
    }

    async fn write(&self, state: State<E::Value>) -> Result<(State<E::Value>, Version), Error> {
        let envref = upgrade(&self.env)?;
        let manager = envref.get_asset_manager();
        manager.set_state(&self.key, state).await?;
        let asset = manager.get(&self.key).await?;
        self.read_asset(&asset).await
    }

    async fn info(&self) -> Result<AccessorInfo, Error> {
        let envref = upgrade(&self.env)?;
        let manager = envref.get_asset_manager();
        match manager.get_asset_info(&self.key).await {
            Ok(asset_info) => {
                let metadata = match self.relay.current() {
                    Some(asset) => Some(asset.get_metadata().await?),
                    None => {
                        let store = envref.get_async_store();
                        if store.contains(&self.key).await? {
                            Some(store.get_metadata(&self.key).await?)
                        } else {
                            None
                        }
                    }
                };
                Ok(self.relay.info(asset_info, metadata.as_ref()))
            }
            Err(e) if e.error_type == ErrorType::AssetNotFound => {
                Ok(AccessorInfo::with_status(Status::None))
            }
            Err(e) => Err(e),
        }
    }

    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        let metadata = match self.relay.current() {
            Some(asset) => asset.get_metadata().await?,
            None => {
                let envref = upgrade(&self.env)?;
                let store = envref.get_async_store();
                if !store.contains(&self.key).await? {
                    return Ok(Vec::new());
                }
                store.get_metadata(&self.key).await?
            }
        };
        Ok(log_of(&metadata).into_iter().skip(since).collect())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: true,
            notify: true,
        }
    }

    async fn poll_event(&self) -> Option<RootEvent> {
        match self.relay.poll().await {
            Relayed::Event(event) => Some(event),
            Relayed::Nothing => None,
            Relayed::Removed => {
                self.relay.detach();
                // A replacement (a set, or a write through another accessor) also removes the
                // old asset: while the key can still be made, the value has changed, not gone.
                let can_make = match upgrade(&self.env) {
                    Ok(envref) => envref
                        .get_asset_manager()
                        .can_make(&self.key)
                        .await
                        .unwrap_or(false),
                    Err(_) => false,
                };
                Some(if can_make {
                    RootEvent::Changed(Version::unknown())
                } else {
                    RootEvent::Removed
                })
            }
        }
    }

    fn watch(&self) -> Option<BoxFuture<'static, ()>> {
        Some(self.relay.watch())
    }

    fn not_found(&self) -> Error {
        Error::asset_not_found(&self.key)
    }
}

/// An accessor of the keyed asset `key`. A set makes it an `Override` when the key has a recipe and
/// a `Source` when it has none, creating it when it does not exist; a key nothing can make has
/// status `None`.
pub fn key_accessor<E: Environment>(
    envref: EnvRef<E>,
    key: Key,
    address: Query,
) -> Arc<dyn ValueAccessor<E::Value>> {
    AccessorHub::new(
        Arc::new(KeyRoot::new(&envref, key)),
        address,
        ExpiryPolicy::Relay,
    )
}

/// An asset the holder already has (a query asset, an `apply` result), read-only: the asset
/// manager has no write for a non-keyed asset. It relays the asset's status, progress and log;
/// its removal is final.
pub struct AssetRefRoot<E: Environment> {
    asset: AssetRef<E>,
    removed: AtomicBool,
    relay: AssetRelay<E>,
}

impl<E: Environment> Debug for AssetRefRoot<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetRefRoot")
            .field("asset", &self.asset.id())
            .finish()
    }
}

impl<E: Environment> AssetRefRoot<E> {
    pub fn new(asset: AssetRef<E>) -> Self {
        AssetRefRoot {
            asset,
            removed: AtomicBool::new(false),
            relay: AssetRelay::new(),
        }
    }

    fn is_removed(&self) -> bool {
        self.removed.load(Ordering::SeqCst)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment> AccessorRoot<E::Value> for AssetRefRoot<E> {
    async fn read(&self) -> Result<Option<(State<E::Value>, Version)>, Error> {
        if self.is_removed() {
            return Ok(None);
        }
        self.relay.attach(&self.asset).await;
        let state = self.asset.get().await?;
        let version = self
            .asset
            .get_metadata()
            .await?
            .version()
            .unwrap_or_else(Version::unknown);
        Ok(Some((state, version)))
    }

    async fn info(&self) -> Result<AccessorInfo, Error> {
        if self.is_removed() {
            return Ok(AccessorInfo::with_status(Status::None));
        }
        let asset_info = self.asset.get_asset_info().await?;
        let metadata = self.asset.get_metadata().await?;
        Ok(self.relay.info(asset_info, Some(&metadata)))
    }

    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> {
        let metadata = self.asset.get_metadata().await?;
        Ok(log_of(&metadata).into_iter().skip(since).collect())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: false,
            notify: true,
        }
    }

    async fn poll_event(&self) -> Option<RootEvent> {
        // Subscribe on the first poll, so events before the first read are relayed too.
        if !self.is_removed() {
            self.relay.attach(&self.asset).await;
        }
        match self.relay.poll().await {
            Relayed::Event(event) => Some(event),
            Relayed::Nothing => None,
            Relayed::Removed => {
                self.removed.store(true, Ordering::SeqCst);
                self.relay.detach();
                Some(RootEvent::Removed)
            }
        }
    }

    fn watch(&self) -> Option<BoxFuture<'static, ()>> {
        if self.is_removed() {
            return None;
        }
        Some(self.relay.watch())
    }

    fn not_found(&self) -> Error {
        let mut error = Error::not_available();
        error.message = "the asset was removed".to_string();
        error
    }
}

/// A read-only accessor of an asset the holder already has: its value, status, progress and log.
pub fn asset_accessor<E: Environment>(
    asset: AssetRef<E>,
    address: Query,
) -> Arc<dyn ValueAccessor<E::Value>> {
    AccessorHub::new(
        Arc::new(AssetRefRoot::new(asset)),
        address,
        ExpiryPolicy::Relay,
    )
}
