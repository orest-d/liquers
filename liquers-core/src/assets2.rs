//! # Assets
//!
//! Asset represents a unit of data that is available, is being produced (i.e. just being calculated)
//! or can be produced later on demand.
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
//! - data: Arc<V> where V is the value type
//! - binary: Arc<Vec<u8>> representing the serialized value
//!
//! ## Asset lifecycle
//! Asset goes through these stages:
//! 1) **initial** - a state the asset is in after creation. Only the recipe is known, none of the data, binary or metadata is available.
//! 2) **prepare** - check is binary data is available. In such a case value is deserialized.
//! 3) **run** - start recipe execution and the loop processing the service messages.
//! 4) **finished** - cancelled, error or success. Cancelled or error can be restarted.
//!

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use scc;
use tokio::sync::{mpsc, watch, Mutex, RwLock};

use crate::context2::Context;
use crate::interpreter2::apply_plan;
use crate::{
    context2::{EnvRef, Environment},
    error::Error,
    metadata::{Metadata, Status},
    query::{Key, Query},
    recipes2::{AsyncRecipeProvider, DefaultRecipeProvider, Recipe},
    state::State,
    value::DefaultValueSerializer,
};

/// Message for internal service communication (reliable, for control)
#[derive(Debug, Clone)]
pub enum AssetServiceMessage {
    SetStatus(Status),
    LogMessage(String),
    Cancel,
    UpdateProgress(f32),
    JobSubmitted,
    JobStarted,
    JobFinished,
}

/// Message for notifications to clients (best-effort, for updates)
#[derive(Debug, Clone)]
pub enum AssetNotificationMessage {
    Initial,
    Loading,
    Loaded,
    StatusChanged(Status), // TODO: remove argument?
    ValueProduced,
    ErrorOccurred(Error),
    ProgressUpdated(f32),
    LogMessage,
}

/// Enhanced version of AssetMessage to support job queue operations
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AssetMessage {
    StatusChanged(Status),
    ValueProduced,
    ErrorOccurred(String),
    JobSubmitted,
    JobStarted,
    JobFinished,
}

pub struct AssetData<E: Environment> {
    pub recipe: Recipe,

    // Service channel (mpsc) for control messages
    service_tx: mpsc::Sender<AssetServiceMessage>,
    _service_rx: Arc<Mutex<mpsc::Receiver<AssetServiceMessage>>>,

    // Notification channel (watch) for client notifications
    notification_tx: watch::Sender<AssetNotificationMessage>,
    _notification_rx: watch::Receiver<AssetNotificationMessage>,

    initial_state: State<E::Value>,

    /// This is used to store the data in the asset if available.
    data: Option<Arc<E::Value>>,

    /// This is used to store the binary representation of the data in the asset if available.
    /// If both data and binary is available, they will represent the same data and can be used interchangeably.
    binary: Option<Arc<Vec<u8>>>,

    /// Metadata
    metadata: Option<Arc<Metadata>>,

    /// Current status
    status: Status,

    _marker: std::marker::PhantomData<E>,
}

impl<E: Environment> AssetData<E> {
    pub fn new(recipe: Recipe) -> Self {
        // Create service channel with buffer size 32
        let (service_tx, service_rx) = mpsc::channel(32);

        // Create notification channel with initial status None
        let (notification_tx, notification_rx) = watch::channel(AssetNotificationMessage::Initial);

        AssetData {
            recipe,
            service_tx,
            _service_rx: Arc::new(Mutex::new(service_rx)),
            notification_tx,
            _notification_rx: notification_rx,
            initial_state: State::new(),
            data: None,
            binary: None,
            metadata: None,
            _marker: std::marker::PhantomData,
            status: Status::None,
        }
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

    /// Check if the asset is a pure query (no initial value and recipe is a pure query)
    pub fn is_pure_query(&self) -> Result<bool, Error> {
        Ok((!self.has_initial_value()?) && self.recipe.is_pure_query())
    }

    /// This tries to get an asset value by quickly evaluation strategies.
    /// For example, if the asset is a resource an attempt is made to deserialize it.
    /// These strategies are tried before the asset is queued for evaluation.
    /// A queue might be occupied by long running tasks, so it is beneficial
    /// to try to load the asset quickly.
    /// If the asset becomes available after the quick evaluation attempt, it is not queued.
    pub async fn try_quick_evaluation(&mut self, envref: EnvRef<E>) -> Result<(), Error> {
        if !self.is_resource()? {
            return Ok(()); // If asset is not a resource, it can't be just loaded
        }

        let store = envref.get_async_store();
        if let Ok(Some(key)) = self.recipe.key() {
            self.notification_tx
                .send(AssetNotificationMessage::Loading)
                .map_err(|e| {
                    Error::general_error(format!("Failed to send loading notification: {}", e))
                        .with_query(&(&key).into())
                })?;
            if store.contains(&key).await? {
                // Asset exists in the store, load binary and metadata
                let (binary, metadata) = store.get(&key).await?;
                self.binary = Some(Arc::new(binary));
                self.metadata = Some(Arc::new(metadata));
                let value = E::Value::deserialize_from_bytes(
                    self.binary.as_ref().unwrap(),
                    &self.metadata.as_ref().unwrap().type_identifier()?,
                    &self.metadata.as_ref().unwrap().get_data_format(),
                )?;
                self.data = Some(Arc::new(value));
                self.status = self.metadata.as_ref().unwrap().status();
                self.notification_tx
                    .send(AssetNotificationMessage::Loaded)
                    .map_err(|e| {
                        Error::general_error(format!("Failed to send loaded notification: {}", e))
                            .with_query(&key.into())
                    })?;

                return Ok(());
            }
        }
        Ok(())
    }

    /// Get a reference to the asset data
    pub fn to_ref(self) -> AssetRef<E> {
        AssetRef::new(self)
    }

    /// Get a clone of the service sender for internal control
    pub fn service_sender(&self) -> mpsc::Sender<AssetServiceMessage> {
        self.service_tx.clone()
    }

    /// Subscribe to the notifications.
    pub fn subscribe_to_notifications(&self) -> watch::Receiver<AssetNotificationMessage> {
        self.notification_tx.subscribe()
    }

    pub fn set_status(&mut self, status: Status) -> Result<(), Error> {
        if status != self.status {
            self.status = status;
            if let Some(metadata) = &mut self.metadata {
                if let Some(meta) = Arc::get_mut(metadata) {
                    meta.set_status(status)?;
                }
            } else {
                return Err(Error::unexpected_error(
                    "Metadata not set in AssetData::set_status".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Poll the current state without any async operations.
    /// Returns None if data or metadata is not available.
    pub fn poll_state(&self) -> Option<State<E::Value>> {
        if let (Some(data), Some(metadata)) = (&self.data, &self.metadata) {
            Some(State {
                data: data.clone(),
                metadata: metadata.clone(),
            })
        } else {
            None
        }
    }

    /// Poll the current binary data and metadata without any async operations.
    /// Returns None if binary or metadata is not available.
    pub fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        self.binary.clone().zip(self.metadata.clone())
    }
}

pub struct AssetRef<E: Environment> {
    pub data: Arc<RwLock<AssetData<E>>>,
}

impl<E: Environment> Clone for AssetRef<E> {
    fn clone(&self) -> Self {
        AssetRef {
            data: self.data.clone(),
        }
    }
}
impl<E: Environment> AssetRef<E> {
    pub fn new(data: AssetData<E>) -> Self {
        AssetRef {
            data: Arc::new(RwLock::new(data)),
        }
    }
    pub fn new_from_recipe(recipe: Recipe) -> Self {
        AssetRef {
            data: Arc::new(RwLock::new(AssetData::new(recipe))),
        }
    }

    /// Process messages from the service channel
    pub(crate) async fn process_service_messages(&self) -> Result<(), Error> {
        let (service_rx_ref, notification_tx) = {
            let lock = self.data.read().await;
            (lock._service_rx.clone(), lock.notification_tx.clone())
        };

        let mut rx = service_rx_ref.lock().await;

        while let Some(msg) = rx.recv().await {
            match msg {
                AssetServiceMessage::SetStatus(status) => {
                    self.set_status(status).await?;
                    let _ = notification_tx.send(AssetNotificationMessage::StatusChanged(status));
                }
                AssetServiceMessage::LogMessage(message) => {
                    // Forward log message to notification channel
                    let _ = notification_tx.send(AssetNotificationMessage::LogMessage);
                    // Update metadata with log message
                    let mut lock = self.data.write().await;
                    if let Some(metadata) = &mut lock.metadata {
                        if let Some(Metadata::MetadataRecord(meta)) = Arc::get_mut(metadata) {
                            meta.info(&message);
                        } else {
                            return Err(Error::unexpected_error(
                                "Metadata is not MetadataRecord in AssetServiceMessage::LogMessage"
                                    .to_owned(),
                            ));
                        }
                    }
                }
                AssetServiceMessage::Cancel => {
                    self.set_status(Status::Cancelled).await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Cancelled));
                    return Ok(());
                }
                AssetServiceMessage::UpdateProgress(progress) => {
                    // Update progress and notify
                    let _ =
                        notification_tx.send(AssetNotificationMessage::ProgressUpdated(progress));
                }
                AssetServiceMessage::JobSubmitted => {
                    self.set_status(Status::Submitted).await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Submitted));
                }
                AssetServiceMessage::JobStarted => {
                    self.set_status(Status::Processing).await?;
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Processing));
                    let _ = notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Processing));
                }
                AssetServiceMessage::JobFinished => {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Run the asset evaluation loop.
    pub(crate) async fn run(&self, envref: EnvRef<E>) -> Result<(), Error> {
        if self.status().await.is_finished() {
            return Ok(()); // Already finished
        }
        tokio::select! {
            res = self.process_service_messages() => res,
            res = self.evaluate_and_store(envref) => res
        }
    }

    async fn initial_state_and_recipe(&self) -> (State<E::Value>, Recipe) {
        let lock = self.data.read().await;
        (lock.initial_state.clone(), lock.recipe.clone())
    }

    pub async fn evaluate_recipe(&self, envref: EnvRef<E>) -> Result<State<E::Value>, Error> {
        let (input_state, recipe) = self.initial_state_and_recipe().await;
        let plan = {
            let cmr = envref.0.get_command_metadata_registry();
            recipe.to_plan(cmr)?
        };
        let context = Context::new(envref.clone(), self.clone()).await; // TODO: reference to asset
                                                          // TODO: Separate evaluation of dependencies
        let res = apply_plan(plan, envref, context, input_state).await?;

        Ok(res)
    }

    pub async fn evaluate_and_store(&self, envref: EnvRef<E>) -> Result<(), Error> {
        let res = self.evaluate_recipe(envref).await;
        match res {
            Ok(state) => {
                let mut lock = self.data.write().await;
                lock.data = Some(state.data.clone());
                lock.metadata = Some(state.metadata.clone());
                lock.status = state.metadata.status();
                let _ = lock
                    .notification_tx
                    .send(AssetNotificationMessage::ValueProduced);
                Ok(())
            }
            Err(e) => {
                let mut lock = self.data.write().await;
                lock.data = None;
                lock.metadata = Some(Arc::new(Metadata::from_error(e.clone())));
                lock.status = Status::Error;
                lock.binary = None;
                let _ = lock
                    .notification_tx
                    .send(AssetNotificationMessage::ErrorOccurred(e.clone()));
                Err(e)
            }
        }
    }

    /// Check if the asset is currently in the job queue
    pub async fn is_in_job_queue(&self) -> bool {
        match self.data.read().await.status {
            Status::Submitted | Status::Processing => true,
            _ => false,
        }
    }

    /// Deserialize the binary data into the asset's data field.
    /// Returns true if the deserialization was successful.
    async fn deserialize_from_binary(&self) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        let value = {
            if let (Some(binary), Some(metadata)) = (&lock.binary, &lock.metadata) {
                let type_identifier = metadata.as_ref().type_identifier()?;
                let extension = metadata.extension().unwrap_or("bin".to_string());
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
    pub async fn subscribe(&self) -> watch::Receiver<AssetNotificationMessage> {
        let lock = self.data.read().await;
        lock.notification_tx.subscribe()
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
        let mut rx = self.subscribe().await;

        // Wait for either notifications or run completion

        loop {
            let notification = rx.borrow().clone();
            match notification {
                AssetNotificationMessage::ValueProduced => {
                                if let Some(state) = self.poll_state().await {
                                    return Ok(state);
                                }
                            }
                AssetNotificationMessage::ErrorOccurred(e) => {
                                return Err(e);
                            }
                AssetNotificationMessage::Initial => { },
                AssetNotificationMessage::Loading => { },
                AssetNotificationMessage::Loaded => {
                    if let Some(state) = self.poll_state().await {
                        return Ok(state);
                    }
                },
                AssetNotificationMessage::StatusChanged(_) => { },
                AssetNotificationMessage::ProgressUpdated(_) => { },
                AssetNotificationMessage::LogMessage => { },
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
            if let Some(b) = self.serialize_to_binary().await?{
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

    pub async fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        let lock = self.data.read().await;
        lock.poll_binary()
    }
}

#[async_trait]
pub trait AssetInterface<E: Environment>: Send + Sync {
    //    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage>;
    async fn get(&self) -> Result<State<E::Value>, Error>;
}

#[async_trait]
impl<E: Environment> AssetInterface<E> for AssetRef<E> {
    /*
    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage> {
        let lock = self.data.read().await;
        lock.tx.subscribe()
    }
    */
    async fn get(&self) -> Result<State<E::Value>, Error> {
        self.get().await
    }
}

#[async_trait]
pub trait AssetManager<E: Environment>: Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset_if_exists(&self, query: &Query) -> Result<Self::Asset, Error>;
    async fn get_asset(&self, query: Query) -> Result<Self::Asset, Error>;
    async fn assets_list(&self) -> Result<Vec<Query>, Error>;
    async fn contains_asset(&self, query: &Query) -> Result<bool, Error>;
}

#[async_trait]
pub trait AssetStore<E: Environment>: Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset(&self, query: &Query) -> Result<Self::Asset, Error>;
    async fn get(&self, key: &Key) -> Result<Self::Asset, Error>;
    async fn create(&self, key: &Key) -> Result<Self::Asset, Error>;
    async fn remove(&self, key: &Key) -> Result<(), Error>;
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
    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error>;

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<Self::Asset, Error>;
}

pub struct DefaultAssetStore<E: Environment> {
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    recipe_provider: std::sync::OnceLock<DefaultRecipeProvider<E>>,
}

impl<E: Environment> Default for DefaultAssetStore<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Environment> DefaultAssetStore<E> {
    pub fn new() -> Self {
        DefaultAssetStore {
            envref: std::sync::OnceLock::new(),
            assets: scc::HashMap::new(),
            query_assets: scc::HashMap::new(),
            recipe_provider: std::sync::OnceLock::new(),
        }
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
        if self
            .recipe_provider
            .set(DefaultRecipeProvider::new(envref))
            .is_err()
        {
            panic!("Recipe provider already set in AssetStore");
        }
    }

    pub fn get_recipe_provider(&self) -> &DefaultRecipeProvider<E> {
        self.recipe_provider
            .get()
            .expect("Recipe provider not set in AssetStore")
    }
}

#[async_trait]
impl<E: Environment> AssetStore<E> for DefaultAssetStore<E> {
    type Asset = AssetRef<E>;

    async fn get_asset(&self, query: &Query) -> Result<Self::Asset, Error> {
        if let Some(key) = query.key() {
            self.get(&key).await
        } else {
            let entry = self
                .query_assets
                .entry_async(query.clone())
                .await
                .or_insert_with(|| AssetRef::<E>::new_from_recipe(query.into()));
            Ok(entry.get().clone())
        }
    }

    async fn get(&self, key: &Key) -> Result<Self::Asset, Error> {
        let entry = self
            .assets
            .entry_async(key.clone())
            .await
            .or_insert_with(|| AssetRef::<E>::new_from_recipe(key.into()));

        let asset_ref = entry.get().clone();

        Ok(asset_ref)
    }

    async fn create(&self, key: &Key) -> Result<Self::Asset, Error> {
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
        self.get_recipe_provider().contains(key).await
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.listdir_keys_deep(&Key::new()).await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let store = self.get_envref().get_async_store();
        let mut names = self
            .get_recipe_provider()
            .assets_with_recipes(key)
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
                    .assets_with_recipes(&subkey)
                    .await?;
                for resourcename in recipes {
                    keys.insert(subkey.join(resourcename.name));
                }
            }
        }

        Ok(keys.into_iter().collect())
    }

    async fn makedir(&self, key: &Key) -> Result<Self::Asset, Error> {
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
        JobQueue {
            jobs: Arc::new(Mutex::new(Vec::new())),
            capacity,
        }
    }

    /// Submit an asset for processing
    pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
        // Update status to Submitted
        {
            let mut lock = asset.data.write().await;
            (*lock).set_status(Status::Submitted)?; // TODO: on error crash job

            //let _ = lock.tx.send(AssetMessage::JobSubmitted);
        }

        // Add to job queue
        self.jobs.lock().await.push(asset);
        Ok(())
    }

    /// Count how many jobs are currently running (Processing status)
    pub async fn pending_jobs_count(&self) -> usize {
        let jobs = self.jobs.lock().await;

        let mut count = 0;
        for asset in jobs.iter() {
            let status = asset.data.read().await.status;

            if status == Status::Processing {
                count += 1;
            }
        }

        count
    }

    /// Start processing jobs up to capacity
    pub async fn run(self: Arc<Self>, envref: EnvRef<E>) {
        loop {
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

                        let status = asset.data.read().await.status;

                        if status == Status::Submitted {
                            jobs_to_start.push(asset.clone());
                        }
                    }
                }

                // Start jobs
                for asset in jobs_to_start {
                    let asset_clone = asset.clone();
                    let envref_clone = envref.clone();

                    // Set status to Processing
                    {
                        let mut lock = asset.data.write().await;
                        lock.set_status(Status::Processing); // TODO: on error crash job
                                                             /*
                                                                 let _ = lock
                                                                 .tx
                                                                 .send(AssetMessage::StatusChanged(Status::Processing));
                                                             let _ = lock.tx.send(AssetMessage::JobStarted);
                                                             */
                    }

                    // Process job in a separate task
                    tokio::spawn(async move {
                        let result = asset_clone.get().await;

                        // Update status based on result
                        let (status, error_msg) = match &result {
                            Ok(_) => {
                                let _ = {
                                    let lock = asset_clone.data.write().await;
                                    //lock.tx.send(AssetMessage::ValueProduced)
                                };
                                (Status::Ready, None)
                            }
                            Err(e) => {
                                let error_msg = e.to_string();
                                let _ = {
                                    let lock = asset_clone.data.write().await;
                                    //lock.tx.send(AssetMessage::ErrorOccurred(error_msg.clone()))
                                };
                                (Status::Error, Some(error_msg))
                            }
                        };

                        // Update final status
                        {
                            let mut lock = asset_clone.data.write().await;

                            //let _ = lock.tx.send(AssetMessage::StatusChanged(status));
                            //let _ = lock.tx.send(AssetMessage::JobFinished);
                        }
                    });
                }
            }

            // Sleep briefly to avoid busy waiting
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Clean up completed jobs (Ready or Error status)
    pub async fn cleanup_completed(&mut self) -> usize {
        let (keep, initial_count, keep_len) = {
            let mut jobs = self.jobs.lock().await;
            let initial_count = jobs.len();
            let mut keep: Vec<AssetRef<E>> = Vec::new();
            for asset in jobs.iter() {
                let lock = asset.data.read().await;
                match lock.status {
                    Status::Processing | Status::Submitted => {
                        keep.push(asset.clone());
                    }
                    _ => {}
                }
            }
            let keep_len = keep.len();
            (keep, initial_count, keep_len)
        };
        self.jobs = Arc::new(Mutex::new(keep));
        initial_count - keep_len
    }
}

// Add methods to DefaultAssetStore to work with JobQueue
impl<E: Environment> crate::assets2::DefaultAssetStore<E> {
    /// Submit an asset to the job queue
    pub async fn submit_to_job_queue(
        &self,
        asset: AssetRef<E>,
        job_queue: Arc<JobQueue<E>>,
    ) -> Result<(), Error> {
        job_queue.submit(asset).await
    }

    /*
    /// Get an asset and optionally submit it to the job queue if it's not already processed
    pub async fn get_asset_and_process(
        &self,
        query: &crate::query::Query,
        job_queue: Option<Arc<JobQueue<E>>>,
    ) -> Result<AssetRef<E>, Error> {
        let asset = self.get_asset(query).await?;

        // If we have a job queue and the asset needs processing, submit it
        if let Some(queue) = job_queue {
            let status = {
                let lock = asset.data.read().await;
                if let Some(metadata) = &lock.metadata {
                    metadata.status
                } else {
                    Status::None
                }
            };

            // Submit if not already processed or in queue
            if status != Status::Ready
                && status != Status::Error
                && status != Status::Processing
                && status != Status::Submitted
            {
                queue.submit(asset.clone()).await?;
            }
        }

        Ok(asset)
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::context2::SimpleEnvironment;
    use crate::metadata::{Metadata, MetadataRecord};
    use crate::parse::{parse_key, parse_query};
    use crate::query::Key;
    use crate::store::{AsyncStoreWrapper, MemoryStore};
    use crate::value::{Value, ValueInterface};

    #[tokio::test]
    async fn test_asset_data_basics() {
        let key = parse_key("test.txt").unwrap();
        let asset_data = AssetData::<SimpleEnvironment<Value>>::new(key.into());
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
                        .clone(),
                ),
            )
            .await
            .unwrap();

        let envref = env.to_ref();

        let mut asset_data = AssetData::<SimpleEnvironment<Value>>::new(key.into());

        let state = asset_data.poll_state();
        assert!(state.is_none());
        let bin = asset_data.poll_binary();
        assert!(bin.is_none());
        asset_data.try_quick_evaluation(envref).await.unwrap();
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

        let mut asset_data = AssetData::<SimpleEnvironment<Value>>::new(query.into());

        let state = asset_data.poll_state();
        assert!(state.is_none());
        let bin = asset_data.poll_binary();
        assert!(bin.is_none());
        asset_data
            .try_quick_evaluation(envref.clone())
            .await
            .unwrap();
        let assetref = asset_data.to_ref();
        let state = assetref.poll_state().await;
        assert!(state.is_none());
        let bin = assetref.poll_binary().await;
        assert!(bin.is_none());
        assetref.evaluate_and_store(envref.clone()).await.unwrap();

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

        let asset_data = AssetData::<SimpleEnvironment<Value>>::new(query.into());

        let assetref = asset_data.to_ref();
        assetref.run(envref.clone()).await.unwrap();

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

        let asset_data = AssetData::<SimpleEnvironment<Value>>::new(query.into());

        let assetref = asset_data.to_ref();
        assert!(assetref.poll_state().await.is_none());

        let handle =tokio::spawn({
            let assetref = assetref.clone();
            async move {
                assetref.get().await
            }
        });

        assetref.run(envref.clone()).await.unwrap();

        let result = handle.await.unwrap().unwrap().try_into_string().unwrap();
        assert_eq!(result, "Hello, world!");
        assert_eq!(assetref.status().await, Status::Ready);
        assert!(assetref.poll_state().await.is_some());

        let (b,_) = assetref.get_binary().await.unwrap();
        assert_eq!(b.as_ref(), b"Hello, world!");
    }


}
