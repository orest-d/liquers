//! # Assets
//!
//! Asset can be seen as the outer-most (3rd layer) of value encapsulation in Liquers:
//! - 1st layer: Value - represents the actual data and its type - basically an enum
//! - 2nd layer: State - represents a value with its metadata (status, type, logs, etc.)
//! - 3rd layer: Asset - represents a state that may be ready, it is being queued or produced or can be produced on demand.
//!
//! Asset provides access to the data, its metadata and binary representation and progress updates.
//! Asset data are stored in an [AssetData] structure, which is accessed via [AssetRef].
//! [AssetRef] is clonable and can be shared between multiple tasks.
//! [AssetRef] is basically arc [tokio::sync::RwLock] on [AssetData].
//!
//! ## Asset Communication
//! AssetData communicates via two channels:
//! - **service channel** (mpsc) that can trigger asset changes (monitor progress and cancelation)
//! - **notification channel** (watch) that notifies about asset changes (status and progress updates, new data, errors)
//!
//! Service channel should be considered internal. It communicates via [AssetServiceMessage].
//! Service channel must be reliable and not drop messages. Context and JobQueue uses the service channel to send messages to the asset.
//! Context typically uses it for sending log messages and progress updates. JobQueue uses it to notify about job status changes and cancelation.
//!
//! Notification channel communicates notifications towards clients.
//! Notification channel communicates via [AssetNotificationMessage].
//! Since AssetData is maintaining a consistent authoritative state, it is not a problem if client will miss a notification.
//! Client can always query the current state of the asset.
//!
//! ## AssetData structure
//! AssetData holds a [Recipe] data structure describing the task to construct the value.
//! Initial value may also be provided to represent "apply" operation, e.g. where a query is applied to an existing value.
//! The resulting data are hold in 3 optional fields:
//! - metadata: [Metadata] - Always [crate::metadata::MetadataRecord] for new assets, but it can be legacy if binary data is available.
//! - data: `Arc<V>` where V is the value type
//! - binary: `Arc<Vec<u8>>` representing the serialized value
//!
//! ## Asset lifecycle
//! Asset typically goes through these stages:
//! 1) **initial** - a state the asset is in after creation. Only the recipe is known, none of the data, binary or metadata is available.
//! 2) **prepare** - check is binary data is available. In such a case value is deserialized.
//! 3) **run** - start recipe execution and the loop processing the service messages.
//! 4) **finished** - cancelled, error or success. Cancelled or error can be restarted.
//!

use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
};

use async_trait::async_trait;
use futures::lock;
use scc;
use tokio::sync::{mpsc, watch, Mutex, RwLock};

use crate::context::Context;
use crate::interpreter::IsVolatile;
use crate::metadata::{AssetInfo, LogEntry, MetadataRecord, ProgressEntry};
use crate::value::ValueInterface;
use crate::{
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{Metadata, Status},
    query::{Key, Query},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    value::DefaultValueSerializer,
};

/// Message for internal service communication (reliable, for control)
#[derive(Debug, Clone)]
pub enum AssetServiceMessage {
    /// Job has been submitted to the queue
    JobSubmitted,
    /// Job has started execution
    JobStarted,
    /// Log message has been emitted
    LogMessage(LogEntry),
    /// Primary progress has been updated
    UpdatePrimaryProgress(ProgressEntry),
    /// Secondary progress has been updated
    UpdateSecondaryProgress(ProgressEntry),
    /// Job is requested to be cancelled
    Cancel,
    /// Error occured, job will finish
    ErrorOccurred(Error),
    /// Job is about to finish - only some houskeeping remains
    JobFinishing,
    /// Job is finished, no further action will be taken
    JobFinished,
}

/// Message for notifications to clients (best-effort, for updates)
#[derive(Debug, Clone)]
pub enum AssetNotificationMessage {
    Initial,
    JobSubmitted,
    JobStarted,
    StatusChanged(Status), // TODO: remove argument?
    ValueProduced,
    ErrorOccurred(Error),
    LogMessage,
    PrimaryProgressUpdated(ProgressEntry),
    SecondaryProgressUpdated(ProgressEntry),
    JobFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceStatus {
    /// No persistence attempt has been made yet.
    None,
    /// Value and metadata have been persisted.
    Persisted,
    /// Value cannot be serialized in current representation.
    NonSerializable,
    /// Persistence was attempted but failed.
    NotPersisted,
}

pub struct MetadataSaver {
    metadata: Mutex<Option<Metadata>>,
    interval: std::time::Duration,
    last_save: Option<std::time::Instant>,
}

impl MetadataSaver {
    pub fn new(interval: std::time::Duration) -> Self {
        Self {
            metadata: Mutex::new(None),
            interval,
            last_save: None,
        }
    }
    // TODO: Make a proper save immediately task
    /*
    pub async fn save(&mut self, metadata: &Metadata, envref: EnvRef<impl Environment>) -> Result<(), Error> {
        tokio::spawn(async move{
            self.save_task(metadata, envref);
        });
        Ok(())
    }
    */
    async fn save_task(
        &mut self,
        metadata: &Metadata,
        envref: EnvRef<impl Environment>,
    ) -> Result<(), Error> {
        let mut lock = self.metadata.lock().await;
        if lock.is_some() {
            *lock = Some(metadata.clone());
        } else {
            if self.can_save_now() {
                let store = envref.get_async_store();
                if let Some(key) = metadata.key()? {
                    store.set_metadata(&key, metadata).await?;
                    self.last_save = Some(std::time::Instant::now());
                }
                *lock = None;
            } else {
                *lock = Some(metadata.clone());
                drop(lock);
                tokio::time::sleep(self.duration_to_next_save()).await;
                let mut lock = self.metadata.lock().await;
                if let Some(metadata) = lock.take() {
                    let store = envref.get_async_store();
                    if let Some(key) = metadata.key()? {
                        store.set_metadata(&key, &metadata).await?;
                        self.last_save = Some(std::time::Instant::now());
                    }
                }
            }
        }
        Ok(())
    }

    fn duration_since_last_save(&self) -> Option<std::time::Duration> {
        if let Some(last_save) = self.last_save {
            Some(std::time::Instant::now().duration_since(last_save))
        } else {
            None
        }
    }

    fn duration_to_next_save(&self) -> std::time::Duration {
        if let Some(duration) = self.duration_since_last_save() {
            if duration >= self.interval {
                std::time::Duration::from_secs(0)
            } else {
                self.interval - duration
            }
        } else {
            std::time::Duration::from_secs(0)
        }
    }

    fn can_save_now(&self) -> bool {
        if let Some(duration) = self.duration_since_last_save() {
            duration >= self.interval
        } else {
            true
        }
    }
}

pub struct AssetData<E: Environment> {
    id: u64,
    pub recipe: Recipe,

    envref: EnvRef<E>,
    // Service channel (mpsc) for control messages
    service_tx: mpsc::UnboundedSender<AssetServiceMessage>,
    service_rx: Arc<Mutex<mpsc::UnboundedReceiver<AssetServiceMessage>>>,

    // Notification channel (watch) for client notifications
    notification_tx: watch::Sender<AssetNotificationMessage>,
    _notification_rx: watch::Receiver<AssetNotificationMessage>,

    initial_state: State<E::Value>,

    query: Arc<Option<Query>>,

    /// This is used to store the data in the asset if available.
    data: Option<Arc<E::Value>>,

    /// This is used to store the binary representation of the data in the asset if available.
    /// If both data and binary is available, they will represent the same data and can be used interchangeably.
    binary: Option<Arc<Vec<u8>>>,

    /// Metadata
    pub(crate) metadata: Metadata,

    /// Current status
    status: Status,

    /// If true, the asset will be saved to the store in the background.
    /// By default true.
    /// This may not be ideal for some use cases, e.g. when the binary representation needs
    /// to be created in python
    pub(crate) save_in_background: bool,

    /// If true, this asset has been cancelled and should not write results.
    /// Any ValueProduced or store write attempts should be silently dropped.
    /// This is used to prevent race conditions when cancelling long-running tasks.
    cancelled: bool,

    /// If true, this asset is volatile (computed from recipe/plan before execution)
    is_volatile: bool,

    /// Tracks persistence lifecycle for value-producing paths.
    persistence_status: PersistenceStatus,
    /// Last persistence error, when relevant.
    last_persistence_error: Option<Error>,

    _marker: std::marker::PhantomData<E>,
}

impl<E: Environment> AssetData<E> {
    pub fn new(id: u64, recipe: Recipe, envref: EnvRef<E>) -> Self {
        Self::new_ext(id, recipe, State::new(), envref)
    }

    /// Creates a temporary asset data structure.
    pub fn new_temporary(envref: EnvRef<E>) -> Self {
        let asset = Self::new_ext(0, Recipe::default(), State::new(), envref);
        asset
    }

    pub fn new_ext(
        id: u64,
        recipe: Recipe,
        initial_state: State<E::Value>,
        envref: EnvRef<E>,
    ) -> Self {
        let (service_tx, service_rx) = mpsc::unbounded_channel();
        let (notification_tx, notification_rx) = watch::channel(AssetNotificationMessage::Initial);
        let query = if recipe.is_pure_query() {
            if let Ok(q) = recipe.get_query() {
                Arc::new(Some(q))
            } else {
                Arc::new(None)
            }
        } else {
            Arc::new(None)
        };
        let mut assetinfo = recipe
            .get_asset_info()
            .unwrap_or_else(|_| AssetInfo::default());
        assetinfo.type_identifier = initial_state.type_identifier().to_string();
        let asset = AssetData {
            id,
            envref,
            recipe,
            service_tx,
            service_rx: Arc::new(Mutex::new(service_rx)),
            notification_tx,
            _notification_rx: notification_rx,
            initial_state,
            query,
            data: None,
            binary: None,
            metadata: assetinfo.into(),
            save_in_background: true,
            cancelled: false,
            is_volatile: false,
            persistence_status: PersistenceStatus::None,
            last_persistence_error: None,
            _marker: std::marker::PhantomData,
            status: Status::None,
        };

        asset
    }

    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    async fn save_metadata_to_store_now(&self) -> Result<(), Error> {
        let envref = self.get_envref();
        let store = envref.get_async_store();
        let key = self.recipe.key()?.or(self.recipe.store_to_key()?);
        if let Some(key) = key.as_ref() {
            store.set_metadata(key, &self.metadata).await
        } else {
            Ok(()) // No key => nowhere and no need to save
        }
    }

    async fn save_metadata_to_store(&self) -> Result<(), Error> {
        // TODO: prevent too frequent saving
        self.save_metadata_to_store_now().await?;
        Ok(())
    }

    /// Check if the asset has an initial value
    pub fn has_initial_value(&self) -> Result<bool, Error> {
        Ok((!self.initial_state.is_error()?) && (!self.initial_state.is_none()))
    }

    /// Check if the asset is a resource (has a key in the recipe and no initial value)
    pub fn is_resource(&self) -> Result<bool, Error> {
        if self.has_initial_value()? {
            return Ok(false);
        }

        if let Ok(Some(_key)) = self.recipe.key() {
            return Ok(true);
        }

        Ok(false)
    }

    /// Get asset info structure for the asset
    pub fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        let mut assetinfo = self.metadata.get_asset_info().unwrap_or_default();
        if let Some(key) = self.recipe.key()?.or(self.recipe.store_to_key()?) {
            assetinfo.with_key(key);
        }
        assetinfo.query = Some(self.recipe.get_query()?);
        assetinfo.status = self.status;
        Ok(assetinfo)
    }

    pub fn asset_reference(&self) -> String {
        if self.is_resource().unwrap_or(false) {
            if let Ok(Some(key)) = self.recipe.key() {
                return format!("Resource asset {}: {}", self.id(), key);
            }
        }
        if let Ok(true) = self.is_pure_query() {
            let q = self.recipe.get_query().unwrap();
            return format!("Pure query asset {}: {}", self.id(), q);
        }
        format!("Complex asset {}: {:?}", self.id(), self.recipe)
    }

    /// Check if the asset is a pure query (no initial value and recipe is a pure query)
    pub fn is_pure_query(&self) -> Result<bool, Error> {
        Ok((!self.has_initial_value()?) && self.recipe.is_pure_query())
    }

    /// This tries to get an asset value by quickly evaluation strategies.
    /// For example, if the asset is a resource an attempt is made to deserialize it.
    /// These strategies are tried before the asset is queued for evaluation.
    /// A queue might be occupied by long running tasks, so it is beneficial
    /// to try to load the asset quickly.
    /// If the asset becomes available after the fast track attempt, it is not queued.
    ///
    /// The purpose of the fast track is to avoid unnecessary queuing of assets
    /// and thus prevent blocking of assets that are in store (i.e. they are ready, just not loaded).
    /// For example - if the queue is blocked by long running task(s),
    /// a server can still reply immediately if the asset is in the store.
    /// This can be generalized further to support volatile and fast queries.
    pub async fn try_fast_track(&mut self) -> Result<bool, Error> {
        eprintln!("Trying fast track for asset {}", self.id());
        if !self.is_resource()? {
            // TODO: support for quick plans
            eprintln!("Asset {} is not a resource, cannot fast track", self.id());
            return Ok(false); // If asset is not a resource, it can't be just loaded
        }

        let store = self.get_envref().get_async_store();
        if let Ok(Some(key)) = self.recipe.key() {
            eprintln!("Asset {} is a resource with key {}", self.id(), key);
            if store.contains(&key).await? {
                eprintln!("Asset {} exists in the store, loading", self.id());
                // Asset exists in the store, load binary and metadata
                let (binary, metadata) = store.get(&key).await?;
                if metadata.is_error() == Ok(true) {
                    // Stored as error
                    self.metadata = metadata;
                    self.status = self.metadata.status();
                    self.binary = None;
                    self.data = None;
                    return Ok(true);
                } else if metadata.status().has_data() {
                    // TODO: if binary is supplied, it does have data...
                    self.binary = Some(Arc::new(binary));
                    eprintln!("Asset {} has data, deserializing", self.id());
                    let value = E::Value::deserialize_from_bytes(
                        self.binary.as_ref().unwrap(),
                        &metadata.type_identifier()?,
                        &metadata.get_data_format(),
                    )?; // TODO: If it fails to deserialize, it might be corrupted
                    self.data = Some(Arc::new(value));
                    self.status = metadata.status();
                    self.metadata = metadata;
                    match self.status {
                        Status::Ready | Status::Source | Status::Override | Status::Volatile => {
                            self.notification_tx
                                .send(AssetNotificationMessage::JobFinished)
                                .map_err(|e| {
                                    Error::general_error(format!(
                                        "Failed to send job finished notification: {}",
                                        e
                                    ))
                                    .with_query(&key.into())
                                })?;
                            eprintln!("Asset {} loaded successfully", self.id());
                            return Ok(true);
                        }
                        Status::None
                        | Status::Directory
                        | Status::Recipe
                        | Status::Submitted
                        | Status::Dependencies
                        | Status::Processing
                        | Status::Partial
                        | Status::Error
                        | Status::Storing
                        | Status::Expired
                        | Status::Cancelled => {
                            self.notification_tx
                                .send(AssetNotificationMessage::StatusChanged(self.status))
                                .map_err(|e| {
                                    Error::general_error(format!(
                                        "Failed to send status change notification: {}",
                                        e
                                    ))
                                    .with_query(&key.into())
                                })?;
                            eprintln!("Asset {} loaded data that is not ready", self.id());
                            return Ok(false);
                        }
                    }
                } else {
                    return Err(Error::general_error(format!("Inconsistent status of asset {}: Asset is stored, having binary size {}, but it has status: {:?}",
                    self.id(), binary.len(), self.status)).with_key(&key));
                }
            } else {
                eprintln!("Asset {} does not exist in the store", self.id());
            }
        }
        Ok(false)
    }

    /// Get a reference to the asset data
    pub fn to_ref(self) -> AssetRef<E> {
        AssetRef::new(self)
    }

    /// Get a clone of the service sender for internal control
    pub fn service_sender(&self) -> mpsc::UnboundedSender<AssetServiceMessage> {
        self.service_tx.clone()
    }

    /// Subscribe to the notifications.
    pub fn subscribe_to_notifications(&self) -> watch::Receiver<AssetNotificationMessage> {
        self.notification_tx.subscribe()
    }

    pub fn set_status(&mut self, status: Status) -> Result<(), Error> {
        if status != self.status {
            eprintln!(
                "Asset {} status changed from {:?} to {:?}",
                self.id(),
                self.status,
                status
            );
            self.status = status;
            self.metadata.set_status(status)?;
        }
        Ok(())
    }

    /// Poll the current state without any async operations.
    /// Returns None if data or metadata is not available.
    pub fn poll_state(&self) -> Option<State<E::Value>> {
        match self.status {
            Status::None => None,
            Status::Directory => {
                let mut metadata = self.metadata.clone();
                metadata.with_type_identifier("dir".to_string());
                Some(State {
                    data: Arc::new(E::Value::none()),
                    metadata: Arc::new(metadata),
                })
            }
            Status::Recipe => None,
            Status::Submitted => None,
            Status::Dependencies => None,
            Status::Processing => None,
            Status::Partial => None,
            Status::Error | Status::Cancelled => {
                let mut metadata = self.metadata.clone();
                Some(State {
                    data: Arc::new(E::Value::none()),
                    metadata: Arc::new(metadata),
                })
            }
            Status::Storing => None,
            Status::Ready
            | Status::Expired
            | Status::Source
            | Status::Override
            | Status::Volatile => {
                if let Some(data) = &self.data {
                    let mut metadata = self.metadata.clone();
                    metadata.with_type_identifier(data.identifier().to_string());
                    metadata.with_type_name(data.type_name().to_string());

                    Some(State {
                        data: data.clone(),
                        metadata: Arc::new(metadata),
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Check if the asset has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Set the cancelled flag
    pub fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }

    /// Poll the current binary data and metadata without any async operations.
    /// Returns None if binary or metadata is not available.
    pub fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        let mut metadata = self.metadata.clone();
        if let Some(data) = self.data.as_ref() {
            metadata.with_type_identifier(data.identifier().to_string());
        }
        self.binary.clone().zip(Some(Arc::new(metadata)))
    }

    /// Reset the asset data, binary and metadata.
    /// Status is set to None.
    pub fn reset(&mut self) {
        self.data = None;
        self.binary = None;
        self.metadata = Metadata::new().into();
        self.status = Status::None;
        self.persistence_status = PersistenceStatus::None;
        self.last_persistence_error = None;
        self.notification_tx
            .send(AssetNotificationMessage::Initial)
            .ok();
    }

    fn set_persistence_status(&mut self, status: PersistenceStatus, error: Option<Error>) {
        self.persistence_status = status;
        self.last_persistence_error = error.clone();
        match status {
            PersistenceStatus::None | PersistenceStatus::Persisted => {}
            PersistenceStatus::NonSerializable | PersistenceStatus::NotPersisted => {
                let detail = if let Some(e) = error.as_ref() {
                    e.to_string()
                } else {
                    "No additional error details available".to_owned()
                };
                let message = format!(
                    "Persistence status {:?} for {}: {}",
                    status,
                    self.asset_reference(),
                    detail
                );
                let _ = self.metadata.add_log_entry(LogEntry::warning(message));
            }
        }
    }

    /// Get the unique id of the asset
    fn id(&self) -> u64 {
        self.id
    }
}

/// Asset reference is a mean to get the state and status updates of an asset
/// It is created and returned by an asset manager.
pub struct AssetRef<E: Environment> {
    id: u64,
    pub data: Arc<RwLock<AssetData<E>>>,
}

impl<E: Environment> Clone for AssetRef<E> {
    fn clone(&self) -> Self {
        AssetRef {
            id: self.id,
            data: self.data.clone(),
        }
    }
}
impl<E: Environment> AssetRef<E> {
    async fn is_execution_closed_for_messages(&self) -> bool {
        let lock = self.data.read().await;
        lock.status.is_finished() || lock.metadata.primary_progress().is_done()
    }

    async fn finalize_primary_progress(&self) {
        let status = self.status().await;
        let message = match status {
            Status::Cancelled => "Cancelled",
            Status::Error => "Error",
            _ => "Done",
        };
        let progress = ProgressEntry::done(message.to_string());

        let mut lock = self.data.write().await;
        lock.metadata.set_primary_progress(&progress);
        let _ = lock.metadata.set_updated_now();
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::PrimaryProgressUpdated(progress));
    }

    /// Create a new asset reference from asset data.
    pub(crate) fn new(data: AssetData<E>) -> Self {
        AssetRef {
            id: data.id(),
            data: Arc::new(RwLock::new(data)),
        }
    }

    /// Create a new asset reference from a recipe.
    pub(crate) fn new_from_recipe(id: u64, recipe: Recipe, envref: EnvRef<E>) -> Self {
        AssetRef {
            id,
            data: Arc::new(RwLock::new(AssetData::new(id, recipe, envref))),
        }
    }

    /// Creates a temporary asset reference.
    /// This spawns the event processing loop immediately.
    pub fn new_temporary(envref: EnvRef<E>) -> Self {
        let assetref = AssetData::new_temporary(envref).to_ref();
        let assetref1 = assetref.clone();
        let _handle = tokio::spawn(async move { assetref1.process_service_messages().await });

        assetref
    }

    /// Get the unique id of the asset
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get a reference to the environment
    pub async fn get_envref(&self) -> EnvRef<E> {
        let lock = self.data.read().await;
        lock.get_envref()
    }

    pub async fn create_context(&self) -> Context<E> {
        let is_volatile = {
            let lock = self.data.read().await;
            lock.is_volatile
        };
        Context::new(self.clone(), is_volatile).await
    }

    /// Get a string representation describing the asset
    pub async fn asset_reference(&self) -> String {
        // TODO: Make it to return non-async shared Arc string
        let lock = self.data.read().await;
        lock.asset_reference()
    }

    /// Get asset info structure for the asset
    pub async fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        let lock = self.data.read().await;
        lock.get_asset_info()
    }

    /// Get asset info structure for the asset
    pub async fn get_metadata(&self) -> Result<Metadata, Error> {
        let lock = self.data.read().await;
        Ok(lock.metadata.clone())
    }

    pub async fn persistence_status(&self) -> PersistenceStatus {
        let lock = self.data.read().await;
        lock.persistence_status
    }

    async fn set_persistence_status(&self, status: PersistenceStatus, error: Option<Error>) {
        let mut lock = self.data.write().await;
        lock.set_persistence_status(status, error);
    }

    fn classify_persistence_error(error: &Error) -> PersistenceStatus {
        match error.error_type {
            ErrorType::SerializationError => PersistenceStatus::NonSerializable,
            _ => PersistenceStatus::NotPersisted,
        }
    }

    async fn record_persistence_result(&self, result: Result<(), Error>) {
        match result {
            Ok(()) => {
                self.set_persistence_status(PersistenceStatus::Persisted, None)
                    .await;
            }
            Err(error) => {
                let status = Self::classify_persistence_error(&error);
                self.set_persistence_status(status, Some(error)).await;
            }
        }
    }

    async fn persist_with_status_tracking(&self, save_in_background: bool, cancelled: bool) {
        if cancelled {
            self.set_persistence_status(PersistenceStatus::None, None)
                .await;
            return;
        }

        let assetref = self.clone();
        if save_in_background {
            tokio::spawn(async move {
                let result = assetref.save_to_store().await;
                assetref.record_persistence_result(result).await;
            });
        } else {
            let result = self.save_to_store().await;
            self.record_persistence_result(result).await;
        }
    }

    /// Inform the asset that it has been submitted
    pub(crate) async fn submitted(&self) -> Result<(), Error> {
        self.set_status(Status::Submitted).await?;
        let lock = self.data.read().await;

        lock.service_tx
            .send(AssetServiceMessage::JobSubmitted)
            .map_err(|e| {
                Error::general_error(format!("Failed to send JobSubmitted message: {}", e))
            })
    }

    /// Reset the asset
    pub async fn reset(&self) {
        let mut lock = self.data.write().await;
        lock.reset();
    }

    /// Process messages from the service channel
    pub(crate) async fn process_service_messages(&self) -> Result<(), Error> {
        println!(
            "Starting to process service messages for asset {}",
            self.id()
        );
        let (service_rx_ref, notification_tx) = {
            let lock = self.data.read().await;
            (lock.service_rx.clone(), lock.notification_tx.clone())
        };

        let mut rx = service_rx_ref.lock().await;

        while let Some(msg) = rx.recv().await {
            println!("Received message: {:?} by asset {}", msg, self.id());
            if self.is_execution_closed_for_messages().await {
                let should_ignore = matches!(
                    msg,
                    AssetServiceMessage::UpdatePrimaryProgress(_)
                        | AssetServiceMessage::UpdateSecondaryProgress(_)
                        | AssetServiceMessage::JobSubmitted
                        | AssetServiceMessage::JobStarted
                        | AssetServiceMessage::Cancel
                        | AssetServiceMessage::ErrorOccurred(_)
                );
                if should_ignore {
                    println!(
                        "Ignoring late service message {:?} for finished asset {}",
                        msg,
                        self.id()
                    );
                    continue;
                }
            }
            match msg {
                AssetServiceMessage::LogMessage(entry) => {
                    // Forward log message to notification channel
                    // Update metadata with log message
                    let mut lock = self.data.write().await;
                    lock.metadata.add_log_entry(entry)?;
                    lock.save_metadata_to_store().await?;
                    let _ = notification_tx.send(AssetNotificationMessage::LogMessage);
                }
                AssetServiceMessage::Cancel => {
                    self.set_status(Status::Cancelled).await?;
                    {
                        let mut lock = self.data.write().await;
                        lock.metadata
                            .set_primary_progress(&ProgressEntry::done("Cancelled".to_string()));
                    }
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Cancelled));
                    let _ = notification_tx.send(AssetNotificationMessage::JobFinished);
                    self.save_metadata_to_store().await?;
                    return Ok(());
                }
                AssetServiceMessage::UpdatePrimaryProgress(progress) => {
                    println!(
                        "Asset {} updating primary progress: {:?}",
                        self.id(),
                        progress
                    );

                    let mut lock = self.data.write().await;
                    lock.metadata.set_primary_progress(&progress);
                    lock.save_metadata_to_store().await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::PrimaryProgressUpdated(progress));
                }
                AssetServiceMessage::UpdateSecondaryProgress(progress_entry) => {
                    let mut lock = self.data.write().await;
                    lock.metadata.set_secondary_progress(&progress_entry);
                    lock.save_metadata_to_store().await?;
                    let _ = notification_tx.send(
                        AssetNotificationMessage::SecondaryProgressUpdated(progress_entry),
                    );
                }
                AssetServiceMessage::JobSubmitted => {
                    self.set_status(Status::Submitted).await?;
                    self.save_metadata_to_store().await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Submitted));
                    let _ = notification_tx.send(AssetNotificationMessage::JobSubmitted);
                }
                AssetServiceMessage::JobStarted => {
                    self.set_status(Status::Processing).await?;
                    self.save_metadata_to_store().await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Processing));
                    let _ = notification_tx.send(AssetNotificationMessage::JobStarted);
                }
                AssetServiceMessage::JobFinished => {
                    return Ok(());
                }
                AssetServiceMessage::JobFinishing => {
                    // The message should not be sent, otherwise finishing is before results are recorder in the asset
                    // let _ = notification_tx.send(AssetNotificationMessage::JobFinished);
                    return Ok(());
                }
                AssetServiceMessage::ErrorOccurred(error) => {
                    {
                        let mut lock = self.data.write().await;
                        lock.status = Status::Error;
                        lock.metadata.with_error(error.clone());
                        lock.metadata
                            .set_primary_progress(&ProgressEntry::done("Error".to_string()));
                        lock.save_metadata_to_store().await?;
                    }
                    let _ = notification_tx.send(AssetNotificationMessage::ErrorOccurred(error));
                    let _ = notification_tx.send(AssetNotificationMessage::JobFinished);
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn wait_to_finish(&self) -> Result<(), Error> {
        let mut rx = self.subscribe_to_notifications().await;
        loop {
            let notification = rx.borrow().clone();
            eprintln!(
                "Waiting for asset {} to finish, current notification: {:?}",
                self.id(),
                notification
            );
            match notification {
                AssetNotificationMessage::JobFinished => {
                    return Ok(());
                }
                _ => {}
            }
            rx.changed().await.map_err(|e| {
                Error::general_error(format!("Failed to receive notification: {}", e))
            })?;
        }
    }

    /// Run the asset evaluation loop.
    pub(crate) async fn run(&self) -> Result<(), Error> {
        if self.status().await.is_finished() {
            return Ok(()); // Already finished
        }
        let assetref = self.clone();
        let psm = tokio::spawn(async move { assetref.process_service_messages().await });
        let mut result = tokio::select! {
            res = self.wait_to_finish() => res,
            res = self.evaluate_and_store() => res
        };
        println!(
            "Asset {} evaluation finished, waiting for service messages to complete",
            self.id()
        );
        self.finalize_primary_progress().await;
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinishing)
            .ok();
        let psm_result = psm.await;
        println!("Asset {} process_service_messages task joined", self.id());
        match psm_result {
            Ok(Ok(())) => {
                println!(
                    "Asset {} process_service_messages task finished successfully",
                    self.id()
                );
            }
            Ok(Err(e)) => {
                result = Err(e);
            }
            Err(e) => {
                if result.is_ok() {
                    result = Err(Error::general_error(format!(
                        "Failed to join process_service_messages task: {}",
                        e
                    )));
                } else {
                    let mut lock = self.data.write().await;
                    lock.metadata
                        .add_log_entry(LogEntry::error(format!(
                            "Failed to join process_service_messages task: {}",
                            e
                        )))
                        .ok();
                }
            }
        }

        if let Err(e) = &result {
            println!("Asset {} evaluation finished with error: {}", self.id(), e);
            let mut lock = self.data.write().await;
            lock.data = None;
            lock.status = Status::Error;
            lock.binary = None;
            lock.metadata = Metadata::from_error(e.clone());
        } else {
            println!("Asset {} evaluation finished without an error", self.id(),);
            async fn try_to_set_ready(assetref: AssetRef<impl Environment>) {
                eprintln!(
                    "Trying to set asset {} to ready - status {:?}",
                    assetref.id(),
                    assetref.status().await
                );
                let mut lock = assetref.data.write().await;
                if lock.data.is_some() {
                    if lock.is_volatile {
                        lock.status = Status::Volatile;
                        if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                            mr.status = Status::Volatile;
                            mr.is_volatile = true;
                        }
                    } else {
                        lock.status = Status::Ready;
                        if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                            mr.status = Status::Ready;
                        }
                    }
                } else {
                    lock.status = Status::Error;
                    let e = Error::unexpected_error(format!(
                        "Asset evaluation finished ({:?} status) but no data available",
                        lock.status
                    ));
                    if let Err(e) = lock.metadata.add_log_entry(LogEntry::from_error(&e)) {
                        eprintln!("!!!ERROR!!! Failed to add log entry: {}", e);
                    }
                }
            }
            match self.status().await {
                Status::None => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Recipe => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Submitted => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Dependencies => todo!(),
                Status::Processing => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Partial => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Error => {
                    let mut lock = self.data.write().await;
                    lock.data = None;
                    lock.binary = None;
                    let _ = lock.metadata.add_log_entry(LogEntry::error(
                        "Asset ended in error status after evaluation".to_string(),
                    ));
                }
                Status::Storing => {
                    let mut lock = self.data.write().await;
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(
                        "Asset ended in status 'Storing' after evaluation".to_string(),
                    ));
                }
                Status::Ready => {}
                Status::Expired => {}
                Status::Cancelled => {}
                Status::Source => {}
                Status::Directory => {}
                Status::Override => {}
                Status::Volatile => {}
            }
        }
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinished)
            .ok();
        {
            let mut lock = self.data.write().await;
            println!("Asset {} sending JobFinished notification", self.id());
            lock.notification_tx
                .send(AssetNotificationMessage::JobFinished)
                .ok();
        }
        result
    }

    /// Run the asset evaluation loop.
    pub(crate) async fn run_immediately(&self, payload: Option<E::Payload>) -> Result<(), Error> {
        if self.status().await.is_finished() {
            return Ok(()); // Already finished
        }
        let assetref = self.clone();
        let psm = tokio::spawn(async move { assetref.process_service_messages().await });
        let mut result = tokio::select! {
            res = self.wait_to_finish() => res,
            res = self.evaluate_immediately(payload) => res
        };
        self.finalize_primary_progress().await;
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinishing)
            .ok();
        let psm_result = psm.await;
        match psm_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                result = Err(e);
            }
            Err(e) => {
                if result.is_ok() {
                    result = Err(Error::general_error(format!(
                        "Failed to join process_service_messages task: {}",
                        e
                    )));
                } else {
                    let mut lock = self.data.write().await;
                    lock.metadata
                        .add_log_entry(LogEntry::error(format!(
                            "Failed to join process_service_messages task: {}",
                            e
                        )))
                        .ok();
                }
            }
        }

        if let Err(e) = &result {
            let mut lock = self.data.write().await;
            lock.data = None;
            lock.status = Status::Error;
            lock.binary = None;
            lock.metadata = Metadata::from_error(e.clone());
        } else {
            async fn try_to_set_ready(assetref: AssetRef<impl Environment>) {
                eprintln!(
                    "Trying to set asset {} to ready - status {:?}",
                    assetref.id(),
                    assetref.status().await
                );
                let mut lock = assetref.data.write().await;
                if lock.data.is_some() {
                    if lock.is_volatile {
                        lock.status = Status::Volatile;
                        if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                            mr.status = Status::Volatile;
                            mr.is_volatile = true;
                        }
                    } else {
                        lock.status = Status::Ready;
                        if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                            mr.status = Status::Ready;
                        }
                    }
                } else {
                    lock.status = Status::Error;
                    let e = Error::unexpected_error(format!(
                        "Asset evaluation finished ({:?} status) but no data available",
                        lock.status
                    ));
                    if let Err(e) = lock.metadata.add_log_entry(LogEntry::from_error(&e)) {
                        eprintln!("!!!ERROR!!! Failed to add log entry: {}", e);
                    }
                }
            }
            match self.status().await {
                Status::None => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Recipe => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Submitted => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Dependencies => todo!(),
                Status::Processing => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Partial => {
                    try_to_set_ready(self.clone()).await;
                }
                Status::Error => {
                    let mut lock = self.data.write().await;
                    lock.data = None;
                    lock.binary = None;
                    let _ = lock.metadata.add_log_entry(LogEntry::error(
                        "Asset ended in error status after evaluation".to_string(),
                    ));
                }
                Status::Storing => {
                    let mut lock = self.data.write().await;
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(
                        "Asset ended in status 'Storing' after evaluation".to_string(),
                    ));
                }
                Status::Ready => {}
                Status::Directory => {}
                Status::Expired => {}
                Status::Cancelled => {}
                Status::Source => {}
                Status::Override => {}
                Status::Volatile => {}
            }
        }
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinished)
            .ok();
        {
            let mut lock = self.data.write().await;
            lock.notification_tx
                .send(AssetNotificationMessage::JobFinished)
                .ok();
        }
        result
    }

    async fn initial_state_and_recipe(&self) -> (State<E::Value>, Recipe) {
        let lock = self.data.read().await;
        (lock.initial_state.clone(), lock.recipe.clone())
    }

    pub async fn evaluate_recipe(&self) -> Result<State<E::Value>, Error> {
        let (input_state, recipe) = {
            let (input_state, recipe) = self.initial_state_and_recipe().await;
            if let Ok(Some(key)) = recipe.key() {
                let envref = self.get_envref().await;
                let manager = envref.get_asset_manager();
                let asset = manager.get(&key).await?;
                if asset.id() == self.id() {
                    let recipe = envref
                        .clone()
                        .get_recipe_provider()
                        .recipe(&key, envref)
                        .await?;
                    println!(
                        "Evaluating asset {} using its own recipe for key {}:\n{}\n",
                        self.id(),
                        key,
                        recipe
                    );
                    (input_state, recipe)
                } else {
                    println!(
                        "Delegating evaluation of asset {} to asset {} - FIXME",
                        self.id(),
                        asset.id()
                    );
                    // FIXME: !!! this should make sure that the recipe starts in the queue, otherwise it might lead to deadlock
                    return asset.get().await;
                }
            } else {
                (input_state, recipe)
            }
        };

        println!("Evaluating recipe {:?}", &recipe);
        let envref = self.get_envref().await;
        /*
        let plan = {
            let cmr = envref.0.get_command_metadata_registry();
            recipe.to_plan(cmr)?
        };
        */
        let context = Context::new(self.clone(), recipe.volatile).await; // TODO: reference to asset
                                                                         // TODO: Separate evaluation of dependencies
                                                                         //let res = apply_plan(plan, envref, context, input_state).await?;
                                                                         //let res = apply_plan_new(
                                                                         //    plan, input_state, context, envref).await?;
        println!("Applying recipe");
        let res = envref.apply_recipe(input_state, recipe, context).await?;
        println!("Recipe evaluated, result: {:?}", &res);

        let mut metadata = self.data.read().await.metadata.clone();
        if let Some(data) = self.data.read().await.data.as_ref() {
            metadata.with_type_identifier(data.identifier().to_string());
            metadata.with_type_name(data.type_name().to_string());
        }
        Ok(State {
            data: res,
            metadata: Arc::new(metadata),
        })
    }

    pub async fn evaluate_and_store(&self) -> Result<(), Error> {
        let res = self.evaluate_recipe().await;
        match res {
            Ok(State { data, metadata }) => {
                let mut lock = self.data.write().await;
                let mut metadata_clone = (*metadata).clone();
                metadata_clone
                    .with_type_identifier(data.identifier().to_string())
                    .with_type_name(data.type_name().to_string());
                lock.data = Some(data);
                lock.status = metadata_clone.status();
                lock.metadata = metadata_clone;
                match lock.status {
                    Status::None
                    | Status::Recipe
                    | Status::Submitted
                    | Status::Dependencies
                    | Status::Processing
                    | Status::Storing => {
                        // here is a value, so this is probably an old state - mark as ready or volatile
                        if lock.is_volatile {
                            lock.status = Status::Volatile;
                            if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                                mr.status = Status::Volatile;
                                mr.is_volatile = true;
                            }
                        } else {
                            lock.status = Status::Ready;
                            lock.metadata.set_status(Status::Ready)?;
                        }
                    }
                    Status::Ready => {}
                    Status::Partial => {}
                    Status::Error => {}
                    Status::Directory => {}
                    Status::Cancelled => {}
                    Status::Source => {}
                    Status::Expired => {}
                    Status::Override => {}
                    Status::Volatile => {}
                }
                let _ = lock
                    .notification_tx
                    .send(AssetNotificationMessage::ValueProduced);
                let save_in_background = lock.save_in_background;
                let cancelled = lock.is_cancelled();
                drop(lock);

                self.persist_with_status_tracking(save_in_background, cancelled)
                    .await;
                Ok(())
            }
            Err(e) => {
                println!("Error during evaluation of asset {}: {}", self.id(), e);
                let mut lock = self.data.write().await;
                lock.data = None;
                lock.metadata.with_error(e.clone());
                lock.status = Status::Error;
                lock.binary = None;
                let _ = lock
                    .notification_tx
                    .send(AssetNotificationMessage::ErrorOccurred(e.clone()));
                Err(e)
            }
        }
    }

    pub async fn evaluate_immediately(&self, payload: Option<E::Payload>) -> Result<(), Error> {
        let (input_state, recipe) = self.initial_state_and_recipe().await;

        let envref = self.get_envref().await;
        let mut context = Context::new(self.clone(), recipe.volatile).await;
        if let Some(payload) = payload {
            context.set_payload(payload);
        }
        let res = envref.apply_recipe(input_state, recipe, context).await?;

        let mut lock = self.data.write().await;
        lock.data = Some(res.clone());
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::ValueProduced);
        Ok(())
    }

    async fn save_to_store(&self) -> Result<(), Error> {
        // Check cancelled flag before writing to store (cancel-safety)
        // This prevents orphaned tasks from overwriting data after cancellation
        if self.is_cancelled().await {
            println!(
                "Asset {} cancelled, skipping store write in save_to_store",
                self.id()
            );
            return Ok(());
        }

        let mut x = self.poll_binary().await;
        if x.is_none() {
            x = self.serialize_to_binary().await?;
        }

        if let Some((data, metadata)) = x {
            let lock = self.data.read().await;

            // Double-check cancelled flag after potentially long serialization
            if lock.is_cancelled() {
                println!(
                    "Asset {} cancelled after serialization, skipping store write",
                    self.id()
                );
                return Ok(());
            }

            let envref = lock.get_envref();
            let store = envref.get_async_store();
            let key = lock.recipe.key()?.or(lock.recipe.store_to_key()?);
            drop(lock);
            if let Some(key) = key.as_ref() {
                store.set(key, &data, &metadata).await
            } else {
                Err(Error::general_error(format!(
                    "Cannot determine key to store asset - {}",
                    self.asset_reference().await
                )))
            }
        } else {
            Err(Error::unexpected_error(format!(
                "Failed to obtain binary value for storing of the asset - {}",
                self.asset_reference().await
            )))
        }
    }

    async fn save_metadata_to_store(&self) -> Result<(), Error> {
        let lock = self.data.read().await;
        lock.save_metadata_to_store().await
    }

    /*
    async fn save_metadata_to_store(&self) -> Result<(), Error> {
        let lock = self.data.read().await;

        let envref = lock.get_envref();
        let metadata = lock.metadata.clone();
        let store = envref.get_async_store();
        let key = lock.recipe.key()?.or(lock.recipe.store_to_key()?);
        drop(lock);
        if let Some(key) = key.as_ref() {
            store.set_metadata(key, &metadata).await
        } else {
            Err(Error::general_error(format!(
                "Cannot determine key to store asset metadata - {}",
                self.asset_reference().await
            )))
        }
    }
    */

    /// Serialize the asset's data into binary form
    /// Data format from the metadata is used
    /// This always serializes the asset, even when binary is available
    /// If data is not available, None is returned
    async fn serialize_to_binary(&self) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error> {
        if let Some(data) = self.poll_state().await {
            let binary = data.as_bytes()?;
            let mut lock = self.data.write().await;
            let arc_binary = Arc::new(binary);
            lock.binary = Some(arc_binary.clone());

            Ok(Some((arc_binary, data.metadata.clone())))
        } else {
            Ok(None)
        }
    }

    /// Subscribe to asset notifications.
    pub async fn subscribe_to_notifications(&self) -> watch::Receiver<AssetNotificationMessage> {
        let lock = self.data.read().await;
        lock.notification_tx.subscribe()
    }

    /// Get a clone of the service sender for internal control
    pub(crate) async fn service_sender(&self) -> mpsc::UnboundedSender<AssetServiceMessage> {
        let lock = self.data.read().await;
        lock.service_sender()
    }

    /// Cancel the asset processing.
    /// This method:
    /// 1. Checks if asset is being evaluated (Submitted, Dependencies, or Processing) - otherwise returns Ok
    /// 2. Sets cancelled = true on AssetData to prevent orphan writes
    /// 3. Sends Cancel message to the service channel
    /// 4. Waits (with timeout) for status to change to Cancelled or JobFinished on notification channel
    /// 5. Returns Ok even if timeout occurs (best-effort)
    pub async fn cancel(&self) -> Result<(), Error> {
        let status = self.status().await;

        // Check if asset is in a cancellable state
        match status {
            Status::Submitted | Status::Dependencies | Status::Processing | Status::Partial => {
                // Asset is being evaluated, proceed with cancellation
            }
            Status::None
            | Status::Directory
            | Status::Recipe
            | Status::Error
            | Status::Storing
            | Status::Ready
            | Status::Expired
            | Status::Cancelled
            | Status::Source
            | Status::Override
            | Status::Volatile => {
                // Already finished or not started, nothing to cancel
                return Ok(());
            }
        }

        // Set cancelled flag to prevent orphan writes
        {
            let mut lock = self.data.write().await;
            lock.set_cancelled(true);
        }

        // Send cancel message
        let service_sender = self.service_sender().await;
        let _ = service_sender.send(AssetServiceMessage::Cancel);

        // Wait for cancellation with timeout
        let mut rx = self.subscribe_to_notifications().await;
        let timeout = tokio::time::Duration::from_secs(5);

        let result = tokio::time::timeout(timeout, async {
            loop {
                let notification = rx.borrow().clone();
                match notification {
                    AssetNotificationMessage::JobFinished => {
                        return Ok(());
                    }
                    AssetNotificationMessage::StatusChanged(Status::Cancelled) => {
                        return Ok(());
                    }
                    _ => {}
                }
                if rx.changed().await.is_err() {
                    // Channel closed
                    return Ok(());
                }
            }
        })
        .await;

        // Return Ok even on timeout (best-effort cancellation)
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Ok(()), // Timeout, but still return Ok
        }
    }

    /// Check if the asset has been cancelled
    pub async fn is_cancelled(&self) -> bool {
        let lock = self.data.read().await;
        lock.is_cancelled()
    }

    /// Convert asset status to Override, preventing re-evaluation.
    /// Behavior depends on current status:
    /// - Directory, Source: No change (ignored)
    /// - None, Recipe, Submitted, Dependencies, Processing, Error, Cancelled:
    ///   Cancel if necessary, set value to Value::none(), set status to Override
    /// - Partial, Storing, Expired, Volatile, Ready:
    ///   Keep existing value, set status to Override
    pub async fn to_override(&self) -> Result<(), Error> {
        let mut data = self.data.write().await;

        match data.status {
            // Ignore these - no change
            Status::Directory | Status::Source => {
                // No-op
            }

            // In-progress or failed states: cancel, set to none value, mark Override
            Status::None
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Error
            | Status::Cancelled => {
                // Use existing cancel() method for in-flight evaluations
                // Drop the write lock before calling cancel() to avoid deadlock
                drop(data);

                // Cancel using AssetRef::cancel() method
                self.cancel().await?;

                // Re-acquire write lock to set Override state
                let mut data = self.data.write().await;

                data.data = Some(Arc::new(E::Value::none()));
                data.binary = None;
                data.status = Status::Override;
                if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                    mr.status = Status::Override;
                }
            }

            // States with data: keep value, mark Override
            Status::Partial
            | Status::Storing
            | Status::Expired
            | Status::Volatile
            | Status::Ready => {
                data.status = Status::Override;
                if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                    mr.status = Status::Override;
                }
            }

            // Already Override - no-op (idempotent)
            Status::Override => {
                // No-op
            }
        }

        Ok(())
    }

    /// Get the final state of the asset.
    /// This waits for the asset to be evaluated if necessary.
    /// It requires to call the [Self::run] method, which is done by the [AssetManager].
    /// If the asset is not running, the get may hang indefinitely.
    pub async fn get(&self) -> Result<State<E::Value>, Error> {
        if let Some(state) = self.poll_state().await {
            return Ok(state);
        }

        // Subscribe to notifications before starting evaluation
        let mut rx = self.subscribe_to_notifications().await;

        // Wait for either notifications or run completion

        loop {
            let notification = rx.borrow().clone();
            println!(
                "Getting asset {} state, current notification: {:?}",
                self.id(),
                notification
            );
            match notification {
                AssetNotificationMessage::ValueProduced => {
                    if let Some(state) = self.poll_state().await {
                        return Ok(state);
                    }
                }
                AssetNotificationMessage::ErrorOccurred(e) => {
                    return Err(e);
                }
                AssetNotificationMessage::Initial => {}
                AssetNotificationMessage::StatusChanged(_) => {}
                AssetNotificationMessage::PrimaryProgressUpdated(_) => {}
                AssetNotificationMessage::SecondaryProgressUpdated(_) => {}
                AssetNotificationMessage::LogMessage => {}
                AssetNotificationMessage::JobSubmitted => {}
                AssetNotificationMessage::JobStarted => {}
                AssetNotificationMessage::JobFinished => {
                    if let Some(state) = self.poll_state().await {
                        return Ok(state);
                    } else {
                        return Err(Error::unexpected_error(
                            "Asset finished but no data available".to_owned(),
                        ));
                    }
                }
            }
            rx.changed().await.map_err(|e| {
                Error::general_error(format!("Failed to receive notification: {}", e))
            })?;
        }
    }

    pub async fn get_binary(&self) -> Result<(Arc<Vec<u8>>, Arc<Metadata>), Error> {
        if let Some(b) = self.poll_binary().await {
            return Ok(b);
        }
        self.get().await?;
        if let Some(b) = self.poll_binary().await {
            Ok(b)
        } else {
            if let Some(b) = self.serialize_to_binary().await? {
                Ok(b)
            } else {
                Err(Error::unexpected_error("Failed to get binary".to_owned()))
            }
        }
    }

    pub async fn status(&self) -> Status {
        let lock = self.data.read().await;
        lock.status
    }

    pub async fn set_status(&self, status: Status) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        lock.set_status(status)
    }

    pub async fn poll_state(&self) -> Option<State<E::Value>> {
        let lock = self.data.read().await;
        lock.poll_state()
    }

    pub fn try_poll_state(&self) -> Option<State<E::Value>> {
        if let Ok(lock) = self.data.try_read() {
            lock.poll_state()
        } else {
            None
        }
    }

    pub async fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        let lock = self.data.read().await;
        lock.poll_binary()
    }

    pub fn try_poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        if let Ok(lock) = self.data.try_read() {
            lock.poll_binary()
        } else {
            None
        }
    }

    pub(crate) async fn set_value(&self, value: <E as Environment>::Value) -> Result<(), Error> {
        println!("Setting value for asset {}", self.id());
        let mut lock = self.data.write().await;
        lock.metadata
            .with_type_identifier(value.identifier().to_string());
        lock.metadata.with_type_name(value.type_name().to_string());
        lock.data = Some(Arc::new(value));
        lock.binary = None; // Invalidate binary
        if lock.is_volatile {
            lock.status = Status::Volatile;
            if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                mr.status = Status::Volatile;
                mr.is_volatile = true;
            }
        } else {
            lock.set_status(Status::Ready)?;
        }
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::ValueProduced);
        let save_in_background = lock.save_in_background;
        let cancelled = lock.is_cancelled();
        lock.service_sender()
            .send(AssetServiceMessage::JobFinishing)
            .map_err(|e| {
                Error::general_error(format!("Failed to send JobFinishing message: {}", e))
            })?;
        drop(lock);
        self.persist_with_status_tracking(save_in_background, cancelled)
            .await;
        Ok(())
    }

    /// Set the complete state of the asset
    /// This is not a public method and should not be used outside the core.
    /// Only certain assets are allowed to be set (overriden) by the user.
    /// Use AssetManager::set_state instead.
    pub(crate) async fn set_state(
        &self,
        state: State<<E as Environment>::Value>,
    ) -> Result<(), Error> {
        println!("Setting state for asset {}", self.id());
        let mut lock = self.data.write().await;
        let data = state.data.clone();
        lock.data = Some(data);
        let mut merged_metadata = (*state.metadata).clone();
        merged_metadata.with_type_identifier(state.data.identifier().to_string());
        merged_metadata.with_type_name(state.data.type_name().to_string());
        lock.metadata = merged_metadata;
        lock.binary = None; // Invalidate binary
        let status = lock.metadata.status();
        let save_in_background = lock.save_in_background;
        let cancelled = lock.is_cancelled();
        if status == Status::Ready {
            let _ = lock
                .notification_tx
                .send(AssetNotificationMessage::ValueProduced);
            lock.service_sender()
                .send(AssetServiceMessage::JobFinishing)
                .map_err(|e| {
                    Error::general_error(format!("Failed to send JobFinishing message: {}", e))
                })?;
        } else {
            let res = lock.set_status(status);
            if res.is_err() {
                eprintln!(
                    "WARNING: Asset {} set_state failed to set status: {}",
                    lock.id,
                    res.err().unwrap()
                );
            } else {
                eprintln!(
                    "WARNING: Asset {} set_state called with non-ready state, status set to {:?}",
                    lock.id, lock.status
                );
            }
        }
        drop(lock);
        self.persist_with_status_tracking(save_in_background, cancelled)
            .await;
        Ok(())
    }

    pub(crate) async fn set_error(&self, error: Error) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        lock.data = None;
        lock.metadata = Metadata::from_error(error.clone());
        lock.binary = None; // Invalidate binary
        lock.service_sender()
            .send(AssetServiceMessage::ErrorOccurred(error.clone()))
            .map_err(|e| {
                Error::general_error(format!(
                    "Failed to send ErrorOccurred message: {}\n{}",
                    e, error
                ))
            })?;
        Ok(())
    }
}

#[async_trait]
pub trait AssetManager<E: Environment>: Send + Sync {
    /// Get Asset for a query
    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error>;
    /// Get Asset they represents applying the recipe to the given value
    async fn apply(&self, recipe: Recipe, to: E::Value) -> Result<AssetRef<E>, Error>; // TODO: to probably should be a state, not a value
    async fn apply_immediately(
        &self,
        recipe: Recipe,
        to: E::Value, // TODO: to probably should be a state, not a value
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error>;
    /// Get Asset for a key
    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error>;
    /// Get Recipe for a key if the recipe exists
    async fn recipe_opt(&self, key: &Key) -> Result<Option<Recipe>, Error>;
    /// Check if resource is volatile
    async fn is_volatile(&self, key: &Key) -> Result<bool, Error>;
    async fn create(&self, key: &Key) -> Result<AssetRef<E>, Error>;

    /// Remove asset data from AssetManager and store.
    /// This does NOT trigger recalculation.
    async fn remove(&self, key: &Key) -> Result<(), Error>;

    /// Remove asset for a query (resolves to key first)
    async fn remove_asset(&self, query: &Query) -> Result<(), Error> {
        if let Some(key) = query.key() {
            self.remove(&key).await
        } else {
            Err(Error::general_error(format!(
                "Cannot remove asset for non-key query: {}",
                query
            )))
        }
    }

    /// Set binary data and metadata for a key asset.
    /// - Sets binary representation and clears any existing deserialized data
    /// - Store only: Does NOT create AssetRef in memory; writes directly to store
    /// - Status is determined by recipe existence (Source/Override) unless input status is Expired or Error
    async fn set(&self, key: &Key, binary: &[u8], metadata: MetadataRecord) -> Result<(), Error>;

    /// Set State (data + metadata) for a key asset.
    /// - Sets deserialized data and metadata from State
    /// - Memory + Store: Creates new AssetRef with State AND serializes to store
    /// - Supports non-serializable data (store metadata only if serialization fails)
    async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error>;

    /// Get asset info
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error>;

    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error>;

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error>;

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error>;

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error>;

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

    /// Return asset info of assets inside a directory specified by key.
    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error>;

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<AssetRef<E>, Error>;
}

pub struct DefaultAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    job_queue: Arc<JobQueue<E>>,
}

impl<E: Environment> Default for DefaultAssetManager<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Environment> DefaultAssetManager<E> {
    pub fn new() -> Self {
        let job_queue = Arc::new(JobQueue::new(2));
        let manager = DefaultAssetManager {
            id: std::sync::atomic::AtomicU64::new(1000),
            envref: std::sync::OnceLock::new(),
            assets: scc::HashMap::new(),
            query_assets: scc::HashMap::new(),
            //recipe_provider: std::sync::OnceLock::new(),
            job_queue: job_queue.clone(),
        };
        tokio::spawn(async move {
            println!("Spawned job queue");
            job_queue.run().await;
        });
        manager
    }
    pub fn next_id(&self) -> u64 {
        self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref
            .get()
            .expect("Environment not set in AssetStore")
            .clone()
    }

    pub fn set_envref(&self, envref: EnvRef<E>) {
        if self.envref.set(envref.clone()).is_err() {
            panic!("Environment already set in AssetStore");
        }
    }

    pub fn create_asset(&self, recipe: Recipe) -> AssetRef<E> {
        let asset = AssetRef::new_from_recipe(self.next_id(), recipe, self.get_envref());
        asset
    }

    pub fn create_dummy_asset(&self) -> AssetRef<E> {
        let recipe = Query::new().into();
        let asset = AssetRef::new_from_recipe(self.next_id(), recipe, self.get_envref());
        asset
    }

    pub fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<E>> {
        self.envref
            .get()
            .expect("Environment not set in AssetStore")
            .get_recipe_provider()
    }

    async fn get_nonvolatile_resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        eprintln!("Getting non-volatile asset for key {}", key);

        let entry = self
            .assets
            .entry_async(key.clone())
            .await
            .or_insert_with(|| {
                AssetRef::<E>::new_from_recipe(self.next_id(), key.into(), self.get_envref())
            });

        Ok(entry.get().clone())
    }

    async fn get_volatile_resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        eprintln!("Getting volatile asset for key {}", key);
        let asset_ref = AssetRef::new_from_recipe(self.next_id(), key.into(), self.get_envref());

        // Set is_volatile flag in AssetData and Metadata
        {
            let mut data = asset_ref.data.write().await;
            data.is_volatile = true;
            if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                mr.is_volatile = true;
            }
        }

        Ok(asset_ref)
    }

    async fn get_resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        if self.is_volatile(key).await? {
            self.get_volatile_resource_asset(key).await
        } else {
            self.get_nonvolatile_resource_asset(key).await
        }
    }

    async fn get_nonvolatile_query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        eprintln!("Getting non-volatile asset for query {}", query);

        let entry = self
            .query_assets
            .entry_async(query.clone())
            .await
            .or_insert_with(|| {
                AssetRef::<E>::new_from_recipe(self.next_id(), query.into(), self.get_envref())
            });

        Ok(entry.get().clone())
    }

    async fn get_volatile_query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        eprintln!("Getting volatile asset for query {}", query);
        let asset_ref = AssetRef::new_from_recipe(self.next_id(), query.into(), self.get_envref());

        // Set is_volatile flag in AssetData and Metadata
        {
            let mut data = asset_ref.data.write().await;
            data.is_volatile = true;
            if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                mr.is_volatile = true;
            }
        }

        Ok(asset_ref)
    }

    async fn get_query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if query.is_volatile(self.get_envref()).await? {
            self.get_volatile_query_asset(query).await
        } else {
            self.get_nonvolatile_query_asset(query).await
        }
    }
}

#[async_trait]
impl<E: Environment> AssetManager<E> for DefaultAssetManager<E> {
    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if let Some(key) = query.key() {
            self.get(&key).await
        } else {
            let assetref = self.get_query_asset(query).await?;
            if assetref.status().await.is_finished() {
                return Ok(assetref);
            }
            {
                let mut lock = assetref.data.write().await;
                if lock.try_fast_track().await? {
                    return Ok(assetref.clone());
                }
            }

            self.job_queue.submit(assetref.clone()).await?;
            Ok(assetref)
        }
    }
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> {
        if self.assets.contains_async(key).await {
            let assetref = self.get(key).await?;
            assetref.get_asset_info().await
        } else {
            let store = self.get_envref().get_async_store();
            println!(
                "Checking if store contains key {} {:?}",
                key,
                store.contains(key).await
            );
            if store.contains(key).await? {
                println!("Getting asset info from store for key {}", key);
                store.get_asset_info(key).await
            } else {
                let rp = self.get_recipe_provider();
                if rp.contains(key, self.get_envref()).await? {
                    rp.get_asset_info(key, self.get_envref()).await
                } else {
                    Err(Error::general_error(format!(
                        "Asset not found for key {} (get_asset_info)",
                        key
                    ))
                    .with_key(key))
                }
            }
        }
    }

    /// Create an ad-hoc asset applied to a value
    async fn apply(&self, recipe: Recipe, to: E::Value) -> Result<AssetRef<E>, Error> {
        let mut metadata = MetadataRecord::new();
        metadata.with_type_identifier(to.identifier().to_string());
        let initial_state = State::from_value_and_metadata(to, Arc::new(metadata.into()));
        let asset_ref =
            AssetData::new_ext(self.next_id(), recipe, initial_state, self.get_envref()).to_ref();
        // No fast track makes sense now, since apply can't be stored, however in the future
        // TODO: support fast-track once it makes sense
        self.job_queue.submit(asset_ref.clone()).await?;

        Ok(asset_ref)
    }

    /// Create an ad-hoc asset applied to a value
    async fn apply_immediately(
        &self,
        recipe: Recipe,
        to: E::Value,
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error> {
        let mut metadata = MetadataRecord::new();
        metadata.with_type_identifier(to.identifier().to_string());
        let initial_state = State::from_value_and_metadata(to, Arc::new(metadata.into()));
        let asset_ref =
            AssetData::new_ext(self.next_id(), recipe, initial_state, self.get_envref()).to_ref();
        // No fast track makes sense now, since apply can't be stored, however in the future
        // TODO: support fast-track once it makes sense
        asset_ref.run_immediately(payload).await?;

        Ok(asset_ref)
    }

    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        eprintln!("Getting asset for key {}", key);
        let asset_ref = self.get_resource_asset(key).await?;
        if asset_ref.status().await.is_finished() {
            return Ok(asset_ref);
        }
        {
            eprintln!("Trying fast track for asset with key {}", key);
            let asset_ref = asset_ref.clone();
            let mut lock = asset_ref.data.write().await;
            if lock.try_fast_track().await? {
                eprintln!("Fast track successful for asset with key {}", key);
                drop(lock);
                return Ok(asset_ref);
            }
        }
        eprintln!("Submitting asset with key {} to job queue", key);
        self.job_queue.submit(asset_ref.clone()).await?;

        Ok(asset_ref)
    }

    async fn recipe_opt(&self, key: &Key) -> Result<Option<Recipe>, Error> {
        self.get_recipe_provider()
            .recipe_opt(key, self.get_envref())
            .await
    }

    async fn is_volatile(&self, key: &Key) -> Result<bool, Error> {
        if let Some(recipe) = self.recipe_opt(key).await? {
            let env = self.get_envref();
            Ok(recipe.is_volatile(env).await?)
        } else {
            Ok(false)
        }
    }

    async fn create(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        // TODO: Probably should create a new asset and make it possible to set its value
        self.get(key).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        // 1. Check if asset exists in memory and cancel if processing
        if self.assets.contains_async(key).await {
            if let Some(asset_entry) = self.assets.get_async(key).await {
                let asset_ref = asset_entry.get().clone();
                drop(asset_entry);

                // Cancel if processing
                asset_ref.cancel().await?;
            }

            // Remove from assets map
            let _ = self.assets.remove_async(key).await;
        }

        // 2. Remove from store
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            store.remove(key).await?;
        }

        Ok(())
    }

    async fn set(
        &self,
        key: &Key,
        binary: &[u8],
        mut metadata: MetadataRecord,
    ) -> Result<(), Error> {
        fn validate_required_metadata_fields(
            key: &Key,
            metadata: &MetadataRecord,
        ) -> Result<(), Error> {
            if metadata.type_identifier.trim().is_empty() {
                return Err(Error::general_error(
                    "Metadata type_identifier must not be empty".to_string(),
                )
                .with_key(key));
            }
            if metadata.type_name.trim().is_empty() {
                return Err(Error::general_error(
                    "Metadata type_name must not be empty".to_string(),
                )
                .with_key(key));
            }
            Ok(())
        }

        fn add_soft_consistency_warnings(metadata: &mut MetadataRecord) {
            let effective_data_format = metadata.get_data_format();
            if let Some(extension) = metadata.extension() {
                if extension != effective_data_format {
                    metadata.add_log_entry(LogEntry::warning(format!(
                        "Filename extension '{extension}' differs from data_format '{effective_data_format}'"
                    )));
                }
            }

            let expected_media_type =
                crate::media_type::file_extension_to_media_type(&effective_data_format);
            if !metadata.media_type.trim().is_empty() && metadata.media_type != expected_media_type
            {
                metadata.add_log_entry(LogEntry::warning(format!(
                    "media_type '{}' differs from expected '{}' for data_format '{}'",
                    metadata.media_type, expected_media_type, effective_data_format
                )));
            } else if metadata.media_type.trim().is_empty() {
                metadata.media_type = expected_media_type.to_string();
            }
        }

        // 1. Cancel any existing processing asset for this key
        if self.assets.contains_async(key).await {
            if let Some(asset_entry) = self.assets.get_async(key).await {
                let asset_ref = asset_entry.get().clone();
                drop(asset_entry);

                // Cancel if processing
                asset_ref.cancel().await?;
            }

            // Remove from assets map (set() is store-only, no AssetRef created)
            let _ = self.assets.remove_async(key).await;
        }

        // 2. Determine status based on input status and recipe existence
        let input_status = metadata.status;
        let final_status = match input_status {
            Status::Expired => Status::Expired,
            Status::Error => Status::Error,
            Status::None
            | Status::Directory
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Storing
            | Status::Ready
            | Status::Cancelled
            | Status::Source
            | Status::Override
            | Status::Volatile => {
                // Check if recipe exists
                if self.recipe_opt(key).await?.is_some() {
                    Status::Override
                } else {
                    Status::Source
                }
            }
        };
        metadata.status = final_status;
        validate_required_metadata_fields(key, &metadata)?;
        add_soft_consistency_warnings(&mut metadata);

        // 3. Update timestamp and add log entry
        metadata.set_updated_now();
        metadata.add_log_entry(LogEntry::info("Data set externally".to_string()));

        // 4. Handle Error status specially - store empty binary with metadata
        let store = self.get_envref().get_async_store();
        if final_status == Status::Error {
            // Store empty binary with error metadata
            store.set(key, &[], &metadata.clone().into()).await?;
        } else {
            // Store binary and metadata
            store
                .set(key, binary, &metadata.clone().into())
                .await
                .map_err(|e| {
                    // On failure, try to clean up (best effort)
                    // Note: We can't do async cleanup in map_err, so just return the error
                    e
                })?;
        }

        Ok(())
    }

    async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error> {
        fn validate_required_metadata_fields(key: &Key, metadata: &Metadata) -> Result<(), Error> {
            if metadata.type_identifier()?.trim().is_empty() {
                return Err(Error::general_error(
                    "Metadata type_identifier must not be empty".to_string(),
                )
                .with_key(key));
            }
            if metadata.type_name()?.trim().is_empty() {
                return Err(Error::general_error(
                    "Metadata type_name must not be empty".to_string(),
                )
                .with_key(key));
            }
            Ok(())
        }

        fn add_soft_consistency_warnings(metadata: &mut Metadata) -> Result<(), Error> {
            let effective_data_format = metadata.get_data_format();
            if let Some(extension) = metadata.extension() {
                if extension != effective_data_format {
                    metadata.add_log_entry(LogEntry::warning(format!(
                        "Filename extension '{extension}' differs from data_format '{effective_data_format}'"
                    )))?;
                }
            }

            let expected_media_type =
                crate::media_type::file_extension_to_media_type(&effective_data_format);
            let media_type = metadata.get_media_type();
            if !media_type.trim().is_empty() && media_type != expected_media_type {
                metadata.add_log_entry(LogEntry::warning(format!(
                    "media_type '{}' differs from expected '{}' for data_format '{}'",
                    media_type, expected_media_type, effective_data_format
                )))?;
            }
            Ok(())
        }

        // 1. Cancel any existing processing asset for this key
        if self.assets.contains_async(key).await {
            if let Some(asset_entry) = self.assets.get_async(key).await {
                let asset_ref = asset_entry.get().clone();
                drop(asset_entry);

                // Cancel if processing
                asset_ref.cancel().await?;
            }

            // Remove from assets map
            let _ = self.assets.remove_async(key).await;
        }

        // 2. Determine status based on input status and recipe existence
        let input_status = state.metadata.status();
        let final_status = match input_status {
            Status::Expired => Status::Expired,
            Status::Error => Status::Error,
            Status::None
            | Status::Directory
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Storing
            | Status::Ready
            | Status::Cancelled
            | Status::Source
            | Status::Override
            | Status::Volatile => {
                // Check if recipe exists
                if self.recipe_opt(key).await?.is_some() {
                    Status::Override
                } else {
                    Status::Source
                }
            }
        };

        // 3. Create metadata record with updated status, timestamp, and log entry
        let mut metadata = state.metadata.as_ref().clone();
        metadata.set_status(final_status)?;
        validate_required_metadata_fields(key, &metadata)?;
        add_soft_consistency_warnings(&mut metadata)?;
        metadata.set_updated_now()?;
        metadata.add_log_entry(LogEntry::info("State set externally".to_string()))?;

        // 4. Create new AssetRef with the state
        let recipe: Recipe = key.into();
        let mut asset_data = AssetData::new_ext(
            self.next_id(),
            recipe,
            State::new(), // Empty initial state
            self.get_envref(),
        );
        asset_data.data = Some(Arc::new(state.data.as_ref().clone()));
        asset_data.metadata = metadata.clone();
        asset_data.status = final_status;
        asset_data.binary = None; // Clear binary, we have the data

        let asset_ref = asset_data.to_ref();

        // 5. Store in assets map
        let _ = self
            .assets
            .insert_async(key.clone(), asset_ref.clone())
            .await;

        // 6. Handle Error status specially - store empty binary with metadata
        let store = self.get_envref().get_async_store();
        if final_status == Status::Error {
            // Store empty binary with error metadata
            store.set(key, &[], &metadata.into()).await?;
        } else {
            // 7. Try to serialize and store (handle non-serializable gracefully)
            match state.as_bytes() {
                Ok(binary) => {
                    store.set(key, &binary, &metadata.into()).await?;
                }
                Err(_) => {
                    // Non-serializable data - store metadata only
                    store.set_metadata(key, &metadata.into()).await?;
                }
            }
        }

        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            return Ok(true);
        }
        self.get_recipe_provider()
            .contains(key, self.get_envref())
            .await
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.listdir_keys_deep(&Key::new()).await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let store = self.get_envref().get_async_store();
        let mut names = self
            .get_recipe_provider()
            .assets_with_recipes(key, self.get_envref())
            .await?
            .into_iter()
            .map(|resourcename| resourcename.name)
            .collect::<BTreeSet<String>>();
        store.listdir(key).await?.into_iter().for_each(|name| {
            names.insert(name);
        });

        Ok(names.into_iter().collect())
    }

    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        Ok(self
            .listdir(key)
            .await?
            .into_iter()
            .map(|name| key.join(name))
            .collect::<Vec<Key>>())
    }

    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let store = self.get_envref().get_async_store();

        let mut keys = store
            .listdir_keys_deep(key)
            .await?
            .into_iter()
            .collect::<BTreeSet<Key>>();
        let mut folders = vec![];
        for k in keys.iter() {
            if store.is_dir(k).await? {
                folders.push(k.clone());
            }
        }

        for subkey in folders {
            if store.is_dir(&subkey).await? {
                let recipes = self
                    .get_recipe_provider()
                    .assets_with_recipes(&subkey, self.get_envref())
                    .await?;
                for resourcename in recipes {
                    keys.insert(subkey.join(resourcename.name));
                }
            }
        }

        Ok(keys.into_iter().collect())
    }

    async fn makedir(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        let store = self.get_envref().get_async_store();
        let _sink = store.makedir(key).await?;
        let asset = self.get(key).await?;
        Ok(asset)
    }
}

/// The job queue structure
pub struct JobQueue<E: Environment> {
    jobs: Arc<Mutex<Vec<AssetRef<E>>>>,
    running_count: Arc<AtomicUsize>,
    capacity: usize,
}

impl<E: Environment + 'static> JobQueue<E> {
    /// Create a new job queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        println!("Creating job queue with capacity {}", capacity);
        JobQueue {
            jobs: Arc::new(Mutex::new(Vec::new())),
            running_count: Arc::new(AtomicUsize::new(0)),
            capacity,
        }
    }

    /// Get current number of running jobs
    pub fn running_count(&self) -> usize {
        self.running_count.load(Ordering::SeqCst)
    }

    /// Submit an asset for processing
    pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
        let asset_id = asset.id();

        // Check for duplicates and add to queue atomically
        {
            let mut jobs = self.jobs.lock().await;
            if jobs.iter().any(|a| a.id() == asset_id) {
                // Asset already in queue, don't add duplicate
                eprintln!("Asset {} already in queue, skipping", asset_id);
                return Ok(());
            }
            // Add to jobs list for tracking
            jobs.push(asset.clone());
        }

        // Check if we can run immediately using atomic counter
        let current_running = self.running_count.load(Ordering::SeqCst);
        if current_running < self.capacity {
            // Try to increment running count
            // Use compare_exchange to avoid race conditions
            if self
                .running_count
                .compare_exchange(
                    current_running,
                    current_running + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                // Successfully reserved a slot - run immediately
                let asset_clone = asset.clone();
                let running_count = self.running_count.clone();

                // Status set directly, since message processing is not running yet
                if let Err(e) = asset_clone.set_status(Status::Processing).await {
                    eprintln!("Failed to set status for asset {}: {}", asset_id, e);
                    // Decrement counter since we won't actually run
                    running_count.fetch_sub(1, Ordering::SeqCst);
                    return Err(e);
                }

                eprintln!(
                    "Starting asset job {} immediately (running: {})",
                    asset_id,
                    current_running + 1
                );
                tokio::spawn(async move {
                    let _ = asset_clone.run().await;
                    // Decrement running count when job finishes
                    running_count.fetch_sub(1, Ordering::SeqCst);
                    eprintln!("Asset job {} finished", asset_clone.id());
                });
                return Ok(());
            }
        }

        // At capacity or lost race - queue the job with Submitted status
        asset.submitted().await?;
        eprintln!(
            "Asset {} queued (running: {}, capacity: {})",
            asset_id,
            self.running_count(),
            self.capacity
        );

        Ok(())
    }

    /// Count how many jobs are currently running (from atomic counter)
    pub fn pending_jobs_count_sync(&self) -> usize {
        self.running_count.load(Ordering::SeqCst)
    }

    /// Count how many jobs are in the queue (any status)
    pub async fn queued_jobs_count(&self) -> usize {
        let jobs = self.jobs.lock().await;
        jobs.len()
    }

    /// Count how many jobs are waiting (Submitted status)
    pub async fn waiting_jobs_count(&self) -> usize {
        let jobs = self.jobs.lock().await;
        let mut count = 0;
        for asset in jobs.iter() {
            if asset.status().await == Status::Submitted {
                count += 1;
            }
        }
        count
    }

    /// Start processing jobs up to capacity
    pub async fn run(self: Arc<Self>) {
        eprintln!("Starting job queue");
        let mut cleanup_counter = 0;
        loop {
            let current_running = self.running_count.load(Ordering::SeqCst);

            // Check if we can start more jobs
            if current_running < self.capacity {
                let available_slots = self.capacity - current_running;
                let mut jobs_to_start = Vec::new();

                // Find submitted jobs
                {
                    let jobs = self.jobs.lock().await;
                    for asset in jobs.iter() {
                        if jobs_to_start.len() >= available_slots {
                            break;
                        }

                        if asset.status().await == Status::Submitted {
                            jobs_to_start.push(asset.clone());
                        }
                    }
                }

                // Start jobs
                for asset in jobs_to_start {
                    // Try to reserve a slot
                    let current = self.running_count.load(Ordering::SeqCst);
                    if current >= self.capacity {
                        break; // No more slots available
                    }

                    if self
                        .running_count
                        .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let asset_clone = asset.clone();
                        let running_count = self.running_count.clone();

                        // Status set directly, since message processing is not running yet
                        if let Err(e) = asset_clone.set_status(Status::Processing).await {
                            eprintln!("Failed to set status for asset {}: {}", asset.id(), e);
                            running_count.fetch_sub(1, Ordering::SeqCst);
                            continue;
                        }

                        eprintln!(
                            "Starting asset job {} from queue (running: {})",
                            asset.id(),
                            current + 1
                        );
                        tokio::spawn(async move {
                            let _ = asset_clone.run().await;
                            running_count.fetch_sub(1, Ordering::SeqCst);
                            eprintln!("Asset job {} finished", asset_clone.id());
                        });
                    }
                }
            }

            // Periodic cleanup of finished jobs
            cleanup_counter += 1;
            if cleanup_counter >= 50 {
                // Every 5 seconds (50 * 100ms)
                cleanup_counter = 0;
                let removed = self.cleanup_completed_internal().await;
                if removed > 0 {
                    eprintln!("Cleaned up {} finished jobs", removed);
                }
            }

            // Sleep briefly to avoid busy waiting
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Internal cleanup method that doesn't require &mut self
    async fn cleanup_completed_internal(&self) -> usize {
        let mut jobs = self.jobs.lock().await;
        let initial_count = jobs.len();

        let mut keep: Vec<AssetRef<E>> = Vec::new();
        for asset in jobs.iter() {
            let status = asset.status().await;
            if !status.is_finished() {
                keep.push(asset.clone());
            }
        }

        let removed = initial_count - keep.len();
        *jobs = keep;
        removed
    }

    /// Clean up completed jobs (Ready or Error status)
    /// Returns the number of jobs removed
    pub async fn cleanup_completed(&self) -> usize {
        self.cleanup_completed_internal().await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::context::{SimpleEnvironment, SimpleEnvironmentWithPayload};
    use crate::metadata::{Metadata, MetadataRecord};
    use crate::parse::{parse_key, parse_query};
    use crate::query::Key;
    use crate::store::{AsyncStoreWrapper, MemoryStore, Store};
    use crate::value::{Value, ValueInterface};

    struct FailingSetStore;

    impl Store for FailingSetStore {
        fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
            Err(Error::key_write_error(
                key,
                "FailingSetStore",
                "intentional store set failure",
            ))
        }

        fn set_metadata(&self, _key: &Key, _metadata: &Metadata) -> Result<(), Error> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_asset_data_basics() {
        let dummy_env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = parse_key("test.txt").unwrap();
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, key.into(), dummy_env.to_ref());
        let state = asset_data.poll_state();
        assert!(state.is_none());
        let bin = asset_data.poll_binary();
        assert!(bin.is_none());
    }

    #[tokio::test]
    async fn test_asset_loading() {
        let key = parse_key("test.txt").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        env.get_async_store()
            .set(
                &key,
                b"Hello, world!",
                &Metadata::MetadataRecord(
                    MetadataRecord::new()
                        .with_key(key.clone())
                        .with_type_identifier("text".to_owned())
                        .with_status(Status::Source)
                        .clone(),
                ),
            )
            .await
            .unwrap();

        let envref = env.to_ref();

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, key.into(), envref.clone());

        let state = asset_data.poll_state();
        assert!(state.is_none());
        let bin = asset_data.poll_binary();
        assert!(bin.is_none());
        asset_data.try_fast_track().await.unwrap();
        let state = asset_data.poll_state();
        assert!(state.is_some());
        let bin = asset_data.poll_binary();
        assert!(bin.is_some());
        assert_eq!(bin.unwrap().0.as_ref(), b"Hello, world!");
        assert_eq!(
            state.unwrap().data.try_into_string().unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_asset_evaluate_and_store() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());

        let state = asset_data.poll_state();
        assert!(state.is_none());
        let bin = asset_data.poll_binary();
        assert!(bin.is_none());
        asset_data.try_fast_track().await.unwrap();
        let assetref = asset_data.to_ref();
        let state = assetref.poll_state().await;
        assert!(state.is_none());
        let bin = assetref.poll_binary().await;
        assert!(bin.is_none());
        assetref.evaluate_and_store().await.unwrap();

        let state = assetref.poll_state().await;
        assert!(state.is_some());
        assert_eq!(
            state.unwrap().data.try_into_string().unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_asset_run() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();

        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());

        let assetref = asset_data.to_ref();
        assetref.run().await.unwrap();

        let state = assetref.poll_state().await;
        assert!(state.is_some());
        assert_eq!(
            state.unwrap().data.try_into_string().unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_late_progress_messages_are_ignored_after_finish() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(4321, query.into(), envref.clone());
        let assetref = asset_data.to_ref();

        assetref.run().await.unwrap();

        let metadata_before = assetref.get_metadata().await.unwrap();
        assert!(metadata_before.primary_progress().is_done());

        let psm_asset = assetref.clone();
        let psm = tokio::spawn(async move { psm_asset.process_service_messages().await });
        let sender = assetref.service_sender().await;
        sender
            .send(AssetServiceMessage::UpdatePrimaryProgress(
                ProgressEntry::tick("late progress".to_string()),
            ))
            .unwrap();
        sender.send(AssetServiceMessage::JobFinishing).unwrap();
        psm.await.unwrap().unwrap();

        let metadata_after = assetref.get_metadata().await.unwrap();
        assert!(metadata_after.primary_progress().is_done());
        assert_ne!(metadata_after.primary_progress().message, "late progress");
    }

    #[tokio::test]
    async fn test_asset_get_state() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();

        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());

        let assetref = asset_data.to_ref();
        assert!(assetref.poll_state().await.is_none());

        let handle = tokio::spawn({
            let assetref = assetref.clone();
            async move { assetref.get().await }
        });
        eprintln!("Waiting for asset to run");
        assetref.run().await.unwrap();
        eprintln!("run completed");

        let result = handle.await.unwrap().unwrap().try_into_string().unwrap();
        assert_eq!(result, "Hello, world!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, world!");
    }

    #[tokio::test]
    async fn test_asset_storage() {
        let query = parse_query("test/test.txt").unwrap();
        let mut recipe: Recipe = query.into();
        recipe.cwd = Some("a/b".to_owned());
        let store_key = recipe.store_to_key().unwrap().unwrap();
        assert_eq!(store_key.to_string(), "a/b/test.txt");

        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, recipe, envref.clone());
        asset_data.save_in_background = false; // Save synchronously for the test

        let assetref = asset_data.to_ref();

        assetref.run().await.unwrap();

        let result = assetref.get().await.unwrap().try_into_string().unwrap();
        assert_eq!(result, "Hello, world!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, world!");

        let store = envref.get_async_store();
        let contains = store.contains(&store_key).await.unwrap();
        println!("store_key: {}", store_key);
        println!("Store keys: {:?}", store.keys().await.unwrap());
        assert!(contains);
        let (data, _metadata) = store.get(&store_key).await.unwrap();
        assert_eq!(data, b"Hello, world!");
    }

    #[tokio::test]
    async fn test_asset_manager_get_state() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let assetref = envref.get_asset_manager().get_asset(&query).await.unwrap();

        let result = assetref.get().await.unwrap().try_into_string().unwrap();
        assert_eq!(result, "Hello, world!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, world!");
    }

    #[tokio::test]
    async fn test_apply() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |s, _, _| {
                let txt = s.try_into_string()?;
                Ok(Value::from(format!("Hello, {txt}!")))
            })
            .expect("register_command failed");

        let envref = env.to_ref();
        let assetref = envref
            .get_asset_manager()
            .apply(query.into(), "WORLD".into())
            .await
            .unwrap();

        let result = assetref.get().await.unwrap().try_into_string().unwrap();
        assert_eq!(result, "Hello, WORLD!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, WORLD!");
    }

    #[tokio::test]
    async fn test_apply_immediately() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |s, _, _| {
                let txt = s.try_into_string()?;
                Ok(Value::from(format!("Hello, {txt}!")))
            })
            .expect("register_command failed");

        let envref = env.to_ref();
        let assetref = envref
            .get_asset_manager()
            .apply_immediately(query.into(), "WORLD".into(), None)
            .await
            .unwrap();

        let result = assetref
            .poll_state()
            .await
            .unwrap()
            .try_into_string()
            .unwrap();
        assert_eq!(result, "Hello, WORLD!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, WORLD!");
    }

    #[tokio::test]
    async fn test_evaluate_with_recipe() {
        use crate::command_metadata::CommandKey;
        use crate::context::{Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::recipes::{DefaultRecipeProvider, Recipe, RecipeList};
        use crate::store::{AsyncStoreWrapper, MemoryStore, Store};
        use crate::value::Value;

        // 1. Create a recipe with a query "hello/hello.txt"
        let recipe = Recipe::new(
            "hello/hello.txt".to_string(),
            "Test Hello Recipe".to_string(),
            "A hello recipe".to_string(),
        )
        .unwrap();

        // 2. Add recipe to a RecipeList and serialize to YAML
        let mut recipe_list = RecipeList::new();
        recipe_list.add_recipe(recipe);
        let yaml_content = serde_yaml::to_string(&recipe_list).unwrap();

        // 3. Set it into memory store under key test/recipes.yaml
        let recipes_key = parse_key("test/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        let memory_store = MemoryStore::new(&parse_key("").unwrap());
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .unwrap();

        // 4. Set the memory store in environment wrapped as AsyncStore
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(memory_store)));

        // 5. Set DefaultAssetProvider as the asset provider for env
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));

        // 6. Register a command hello returning "Hello, world!"
        let key = CommandKey::new_name("hello");
        env.command_registry
            .register_command(key.clone(), |_, _, _| {
                std::thread::sleep(Duration::from_millis(1000));
                Ok(Value::from("Hello, world!"))
            })
            .expect("register_command failed");

        // 7. Evaluate a query "-R/test/hello.txt"
        let envref = env.to_ref();
        let asset1 = envref.evaluate("-R/test/hello.txt").await.unwrap();
        let asset2 = envref.evaluate("-R/test/hello.txt").await.unwrap();
        let state1 = asset1.get().await.expect("Failed to get asset state");

        // 8. Check the result
        let value = state1.try_into_string().unwrap();
        assert_eq!(value, "Hello, world!");
        assert!(!state1.is_error().unwrap());

        // 9. Check the result again to ensure caching works
        let state2 = asset2.get().await.expect("Failed to get asset state");
        let value = state2.try_into_string().unwrap();
        assert_eq!(value, "Hello, world!");
        assert!(!state2.is_error().unwrap());
    }

    #[tokio::test]
    async fn test_apply_immediately_with_payload() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironmentWithPayload<Value, String> =
            SimpleEnvironmentWithPayload::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |s, _arg, context| {
                let txt = s.try_into_string()?;
                let payload = context.get_payload_clone().unwrap();
                Ok(Value::from(format!("{payload}, {txt}!")))
            })
            .expect("register_command failed");

        let envref = env.to_ref();
        let assetref = envref
            .get_asset_manager()
            .apply_immediately(query.into(), "WORLD".into(), Some("Hi".to_owned()))
            .await
            .unwrap();

        let result = assetref
            .poll_state()
            .await
            .unwrap()
            .try_into_string()
            .unwrap();
        assert_eq!(result, "Hi, WORLD!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b, _) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hi, WORLD!");
    }

    #[tokio::test]
    async fn test_asset_log() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, ctx| {
                println!("HELLO from test");
                ctx.info("Hello from test")?;
                Ok(Value::from("Hello, world!"))
            })
            .expect("register_command failed");

        let envref = env.to_ref();
        let assetref = envref.get_asset_manager().get_asset(&query).await.unwrap();

        let result = assetref.get().await.unwrap().try_into_string().unwrap();
        let metadata = assetref.get().await.unwrap().metadata;
        assert_eq!(result, "Hello, world!");
        assert_eq!(assetref.status().await, Status::Ready);
        if let Metadata::MetadataRecord(meta) = &*metadata {
            let log_entry = meta
                .log
                .iter()
                .find(|entry| entry.message == "Hello from test");
            assert!(log_entry.is_some());
        } else {
            panic!("Expected MetadataRecord");
        }
    }

    // ============ JobQueue Tests ============

    #[tokio::test]
    async fn test_jobqueue_new() {
        let queue = JobQueue::<SimpleEnvironment<Value>>::new(4);
        assert_eq!(queue.capacity, 4);
        assert_eq!(queue.running_count(), 0);
        assert_eq!(queue.queued_jobs_count().await, 0);
    }

    #[tokio::test]
    async fn test_jobqueue_submit_no_duplicates() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| {
                // Simulate slow command
                std::thread::sleep(Duration::from_millis(500));
                Ok(Value::from("Hello, world!"))
            })
            .expect("register_command failed");

        let envref = env.to_ref();

        // Create a queue with high capacity so jobs run immediately
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(10));

        // Create one asset
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());
        let assetref = asset_data.to_ref();

        // Submit the same asset twice
        queue.submit(assetref.clone()).await.unwrap();
        queue.submit(assetref.clone()).await.unwrap();

        // Should only be one job in the queue
        assert_eq!(queue.queued_jobs_count().await, 1);
    }

    #[tokio::test]
    async fn test_jobqueue_submit_respects_capacity() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("slow");
        env.command_registry
            .register_command(key.clone(), |_, _, _| {
                // Simulate slow command
                std::thread::sleep(Duration::from_millis(1000));
                Ok(Value::from("Done"))
            })
            .expect("register_command failed");

        let envref = env.to_ref();

        // Create a queue with capacity 2
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(2));

        // Create and submit 4 assets
        let mut assets = Vec::new();
        for i in 0..4 {
            let query = parse_query("slow").unwrap();
            let asset_data =
                AssetData::<SimpleEnvironment<Value>>::new(i, query.into(), envref.clone());
            let assetref = asset_data.to_ref();
            assets.push(assetref.clone());
            queue.submit(assetref).await.unwrap();
        }

        // Give some time for jobs to start
        sleep(Duration::from_millis(50)).await;

        // Should have 2 running and 2 submitted
        let running = queue.running_count();
        assert!(running <= 2, "Running count {} should be <= 2", running);

        // Check that submitted jobs have Submitted status
        let mut submitted_count = 0;
        let mut processing_count = 0;
        for asset in &assets {
            let status = asset.status().await;
            if status == Status::Submitted {
                submitted_count += 1;
            } else if status == Status::Processing {
                processing_count += 1;
            }
        }

        // At least some should be submitted (queued)
        assert!(
            submitted_count >= 2 || processing_count <= 2,
            "With capacity 2, at most 2 should be processing. Got {} processing, {} submitted",
            processing_count,
            submitted_count
        );
    }

    #[tokio::test]
    async fn test_jobqueue_submit_immediate_when_capacity() {
        let query = parse_query("fast").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("fast");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Fast result")))
            .expect("register_command failed");

        let envref = env.to_ref();

        // Create a queue with capacity 5
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(5));

        // Initially running count should be 0
        assert_eq!(queue.running_count(), 0);

        // Submit one asset
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());
        let assetref = asset_data.to_ref();
        queue.submit(assetref.clone()).await.unwrap();

        // Give some time for the job to start
        sleep(Duration::from_millis(50)).await;

        // Status should be Processing or Ready (if already finished)
        let status = assetref.status().await;
        assert!(
            status == Status::Processing || status == Status::Ready,
            "Expected Processing or Ready, got {:?}",
            status
        );
    }

    #[tokio::test]
    async fn test_jobqueue_cleanup_removes_finished() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("quick");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Quick result")))
            .expect("register_command failed");

        let envref = env.to_ref();

        // Create a queue
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(10));

        // Submit some assets
        for i in 0..3 {
            let query = parse_query("quick").unwrap();
            let asset_data =
                AssetData::<SimpleEnvironment<Value>>::new(i, query.into(), envref.clone());
            let assetref = asset_data.to_ref();
            queue.submit(assetref).await.unwrap();
        }

        // Wait for jobs to complete
        sleep(Duration::from_millis(500)).await;

        // Should have 3 jobs in the queue
        let count_before = queue.queued_jobs_count().await;
        assert_eq!(count_before, 3);

        // Cleanup
        let removed = queue.cleanup_completed().await;

        // All 3 should have been removed (they should be Ready by now)
        assert_eq!(
            removed, 3,
            "Expected 3 jobs to be cleaned up, got {}",
            removed
        );
        assert_eq!(queue.queued_jobs_count().await, 0);
    }

    #[tokio::test]
    async fn test_jobqueue_running_count_decrements() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("medium");
        env.command_registry
            .register_command(key.clone(), |_, _, _| {
                // Simulate medium-length command (longer to ensure we catch it running)
                std::thread::sleep(Duration::from_millis(500));
                Ok(Value::from("Medium result"))
            })
            .expect("register_command failed");

        let envref = env.to_ref();

        // Create a queue with capacity 5
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(5));

        // Submit one asset
        let query = parse_query("medium").unwrap();
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1234, query.into(), envref.clone());
        let assetref = asset_data.to_ref();

        // Check running count immediately after submit
        queue.submit(assetref.clone()).await.unwrap();

        // The submit call increments the counter synchronously before spawning
        // So we should see 1 running immediately (or the job may have finished already)
        // Give a tiny bit of time for spawn to start
        sleep(Duration::from_millis(10)).await;

        // Should have 1 running (unless job already finished, which is unlikely with 500ms sleep)
        let running_during = queue.running_count();
        assert!(
            running_during <= 1,
            "Expected at most 1 running job, got {}",
            running_during
        );

        // If the job is still running, verify it
        if running_during == 1 {
            // Wait for job to complete
            sleep(Duration::from_millis(600)).await;

            // Running count should be back to 0
            let running_after = queue.running_count();
            assert_eq!(running_after, 0, "Expected 0 running jobs after completion");
        }

        // Wait for asset to finish (in case timing was off)
        assetref.get().await.ok();

        // Asset should be Ready
        assert_eq!(assetref.status().await, Status::Ready);
    }

    #[tokio::test]
    async fn test_set_without_recipe() {
        // Set binary data on a key without a recipe - should become Source
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let key = parse_key("test/set_source").unwrap();
        let binary = b"test data".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());

        let manager = envref.get_asset_manager();
        manager.set(&key, &binary, metadata).await.unwrap();

        // Check the data was stored correctly
        let store = envref.get_async_store();
        assert!(store.contains(&key).await.unwrap());

        let (stored_binary, stored_metadata) = store.get(&key).await.unwrap();
        assert_eq!(stored_binary, binary);
        assert_eq!(stored_metadata.status(), Status::Source);
    }

    #[tokio::test]
    async fn test_set_with_expired_status() {
        // Set with Expired status - should preserve Expired
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let key = parse_key("test/set_expired").unwrap();
        let binary = b"expired data".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());
        metadata.status = Status::Expired;

        let manager = envref.get_asset_manager();
        manager.set(&key, &binary, metadata).await.unwrap();

        let store = envref.get_async_store();
        let (_, stored_metadata) = store.get(&key).await.unwrap();
        assert_eq!(stored_metadata.status(), Status::Expired);
    }

    #[tokio::test]
    async fn test_set_with_error_status() {
        // Set with Error status - should preserve Error and not store binary
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let key = parse_key("test/set_error").unwrap();
        let binary = b"this should not be stored".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());
        metadata.status = Status::Error;
        metadata.message = "Test error".to_string();

        let manager = envref.get_asset_manager();
        manager.set(&key, &binary, metadata).await.unwrap();

        let store = envref.get_async_store();
        // For error status, empty binary is stored with metadata
        let (stored_binary, stored_metadata) = store.get(&key).await.unwrap();
        assert_eq!(stored_metadata.status(), Status::Error);
        assert!(stored_binary.is_empty());
    }

    #[tokio::test]
    async fn test_set_state_without_recipe() {
        // Set state on a key without a recipe - should become Source
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let key = parse_key("test/set_state_source").unwrap();
        let value = Value::from("test state value");
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = value.identifier().to_string();
        metadata.type_name = value.type_name().to_string();
        metadata.data_format = Some("txt".to_string());
        let state = State::from_value_and_metadata(value, Arc::new(metadata.into()));

        let manager = envref.get_asset_manager();
        manager.set_state(&key, state).await.unwrap();

        // Check the asset is in memory with correct status
        let asset = manager.get(&key).await.unwrap();
        assert_eq!(asset.status().await, Status::Source);

        // Should also be in store
        let store = envref.get_async_store();
        assert!(store.contains(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_asset() {
        // First set an asset, then remove it
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let key = parse_key("test/to_remove").unwrap();
        let binary = b"to be removed".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());

        let manager = envref.get_asset_manager();
        manager.set(&key, &binary, metadata).await.unwrap();

        // Verify it exists
        let store = envref.get_async_store();
        assert!(store.contains(&key).await.unwrap());

        // Remove it
        manager.remove(&key).await.unwrap();

        // Verify it's gone from store
        assert!(!store.contains(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_override_status() {
        // Test that Override status has correct properties
        assert!(Status::Override.has_data());
        assert!(Status::Override.is_finished());
        assert!(!Status::Override.is_processing());
        assert!(!Status::Override.can_have_tracked_dependencies());
    }

    #[tokio::test]
    async fn test_evaluate_store_failure_keeps_value_and_sets_warning() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(FailingSetStore)));
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key, |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let query = parse_query("test/out.txt").unwrap();
        let mut recipe: Recipe = query.into();
        recipe.cwd = Some("a/b".to_string());

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(9001, recipe, envref.clone());
        asset_data.save_in_background = false;
        let assetref = asset_data.to_ref();

        assetref.run().await.unwrap();
        let state = assetref.get().await.unwrap();
        assert_eq!(state.try_into_string().unwrap(), "Hello, world!");
        assert_eq!(
            assetref.persistence_status().await,
            PersistenceStatus::NotPersisted
        );

        let metadata = assetref.get_metadata().await.unwrap();
        if let Metadata::MetadataRecord(meta) = metadata {
            let warning = meta.log.iter().find(|entry| {
                entry.message.contains("Persistence status NotPersisted")
                    && entry.message.contains("intentional store set failure")
            });
            assert!(
                warning.is_some(),
                "Expected persistence warning with complete error details"
            );
        } else {
            panic!("Expected MetadataRecord");
        }
    }

    #[tokio::test]
    async fn test_evaluate_missing_store_key_sets_warning_and_returns_value() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key, |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let query = parse_query("test").unwrap();
        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(9002, query.into(), envref.clone());
        asset_data.save_in_background = false;
        let assetref = asset_data.to_ref();

        assetref.run().await.unwrap();
        let state = assetref.get().await.unwrap();
        assert_eq!(state.try_into_string().unwrap(), "Hello, world!");
        assert_eq!(
            assetref.persistence_status().await,
            PersistenceStatus::NotPersisted
        );

        let metadata = assetref.get_metadata().await.unwrap();
        if let Metadata::MetadataRecord(meta) = metadata {
            let warning = meta.log.iter().find(|entry| {
                entry.message.contains("Persistence status NotPersisted")
                    && entry
                        .message
                        .contains("Cannot determine key to store asset")
            });
            assert!(
                warning.is_some(),
                "Expected warning for missing store key persistence failure"
            );
        } else {
            panic!("Expected MetadataRecord");
        }
    }

    #[tokio::test]
    async fn test_set_value_persists_when_possible_and_marks_persisted() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncStoreWrapper(MemoryStore::new(&Key::new()))));
        let envref = env.to_ref();

        let query = parse_query("dummy/value.txt").unwrap();
        let mut recipe: Recipe = query.into();
        recipe.cwd = Some("persist".to_owned());
        let store_key = recipe.store_to_key().unwrap().unwrap();

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(9003, recipe, envref.clone());
        asset_data.save_in_background = false;
        let assetref = asset_data.to_ref();

        assetref.set_value(Value::from("Persist me")).await.unwrap();

        assert_eq!(
            assetref.persistence_status().await,
            PersistenceStatus::Persisted
        );
        let store = envref.get_async_store();
        assert!(store.contains(&store_key).await.unwrap());
        let (data, _) = store.get(&store_key).await.unwrap();
        assert_eq!(data, b"Persist me");
    }
}
