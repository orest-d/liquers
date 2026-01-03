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

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use futures::lock;
use scc;
use tokio::sync::{mpsc, watch, Mutex, RwLock};

use crate::context::Context;
use crate::interpreter::IsVolatile;
use crate::metadata::{AssetInfo, LogEntry, ProgressEntry};
use crate::value::ValueInterface;
use crate::{
    context::{EnvRef, Environment},
    error::Error,
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
        let assetinfo = recipe
            .get_asset_info()
            .unwrap_or_else(|_| AssetInfo::default());

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
        let key = self.recipe.store_to_key()?;
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
                        Status::Ready | Status::Source => {
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
                        _ => {
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
                Some(State {
                    data: Arc::new(E::Value::none()),
                    metadata: Arc::new(self.metadata.clone()),
                })
            },
            Status::Recipe => None,
            Status::Submitted => None,
            Status::Dependencies => None,
            Status::Processing => None,
            Status::Partial => None,
            Status::Error | Status::Cancelled  => {
                Some(State {
                    data: Arc::new(E::Value::none()),
                    metadata: Arc::new(self.metadata.clone()),
                })
            }
            Status::Storing => None,
            Status::Ready | Status::Expired | Status::Source=> {
                if let Some(data) = &self.data {
                    Some(State {
                        data: data.clone(),
                        metadata: Arc::new(self.metadata.clone()),
                    })
                } else {
                    None
                }
            },
        }
    }

    /// Poll the current binary data and metadata without any async operations.
    /// Returns None if binary or metadata is not available.
    pub fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        self.binary
            .clone()
            .zip(Some(Arc::new(self.metadata.clone())))
    }

    /// Reset the asset data, binary and metadata.
    /// Status is set to None.
    pub fn reset(&mut self) {
        self.data = None;
        self.binary = None;
        self.metadata = Metadata::new().into();
        self.status = Status::None;
        self.notification_tx
            .send(AssetNotificationMessage::Initial)
            .ok();
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
        Context::new(self.clone()).await
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
                    panic!("JobFinished message not expected in process_service_messages");
                    //                    let _ = notification_tx.send(AssetNotificationMessage::JobFinished);
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
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinishing)
            .ok();
        let psm_result = psm.await;
        println!(
            "Asset {} process_service_messages task joined",
            self.id()
        );
        match psm_result {
            Ok(Ok(())) => {println!("Asset {} process_service_messages task finished successfully", self.id());}
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
            println!(
                "Asset {} evaluation finished with error: {}",
                self.id(),
                e
            );
            let mut lock = self.data.write().await;
            lock.data = None;
            lock.status = Status::Error;
            lock.binary = None;
            lock.metadata = Metadata::from_error(e.clone());
        } else {
            println!(
                "Asset {} evaluation finished without an error",
                self.id(),
            );
            async fn try_to_set_ready(assetref: AssetRef<impl Environment>) {
                eprintln!(
                    "Trying to set asset {} to ready - status {:?}",
                    assetref.id(),
                    assetref.status().await
                );
                let mut lock = assetref.data.write().await;
                if lock.data.is_some() {
                    lock.status = Status::Ready;
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
            }
        }
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinished)
            .ok();
        {
            let mut lock = self.data.write().await;
            println!(
                "Asset {} sending JobFinished notification",
                self.id()
            );
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
                    lock.status = Status::Ready;
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
        let (input_state, recipe) = self.initial_state_and_recipe().await;

        println!("Evaluating recipe {:?}", &recipe);
        let envref = self.get_envref().await;
        /*
        let plan = {
            let cmr = envref.0.get_command_metadata_registry();
            recipe.to_plan(cmr)?
        };
        */
        let context = Context::new(self.clone()).await; // TODO: reference to asset
                                                        // TODO: Separate evaluation of dependencies
                                                        //let res = apply_plan(plan, envref, context, input_state).await?;
                                                        //let res = apply_plan_new(
                                                        //    plan, input_state, context, envref).await?;
        println!("Applying recipe");
        let res = envref.apply_recipe(input_state, recipe, context).await?;
        println!("Recipe evaluated, result: {:?}", &res);

        Ok(State {
            data: res,
            metadata: Arc::new(self.data.read().await.metadata.clone()),
        })
    }

    pub async fn evaluate_and_store(&self) -> Result<(), Error> {
        let res = self.evaluate_recipe().await;
        match res {
            Ok(state) => {
                let mut lock = self.data.write().await;
                lock.data = Some(state.data.clone());
                lock.metadata = (*state.metadata).clone();
                lock.status = state.metadata.status();
                match lock.status {
                    Status::None
                    | Status::Recipe
                    | Status::Submitted
                    | Status::Dependencies
                    | Status::Processing
                    | Status::Storing => {
                        // here is a value, so this is probably an old state - mark as ready
                        lock.status = Status::Ready;
                        lock.metadata.set_status(Status::Ready)?;
                    }
                    Status::Ready => {}
                    Status::Partial => {}
                    Status::Error => {}
                    Status::Directory => {}
                    Status::Cancelled => {}
                    Status::Source => {}
                    Status::Expired => {}
                }
                let _ = lock
                    .notification_tx
                    .send(AssetNotificationMessage::ValueProduced);
                let save_in_background = lock.save_in_background;
                drop(lock);
                let assetref = self.clone();
                if save_in_background {
                    tokio::spawn(async move {
                        let _ = assetref.save_to_store().await;
                    });
                } else {
                    let _ = self.save_to_store().await;
                }
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
        let mut context = Context::new(self.clone()).await;
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
        let mut x = self.poll_binary().await;
        if x.is_none() {
            x = self.serialize_to_binary().await?;
        }

        if let Some((data, metadata)) = x {
            let lock = self.data.read().await;

            let envref = lock.get_envref();
            let store = envref.get_async_store();
            let key = lock.recipe.store_to_key()?;
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
        let key = lock.recipe.store_to_key()?;
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

    /// Deserialize the binary data into the asset's data field.
    /// Returns true if the deserialization was successful.
    async fn deserialize_from_binary(&self) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        let value = {
            if let Some(binary) = &lock.binary {
                let type_identifier = lock.metadata.type_identifier()?;
                let extension = lock.metadata.extension().unwrap_or("bin".to_string());
                E::Value::deserialize_from_bytes(binary, &type_identifier, &extension)
            } else {
                return Ok(false);
            }
        }?;

        lock.data = Some(Arc::new(value));
        Ok(true)
    }

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
        lock.data = Some(Arc::new(value));
        lock.binary = None; // Invalidate binary
        lock.set_status(Status::Ready);
        // TODO: Store value in set_value
        // TODO: Update metadata with value info
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::ValueProduced);
        lock.service_sender()
            .send(AssetServiceMessage::JobFinishing)
            .map_err(|e| {
                Error::general_error(format!("Failed to send JobFinishing message: {}", e))
            })?;
        Ok(())
    }

    pub(crate) async fn set_state(
        &self,
        state: State<<E as Environment>::Value>,
    ) -> Result<(), Error> {
        println!("Setting state for asset {}", self.id());
        let mut lock = self.data.write().await;
        lock.data = Some(state.data.clone());
        lock.metadata = (*state.metadata).clone();
        lock.binary = None; // Invalidate binary
                            // TODO: Update metadata with value info
                            // TODO: Store state in store
        let status = lock.metadata.status();
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
    async fn remove(&self, key: &Key) -> Result<(), Error>;
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
        println!("Listing asset info in directory {}", key);
        let keys = self.listdir_keys(key).await?;
        let mut asset_info = Vec::new();
        for k in keys {
            println!("Getting asset info for key {}", k);
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

    pub fn get_recipe_provider(&self) -> Arc<Box<dyn AsyncRecipeProvider<E>>> {
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
            println!("Checking if store contains key {} {:?}", key, store.contains(key).await);
            if store.contains(key).await? {
                println!("Getting asset info from store for key {}", key);
                store.get_asset_info(key).await
            } else {
                let rp = self.get_recipe_provider();
                if rp.contains(key, self.get_envref()).await? {
                    rp.get_asset_info(key, self.get_envref()).await
                }
                else{
                    Err(Error::general_error(format!(
                        "Asset not found for key {} (get_asset_info)",
                        key
                    )).with_key(key))
                }
            }
        }
    }

    /// Create an ad-hoc asset applied to a value
    async fn apply(&self, recipe: Recipe, to: E::Value) -> Result<AssetRef<E>, Error> {
        let metadata = Arc::new(Metadata::new());
        let initial_state = State::from_value_and_metadata(to, metadata);
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
        let metadata = Arc::new(Metadata::new());
        let initial_state = State::from_value_and_metadata(to, metadata);
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

    async fn remove(&self, _key: &Key) -> Result<(), Error> {
        // TODO: Does nothing??
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
    capacity: usize,
}

impl<E: Environment + 'static> JobQueue<E> {
    /// Create a new job queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        println!("Creating job queue with capacity {}", capacity);
        JobQueue {
            jobs: Arc::new(Mutex::new(Vec::new())),
            capacity,
        }
    }

    /// Submit an asset for processing
    pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
        let pending_count = self.pending_jobs_count().await;
        if pending_count < self.capacity {
            // avoid waiting in queue
            let asset_clone = asset.clone();
            // Status set directly, since message processing is not running yet
            if let Err(e) = asset_clone.set_status(Status::Processing).await {
                eprintln!("Failed to set status for asset {}: {}", asset.id(), e);
            }
            eprintln!("Starting asset job {} immediately", asset.id());
            tokio::spawn(async move {
                let _ = asset_clone.run().await;
            });
        } else {
            asset.submitted().await?;
        }
        let mut jobs = self.jobs.lock().await;
        let asset_id = asset.id();
        jobs.push(asset);
        jobs.retain(|a| a.id() != asset_id); // TODO: this should not push the asset to the end of the queue

        Ok(())
    }

    /// Count how many jobs are currently running (Processing status)
    pub async fn pending_jobs_count(&self) -> usize {
        let jobs = self.jobs.lock().await;

        let mut count = 0;
        for asset in jobs.iter() {
            if asset.status().await.is_processing() {
                count += 1;
            }
        }

        count
    }

    /// Start processing jobs up to capacity
    pub async fn run(self: Arc<Self>) {
        eprintln!("Starting job queue");
        loop {
            //eprint!(".");
            let pending_count = self.pending_jobs_count().await;

            // Check if we can start more jobs
            if pending_count < self.capacity {
                let available_slots = self.capacity - pending_count;
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
                    let asset_clone = asset.clone();
                    // Status set directly, since message processing is not running yet
                    if let Err(e) = asset_clone.set_status(Status::Processing).await {
                        eprintln!("Failed to set status for asset {}: {}", asset.id(), e);
                    }
                    eprintln!("Starting asset job {}", asset.id());
                    tokio::spawn(async move {
                        let _ = asset_clone.run().await;
                    });
                }
            }

            // Sleep briefly to avoid busy waiting
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Clean up completed jobs (Ready or Error status)
    /// Returns the number of jobs removed
    pub async fn cleanup_completed(&mut self) -> usize {
        let (keep, initial_count, keep_len) = {
            let jobs = self.jobs.lock().await;
            let initial_count = jobs.len();
            let mut keep: Vec<AssetRef<E>> = Vec::new();
            for asset in jobs.iter() {
                let status = asset.status().await;
                if !status.is_finished() {
                    keep.push(asset.clone());
                }
            }
            let keep_len = keep.len();
            (keep, initial_count, keep_len)
        };
        self.jobs = Arc::new(Mutex::new(keep));
        initial_count - keep_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::context::{SimpleEnvironment, SimpleEnvironmentWithPayload};
    use crate::metadata::{Metadata, MetadataRecord};
    use crate::parse::{parse_key, parse_query};
    use crate::query::Key;
    use crate::store::{AsyncStoreWrapper, MemoryStore};
    use crate::value::{Value, ValueInterface};

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
}
