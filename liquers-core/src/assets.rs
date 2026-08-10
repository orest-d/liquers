//! Runtime assets, evaluation scheduling, and keyed asset operations.
//!
//! # Core model
//!
//! Liquers uses three layers:
//!
//! 1. [`ValueInterface`] represents typed data.
//! 2. [`State`] combines a value with [`Metadata`].
//! 3. An asset represents a state that may already exist, be scheduled, be under
//!    evaluation, or be recoverable from a [`Recipe`].
//!
//! [`AssetData`] is the mutable runtime record. [`AssetRef`] is a cloneable strong
//! handle backed by `Arc<RwLock<AssetData<E>>>`; its clones identify the same
//! runtime asset and share status, data, metadata, and notifications. A
//! [`WeakAssetRef`] does not keep the asset alive.
//!
//! Most applications should obtain an `AssetRef` from
//! [`EnvRef::evaluate`](crate::context::EnvRef::evaluate) or an [`AssetManager`].
//! Direct `AssetData` access is primarily framework infrastructure.
//!
//! # Evaluation entry points
//!
//! The configured manager determines whether ordinary evaluation is queued or
//! inline:
//!
//! | Entry point | Initial state | Payload | `DefaultAssetManager` | `ImmediateAssetManager` |
//! |---|---|---|---|---|
//! | [`AssetManager::get_asset`] / [`AssetManager::get`] | Empty | No | Fast-track or schedule, then return the handle | Fast-track or evaluate inline before returning |
//! | [`AssetManager::apply`] | Supplied | No | Schedule, then return the handle | Evaluate inline before returning |
//! | [`AssetManager::apply_immediately`] | Supplied | Optional | Evaluate before returning, bypassing the job queue | Evaluate inline before returning |
//!
//! [`EnvRef::evaluate`](crate::context::EnvRef::evaluate) delegates to
//! [`AssetManager::get_asset`]. Therefore it returns a scheduled handle with
//! `DefaultAssetManager`, but a completed handle with [`ImmediateAssetManager`].
//! [`EnvRef::evaluate_immediately`](crate::context::EnvRef::evaluate_immediately)
//! uses an empty state and delegates to [`AssetManager::apply_immediately`].
//!
//! `apply_immediately` is also the only entry point above that accepts an execution
//! payload. Its immediate evaluation path does not persist the produced value.
//!
//! # Typical lifecycle
//!
//! An evaluated asset typically moves through these phases:
//!
//! ```text
//! None or Recipe -> Submitted -> Processing -> Ready
//!                                  |
//!                                  +-> Dependencies -> Processing
//!                                  +-> Error or Cancelled
//! ```
//!
//! `Submitted` is specific to queued execution and may be skipped by an inline
//! manager. `Dependencies` means evaluation is waiting for another asset. A
//! successful volatile evaluation finishes as `Volatile` instead of `Ready`.
//! A `Ready` or `Override` asset may later become `Expired`.
//!
//! `Source`, `Override`, and `Directory` are terminal states created through
//! store, override, or directory operations rather than the usual evaluation
//! sequence. `Partial` is reserved for future support for publishing intermediate
//! results; that functionality is not completely implemented.
//!
//! # Volatility
//!
//! Volatility is similar to an extremely short expiration: a volatile result is
//! not retained for reuse, and another manager request evaluates it again. The
//! important difference is that a freshly produced volatile state is valid and
//! available to the dependent evaluation that requested it. Its terminal status
//! is `Volatile`, not `Expired`.
//!
//! Volatility is contagious. A query, command, recipe, immediate-expiration policy,
//! or volatile dependency can make an evaluation volatile. An asset that depends
//! on volatile input also produces a volatile result, so the result is not reused
//! as a stable cached asset.
//!
//! # Identity, reuse, and fast-track loading
//!
//! A pure key query is resolved through the key-asset map. Other queries use the
//! query-asset map. Non-volatile entries may be reused by later manager requests.
//! Volatile requests create a new asset instead of using those maps. Assets created
//! by `apply` or `apply_immediately` are ad hoc and are not inserted into either
//! map.
//!
//! Before evaluating a keyed resource with no supplied initial state, a manager
//! attempts a fast-track load from the store. Stored `Ready`, `Source`, and
//! `Override` values are accepted if deserialization succeeds and recorded
//! dependency versions are not stale. Other statuses, corrupt data, or stale
//! dependencies cause normal evaluation instead. Non-key queries and apply
//! operations are not fast-tracked.
//!
//! # Status and reads
//!
//! [`Status`] describes lifecycle state. Typical queued
//! evaluation passes through `None`, `Submitted`, and `Processing`; it may use
//! `Dependencies` while awaiting another asset and ends in `Ready`, `Volatile`,
//! `Error`, or `Cancelled`. `Source`, `Override`, `Directory`, and `Expired` are
//! also terminal according to [`Status::is_finished`](crate::metadata::Status::is_finished).
//! The status type classifies states; `AssetData::set_status` does not enforce a
//! transition graph.
//!
//! Read methods have deliberately different contracts:
//!
//! | Method | Waits | Expired value | Lock behavior |
//! |---|---:|---:|---|
//! | [`AssetRef::get`] | Yes | Returns an error if expiry is observed while waiting | Async read lock |
//! | [`AssetRef::poll_state`] | No | Hidden | Async read lock |
//! | [`AssetRef::poll_state_any_status`] / [`AssetRef::get_any_status`] | No | Returned when retained | Async read lock |
//! | [`AssetRef::try_poll_state`] | No | Hidden | Returns `None` if the lock is unavailable |
//! | [`AssetRef::get_binary`] | May wait and serialize | Returns an error | Async locks |
//! | [`AssetRef::poll_binary`] | No | Hidden | Async read lock |
//! | [`AssetRef::poll_binary_any_status`] | No | Returned when cached | Async read lock |
//! | [`AssetRef::get_binary_any_status`] | No, but may serialize | Returned, serializing if needed | Async locks |
//! | [`AssetRef::try_poll_binary`] | No | Hidden | Returns `None` if the lock is unavailable |
//!
//! Both families derive from one classifier, `Status::read_exposure()`, so they cannot drift
//! apart: `Value` (`Ready`, `Source`, `Override`, `Volatile`), `MetadataOnly` (`Directory`,
//! `Error`, `Cancelled`), `Expired`, and `Pending` (everything else, including `Partial`).
//!
//! `poll_state` returns an actual value for `Value`, a no-value state carrying metadata for
//! `MetadataOnly`, and `None` for `Expired` and `Pending`. Intermediate reads for `Partial` are
//! not yet supported.
//!
//! The binary family renders the same classification in its own terms. `MetadataOnly` has no
//! binary counterpart — a metadata-only *value* exists, a metadata-only *byte string* does not —
//! so binary reads report absence: `None` where the signature allows it, `Err` from `get_binary`.
//! `Err` carries the asset's own recorded failure for `Error`, and a constructed one for
//! `Cancelled` and `Directory`, which record none.
//!
//! `Expired` is a cache miss for every normal read of either family, including when serialized
//! bytes are still cached. Retained data is reachable only by explicit opt-in: the `*_any_status`
//! reads, or `to_override` to accept it. This applies uniformly, including to an asset labelled
//! `Expired` by `finish_run_with_result` because its evaluation consumed a stale dependency —
//! such a result is fresh but uncacheable, and whether it is acceptable is the caller's judgement
//! to make explicitly.
//! Consequently, a finalized evaluation failure observed through an existing
//! `AssetRef::get` can be represented by `Ok(State)` whose metadata contains the
//! error, rather than by `Err`.
//!
//! # Notifications
//!
//! [`AssetNotificationMessage`] is delivered through a Tokio [`watch`] channel. A
//! watch receiver retains only the latest
//! notification; intermediate notifications may be coalesced or overwritten.
//! Notifications are wake-up hints, not an event log. After waking, read
//! [`AssetRef::status`], [`AssetRef::get_metadata`], or one of the polling methods
//! for authoritative state. Subscribe before checking state when implementing a
//! wait loop, and check state again before awaiting `changed()` to avoid a lost
//! wake-up race.
//!
//! [`AssetServiceMessage`] uses an internal unbounded MPSC channel for reliable
//! control, logs, and progress updates. Although its type is public, application
//! code should not use it as a general asset mutation API.
//!
//! # Persistence
//!
//! Normal evaluation uses `evaluate_and_store`: after producing a value it updates
//! the asset, attempts persistence when the recipe supplies a key or `store_to`
//! key, and records a [`PersistenceStatus`]. With the queued manager, persistence
//! is normally started in the background, so a ready value can be observable
//! before the store write finishes. Immediate-manager evaluation persists
//! synchronously because it does not spawn background work. A serialization failure
//! is recorded as `NonSerializable`; other persistence failures are
//! `NotPersisted`. Persistence failure does not replace a successfully calculated
//! in-memory value with an evaluation error.
//!
//! [`AssetManager::set_binary`] writes directly to the store and removes an
//! existing in-memory entry. [`AssetManager::set_state`] creates an in-memory
//! entry and writes the value or, for non-serializable data, its metadata. External
//! data becomes `Source` when no recipe exists and `Override` when a recipe exists,
//! except that caller-supplied `Expired` and `Error` statuses are preserved.
//!
//! # Expiration, recovery, and removal
//!
//! Normal manager requests treat cached `Expired`, `Error`, and `Cancelled`
//! entries as misses and create a fresh asset. An existing expired handle does not
//! silently recompute; request the key or query from the manager again.
//!
//! `DefaultAssetManager` tracks finite expiration deadlines in a background
//! monitor. [`ImmediateAssetManager`] has no monitor and checks ready assets lazily
//! when they are requested. [`AssetManager::get_any_status`] is a keyed,
//! no-evaluation recovery read that can return retained expired data.
//! [`AssetManager::to_override`] promotes retained keyed data to `Override`.
//! [`AssetManager::remove`] removes the in-memory key entry, dependency record, and
//! stored value, but does not remove its recipe; a later request can therefore
//! evaluate a recipe-backed asset again.
//!
//! [`AssetRef::cancel`] is best-effort. It acts only on submitted, dependency,
//! processing, or partial assets. On native targets it waits at most five seconds
//! for a cancellation or finish notification and returns `Ok(())` on timeout.
//!
//! # Manager implementations
//!
//! `DefaultAssetManager` and `JobQueue` are available only on non-Wasm targets
//! and require a Tokio runtime when constructed because they spawn background
//! tasks. [`ImmediateAssetManager`] is spawn-free and timer-free, runs evaluation
//! in the caller's task, and is the manager used for browser-compatible execution.
//!

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use scc;
use tokio::sync::{mpsc, watch, Mutex, Notify, RwLock};

use crate::expiration::ExpirationTime;
use crate::interpreter::IsVolatile;

// psm-task join error: a real `JoinError` on native (spawned task), but the inline path never
// spawns, so on wasm there is no join error type — `Infallible` stands in (that `Err` arm is
// unreachable there).
#[cfg(not(target_arch = "wasm32"))]
type PsmJoinError = tokio::task::JoinError;
#[cfg(target_arch = "wasm32")]
type PsmJoinError = std::convert::Infallible;
use crate::metadata::{
    AssetInfo, DependencyKey, DependencyRecord, LogEntry, MetadataRecord, ProgressEntry,
    ReadExposure, Version,
};
use crate::value::ValueInterface;
use crate::{context::Context, metadata::LogEntryKind};
use crate::{
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{Metadata, Status},
    query::{Key, Query},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    value::DefaultValueSerializer,
};

/// A reliable message sent to an asset's internal service loop.
///
/// The service channel is an unbounded MPSC channel used by the scheduler and
/// [`Context`] for lifecycle control, logs, and progress. Application consumers
/// should normally observe [`AssetNotificationMessage`] and read authoritative
/// state through [`AssetRef`] instead of sending these messages.
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
    /// An error occurred; the job will finish.
    ErrorOccurred(Error),
    /// The job is about to finish; only housekeeping remains.
    JobFinishing,
    /// Job is finished, no further action will be taken
    JobFinished,
}

/// A best-effort notification that an asset may have changed.
///
/// Notifications use a [`tokio::sync::watch`] channel, so receivers see the latest
/// value and may miss intermediate messages. Treat a notification as a reason to
/// inspect [`AssetRef`] state, not as an auditable event stream.
#[derive(Debug, Clone)]
pub enum AssetNotificationMessage {
    Initial,
    JobSubmitted,
    JobStarted,
    StatusChanged(Status),
    ValueProduced,
    ErrorOccurred(Error),
    LogMessage,
    PrimaryProgressUpdated(ProgressEntry),
    SecondaryProgressUpdated(ProgressEntry),
    JobFinished,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Outcome of the most recent value-persistence path for an asset.
///
/// This is separate from [`Status`]: evaluation can succeed and expose a value
/// while persistence is `NonSerializable` or `NotPersisted`.
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

/// Internal-style coalescing helper for persisting asset metadata updates.
///
/// On native targets it limits background writes to the configured interval. On
/// Wasm it writes the latest pending metadata inline because no timer task is used.
pub struct MetadataSaver {
    state: Mutex<MetadataSaverState>,
    interval: std::time::Duration,
}

#[derive(Default)]
struct MetadataSaverState {
    pending: Option<Metadata>,
    #[cfg(not(target_arch = "wasm32"))]
    last_save: Option<std::time::Instant>,
    #[cfg(not(target_arch = "wasm32"))]
    in_flight: Option<tokio::task::JoinHandle<()>>,
}

impl MetadataSaver {
    /// Constructs and initializes a new runtime object for asset processing.
    ///
    /// Arguments:
    /// - `interval`: Minimum interval between metadata save attempts.
    pub fn new(interval: std::time::Duration) -> Self {
        Self {
            state: Mutex::new(MetadataSaverState::default()),
            interval,
        }
    }

    /// Enqueue metadata for persistence and trigger a single coalescing worker task.
    pub async fn save_immediately<E: Environment>(
        self: &Arc<Self>,
        metadata: Metadata,
        key: Option<Key>,
        envref: EnvRef<E>,
    ) {
        let mut lock = self.state.lock().await;
        lock.pending = Some(metadata);

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(handle) = lock.in_flight.as_ref() {
                if !handle.is_finished() {
                    return;
                }
                lock.in_flight = None;
            }
            let saver = self.clone();
            lock.in_flight = Some(tokio::spawn(async move {
                saver.save_task(key, envref).await;
            }));
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Inline synchronous persist: no spawn, no debounce, no std::time::Instant
            // (which panics on wasm32-unknown-unknown).
            let payload = lock.pending.take();
            drop(lock);
            if let (Some(metadata), Some(key)) = (payload, key.as_ref()) {
                let _ = envref.get_async_store().set_metadata(key, &metadata).await;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn save_task<E: Environment>(self: Arc<Self>, key: Option<Key>, envref: EnvRef<E>) {
        loop {
            let maybe_payload = {
                let mut lock = self.state.lock().await;
                let Some(_pending) = lock.pending.as_ref() else {
                    lock.in_flight = None;
                    return;
                };

                if let Some(last_save) = lock.last_save {
                    let elapsed = std::time::Instant::now().duration_since(last_save);
                    if elapsed < self.interval {
                        let wait = self.interval - elapsed;
                        drop(lock);
                        tokio::time::sleep(wait).await;
                        continue;
                    }
                }

                lock.pending.take()
            };

            if let Some(metadata) = maybe_payload {
                if let Some(key) = key.as_ref() {
                    if let Err(error) = envref.get_async_store().set_metadata(key, &metadata).await
                    {
                        eprintln!("Metadata save failed for key {}: {}", key, error);
                    } else {
                        let mut lock = self.state.lock().await;
                        lock.last_save = Some(std::time::Instant::now());
                    }
                }
            }
        }
    }
}

/// Mutable runtime record behind an [`AssetRef`].
///
/// This stores the recipe, optional input and output representations, metadata,
/// lifecycle status, persistence state, and communication channels. Applications
/// normally use [`AssetRef`] or [`AssetManager`] rather than construct or mutate
/// this type directly.
pub struct AssetData<E: Environment> {
    id: u64,
    /// Recipe associated with this runtime asset.
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
    metadata_saver: Arc<MetadataSaver>,

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

    /// Chain of payload-evaluated queries leading to this asset, used for cycle detection.
    ///
    /// A payload-evaluated asset is not a dependency-graph node, so the graph cannot detect a
    /// cycle among such assets. The path is carried here rather than on `Context` because each
    /// nested asset builds a fresh context; propagating through `AssetData` is how volatility
    /// already crosses that boundary.
    payload_path: Vec<Query>,

    /// Resolved expiration time for this asset
    expiration_time: ExpirationTime,

    /// Tracks persistence lifecycle for value-producing paths.
    persistence_status: PersistenceStatus,
    /// Last persistence error, when relevant.
    last_persistence_error: Option<Error>,

    /// Set when this asset consumed a dependency whose value had expired mid-execution.
    /// The stale value is used (no mid-run recompute), but at completion the asset is
    /// labeled `Expired` so the next access recomputes it. See `wait_for_dependency`.
    stale_dependency: bool,

    _marker: std::marker::PhantomData<E>,
}

/// Deserialize a stored value from its binary representation, given its recorded type
/// identifier and data format. Shared between `AssetData::try_fast_track` and
/// `AssetManager::get_any_status`'s store-fallback path, so the binary/type-identifier handling
/// (including the "bytes"-like identifiers that skip real deserialization) stays in one place.
fn deserialize_stored_value<E: Environment>(
    binary: &[u8],
    type_identifier: &str,
    data_format: &str,
) -> Result<E::Value, Error> {
    if matches!(
        type_identifier.trim().to_ascii_lowercase().as_str(),
        "bytes" | "binary" | "bin" | "b"
    ) {
        Ok(E::Value::from_bytes(binary.to_vec()))
    } else {
        E::Value::deserialize_from_bytes(binary, type_identifier, data_format)
    }
}

impl<E: Environment> AssetData<E> {
    /// Constructs and initializes a new AssetData
    ///
    /// Arguments:
    /// - `id`: Unique asset identifier.
    /// - `recipe`: Recipe describing how the asset should be resolved/evaluated.
    /// - `envref`: Environment reference
    pub fn new(id: u64, recipe: Recipe, envref: EnvRef<E>) -> Self {
        Self::new_ext(id, recipe, State::new(), envref)
    }

    /// Creates a temporary asset data structure.
    pub fn new_temporary(envref: EnvRef<E>) -> Self {
        let asset = Self::new_ext(0, Recipe::default(), State::new(), envref);
        asset
    }

    /// Extended constructor of AssetData
    ///
    /// Arguments:
    /// - `id`: Unique asset identifier.
    /// - `recipe`: Recipe describing how the asset should be resolved/evaluated.
    /// - `initial_state`: Initial input state used when evaluating apply/query recipes.
    /// - `envref`: Environment reference used to access the store, manager, and runtime services.
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
            metadata_saver: Arc::new(MetadataSaver::new(std::time::Duration::from_millis(100))),
            save_in_background: true,
            cancelled: false,
            is_volatile: false,
            payload_path: Vec::new(),
            expiration_time: ExpirationTime::Never,
            persistence_status: PersistenceStatus::None,
            last_persistence_error: None,
            stale_dependency: false,
            _marker: std::marker::PhantomData,
            status: Status::None,
        };

        asset
    }

    /// Returns a requested asset/runtime object needed by higher-level flows.
    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    /// Persists metadata updates to the configured async store.
    /// This is a throttled operation that ensures metadata is not saved too frequently.
    /// Used by: `process_service_messages` in this module.
    async fn save_metadata_to_store(&self) -> Result<(), Error> {
        let key = self.recipe.key()?.or(self.recipe.store_to_key()?);
        self.metadata_saver
            .save_immediately(self.metadata.clone(), key, self.get_envref())
            .await;
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

    /// Provides a human-readable description of the asset
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

    /// Helper method to clean the data used in the fast-track
    fn clear_fast_track_payload(&mut self) {
        self.data = None;
        self.binary = None;
        self.metadata = Metadata::new().into();
        self.status = Status::None;
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
                let stored_status = metadata.status();
                if !matches!(
                    stored_status,
                    Status::Ready | Status::Source | Status::Override
                ) {
                    eprintln!(
                        "Asset {} has stored status {:?}, fast-track disabled",
                        self.id(),
                        stored_status
                    );
                    self.clear_fast_track_payload();
                    return Ok(false);
                }

                let type_identifier = metadata.type_identifier()?;
                let data_format = metadata.get_data_format();
                let value = match deserialize_stored_value::<E>(&binary, &type_identifier, &data_format)
                {
                    Ok(value) => value,
                    Err(e) => {
                        eprintln!(
                            "Asset {} fast-track failed to deserialize stored value (treated as corrupted): {}",
                            self.id(),
                            e
                        );
                        self.clear_fast_track_payload();
                        return Ok(false);
                    }
                };

                self.binary = Some(Arc::new(binary));
                self.data = Some(Arc::new(value));
                self.status = stored_status;
                self.metadata = metadata;

                // Validate stored dependencies against the DM
                let envref = self.get_envref();
                let manager = envref.get_asset_manager();
                let dm = manager.dependency_manager();

                if let Metadata::MetadataRecord(ref mr) = self.metadata {
                    for dep_record in mr.get_dependencies() {
                        if let Some(dm_version) = dm.get_version(&dep_record.key).await {
                            if !dm_version.matches(&dep_record.version) {
                                eprintln!(
                                    "Asset {} stale: dependency {} version mismatch (stored={:?}, DM={:?})",
                                    self.id(), dep_record.key, dep_record.version, dm_version
                                );
                                self.clear_fast_track_payload();
                                return Ok(false); // Force re-evaluation
                            }
                        }
                    }
                }

                // Dependencies are consistent — register in DM
                {
                    let dep_key = DependencyKey::from(&key);
                    if let Some(version) = self.metadata.version() {
                        let expired = dm.register_version(&dep_key, version).await;
                        manager.expire_dependencies_result(expired).await;
                    }
                    let deps = self.metadata.get_dependencies();
                    if !deps.is_empty() {
                        let expired = dm.load_from_records(&dep_key, deps).await;
                        manager.expire_dependencies_result(expired).await;
                    }
                }

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

    /// Set the status
    /// Does not send any notifications.
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
        match self.status.read_exposure() {
            ReadExposure::Value => self.exposed_value_state(),
            ReadExposure::MetadataOnly => self.metadata_only_state(),
            // Expired is a cache miss for normal reads — see `poll_state_any_status` for the
            // explicit recovery read that still returns this data.
            ReadExposure::Expired => None,
            ReadExposure::Pending => None,
        }
    }

    /// The state exposed when a real value is available. `None` if the value is absent.
    fn exposed_value_state(&self) -> Option<State<E::Value>> {
        if let Some(data) = &self.data {
            let mut metadata = self.metadata.clone();
            metadata.with_type_identifier(data.identifier().to_string());
            metadata.with_type_name(data.type_name().to_string());

            Some(State::from_parts(data.clone(), Arc::new(metadata)))
        } else {
            None
        }
    }

    /// The no-value state carrying meaningful metadata, for `ReadExposure::MetadataOnly`.
    ///
    /// `Directory` stamps a `dir` type identifier; `Error` and `Cancelled` do not. That
    /// distinction predates the classifier and is preserved here deliberately — the inner match
    /// is why `MetadataOnly` cannot be collapsed into a single flat arm.
    fn metadata_only_state(&self) -> Option<State<E::Value>> {
        let mut metadata = self.metadata.clone();
        match self.status {
            Status::Directory => {
                metadata.with_type_identifier("dir".to_string());
            }
            Status::Error | Status::Cancelled => {}
            Status::None
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Storing
            | Status::Expired
            | Status::Ready
            | Status::Source
            | Status::Override
            | Status::Volatile => {
                // Not MetadataOnly; unreachable via `poll_state`. Returning None keeps this
                // total without inventing a state for a status that has no metadata-only form.
                return None;
            }
        }
        Some(State::from_parts(
            Arc::new(E::Value::none()),
            Arc::new(metadata),
        ))
    }

    /// Like `poll_state()`, but also returns data for `Status::Expired` — the explicit
    /// "I know it's expired, give it to me anyway" recovery read. Not gated to keyed assets;
    /// the keyed-only restriction lives in the manager-level `AssetManager::get_any_status`.
    pub fn poll_state_any_status(&self) -> Option<State<E::Value>> {
        match self.status.read_exposure() {
            ReadExposure::Expired => self.exposed_value_state(),
            ReadExposure::Value | ReadExposure::MetadataOnly | ReadExposure::Pending => {
                self.poll_state()
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

    /// Poll the cached binary data and metadata without any async operations.
    ///
    /// Subject to the same expiration contract as [`Self::poll_state`]: bytes are exposed only
    /// when the status classifies as [`ReadExposure::Value`]. Retained expired bytes are reachable
    /// through [`Self::poll_binary_any_status`], and the persistence path uses
    /// [`Self::binary_unchecked`].
    ///
    /// Returns `None` if no binary is cached, or if the status does not permit exposing it.
    pub fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        match self.status.read_exposure() {
            ReadExposure::Value => self.binary_unchecked(),
            // A metadata-only state has no binary counterpart; expired bytes need the explicit
            // recovery read; pending statuses have nothing to expose yet.
            ReadExposure::MetadataOnly => None,
            ReadExposure::Expired => None,
            ReadExposure::Pending => None,
        }
    }

    /// Like [`Self::poll_binary`], but also returns retained bytes for `Status::Expired` — the
    /// explicit "I know it is expired, give it to me anyway" recovery read, and the binary twin of
    /// [`Self::poll_state_any_status`].
    ///
    /// This does **not** serialize: it reports what is cached. `AssetRef::get_binary_any_status`
    /// is the variant that materialises bytes from a retained value.
    pub fn poll_binary_any_status(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        match self.status.read_exposure() {
            ReadExposure::Expired => self.binary_unchecked(),
            ReadExposure::Value | ReadExposure::MetadataOnly | ReadExposure::Pending => {
                self.poll_binary()
            }
        }
    }

    /// Status-blind access to the cached binary, for the persistence path.
    ///
    /// This is **not** a read of the asset's exposed value, so it deliberately does not consult
    /// [`ReadExposure`]: writing bytes to the store must not depend on whether a reader would be
    /// allowed to see them. `AssetRef::set_state` persists with whatever status the caller
    /// supplies, so those two concerns genuinely come apart. Named after the precedent set by
    /// [`State::data_unchecked`] — bypassing a guard should be visible at the call site.
    pub(crate) fn binary_unchecked(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
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

    /// Sets the persistence status
    /// Optionally an error can be set for [PersistenceStatus::NotPersisted]
    /// For non-serializable and non-persisted status a warning is added to the log.
    /// Used by: `persist_with_status_tracking`, `record_persistence_result`
    ///
    /// Arguments:
    /// - `status`: Target status to store on the asset/metadata.
    /// - `error`: Error value to classify, record, or propagate.
    fn set_persistence_status(&mut self, status: PersistenceStatus, error: Option<Error>) {
        self.persistence_status = status;
        self.last_persistence_error = error.clone();
        match status {
            PersistenceStatus::None | PersistenceStatus::Persisted => {}
            PersistenceStatus::NonSerializable | PersistenceStatus::NotPersisted => {
                let detail = if let Some(e) = error.as_ref() {
                    format!(": {}", e.to_string())
                } else {
                    "".to_owned()
                };
                let message = format!(
                    "Persistence status {:?} for {}{}",
                    status,
                    self.asset_reference(),
                    detail
                );
                let mut entry = if let Some(e) = error {
                    LogEntry::from_error(&e)
                } else {
                    LogEntry::warning(message.clone())
                };
                entry.message = message;
                entry.kind = LogEntryKind::Warning;
                let _ = self.metadata.add_log_entry(entry);
            }
        }
    }

    /// Returns the runtime-unique asset id assigned by its manager.
    fn id(&self) -> u64 {
        self.id
    }
}

/// Cloneable strong handle to one runtime asset.
///
/// Clones share the same [`AssetData`], unique [`Self::id`], and notification
/// channel. Managers return this handle before or after evaluation depending on
/// their [`EvalMode`]. Use [`Self::get`] to wait for state, polling methods for a
/// snapshot, and [`Self::subscribe_to_notifications`] for change wake-ups.
pub struct AssetRef<E: Environment> {
    id: u64,
    /// Shared low-level asset record.
    ///
    /// Direct locking is available for framework integration, but mutation can
    /// bypass lifecycle notifications and persistence bookkeeping.
    pub data: Arc<RwLock<AssetData<E>>>,
}

/// Weak handle that preserves an asset id without keeping its data alive.
pub struct WeakAssetRef<E: Environment> {
    id: u64,
    /// Weak pointer to the shared asset record.
    pub data: Weak<RwLock<AssetData<E>>>,
}

impl<E: Environment> Clone for AssetRef<E> {
    fn clone(&self) -> Self {
        AssetRef {
            id: self.id,
            data: self.data.clone(),
        }
    }
}

impl<E: Environment> Clone for WeakAssetRef<E> {
    fn clone(&self) -> Self {
        WeakAssetRef {
            id: self.id,
            data: self.data.clone(),
        }
    }
}

impl<E: Environment> AssetRef<E> {
    /// Enter the canonical dependency-waiting status for this asset.
    ///
    /// This is scheduler-local lifecycle bookkeeping only: dependency facts are
    /// recorded in metadata and the `DependencyManager`.
    pub(crate) async fn enter_dependencies(&self, dependency: &AssetRef<E>) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        if lock.status.is_finished() {
            return Ok(());
        }
        lock.set_status(Status::Dependencies)?;
        let _ = lock.metadata.add_log_entry(LogEntry::info(format!(
            "Waiting for dependency asset {}",
            dependency.id()
        )));
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::StatusChanged(
                Status::Dependencies,
            ));
        Ok(())
    }

    /// Leave dependency wait before retrying/resuming evaluation.
    pub(crate) async fn leave_dependencies_for_resubmit(&self) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        if lock.status == Status::Dependencies {
            lock.set_status(Status::Submitted)?;
            let _ = lock
                .notification_tx
                .send(AssetNotificationMessage::StatusChanged(Status::Submitted));
        }
        Ok(())
    }

    /// Fail this asset because a dependency failed.
    pub(crate) async fn fail_due_to_dependency(&self, error: Error) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        lock.data = None;
        lock.binary = None;
        lock.status = Status::Error;
        lock.metadata.with_error(error.clone());
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::ErrorOccurred(error));
        Ok(())
    }

    /// Leave `Status::Dependencies` and resume this asset as `Processing`.
    ///
    /// Counterpart of [`enter_dependencies`](Self::enter_dependencies) for the inline
    /// wait path (truthful status flow `Processing → Dependencies → Processing`).
    /// Unlike [`leave_dependencies_for_resubmit`](Self::leave_dependencies_for_resubmit)
    /// — which re-parks as `Submitted` for the genuine resubmission path — this keeps
    /// the caller's future as the live runner.
    pub(crate) async fn leave_dependencies_and_resume(&self) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        if lock.status == Status::Dependencies {
            lock.set_status(Status::Processing)?;
            let _ = lock
                .notification_tx
                .send(AssetNotificationMessage::StatusChanged(Status::Processing));
        }
        Ok(())
    }

    /// Record that this asset consumed a dependency whose value had expired *mid-execution*.
    ///
    /// Execution-time expiry policy (see `AssetManager::wait_for_dependency`): the dependency
    /// is NOT recomputed here (that risks unbounded re-execution when its freshness window is
    /// shorter than this asset's evaluation time). The stale value is used and this flag is
    /// set so that, at completion, the asset is labeled `Expired` and recomputed on next
    /// access (staleness propagation). A warning is logged now for timing diagnostics.
    pub(crate) async fn note_expired_dependency(
        &self,
        dependency: &AssetRef<E>,
    ) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        lock.stale_dependency = true;
        let _ = lock.metadata.add_log_entry(LogEntry::warning(format!(
            "Dependency asset {} expired during evaluation; using its stale value and \
             marking this asset expired for recomputation on next access",
            dependency.id()
        )));
        Ok(())
    }

    /// Record a direct dependency on another asset in metadata and the dependency manager.
    pub(crate) async fn record_dependency_on_asset(
        &self,
        dependency: &AssetRef<E>,
    ) -> Result<(), Error> {
        let dep_key = {
            let dep_lock = dependency.data.read().await;
            if let Ok(Some(key)) = dep_lock.recipe.key() {
                DependencyKey::from(&key)
            } else if let Some(query) = dep_lock.query.as_ref().as_ref() {
                DependencyKey::from(query)
            } else {
                return Ok(());
            }
        };
        let version = {
            let dep_lock = dependency.data.read().await;
            dep_lock.metadata.version()
        };
        let envref = self.get_envref().await;
        let manager = envref.get_asset_manager();
        let version = version
            .or(manager.dependency_manager().get_version(&dep_key).await)
            .unwrap_or_else(Version::unknown);

        {
            let mut lock = self.data.write().await;
            if let Some(existing) = lock
                .metadata
                .get_dependencies()
                .iter()
                .find(|d| d.key == dep_key)
            {
                // Keep a concrete version if we have one; Version(0) is
                // DependencyManager's "unknown" sentinel and should not
                // downgrade a better record.
                if existing.version.is_unknown() || !version.is_unknown() {
                    let _ = lock
                        .metadata
                        .add_dependency(DependencyRecord::new(dep_key.clone(), version));
                }
            } else {
                let _ = lock
                    .metadata
                    .add_dependency(DependencyRecord::new(dep_key.clone(), version));
            }
        }

        if let Some(current_key) = {
            let lock = self.data.read().await;
            lock.recipe.key().ok().flatten()
        } {
            let current_dep_key = DependencyKey::from(&current_key);
            if manager
                .dependency_manager()
                .would_create_cycle(&current_dep_key, &dep_key)
                .await
            {
                return Err(Error::dependency_cycle(&current_dep_key));
            }
            // Passing Version::unknown() is intentional: dependency-manager
            // semantics still register the edge but skip stale-version checks
            // because no concrete dependency version is available yet.
            if let Ok(expired) = manager
                .dependency_manager()
                .add_dependency(&current_dep_key, &dep_key, version)
                .await
            {
                manager.expire_dependencies_result(expired).await;
            }
        }
        Ok(())
    }

    /// Returns true if asset is finished and can't receive/process more messages
    /// Used by: `process_service_messages` in this module.
    async fn is_finished(&self) -> bool {
        let lock = self.data.read().await;
        lock.status.is_finished()
    }

    /// Helper to finalize primary progress
    /// This is used once the asset is finished so that the metadata does not indicate a progress.
    /// Used by: `run_with_future`.
    async fn finalize_primary_progress(&self) {
        let mut lock = self.data.write().await;
        lock.metadata.remove_progress();
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
        // Native/queued mode runs the service-message loop in a background task. On wasm
        // (immediate mode) there is no runtime; the inline run path drives the psm loop itself.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let assetref1 = assetref.clone();
            let _handle = tokio::spawn(async move { assetref1.process_service_messages().await });
        }
        assetref
    }

    /// Returns the runtime-unique asset id assigned by its manager.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The query this asset was created from, if any. Used for schedule-time
    /// classification of a non-keyed (expression) dependent.
    pub(crate) async fn query(&self) -> Option<Query> {
        let lock = self.data.read().await;
        (*lock.query).clone()
    }

    /// Create a weak reference to this asset.
    pub fn downgrade(&self) -> WeakAssetRef<E> {
        WeakAssetRef {
            id: self.id,
            data: Arc::downgrade(&self.data),
        }
    }

    /// Returns the environment associated with this asset.
    pub async fn get_envref(&self) -> EnvRef<E> {
        let lock = self.data.read().await;
        lock.get_envref()
    }

    /// Creates the execution context used by the interpreter and commands.
    ///
    /// The context refers back to this asset and exposes its environment services.
    pub async fn create_context(&self) -> Context<E> {
        let (is_volatile, payload_path) = {
            let lock = self.data.read().await;
            (lock.is_volatile, lock.payload_path.clone())
        };
        let context = Context::new(self.clone(), is_volatile).await;
        context.seed_payload_path(payload_path).await;
        context
    }

    /// Records the chain of payload-evaluated queries leading to this asset, so that the
    /// context created for it can continue cycle detection down the evaluation path.
    pub(crate) async fn set_payload_path(&self, path: Vec<Query>) {
        let mut lock = self.data.write().await;
        lock.payload_path = path;
    }

    /// Estimate the volatility before execution.
    /// Used e.g. by: `evaluate_and_store`, `evaluate_immediately`, `evaluate_recipe`
    async fn resolve_volatility_before_evaluation(&self) {
        let mut lock = self.data.write().await;
        let resolved =
            lock.is_volatile || lock.recipe.volatile || lock.recipe.expires.is_volatile();
        lock.is_volatile = resolved;
        if resolved {
            let metadata = &mut lock.metadata;
            let _ = metadata.set_volatile();
        }
    }

    /// Manages asset expiration timing and expiration state transitions.
    /// Labels asset as Ready or Volatile when it has data, Error otherwise.
    /// Used by: `finish_run_with_result` in this module.
    async fn try_to_set_ready(&self) {
        eprintln!(
            "Trying to set asset {} to ready - status {:?}",
            self.id(),
            self.status().await
        );
        let mut lock = self.data.write().await;
        if lock.data.is_some() {
            let metadata_expires = lock.metadata.expires();
            let should_be_volatile = lock.is_volatile || metadata_expires.is_volatile();
            if should_be_volatile {
                lock.is_volatile = true;
                lock.status = Status::Volatile;
                if let Err(e) = lock.metadata.set_volatile() {
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(format!(
                        "Failed to set volatile metadata on asset {}: {}",
                        self.id(),
                        e,
                    )));
                }
                lock.expiration_time = lock.metadata.expiration_time();
            } else {
                lock.status = Status::Ready;
                if let Err(e) = lock.metadata.set_status(Status::Ready) {
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(format!(
                        "Failed to set ready status in metadata on asset {}: {}",
                        self.id(),
                        e,
                    )));
                }
                if let Err(e) = lock.metadata.set_expiration_time_from(&metadata_expires) {
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(format!(
                        "Failed to set expiration metadata on asset {}: {}",
                        self.id(),
                        e,
                    )));
                }
                lock.expiration_time = lock.metadata.expiration_time();
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

    /// Returns a human-readable description of this runtime asset.
    pub async fn asset_reference(&self) -> String {
        let lock = self.data.read().await;
        lock.asset_reference()
    }

    /// Returns the asset information derived from metadata and its recipe.
    pub async fn get_asset_info(&self) -> Result<AssetInfo, Error> {
        let lock = self.data.read().await;
        lock.get_asset_info()
    }

    /// Returns a snapshot of the asset metadata.
    pub async fn get_metadata(&self) -> Result<Metadata, Error> {
        let lock = self.data.read().await;
        Ok(lock.metadata.clone())
    }

    /// Returns the latest value-persistence outcome.
    pub async fn persistence_status(&self) -> PersistenceStatus {
        let lock = self.data.read().await;
        lock.persistence_status
    }

    /// Set the persistance status of the asset.
    /// Used by: `persist_with_status_tracking`, `record_persistence_result` in this module.
    ///
    /// Arguments:
    /// - `status`: Target status to store on the asset/metadata.
    /// - `error`: Error value to classify, record, or propagate.
    async fn set_persistence_status(&self, status: PersistenceStatus, error: Option<Error>) {
        let mut lock = self.data.write().await;
        lock.set_persistence_status(status, error);
    }

    /// Based on the error type assess if the persistence failure is due to non-serializability or some other issue.
    /// [ErrorType::SerializationError] is considered as non-serializable, any other error is considered as not persisted.
    /// Non-serializable status indicates that the asset value cannot be persisted in the current store due to its nature or representation,
    /// and thus lack of persistence is not an error.
    fn classify_persistence_error(error: &Error) -> PersistenceStatus {
        match error.error_type {
            ErrorType::SerializationError => PersistenceStatus::NonSerializable,
            ErrorType::ArgumentMissing
            | ErrorType::ActionNotRegistered
            | ErrorType::CommandAlreadyRegistered
            | ErrorType::ParseError
            | ErrorType::ParameterError
            | ErrorType::TooManyParameters
            | ErrorType::ConversionError
            | ErrorType::General
            | ErrorType::CacheNotSupported
            | ErrorType::UnknownCommand
            | ErrorType::NotSupported
            | ErrorType::NotAvailable
            | ErrorType::KeyNotFound
            | ErrorType::KeyNotSupported
            | ErrorType::KeyReadError
            | ErrorType::KeyWriteError
            | ErrorType::UnexpectedError
            | ErrorType::ExecutionError
            | ErrorType::DependencyVersionMismatch
            | ErrorType::DependencyCycle
            | ErrorType::Cancelled => PersistenceStatus::NotPersisted,
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

    /// Save to store and track persistence outcomes.
    /// Short-circuits when cancelled, then persists either in a background task or synchronously and records the resulting persistence status.
    /// Used by: `evaluate_and_store`, `set_state`, `set_value` in this module.
    ///
    /// Arguments:
    /// - `save_in_background`: When true, performs persistence asynchronously in a spawned task.
    /// - `cancelled`: Cancellation flag used to skip writes after cancellation.
    async fn persist_with_status_tracking(&self, save_in_background: bool, cancelled: bool) {
        if cancelled {
            self.set_persistence_status(PersistenceStatus::None, None)
                .await;
            return;
        }

        // [opus B2] On wasm the inline path cannot spawn (no tokio runtime / `rt`), so persist
        // synchronously. Also on native: when the owning manager is in `Inline` mode, persist
        // synchronously so immediate evaluation stays fully spawn-free (runnable with no runtime).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let inline = self.get_envref().await.get_asset_manager().eval_mode() == EvalMode::Inline;
            let assetref = self.clone();
            if save_in_background && !inline {
                tokio::spawn(async move {
                    let result = assetref.save_to_store().await;
                    assetref.record_persistence_result(result).await;
                });
            } else {
                let result = self.save_to_store().await;
                self.record_persistence_result(result).await;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = save_in_background;
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
    pub(crate) async fn reset(&self) {
        let mut lock = self.data.write().await;
        lock.reset();
    }

    /// Process messages from the service channel
    pub(crate) async fn process_service_messages(&self) -> Result<(), Error> {
        eprintln!(
            "Starting to process service messages for asset {}",
            self.id()
        );
        let (service_rx_ref, notification_tx) = {
            let lock = self.data.read().await;
            (lock.service_rx.clone(), lock.notification_tx.clone())
        };

        let mut rx = service_rx_ref.lock().await;

        while let Some(msg) = rx.recv().await {
            eprintln!("Received message: {:?} by asset {}", msg, self.id());
            if self.is_finished().await {
                // Post-finish message policy: once the asset is finalized, DISPLAY-mutating and
                // control messages are dropped so a late producer cannot corrupt the terminal
                // state (status, progress) or resurrect processing. A late `LogMessage` is NOT
                // dropped here: it can at most append one log entry (harmless, and the contract
                // in specs tolerates it), and dropping it would race with legitimate in-run logs
                // that are dequeued just after the status flips to Ready on the fast path.
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
                    // Interim debug logging (WP-6 will migrate to `tracing::debug!`).
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "Dropping late service message {:?} for finished asset {}",
                            msg,
                            self.id()
                        );
                    }
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
                    eprintln!(
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
                    // Unified failure routine (metadata-preserving, single notify).
                    let _ = self.fail_asset(error).await;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Wait for the job finished notification.
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

    /// Coordinates asset evaluation with the service-message loop.
    /// This is executed after the processing of service messages finishes (due to successful job evaluation,
    /// error or cancelation).
    /// Sends lifecycle updates through the asset notification channel.
    /// Used by: `run_with_future` in this module.
    ///
    /// Arguments:
    /// - `result`: Evaluation/persistence result to convert into asset state updates.
    /// - `psm_result`: output of process_service_messages task to check if the service loop finished without errors.
    async fn finish_run_with_result(
        &self,
        mut result: Result<(), Error>,
        psm_result: Result<Result<(), Error>, PsmJoinError>,
    ) -> Result<(), Error> {
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
            // Unified failure routine: preserves the metadata audit trail (with_error) instead
            // of replacing it (Metadata::from_error).
            let _ = self.fail_asset(e.clone()).await;
        } else {
            // Finalization is guarded by current status because concurrent service messages may
            // already have transitioned the asset (e.g. cancellation/error). This is non-recursive:
            // try_to_set_ready() mutates local state only and never calls run()/run_immediately().
            match self.status().await {
                Status::None
                | Status::Recipe
                | Status::Submitted
                | Status::Dependencies
                | Status::Processing
                | Status::Partial => {
                    self.try_to_set_ready().await;
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
                Status::Ready
                | Status::Directory
                | Status::Expired
                | Status::Cancelled
                | Status::Source
                | Status::Override
                | Status::Volatile => {}
            }
            // If a dependency expired mid-evaluation, we used its stale value rather than
            // recompute (see `wait_for_dependency`). Do not cache this result as fresh:
            // label the asset `Expired` so the next access recomputes it.
            {
                let mut lock = self.data.write().await;
                if lock.stale_dependency && lock.status == Status::Ready {
                    let _ = lock.set_status(Status::Expired);
                    let _ = lock.metadata.add_log_entry(LogEntry::warning(
                        "Asset evaluated with an expired dependency value; labeled expired \
                         for recomputation on next access"
                            .to_string(),
                    ));
                }
            }
            // Schedule expiration if asset has a finite expiration time
            let exp_time = self.expiration_time().await;
            if !exp_time.is_never() && !exp_time.is_expired() {
                self.schedule_expiration(&exp_time).await;
            }
        }

        // Note: the psm loop has already terminated via the JobFinishing message sent in
        // run_with_future, so the previous JobFinished *service* message here was dead code
        // (resolves the "meaningless send" FIXME). Only the notification wake-up remains.
        {
            let lock = self.data.write().await;
            lock.notification_tx
                .send(AssetNotificationMessage::JobFinished)
                .ok();
        }
        result
    }

    /// Coordinates asset evaluation with the service-message loop.
    /// Spawns background asynchronous work with `tokio::spawn`.
    /// Used by: `run`, `run_immediately` in this module.
    ///
    /// GENERATED
    #[cfg(not(target_arch = "wasm32"))]
    async fn run_with_future<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
    where
        Fut: std::future::Future<Output = Result<(), Error>>,
    {
        self.resolve_volatility_before_evaluation().await;
        if self.status().await.is_finished() {
            return Ok(()); // Already finished
        }
        let assetref = self.clone();
        // all service messages should be processed, therefore they run in a separate task
        let psm = tokio::spawn(async move { assetref.process_service_messages().await });
        // Wait until either the asset evaluation future is evaluated or the service message loop finishes (which may happen in case of errors or cancellation).
        let result = tokio::select! {
            res = self.wait_to_finish() => res,
            res = evaluate_future => res
        };
        self.finalize_primary_progress().await;
        self.service_sender()
            .await
            .send(AssetServiceMessage::JobFinishing)
            .ok(); // Notify the service loop that the job is finishing, so it should smoothly end.
        self.finish_run_with_result(result, psm.await).await
    }

    /// Run the asset evaluation loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn run(&self) -> Result<(), Error> {
        self.run_with_future(self.evaluate_and_store()).await
    }

    /// Run the asset evaluation loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn run_immediately(&self, payload: Option<E::Payload>) -> Result<(), Error> {
        self.run_with_future(self.evaluate_immediately(payload))
            .await
    }

    /// Single-task (spawn-free) variant of [`run_with_future`] used by immediate-mode managers.
    /// The service-message loop runs concurrently via `futures::join!` (not `tokio::spawn`), and
    /// the evaluate/wait race uses `futures::select!` — both executor-agnostic, so this compiles
    /// and runs on `wasm32` with no tokio runtime. Sound because the psm loop terminates on the
    /// `JobFinishing` message sent below (see `finish_run_with_result` note).
    pub(crate) async fn run_with_future_inline<Fut>(&self, evaluate_future: Fut) -> Result<(), Error>
    where
        Fut: core::future::Future<Output = Result<(), Error>>,
    {
        use futures::FutureExt;
        self.resolve_volatility_before_evaluation().await;
        if self.status().await.is_finished() {
            return Ok(());
        }
        let eval_side = async {
            let wait = self.wait_to_finish().fuse();
            let ev = evaluate_future.fuse();
            futures::pin_mut!(wait, ev);
            let result = futures::select! {
                res = wait => res,
                res = ev => res,
            };
            self.finalize_primary_progress().await;
            self.service_sender()
                .await
                .send(AssetServiceMessage::JobFinishing)
                .ok();
            result
        };
        let psm_side = self.process_service_messages();
        let (result, psm_result) = futures::join!(eval_side, psm_side);
        // Wrap the inline psm outcome as the JoinError-free `Ok(..)` variant so the shared
        // `finish_run_with_result` is reused unchanged.
        self.finish_run_with_result(result, Ok(psm_result)).await
    }

    /// Inline (no-spawn) counterpart of [`run`].
    pub(crate) async fn run_inline(&self) -> Result<(), Error> {
        self.run_with_future_inline(self.evaluate_and_store()).await
    }

    /// Inline (no-spawn) counterpart of [`run_immediately`].
    pub(crate) async fn run_immediately_inline(
        &self,
        payload: Option<E::Payload>,
    ) -> Result<(), Error> {
        self.run_with_future_inline(self.evaluate_immediately(payload))
            .await
    }

    /// Fetch initial state and recipe of the asset
    /// Used by: `evaluate_immediately`, `evaluate_recipe` in this module.
    async fn initial_state_and_recipe(&self) -> (State<E::Value>, Recipe) {
        let lock = self.data.read().await;
        (lock.initial_state.clone(), lock.recipe.clone())
    }

    /// Evaluates the asset's recipe and returns the resulting state.
    ///
    /// This resolves a key recipe through the recipe provider when necessary and
    /// records observed dependencies. It does not by itself install the returned
    /// value as this asset's final data or persist it; those steps are performed by
    /// [`Self::evaluate_and_store`].
    pub async fn evaluate_recipe(&self) -> Result<State<E::Value>, Error> {
        self.resolve_volatility_before_evaluation().await;
        let (input_state, recipe) = {
            let (input_state, recipe) = self.initial_state_and_recipe().await;
            if let Ok(Some(key)) = recipe.key() {
                let envref = self.get_envref().await;
                let manager = envref.get_asset_manager();
                // Who owns this key? Asked with a map read, never with an evaluation:
                // `AssetManager::get` would run the asset inline under an inline manager, and
                // this asset *is* that asset, so it recursed until the wasm stack was gone.
                // See `specs/design/keyed-recipe-ownership/`.
                match manager.owned_key_asset(&key).await {
                    // Another asset is the registered owner: delegate to it.
                    Some(asset) if asset.id() != self.id() => {
                        eprintln!(
                            "Delegating evaluation of asset {} to asset {}",
                            self.id(),
                            asset.id()
                        );
                        // Record delegation as a dependency wait, then delegate the F-1 inline
                        // guard onto the shared, claim-based wait primitive: it drains this
                        // asset's own local queue, direct-claims the child if still runnable
                        // (no queue slot consumed), or subscribes — guaranteeing progress for
                        // pure-key delegation chains without the old ad-hoc inline run.
                        self.record_dependency_on_asset(&asset).await?;
                        let envref = self.get_envref().await;
                        let manager = envref.get_asset_manager();
                        let state = manager.wait_for_dependency(self, &asset).await?;
                        return Ok(state);
                    }
                    // This asset is the registered owner, or nothing is registered — a
                    // volatile key, which the manager deliberately never registers, or an
                    // untracked asset built from a key recipe. Either way the recipe is
                    // resolved and evaluated here.
                    Some(_) | None => {
                        let recipe = envref
                            .clone()
                            .get_recipe_provider()
                            .recipe(&key, envref.clone())
                            .await?;
                        // Keys are a payload boundary: reject a keyed recipe that requires an
                        // evaluation payload, since no payload can reach a keyed asset.
                        recipe.to_plan_for_key(envref.get_command_metadata_registry(), &key)?;
                        // The asset starts with an ad-hoc key recipe before provider lookup. Once
                        // the stored recipe is resolved, make its identity and human-facing
                        // metadata the asset's authoritative recipe metadata.
                        {
                            let mut lock = self.data.write().await;
                            lock.recipe = recipe.clone();
                            if let Metadata::MetadataRecord(metadata) = &mut lock.metadata {
                                metadata
                                    .with_title(recipe.title.clone())
                                    .with_description(recipe.description.clone());
                            }
                        }
                        eprintln!(
                            "Evaluating asset {} using its own recipe for key {}:\n{}\n",
                            self.id(),
                            key,
                            recipe
                        );
                        (input_state, recipe)
                    }
                }
            } else {
                (input_state, recipe)
            }
        };

        eprintln!("Evaluating recipe {:?}", &recipe);
        let envref = self.get_envref().await;
        /*
        let plan = {
            let cmr = envref.0.get_command_metadata_registry();
            recipe.to_plan(cmr)?
        };
        */
        let context = self.create_context().await;
        let context_for_deps = context.clone(); // shares pending_dependencies Arc
        eprintln!("Applying recipe");
        let res = envref.apply_recipe(input_state, recipe, context).await?;
        //eprintln!("Recipe evaluated, result: {:?}", &res);

        // Collect observed dependencies from context into metadata
        let observed_deps = context_for_deps.take_pending_dependencies().await;

        let mut metadata = self.data.read().await.metadata.clone();
        if let Some(data) = self.data.read().await.data.as_ref() {
            metadata.with_type_identifier(data.identifier().to_string());
            metadata.with_type_name(data.type_name().to_string());
        }

        for dep in observed_deps {
            let _ = metadata.add_dependency(dep);
        }

        Ok(State::from_parts(res, Arc::new(metadata)))
    }

    /// Evaluates the recipe, installs the result, and attempts persistence.
    ///
    /// The asset is made `Ready` or `Volatile` before persistence begins. The
    /// persistence outcome is recorded separately in [`PersistenceStatus`].
    pub async fn evaluate_and_store(&self) -> Result<(), Error> {
        self.resolve_volatility_before_evaluation().await;
        let res = self.evaluate_recipe().await;
        match res {
            Ok(state) => {
                let data = state.data_unchecked().clone();
                let metadata = state.metadata.clone();
                {
                    let mut lock = self.data.write().await;
                    let mut metadata_clone = (*metadata).clone();
                    metadata_clone
                        .with_type_identifier(data.identifier().to_string())
                        .with_type_name(data.type_name().to_string());
                    lock.data = Some(data);
                    lock.metadata = metadata_clone;
                }
                // Finalize status and expiration in one place (replaces inline match block).
                // Must happen before persistence so poll_state() returns Some for serialization.
                self.try_to_set_ready().await;
                let (save_in_background, cancelled, lock_is_volatile) = {
                    let lock = self.data.read().await;
                    let _ = lock
                        .notification_tx
                        .send(AssetNotificationMessage::ValueProduced);
                    (
                        lock.save_in_background,
                        lock.is_cancelled(),
                        lock.is_volatile,
                    )
                };

                self.persist_with_status_tracking(save_in_background, cancelled)
                    .await;

                // Register in DM for non-volatile assets
                if !lock_is_volatile {
                    let envref = self.get_envref().await;
                    let manager = envref.get_asset_manager();
                    let dm = manager.dependency_manager();
                    let expired = dm.track_asset(self).await;
                    manager.expire_dependencies_result(expired).await;
                }

                Ok(())
            }
            Err(e) => {
                eprintln!("Error during evaluation of asset {}: {}", self.id(), e);
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

    /// Evaluates the recipe with an optional payload and installs the value.
    ///
    /// This is the computation used by [`AssetManager::apply_immediately`]. It does
    /// not persist the result. Lifecycle finalization is performed by the
    /// surrounding immediate-run harness.
    ///
    /// Arguments:
    /// - `payload`: Optional execution payload injected into evaluation context.
    pub async fn evaluate_immediately(&self, payload: Option<E::Payload>) -> Result<(), Error> {
        self.resolve_volatility_before_evaluation().await;
        let (input_state, recipe) = self.initial_state_and_recipe().await;

        let envref = self.get_envref().await;
        let mut context = self.create_context().await;
        let context_for_deps = context.clone();
        if let Some(payload) = payload {
            context.set_payload(payload);
        }
        let res = envref.apply_recipe(input_state, recipe, context).await?;
        let observed_deps = context_for_deps.take_pending_dependencies().await;

        let mut lock = self.data.write().await;
        lock.data = Some(res.clone());
        lock.metadata
            .with_type_identifier(res.identifier().to_string())
            .with_type_name(res.type_name().to_string());
        for dep in observed_deps {
            let _ = lock.metadata.add_dependency(dep);
        }
        let _ = lock
            .notification_tx
            .send(AssetNotificationMessage::ValueProduced);
        Ok(())
    }

    /// Persists metadata updates to the configured async store.
    /// Used by: `persist_with_status_tracking`
    async fn save_to_store(&self) -> Result<(), Error> {
        // Check cancelled flag before writing to store (cancel-safety)
        // This prevents orphaned tasks from overwriting data after cancellation
        if self.is_cancelled().await {
            eprintln!(
                "Asset {} cancelled, skipping store write in save_to_store",
                self.id()
            );
            return Ok(());
        }

        // `binary_unchecked`, not `poll_binary`: persisting is not a read of the asset's exposed
        // value, and this path runs at statuses the read gate hides. `AssetRef::set_state`
        // persists with whatever status the caller supplied — reachable from `Context::set_state`
        // — so consulting the gate here would turn a successful persist into an error.
        let mut x = {
            let lock = self.data.read().await;
            lock.binary_unchecked()
        };
        if x.is_none() {
            x = self.serialize_to_binary().await?;
        }

        if let Some((data, metadata)) = x {
            let lock = self.data.read().await;

            // Double-check cancelled flag after potentially long serialization
            if lock.is_cancelled() {
                eprintln!(
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

    /// Persists metadata updates to the configured async store.
    /// Used by: `process_service_messages`
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

    /// Subscribes to best-effort asset-change notifications.
    ///
    /// This is a watch receiver: it retains the latest message rather than every
    /// message. Re-read authoritative asset state after each change.
    pub async fn subscribe_to_notifications(&self) -> watch::Receiver<AssetNotificationMessage> {
        let lock = self.data.read().await;
        lock.notification_tx.subscribe()
    }

    /// Get a clone of the service sender for internal control
    pub(crate) async fn service_sender(&self) -> mpsc::UnboundedSender<AssetServiceMessage> {
        let lock = self.data.read().await;
        lock.service_sender()
    }

    /// Requests cancellation of an in-flight asset.
    ///
    /// Only `Submitted`, `Dependencies`, `Processing`, and `Partial` are
    /// cancellable; other statuses return `Ok(())` without change. Cancellation
    /// sets a flag that prevents later store writes and sends an internal control
    /// message. On native targets this waits for a cancellation or finish
    /// notification for at most five seconds and still returns `Ok(())` on timeout.
    /// On Wasm it returns after setting the flag and sending the message.
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

        // Wait for cancellation. Native: a background task may still be processing, so bound
        // the wait with a 5s timeout. Wasm/immediate: evaluation runs inline in the caller's
        // task, so there is nothing running in the background to await — best-effort cancel is
        // already complete, and `tokio::time` is unavailable anyway.
        #[cfg(not(target_arch = "wasm32"))]
        {
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
                        return Ok(());
                    }
                }
            })
            .await;
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_) => Ok(()), // Timeout, but still return Ok
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(())
        }
    }

    /// Returns whether cancellation has been requested for this asset.
    pub async fn is_cancelled(&self) -> bool {
        let lock = self.data.read().await;
        lock.is_cancelled()
    }

    /// Converts this runtime asset to `Override`, preventing recipe re-evaluation.
    ///
    /// For keyed recovery, prefer [`AssetManager::to_override`], which also
    /// coordinates manager reachability and persistence.
    ///
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

    /// Expires a `Ready` or `Override` asset and cascades to its dependents.
    ///
    /// `Expired` is idempotent. A `Source` cannot expire because it has no recipe
    /// from which to recover. Other statuses return an error.
    pub async fn expire(&self) -> Result<(), Error> {
        let key_opt = {
            let lock = self.data.read().await;
            lock.recipe.key().ok().flatten()
        };

        let transitioned_to_expired = self.mark_expired_status().await?;

        if transitioned_to_expired {
            if let Some(key) = key_opt {
                let dep_key = DependencyKey::from(&key);
                let envref = self.get_envref().await;
                let manager = envref.get_asset_manager();
                manager.cascade_expire_dependents(&dep_key).await;
            }
        }

        Ok(())
    }

    async fn mark_expired_status(&self) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        let mut transitioned_to_expired = false;
        let result = match lock.status {
            Status::Ready | Status::Override => {
                lock.status = Status::Expired;
                transitioned_to_expired = true;
                if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                    mr.status = Status::Expired;
                }
                let _ = lock.notification_tx.send(AssetNotificationMessage::Expired);
                Ok(())
            }
            Status::Expired => Ok(()),
            Status::Source => Err(Error::general_error(
                "Cannot expire Source asset (no recipe to recover)".to_string(),
            )),
            Status::None
            | Status::Directory
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Error
            | Status::Storing
            | Status::Cancelled
            | Status::Volatile => Err(Error::general_error(format!(
                "Cannot expire asset in state {:?}",
                lock.status
            ))),
        };
        // WP-3: for a KEYED asset, persist the Expired status to the store too. Without this,
        // a subsequent store-load (`try_fast_track`) after this in-memory entry is evicted would
        // find the store's metadata still saying `Ready`/`Override` and serve the stale bytes
        // right back, defeating "expired keyed assets are always a cache miss" for any access
        // that happens after eviction. Collected while still holding the lock; the store write
        // itself happens after dropping it.
        let persist_info = if transitioned_to_expired {
            lock.recipe
                .key()
                .ok()
                .flatten()
                .map(|key| (key, lock.metadata.clone(), lock.get_envref()))
        } else {
            None
        };
        drop(lock);

        if let Some((key, metadata, envref)) = persist_info {
            let store = envref.get_async_store();
            // Only rewrite metadata for a key the store already has. A value that was never
            // persisted (e.g. `PersistenceStatus::NonSerializable`) has no store entry to
            // invalidate, and `set_metadata` on a missing key would otherwise create a phantom
            // entry with empty bytes — breaking "nothing is written to the store" for that case.
            let already_persisted = store.contains(&key).await.unwrap_or(false);
            if already_persisted {
                if let Err(e) = store.set_metadata(&key, &metadata).await {
                    eprintln!(
                        "Asset {} mark_expired_status(): failed to persist Expired status to store: {}",
                        self.id(),
                        e
                    );
                }
            }
        }

        result.map(|_| transitioned_to_expired)
    }

    pub(crate) async fn expire_without_cascade(&self) -> Result<(), Error> {
        self.mark_expired_status().await.map(|_| ())
    }

    /// Returns the resolved expiration time.
    pub async fn expiration_time(&self) -> ExpirationTime {
        self.data.read().await.expiration_time.clone()
    }

    /// Sets the resolved expiration time without scheduling a monitor entry.
    pub async fn set_expiration_time(&self, et: ExpirationTime) {
        let mut lock = self.data.write().await;
        lock.expiration_time = et;
    }

    /// Returns whether the current status is `Expired`.
    pub async fn is_expired(&self) -> bool {
        self.data.read().await.status == Status::Expired
    }

    /// Returns whether this asset is volatile: flagged before evaluation, or volatile as a
    /// final status.
    ///
    /// A volatile asset is never owned by an asset manager and is never reused — see
    /// [`AssetManager::owned_key_asset`].
    ///
    /// Deliberately does **not** consult [`Metadata::is_volatile`], which is also true for an
    /// `Override` entry whose stored metadata carries the flag. That is the user-supplied
    /// override, and it must stay reusable.
    pub async fn is_volatile(&self) -> bool {
        let lock = self.data.read().await;
        lock.is_volatile || lock.status == Status::Volatile
    }

    /// Schedule automatic expiration via the centralized expiration monitor.
    /// Routes through `envref` to `DefaultAssetManager::track_expiration`.
    /// Only `ExpirationTime::At(_)` is tracked; Never/Immediately are no-ops.
    pub(crate) async fn schedule_expiration(&self, expiration_time: &ExpirationTime) {
        if let ExpirationTime::At(_) = expiration_time {
            let envref = self.get_envref().await;
            envref
                .get_asset_manager()
                .track_expiration(self, expiration_time);
        }
    }

    /// Waits for and returns a state exposed by the asset.
    ///
    /// Evaluation must already have been started by an [`AssetManager`]; otherwise
    /// this can wait indefinitely. Error and cancelled terminal states are exposed
    /// as no-value states with diagnostic metadata. If the asset expires while
    /// waiting, this returns an error.
    pub async fn get(&self) -> Result<State<E::Value>, Error> {
        if let Some(state) = self.poll_state().await {
            return Ok(state);
        }

        // Pre-wait expiry check. `poll_state` hides expired data, so without this an
        // already-expired asset would subscribe below and wait for a notification that has
        // already been sent and will not come again. This mirrors `get_binary`, and matches the
        // error this method already returns when expiry is observed *while* waiting.
        let status = self.status().await;
        if status.read_exposure() == ReadExposure::Expired {
            return Err(self.expired_read_error(status).await);
        }

        // Subscribe to notifications before starting evaluation
        let mut rx = self.subscribe_to_notifications().await;

        // Wait for either notifications or run completion

        loop {
            // Notification channels are lossy wrt intermediate states (watch keeps only latest).
            // Always check authoritative asset state first to avoid waiting forever if
            // ValueProduced was overwritten by a later progress/log notification.
            if let Some(state) = self.poll_state().await {
                return Ok(state);
            }

            let notification = rx.borrow().clone();
            match notification {
                AssetNotificationMessage::ValueProduced => {
                    if let Some(state) = self.poll_state().await {
                        return Ok(state);
                    }
                }
                // The notification CONTENT is no longer matched for terminal decisions: a failed
                // asset now has a terminal error-state that poll_state() returns as Ok(...). This
                // arm is a pure wake-up, so an overwritten ErrorOccurred cannot lose the error.
                AssetNotificationMessage::ErrorOccurred(_) => {}
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
                AssetNotificationMessage::Expired => {
                    return Err(Error::general_error(
                        "Asset expired while waiting for data".to_owned(),
                    ));
                }
            }
            rx.changed().await.map_err(|e| {
                Error::general_error(format!("Failed to receive notification: {}", e))
            })?;
        }
    }

    /// Returns a serialized representation and its matching metadata.
    ///
    /// A cached binary is returned immediately. Otherwise this waits through
    /// [`Self::get`] and serializes the value. The binary and metadata are returned
    /// together to preserve their association.
    pub async fn get_binary(&self) -> Result<(Arc<Vec<u8>>, Arc<Metadata>), Error> {
        // Consult the status BEFORE short-circuiting on cached bytes. Without this, an expired
        // asset returns stale bytes; and with the gate but without the check, it would fall
        // through to `get()` and block on a notification that is never coming.
        let (exposure, status) = {
            let lock = self.data.read().await;
            (lock.status.read_exposure(), lock.status)
        };
        match exposure {
            ReadExposure::Expired => return Err(self.expired_read_error(status).await),
            ReadExposure::MetadataOnly => return Err(self.no_binary_error(status).await),
            ReadExposure::Value | ReadExposure::Pending => {}
        }

        if let Some(b) = self.poll_binary().await {
            return Ok(b);
        }
        self.get().await?;
        // The wait may have finished into a status with no binary — re-derive rather than assume.
        let status = self.status().await;
        match status.read_exposure() {
            ReadExposure::Expired => return Err(self.expired_read_error(status).await),
            ReadExposure::MetadataOnly => return Err(self.no_binary_error(status).await),
            ReadExposure::Value | ReadExposure::Pending => {}
        }
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

    /// The error reported when a normal read meets `Status::Expired`.
    ///
    /// Shared by [`Self::get`] and [`Self::get_binary`] so the two report the identical condition
    /// identically — the symmetry rule as a caller experiences it.
    async fn expired_read_error(&self, status: Status) -> Error {
        let _ = status;
        Error::general_error(format!(
            "Asset {} is expired; use get_any_status or get_binary_any_status to read retained \
             data, or to_override to accept it",
            self.asset_reference().await
        ))
    }

    /// The error reported when a binary read meets a status that has no binary representation.
    ///
    /// `Error` reuses the asset's own recorded failure so the traceback survives; `Cancelled` and
    /// `Directory` record none, so one is constructed. They are separate arms despite sharing a
    /// `ReadExposure` precisely because of that difference.
    async fn no_binary_error(&self, status: Status) -> Error {
        match status {
            Status::Error => {
                let state = self.poll_state().await;
                match state.and_then(|s| s.value_error()) {
                    Some(e) => e,
                    None => Error::general_error(format!(
                        "Asset {} failed; no binary representation",
                        self.asset_reference().await
                    )),
                }
            }
            Status::Cancelled => Error::general_error(format!(
                "Asset {} was cancelled; no binary representation",
                self.asset_reference().await
            )),
            Status::Directory => Error::general_error(format!(
                "Asset {} is a directory; no binary representation",
                self.asset_reference().await
            )),
            Status::None
            | Status::Recipe
            | Status::Submitted
            | Status::Dependencies
            | Status::Processing
            | Status::Partial
            | Status::Storing
            | Status::Expired
            | Status::Ready
            | Status::Source
            | Status::Override
            | Status::Volatile => Error::unexpected_error(format!(
                "Asset {} has status {:?}, which has a binary representation; \
                 no_binary_error should not have been called",
                self.asset_reference().await,
                status
            )),
        }
    }

    /// Returns the current authoritative status.
    pub async fn status(&self) -> Status {
        let lock = self.data.read().await;
        lock.status
    }

    /// set status
    pub(crate) async fn set_status(&self, status: Status) -> Result<(), Error> {
        let mut lock = self.data.write().await;
        lock.set_status(status)
    }

    /// Returns the currently exposed state without waiting.
    ///
    /// Expired data is intentionally hidden; use
    /// [`Self::poll_state_any_status`] for explicit recovery.
    pub async fn poll_state(&self) -> Option<State<E::Value>> {
        let lock = self.data.read().await;
        lock.poll_state()
    }

    /// Polls state while also allowing retained `Expired` data.
    pub async fn poll_state_any_status(&self) -> Option<State<E::Value>> {
        let lock = self.data.read().await;
        lock.poll_state_any_status()
    }

    /// Returns any currently exposed state without waiting for notifications.
    ///
    /// This is an alias for [`Self::poll_state_any_status`].
    pub async fn get_any_status(&self) -> Option<State<E::Value>> {
        self.poll_state_any_status().await
    }

    /// Attempts to poll state without waiting for the asset lock.
    ///
    /// Returns `None` if state is unavailable or the read lock cannot be acquired
    /// immediately. Expired data remains hidden.
    pub fn try_poll_state(&self) -> Option<State<E::Value>> {
        if let Ok(lock) = self.data.try_read() {
            lock.poll_state()
        } else {
            None
        }
    }

    /// Returns cached binary data and its metadata without waiting.
    ///
    /// Expired data is intentionally hidden, exactly as in [`Self::poll_state`]; use
    /// [`Self::poll_binary_any_status`] or [`Self::get_binary_any_status`] for explicit recovery.
    pub async fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        let lock = self.data.read().await;
        lock.poll_binary()
    }

    /// Polls cached binary while also allowing retained `Expired` data.
    ///
    /// Reports only what is cached — see [`Self::get_binary_any_status`] to materialise bytes from
    /// a retained value.
    pub async fn poll_binary_any_status(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        let lock = self.data.read().await;
        lock.poll_binary_any_status()
    }

    /// Recovery read for binary data: returns retained bytes regardless of status, serializing
    /// from the retained value when nothing is cached.
    ///
    /// This is the binary twin of [`Self::get_any_status`], but **not** a mere alias for
    /// [`Self::poll_binary_any_status`] the way `get_any_status` aliases `poll_state_any_status`.
    /// A retained value needs no materialising, so the state side can alias; bytes do. Since
    /// `AssetData::binary` is populated at two sites and cleared at roughly ten, an expired asset
    /// commonly retains its value and no bytes at all — and that is precisely when a caller needs
    /// recovery, so returning `None` there would make the escape hatch useless.
    ///
    /// `Ok(None)` means nothing is retained; `Err` means retained but not serializable.
    pub async fn get_binary_any_status(
        &self,
    ) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error> {
        if let Some(binary) = self.poll_binary_any_status().await {
            return Ok(Some(binary));
        }
        // Only a value-bearing or retained-expired state can be serialized. `poll_state_any_status`
        // also returns a metadata-only sentinel for Directory/Error/Cancelled, and serializing that
        // is wrong in two different ways: `Error` would surface the recorded value error as if
        // serialization had failed, while `Cancelled` and `Directory` have no value error and would
        // fabricate bytes from a `none` value. Those statuses have no binary representation at all,
        // so the answer is `Ok(None)` — matching the manager's store-backed path, which already
        // declines them via `has_data()`.
        let (exposure, state) = {
            let lock = self.data.read().await;
            (lock.status.read_exposure(), lock.poll_state_any_status())
        };
        match exposure {
            ReadExposure::Value | ReadExposure::Expired => {}
            ReadExposure::MetadataOnly | ReadExposure::Pending => return Ok(None),
        }
        let Some(state) = state else {
            return Ok(None);
        };
        let binary = Arc::new(state.as_bytes()?);
        {
            let mut lock = self.data.write().await;
            lock.binary = Some(binary.clone());
        }
        Ok(Some((binary, state.metadata.clone())))
    }

    /// Attempts to poll cached binary data without waiting for the asset lock.
    ///
    /// Returns `None` if the lock cannot be acquired immediately, or if the status does not permit
    /// exposing the bytes. Expired data remains hidden.
    pub fn try_poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)> {
        if let Ok(lock) = self.data.try_read() {
            lock.poll_binary()
        } else {
            None
        }
    }

    /// Set the asset value.
    /// Updates the value and publishes asset lifecycle notifications to subscribers and persists the value in store (if possible).
    /// This is not a public method and should not be used outside the core.
    /// Only certain assets are allowed to be set (overriden) by the user.
    /// Use AssetManager::set_state instead.
    pub(crate) async fn set_value(&self, value: <E as Environment>::Value) -> Result<(), Error> {
        // TODO: Remove?
        eprintln!("Setting value for asset {}", self.id());
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
        eprintln!("Setting state for asset {}", self.id());
        let mut lock = self.data.write().await;
        let data = state.data_unchecked().clone();
        lock.data = Some(data);
        let mut merged_metadata = (*state.metadata).clone();
        merged_metadata.with_type_identifier(state.data_unchecked().identifier().to_string());
        merged_metadata.with_type_name(state.data_unchecked().type_name().to_string());
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

    /// The single "asset failed" routine. Puts the asset into a terminal `Status::Error`,
    /// **preserving** the metadata audit trail (log, query, type info) via `metadata.with_error`
    /// (NOT `Metadata::from_error`, which replaces the record). Clears data and binary, records
    /// the typed error, and notifies subscribers once.
    ///
    /// Idempotent: if the asset is already in `Status::Error`, the first error is kept.
    /// Used ONLY for computed errors — cancellation is a separate status (`Status::Cancelled`)
    /// that stores no error and does not use this routine.
    pub(crate) async fn fail_asset(&self, error: Error) -> Result<(), Error> {
        let notification_tx = {
            let mut lock = self.data.write().await;
            if lock.status == Status::Error {
                return Ok(()); // already failed; keep the first error
            }
            lock.data = None;
            lock.binary = None;
            lock.status = Status::Error;
            lock.metadata.with_error(error.clone());
            let _ = lock.metadata.set_status(Status::Error);
            lock.metadata
                .set_primary_progress(&ProgressEntry::done("Error".to_string()));
            lock.notification_tx.clone()
        };
        // A persistence hiccup here must not mask the computed error, so ignore its result.
        let _ = self.save_metadata_to_store().await;
        let _ = notification_tx.send(AssetNotificationMessage::ErrorOccurred(error));
        let _ = notification_tx.send(AssetNotificationMessage::JobFinished);
        Ok(())
    }

    /// Sets an error state for the asset, with the provided error information.
    /// Delegates to the unified [`Self::fail_asset`] routine (metadata-preserving).
    pub(crate) async fn set_error(&self, error: Error) -> Result<(), Error> {
        self.fail_asset(error).await
    }
}

impl<E: Environment> WeakAssetRef<E> {
    /// Get the unique id of the asset.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Try to upgrade to a strong [AssetRef].
    pub fn upgrade(&self) -> Option<AssetRef<E>> {
        self.data
            .upgrade()
            .map(|data| AssetRef { id: self.id, data })
    }
}

/// Evaluation mode of a manager, not of an individual asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalMode {
    /// Ordinary get/apply operations schedule work on a background `JobQueue`.
    Queued,
    /// Ordinary get/apply operations run in the caller's task before returning.
    Inline,
}

/// Shared helper: register command metadata/impl versions into the dependency manager.
/// Extracted from the former `DefaultAssetManager::load_command_versions` so both managers'
/// `start()` reuse one implementation. Idempotent.
pub(crate) async fn load_command_versions<E: Environment>(
    dm: &crate::dependencies::DependencyManager<E>,
    cmr: &crate::command_metadata::CommandMetadataRegistry,
) {
    for cmd in &cmr.commands {
        let ck = cmd.key();
        if !cmd.metadata_version.is_unknown() {
            dm.register_version(
                &crate::metadata::DependencyKey::for_command_metadata(&ck),
                cmd.metadata_version,
            )
            .await;
        }
        if !cmd.impl_version.is_unknown() {
            dm.register_version(
                &crate::metadata::DependencyKey::for_command_implementation(&ck),
                cmd.impl_version,
            )
            .await;
        }
    }
}

/// Asset evaluation, keyed mutation, recovery, directory, and lifecycle service.
///
/// An [`Environment`] selects one implementation through its `AssetManager`
/// associated type. `DefaultAssetManager` provides queued native execution;
/// [`ImmediateAssetManager`] provides inline execution. Call [`Self::start`] after
/// the environment back-reference has been installed; environment initialization
/// normally performs that lifecycle step.
///
/// Methods such as [`Self::set_binary`], [`Self::set_state`], [`Self::remove`],
/// [`Self::get_any_status`], and [`Self::to_override`] operate only on keyed
/// assets. Query evaluation is available through [`Self::get_asset`].
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait AssetManager<E: Environment>:
    crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync
{
    /// Resolves a query to an asset.
    ///
    /// A pure key query delegates to [`Self::get`]. The return-time guarantee
    /// depends on [`Self::eval_mode`]: queued managers may return a scheduled
    /// handle, while inline managers return after evaluation.
    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error>;
    /// Applies a recipe to a supplied initial state.
    ///
    /// The operation is ad hoc and does not insert the asset into a key/query
    /// cache. Queued managers schedule it; inline managers evaluate before return.
    async fn apply(&self, recipe: Recipe, to: State<E::Value>) -> Result<AssetRef<E>, Error>;
    /// Applies a recipe to a supplied state and evaluates it before returning.
    ///
    /// This ad-hoc path accepts an optional execution payload, bypasses the queued
    /// manager's job queue, and does not persist the produced result.
    async fn apply_immediately(
        &self,
        recipe: Recipe,
        to: State<E::Value>,
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error>;
    /// Resolves a keyed asset.
    ///
    /// Managers may reuse a non-volatile cached entry or fast-track a stored value.
    /// Cached `Expired`, `Error`, and `Cancelled` entries are treated as misses.
    /// Return timing depends on [`Self::eval_mode`].
    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error>;
    /// Returns the recipe for a key, if one exists.
    ///
    /// A recipe that requires an evaluation payload is rejected here: keys are a payload
    /// boundary. This is the earliest funnel where a recipe is resolved *for a key*, so the
    /// rejection precedes asset creation and volatility resolution.
    async fn recipe_opt(&self, key: &Key) -> Result<Option<Recipe>, Error> {
        let recipe = self
            .get_recipe_provider()
            .recipe_opt(key, self.get_envref())
            .await?;
        if let Some(recipe) = &recipe {
            let envref = self.get_envref();
            recipe.to_plan_for_key(envref.get_command_metadata_registry(), key)?;
        }
        Ok(recipe)
    }
    /// Returns whether the keyed resource's recipe is volatile.
    async fn is_volatile(&self, key: &Key) -> Result<bool, Error> {
        if let Some(recipe) = self.recipe_opt(key).await? {
            Ok(recipe.is_volatile(self.get_envref()).await?)
        } else {
            Ok(false)
        }
    }

    /// Resolve the asset for `query` and schedule it as a dependency of `parent`: start it
    /// immediately if queue capacity allows, otherwise enqueue it on `parent`'s local
    /// dependency queue (local-only parking — NOT parked in the global jobs list here).
    /// Does NOT record dependency facts or cycle-check — `Context` does that.
    ///
    /// Default: plain `get_asset` (global submit) — a safe fallback for managers without a
    /// JobQueue; `DefaultAssetManager` overrides.
    async fn get_dependency_asset(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
    ) -> Result<AssetRef<E>, Error> {
        let _ = parent;
        self.get_asset(query).await
    }

    /// Resolve the asset for `query` as a dependency of `parent`, forwarding `payload` to
    /// the evaluation.
    ///
    /// Called instead of [`Self::get_dependency_asset`] when the dependency's plan requires
    /// a payload. Such an asset is evaluated inline rather than through a job queue: it is
    /// volatile by construction, so it is fresh, unshared, and never persisted or reused,
    /// and there is nothing to gain by queueing it.
    ///
    /// Default: ignore the payload and delegate, preserving existing behavior for managers
    /// that have not opted in. Implementors that support payload evaluation override this.
    async fn get_dependency_asset_with_payload(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
        payload: Option<E::Payload>,
        payload_path: Vec<Query>,
    ) -> Result<AssetRef<E>, Error> {
        let _ = (payload, payload_path);
        self.get_dependency_asset(parent, query).await
    }

    /// Drain `parent`'s local dependency queue: claim and inline-run each still-runnable
    /// entry sequentially inside the caller's future. Default: no-op (managers without
    /// local queues have nothing to drain).
    async fn drain_dependencies(&self, parent: &AssetRef<E>) -> Result<(), Error> {
        let _ = parent;
        Ok(())
    }

    /// Claim-aware wait for `dependency` on behalf of `parent`: guarantees progress and
    /// maintains `Status::Dependencies` on the parent while waiting. Default: enter
    /// dependencies, await the dependency's own `get()`, then resume (correct, but without
    /// the local-queue progress guarantee — fine for managers whose `get_asset` submits
    /// globally).
    async fn wait_for_dependency(
        &self,
        parent: &AssetRef<E>,
        dependency: &AssetRef<E>,
    ) -> Result<State<E::Value>, Error> {
        parent.enter_dependencies(dependency).await?;
        let result = dependency.get().await;
        parent.leave_dependencies_and_resume().await?;
        result
    }

    /// Remove asset data from AssetManager and store.
    /// This does NOT trigger recalculation.
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        if let Some(asset_ref) = self.lookup_key_asset(key) {
            asset_ref.cancel().await?;
            self.untrack_expiration(asset_ref.id());
            self.remove_key_asset(key).await;
        }
        self.dependency_manager()
            .remove(&crate::metadata::DependencyKey::from(key))
            .await;
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            store.remove(key).await?;
        }
        Ok(())
    }

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
    async fn set_binary(
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

        if let Some(asset_ref) = self.lookup_key_asset(key) {
            asset_ref.cancel().await?;
            self.untrack_expiration(asset_ref.id());
            self.remove_key_asset(key).await;
        }

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

        metadata.set_updated_now();
        metadata.add_log_entry(LogEntry::info("Data set externally".to_string()));

        let dep_key = crate::metadata::DependencyKey::from(key);
        if final_status != Status::Volatile && final_status != Status::Error {
            let version = crate::metadata::Version::from_bytes(binary);
            metadata.version = Some(version);
        }

        let store = self.get_envref().get_async_store();
        if final_status == Status::Error {
            store.set(key, &[], &metadata.clone().into()).await?;
        } else {
            store.set(key, binary, &metadata.clone().into()).await?;
        }

        if matches!(
            final_status,
            Status::Ready | Status::Source | Status::Override
        ) {
            if let Some(version) = metadata.version {
                let expired = self
                    .dependency_manager()
                    .register_version(&dep_key, version)
                    .await;
                self.expire_dependencies_result(expired).await;
            }
        }
        Ok(())
    }

    /// Set State (data + metadata) for a key asset.
    /// - Sets deserialized data and metadata from State
    /// - Memory + Store: Creates new AssetRef with State AND serializes to store
    /// - Supports non-serializable data (store metadata only if serialization fails)
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

        if let Some(asset_ref) = self.lookup_key_asset(key) {
            asset_ref.cancel().await?;
            self.untrack_expiration(asset_ref.id());
            self.remove_key_asset(key).await;
        }

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
                if self.recipe_opt(key).await?.is_some() {
                    Status::Override
                } else {
                    Status::Source
                }
            }
        };

        let mut metadata = state.metadata.as_ref().clone();
        metadata.set_status(final_status)?;
        validate_required_metadata_fields(key, &metadata)?;
        add_soft_consistency_warnings(&mut metadata)?;
        metadata.set_updated_now()?;
        metadata.add_log_entry(LogEntry::info("State set externally".to_string()))?;

        let dep_key = crate::metadata::DependencyKey::from(key);
        if final_status != Status::Volatile && final_status != Status::Error {
            let version = match state.as_bytes() {
                Ok(binary) => crate::metadata::Version::from_bytes(&binary),
                Err(_) => crate::metadata::Version::from_time_now(),
            };
            metadata.set_version(Some(version))?;
        }

        let recipe: Recipe = key.into();
        let mut asset_data =
            AssetData::new_ext(self.next_id_for_asset(), recipe, State::new(), self.get_envref());
        asset_data.data = Some(Arc::new(state.data_unchecked().as_ref().clone()));
        asset_data.metadata = metadata.clone();
        asset_data.status = final_status;
        asset_data.binary = None;

        let asset_ref = asset_data.to_ref();
        self.insert_key_asset(key, asset_ref.clone()).await;

        let store = self.get_envref().get_async_store();
        if final_status == Status::Error {
            store.set(key, &[], &metadata.clone().into()).await?;
        } else {
            match state.as_bytes() {
                Ok(binary) => {
                    store.set(key, &binary, &metadata.clone().into()).await?;
                }
                Err(_) => {
                    store.set_metadata(key, &metadata.clone().into()).await?;
                }
            }
        }

        if matches!(
            final_status,
            Status::Ready | Status::Source | Status::Override
        ) {
            if let Some(version) = metadata.version() {
                let expired = self
                    .dependency_manager()
                    .register_version(&dep_key, version)
                    .await;
                self.expire_dependencies_result(expired).await;
            }
            let expired = self.dependency_manager().track_asset(&asset_ref).await;
            self.expire_dependencies_result(expired).await;
        }
        Ok(())
    }

    /// Get asset info
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> {
        if self.lookup_key_asset(key).is_some() {
            let assetref = self.get(key).await?;
            assetref.get_asset_info().await
        } else {
            let store = self.get_envref().get_async_store();
            if store.contains(key).await? {
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

    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            return Ok(true);
        }
        self.get_recipe_provider()
            .contains(key, self.get_envref())
            .await
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.listdir_keys_deep(&Key::new()).await
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
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

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        Ok(self
            .listdir(key)
            .await?
            .into_iter()
            .map(|name| key.join(name))
            .collect::<Vec<Key>>())
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

    /// Return asset info of assets inside a directory specified by key.
    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
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

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        let store = self.get_envref().get_async_store();
        let _sink = store.makedir(key).await?;
        self.get(key).await
    }

    // --- lifecycle / manager-primitive methods (moved from the concrete manager) ---

    /// Set the environment back-reference. Called once from `Environment::init_with_envref`.
    fn set_envref(&self, envref: EnvRef<E>);

    /// Access the runtime dependency manager.
    fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E>;

    /// This manager's evaluation mode (constant per implementation).
    fn eval_mode(&self) -> EvalMode;

    /// Look up a cached asset by key in this manager's key→asset map (sync, brief).
    fn lookup_key_asset(&self, key: &Key) -> Option<AssetRef<E>>;

    /// Remove a cached asset by key from this manager's key→asset map.
    async fn remove_key_asset(&self, key: &Key);

    /// Remove this key's entry only if it is still the asset with `asset_id`, reporting
    /// whether an entry was removed.
    ///
    /// The id check is what keeps a slow caller from evicting a replacement registered after
    /// its own lookup. The default below is lookup-compare-remove, which is correct but not
    /// atomic; both in-tree managers override it to do the whole thing under their map lock.
    async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool {
        match self.lookup_key_asset(key) {
            Some(existing) if existing.id() == asset_id => {
                self.remove_key_asset(key).await;
                true
            }
            Some(_) | None => false,
        }
    }

    /// The asset currently registered as this key's owner, if any.
    ///
    /// **Non-evaluating.** This reads the key→asset map and never starts, submits, fast-tracks
    /// or resolves an evaluation, which is what makes it usable from *inside* an evaluation —
    /// [`AssetRef::evaluate_recipe`] asks it whether it owns the key it is evaluating. Asking
    /// [`AssetManager::get`] instead recurses forever under an inline manager, which is the
    /// defect this method exists to remove.
    ///
    /// A volatile asset is never an owner: it cannot be shared and cannot be reused, so a
    /// volatile entry is dropped and `None` is returned. `None` therefore means "no asset is
    /// registered for this key", and the caller owns the key's recipe itself.
    async fn owned_key_asset(&self, key: &Key) -> Option<AssetRef<E>> {
        let asset = self.lookup_key_asset(key)?;
        if asset.is_volatile().await {
            self.remove_key_asset_if(key, asset.id()).await;
            return None;
        }
        Some(asset)
    }

    /// Insert an asset into this manager's key→asset map.
    async fn insert_key_asset(&self, key: &Key, asset: AssetRef<E>);

    /// Next monotonic asset id.
    fn next_id_for_asset(&self) -> u64;

    /// The environment reference (set via `set_envref`). Panics if not yet set.
    fn get_envref(&self) -> EnvRef<E>;

    /// Cancel any pending expiration tracking for the given asset id. Immediate managers no-op.
    fn untrack_expiration(&self, asset_id: u64) {
        let _ = asset_id;
    }

    /// The recipe provider (via the environment).
    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<E>> {
        self.get_envref().get_recipe_provider()
    }

    /// Create a temporary (non-addressable) asset owned by this manager.
    fn create_temporary_asset(&self) -> AssetRef<E>;

    /// Idempotent startup: register command versions (calls the shared
    /// internal `load_command_versions` helper). Eager on `DefaultAssetManager`, lazy on immediate.
    async fn start(&self);

    /// Schedule expiration tracking for a Ready asset. Immediate managers no-op (lazy check).
    fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime);

    /// Remove an expired `AssetRef` from this manager's in-memory maps (used by the
    /// expiration monitor, reached generically via `envref.get_asset_manager()`).
    async fn remove_expired_from_maps(
        &self,
        asset_id: u64,
        query: Option<&Query>,
        key: Option<&Key>,
    ) -> bool;

    // --- shared default methods (identical for all managers; see Q1) ---

    /// Cascade-expire all dependents of a changed dependency key.
    async fn cascade_expire_dependents(&self, dep_key: &crate::metadata::DependencyKey) {
        let expired = self.dependency_manager().expire(dep_key).await;
        self.expire_dependencies_result(expired).await;
    }

    /// Apply an `ExpiredDependents` result: expire keyed and untracked assets.
    async fn expire_dependencies_result(&self, expired: crate::dependencies::ExpiredDependents<E>) {
        for dk in &expired.keys {
            if let Ok(k) = Key::try_from(dk) {
                if let Some(ar) = self.lookup_key_asset(&k) {
                    let _ = ar.expire_without_cascade().await;
                }
            }
        }
        for weak_ref in &expired.assets {
            if let Some(ar) = weak_ref.upgrade() {
                let _ = ar.expire_without_cascade().await;
            }
        }
    }

    /// Register plan dependencies into the dependency manager.
    async fn register_plan_dependencies(
        &self,
        dependent_key: &Key,
        plan_deps: &[crate::dependencies::PlanDependency],
    ) -> Result<(), Error> {
        let dep_key = crate::metadata::DependencyKey::from(dependent_key);
        for plan_dep in plan_deps {
            if let Some(ver) = self.dependency_manager().get_version(&plan_dep.key).await {
                if let Ok(expired) = self
                    .dependency_manager()
                    .add_dependency(&dep_key, &plan_dep.key, ver)
                    .await
                {
                    self.expire_dependencies_result(expired).await;
                }
            }
        }
        Ok(())
    }

    /// Recovery-only read for a KEYED asset: returns its state regardless of status — including
    /// `Status::Expired` — without submitting evaluation, without touching the dependency
    /// manager, and without registering the entry back into the manager's normal in-memory
    /// cache. `Ok(None)` if the key has no data-bearing state (in memory or in the store).
    async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error> {
        if let Some(asset_ref) = self.lookup_key_asset(key) {
            return Ok(asset_ref.get_any_status().await);
        }
        let store = self.get_envref().get_async_store();
        if !store.contains(key).await? {
            return Ok(None);
        }
        let (binary, metadata) = store.get(key).await?;
        if !metadata.status().has_data() {
            return Ok(None);
        }
        let type_identifier = metadata.type_identifier()?;
        let data_format = metadata.get_data_format();
        let value = deserialize_stored_value::<E>(&binary, &type_identifier, &data_format)?;
        Ok(Some(State::from_parts(Arc::new(value), Arc::new(metadata))))
    }

    /// Recovery-only binary read for a KEYED asset: returns its serialized form regardless of
    /// status — including `Status::Expired` — without submitting evaluation, without touching the
    /// dependency manager, and without registering the entry back into the manager's normal
    /// in-memory cache. `Ok(None)` if the key has no data-bearing content in memory or in store.
    ///
    /// The binary twin of [`Self::get_any_status`], and cheaper than it on the store path: the
    /// bytes are returned as read, with no `deserialize_stored_value` round-trip. That also makes
    /// it usable for a stored type this build cannot deserialize.
    async fn get_binary_any_status(
        &self,
        key: &Key,
    ) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error> {
        if let Some(asset_ref) = self.lookup_key_asset(key) {
            // Serializes on demand, so an in-memory expired asset holding a value but no cached
            // bytes is recoverable rather than reported absent.
            return asset_ref.get_binary_any_status().await;
        }
        let store = self.get_envref().get_async_store();
        if !store.contains(key).await? {
            return Ok(None);
        }
        let (binary, metadata) = store.get(key).await?;
        // `has_data()` is the right question here — this asks whether the store entry holds a
        // value at all, not whether a reader may see it. That is the distinction from
        // `ReadExposure`, which gates reads.
        if !metadata.status().has_data() {
            return Ok(None);
        }
        Ok(Some((Arc::new(binary), Arc::new(metadata))))
    }

    /// Pin a KEYED asset's current value (whatever status it is in — `Ready`, `Expired`, etc.) as
    /// `Status::Override`, preserving the value without recomputation. Avoids double-
    /// serialization: if the value is already correctly persisted, only the metadata's status
    /// field is rewritten; if it was never serializable, nothing is written to the store; if the
    /// original save failed or was never attempted, a fresh persist is retried. Errors
    /// (`Error::key_not_found`) if there is no data-bearing state for `key` to promote.
    async fn to_override(&self, key: &Key) -> Result<(), Error> {
        if let Some(asset_ref) = self.lookup_key_asset(key) {
            asset_ref.to_override().await?;
            match asset_ref.persistence_status().await {
                PersistenceStatus::Persisted => {
                    let metadata = asset_ref.get_metadata().await?;
                    self.get_envref()
                        .get_async_store()
                        .set_metadata(key, &metadata)
                        .await?;
                }
                PersistenceStatus::NonSerializable => {}
                PersistenceStatus::NotPersisted | PersistenceStatus::None => {
                    asset_ref.persist_with_status_tracking(false, false).await;
                }
            }
            // Idempotent: ensures the now-Override entry is reachable even if a concurrent
            // eviction (lazy expiry check, monitor) raced it out of the map in between.
            self.insert_key_asset(key, asset_ref).await;
            return Ok(());
        }
        let store = self.get_envref().get_async_store();
        if !store.contains(key).await? {
            return Err(Error::key_not_found(key));
        }
        let (_binary, mut metadata) = store.get(key).await?;
        if !metadata.status().has_data() {
            return Err(Error::key_not_found(key));
        }
        metadata.set_status(Status::Override)?;
        store.set_metadata(key, &metadata).await?;
        Ok(())
    }
}

/// Heap element for the expiration monitor priority queue.
/// Ordered by expiration time (ascending, earliest first), ties broken by asset_id.
/// Wrapped in `std::cmp::Reverse` for use in `BinaryHeap` (min-heap).
#[cfg(not(target_arch = "wasm32"))]
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: WeakAssetRef<E>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> PartialEq for TimedAsset<E> {
    fn eq(&self, other: &Self) -> bool {
        self.expiration == other.expiration && self.asset_id == other.asset_id
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> Eq for TimedAsset<E> {}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> PartialOrd for TimedAsset<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> Ord for TimedAsset<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.expiration
            .cmp(&other.expiration)
            .then(self.asset_id.cmp(&other.asset_id))
    }
}

/// Message for expiration monitor task
#[cfg(not(target_arch = "wasm32"))]
enum ExpirationMonitorMessage<E: Environment> {
    /// Track an asset for expiration
    Track {
        asset_ref: AssetRef<E>,
        expiration_time: ExpirationTime,
    },
    /// Untrack an asset by its unique id (e.g., removed or recalculated)
    Untrack { asset_id: u64 },
    /// Shut down the monitor task
    Shutdown,
}

/// Native queued asset manager with in-memory reuse, a bounded-concurrency job
/// queue, dependency tracking, store fast-track loading, and an expiration monitor.
///
/// Construction spawns the job-queue and expiration-monitor tasks and therefore
/// requires an active Tokio runtime. [`Self::new`] uses a job capacity of four;
/// [`Self::with_capacity`] selects another maximum. Call [`Self::shutdown`] for an
/// orderly stop when the environment's lifecycle does not simply drop the manager.
#[cfg(not(target_arch = "wasm32"))]
pub struct DefaultAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    job_queue: Arc<JobQueue<E>>,
    /// Channel to send messages to the expiration monitor task
    monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage<E>>,
    /// Runtime dependency graph for cascade expiration
    dependency_manager: crate::dependencies::DependencyManager<E>,
    /// Maximum retries for DependencyVersionMismatch during evaluation
    max_dependency_retries: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> Default for DefaultAssetManager<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> DefaultAssetManager<E> {
    /// Constructs and initializes a default asset manager
    pub fn new() -> Self {
        Self::with_capacity(4)
    }

    /// Constructs and initializes a default asset manager with a custom job capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let job_queue = Arc::new(JobQueue::new(capacity));
        let (monitor_tx, monitor_rx) = mpsc::unbounded_channel();
        let manager = DefaultAssetManager {
            id: std::sync::atomic::AtomicU64::new(1000),
            envref: std::sync::OnceLock::new(),
            assets: scc::HashMap::new(),
            query_assets: scc::HashMap::new(),
            job_queue: job_queue.clone(),
            monitor_tx,
            dependency_manager: crate::dependencies::DependencyManager::new(),
            max_dependency_retries: 3,
        };
        tokio::spawn(async move {
            eprintln!("Spawned job queue");
            job_queue.run().await;
        });
        tokio::spawn(Self::run_expiration_monitor(monitor_rx));
        manager
    }

    /// Shut down background manager tasks.
    pub fn shutdown(&self) {
        self.job_queue.shutdown();
        let _ = self.monitor_tx.send(ExpirationMonitorMessage::Shutdown);
    }

    /// Expiration monitor task: manages a priority queue of pending expirations.
    /// Uses tokio::select! to either process messages or fire the next expiration.
    ///
    /// Note: ExpirationTime::Immediately is NOT tracked by the monitor because
    /// assets with Immediately expiration are treated as volatile and never reach Ready status.
    async fn run_expiration_monitor(mut rx: mpsc::UnboundedReceiver<ExpirationMonitorMessage<E>>) {
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap};

        // Min-heap of assets ordered by expiration time (soonest first via Reverse)
        let mut heap: BinaryHeap<Reverse<TimedAsset<E>>> = BinaryHeap::new();
        // Canonical active deadline per asset id (earliest-deadline-wins).
        // Heap entries are advisory and validated against this map at fire time.
        let mut active_deadline_by_id: HashMap<u64, chrono::DateTime<chrono::Utc>> = HashMap::new();

        loop {
            let next_expiry = heap.peek().map(|Reverse(t)| t.expiration);

            if let Some(next_dt) = next_expiry {
                let now = chrono::Utc::now();
                let sleep_duration = if next_dt > now {
                    (next_dt - now)
                        .to_std()
                        .unwrap_or(std::time::Duration::from_millis(100))
                } else {
                    std::time::Duration::from_millis(0)
                };

                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(ExpirationMonitorMessage::Track { asset_ref, expiration_time }) => {
                                if let ExpirationTime::At(dt) = expiration_time {
                                    let asset_id = asset_ref.id();
                                    let should_update = match active_deadline_by_id.get(&asset_id) {
                                        Some(existing) => dt < *existing,
                                        None => true,
                                    };
                                    if should_update {
                                        active_deadline_by_id.insert(asset_id, dt);
                                        heap.push(Reverse(TimedAsset {
                                            expiration: dt,
                                            asset_id,
                                            asset_ref: asset_ref.downgrade(),
                                        }));
                                    }
                                }
                            }
                            Some(ExpirationMonitorMessage::Untrack { asset_id }) => {
                                active_deadline_by_id.remove(&asset_id);
                            }
                            Some(ExpirationMonitorMessage::Shutdown) | None => {
                                return;
                            }
                        }
                    }
                    _ = tokio::time::sleep(sleep_duration) => {
                        // Fire all entries whose expiration time has passed
                        while let Some(Reverse(timed)) = heap.peek() {
                            if timed.expiration <= chrono::Utc::now() {
                                let Reverse(timed) = match heap.pop() {
                                    Some(r) => r,
                                    None => break, // heap was non-empty at peek; safe fallback
                                };

                                // Stale heap entries are ignored; canonical map is authoritative.
                                if !matches!(
                                    active_deadline_by_id.get(&timed.asset_id),
                                    Some(active) if *active == timed.expiration
                                ) {
                                    continue;
                                }
                                active_deadline_by_id.remove(&timed.asset_id);

                                let asset_id = timed.asset_id;
                                let Some(asset_ref) = timed.asset_ref.upgrade() else {
                                    // Asset already dropped elsewhere (no strong refs remain) —
                                    // nothing to expire or evict.
                                    continue;
                                };

                                // 1. Expire the asset and decide whether map-eviction is safe.
                                //    In-flight states are preserved on expire failure.
                                let expire_result = asset_ref.expire().await;
                                let should_evict = match expire_result {
                                    Ok(()) => true,
                                    Err(e) => {
                                        let status = asset_ref.status().await;
                                        let evict_on_failure = matches!(
                                            status,
                                            Status::None
                                                | Status::Directory
                                                | Status::Recipe
                                                | Status::Error
                                                | Status::Cancelled
                                                | Status::Source
                                                | Status::Volatile
                                        );
                                        if !evict_on_failure {
                                            eprintln!(
                                                "Expiration monitor: preserve asset {} after expire failure in {:?}: {}",
                                                asset_id, status, e
                                            );
                                        }
                                        evict_on_failure
                                    }
                                };
                                if !should_evict {
                                    continue;
                                }

                                // 2. Read query and key while holding the data lock briefly.
                                //    Release lock before any async manager operations.
                                let (query, key) = {
                                    let data = asset_ref.data.read().await;
                                    let query = data.query.as_ref().as_ref().cloned();
                                    let key = data.recipe.key().ok().flatten();
                                    (query, key)
                                };

                                // 3. Remove from manager maps (if still occupied by this asset_id).
                                let envref = asset_ref.get_envref().await;
                                let manager = envref.get_asset_manager();
                                let removed = manager
                                    .remove_expired_from_maps(asset_id, query.as_ref(), key.as_ref())
                                    .await;
                                if !removed && (query.is_some() || key.is_some()) {
                                    eprintln!(
                                        "Expiration monitor: could not find matching map entry for expired asset {}",
                                        asset_id
                                    );
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            } else {
                // No pending expirations — wait for next message
                match rx.recv().await {
                    Some(ExpirationMonitorMessage::Track {
                        asset_ref,
                        expiration_time,
                    }) => {
                        if let ExpirationTime::At(dt) = expiration_time {
                            let asset_id = asset_ref.id();
                            let should_update = match active_deadline_by_id.get(&asset_id) {
                                Some(existing) => dt < *existing,
                                None => true,
                            };
                            if should_update {
                                active_deadline_by_id.insert(asset_id, dt);
                                heap.push(Reverse(TimedAsset {
                                    expiration: dt,
                                    asset_id,
                                    asset_ref: asset_ref.downgrade(),
                                }));
                            }
                        }
                    }
                    Some(ExpirationMonitorMessage::Untrack { asset_id }) => {
                        active_deadline_by_id.remove(&asset_id);
                    }
                    Some(ExpirationMonitorMessage::Shutdown) | None => {
                        return;
                    }
                }
            }
        }
    }

    pub fn next_id(&self) -> u64 {
        self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Get an environment reference
    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref
            .get()
            .expect("Environment not set in AssetStore")
            .clone()
    }

    /// Set environment reference used to access the store, manager, and runtime services.
    /// The envref can only be set once during environment initialization.
    pub fn set_envref(&self, envref: EnvRef<E>) {
        if self.envref.set(envref.clone()).is_err() {
            panic!("Environment already set in AssetStore");
        }
    }

    /// Access the internal dependency manager (pub(crate) — not public API).
    pub(crate) fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E> {
        &self.dependency_manager
    }

    // NOTE (async-wasm-refactor M-B): `load_command_versions` is now the shared free helper
    // `crate::assets::load_command_versions(dm, cmr)`, called from `start()`. The
    // `cascade_expire_dependents` / `expire_dependencies_result` / `register_plan_dependencies`
    // methods are now shared default methods on the `AssetManager` trait (Q1). Removed here.

    /// Remove an expired `AssetRef` from the manager's in-memory maps.
    ///
    /// Called by the expiration monitor after `expire()` on the asset.
    /// Only removes the map entry if the stored entry has the same `asset_id`, guarding
    /// against a newer replacement already occupying the same slot.
    ///
    /// # Arguments
    /// - `asset_id`: Unique id of the asset that expired.
    /// - `query`: If the asset is a query-asset, its key into `query_assets`.
    /// - `key`: If the asset is a key-asset, its key into `assets`.
    pub async fn remove_expired_from_maps(
        &self,
        asset_id: u64,
        query: Option<&Query>,
        key: Option<&Key>,
    ) -> bool {
        let mut removed = false;
        if let Some(query) = query {
            if let Some(entry) = self.query_assets.get_async(query).await {
                if entry.get().id() == asset_id {
                    drop(entry);
                    let _ = self.query_assets.remove_async(query).await;
                    removed = true;
                }
            }
        }
        if !removed {
            if let Some(key) = key {
                if let Some(entry) = self.assets.get_async(key).await {
                    if entry.get().id() == asset_id {
                        drop(entry);
                        let _ = self.assets.remove_async(key).await;
                        removed = true;
                    }
                }
            }
        }
        if removed {
            return true;
        }

        // Ad-hoc assets (neither query nor key): no map entry to remove.
        false
    }

    /// Track an asset for expiration via the monitor task.
    /// Only `ExpirationTime::At(_)` entries are tracked; Never/Immediately are ignored.
    pub(crate) fn track_expiration(
        &self,
        asset_ref: &AssetRef<E>,
        expiration_time: &ExpirationTime,
    ) {
        if let ExpirationTime::At(_) = expiration_time {
            let _ = self.monitor_tx.send(ExpirationMonitorMessage::Track {
                asset_ref: asset_ref.clone(),
                expiration_time: expiration_time.clone(),
            });
        }
        // Never / Immediately: not tracked (Immediately assets never reach Ready status)
    }

    /// Cancel any pending expiration tracking for the asset with the given id.
    pub fn untrack_expiration(&self, asset_id: u64) {
        let _ = self
            .monitor_tx
            .send(ExpirationMonitorMessage::Untrack { asset_id });
    }

    /// Constructs and initializes a new asset from a recipe.
    /// Asset is not tracked by the asset manager.
    /// Arguments:
    /// - `recipe`: Recipe describing how the asset should be resolved/evaluated.
    pub fn create_asset(&self, recipe: Recipe) -> AssetRef<E> {
        let asset = AssetRef::new_from_recipe(self.next_id(), recipe, self.get_envref());
        asset
    }

    /// Constructs and initializes a new dummy asset
    /// A dummy asset is an asset created from an empty recipe
    pub fn create_dummy_asset(&self) -> AssetRef<E> {
        let recipe = Query::new().into();
        let asset = AssetRef::new_from_recipe(self.next_id(), recipe, self.get_envref());
        asset
    }

    /// Convinience method to get a recipe provider from the environment reference.
    /// Used e.g. by: `contains`, `evaluate_recipe`, `get_asset_info`
    pub fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<E>> {
        self.envref
            .get()
            .expect("Environment not set in AssetStore")
            .get_recipe_provider()
    }

    /// Returns a resource asset assuming it is non-volatile
    /// Used by: `get_resource_asset` in this module.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
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

    /// Returns resource asset assuming it is volatile
    /// Used by: `get_resource_asset` in this module.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
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

    /// Returns a resource asset
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    async fn get_resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        if self.is_volatile(key).await? {
            self.get_volatile_resource_asset(key).await
        } else {
            self.get_nonvolatile_resource_asset(key).await
        }
    }

    /// Returns a query asset assuming it is non-volatile
    /// Used by: `get_query_asset` in this module.
    ///
    /// Arguments:
    /// - `query`: Query used to create an asset
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

    /// Returns a query asset assuming it is volatile
    /// Used by: `get_query_asset` in this module.
    ///
    /// Arguments:
    /// - `query`: Query used to create an asset
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

    /// Returns a query asset
    /// Used by: `get_asset` in this module.
    ///
    /// Arguments:
    /// - `query`: Query used to create an asset
    async fn get_query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if query.is_volatile(self.get_envref()).await? {
            self.get_volatile_query_asset(query).await
        } else {
            self.get_nonvolatile_query_asset(query).await
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment> Drop for DefaultAssetManager<E> {
    fn drop(&mut self) {
        // Send Shutdown to the monitor task. Unbounded send is synchronous, safe in Drop.
        let _ = self.monitor_tx.send(ExpirationMonitorMessage::Shutdown);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl<E: Environment> AssetManager<E> for DefaultAssetManager<E> {
    /// Returns an asset evaluating the query
    /// Asset is either cached of scheduled for execution.
    /// If asset contains a value, it can be considered to have a valid (ready or volatile, not expired) value.
    /// Due to timing issues it may happen that it accidentaly expires before it can be used,
    /// so it is good to check the expiration before use.
    /// Once an asset is expired, a new get_asset request is needed.
    ///
    /// Arguments:
    /// - `query`: Query used to create an asset
    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if let Some(key) = query.key() {
            self.get(&key).await
        } else {
            loop {
                let assetref = self.get_query_asset(query).await?;
                let status = assetref.status().await;
                // Stale-terminal states are a cache miss: Expired, Error and Cancelled are all
                // re-evaluated on a fresh manager request (a failure may be transient). Volatile
                // joins them for a different reason — a volatile asset is used once and never
                // reused, so it must not be served from the map even though it is finished.
                // Dropping the entry rebuilds a fresh Recipe asset next iteration (no infinite
                // loop, since a stored Error/Cancelled is not fast-tracked, and a volatile asset
                // is never inserted in the first place).
                if matches!(
                    status,
                    Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
                ) {
                    let asset_id = assetref.id();
                    if let Some(entry) = self.query_assets.get_async(query).await {
                        if entry.get().id() == asset_id {
                            drop(entry);
                            let _ = self.query_assets.remove_async(query).await;
                        }
                    }
                    continue;
                }
                if status.is_finished() {
                    return Ok(assetref);
                }
                {
                    let mut lock = assetref.data.write().await;
                    if lock.try_fast_track().await? {
                        return Ok(assetref.clone());
                    }
                }

                self.job_queue.submit(assetref.clone()).await?;
                return Ok(assetref);
            }
        }
    }

    async fn get_dependency_asset(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
    ) -> Result<AssetRef<E>, Error> {
        loop {
            // Resolve the AssetRef WITHOUT global submission (captured exactly once).
            let asset = if let Some(key) = query.key() {
                self.get_resource_asset(&key).await?
            } else {
                self.get_query_asset(query).await?
            };
            // Stale-terminal at SCHEDULING time: evict and recompute. Expired, Error and
            // Cancelled are all treated as a cache miss (a failure may be transient), and Volatile
            // because a volatile asset is never reused, mirroring
            // the stale-terminal eviction in `get`/`get_asset` so a dependency request rebuilds
            // exactly like a top-level one. This is the ONLY point where such a dependency is
            // refreshed — once the dependent is executing, `wait_for_dependency` uses the stale
            // value (Expired) or fails the parent (a *fresh* Error/Cancelled produced during this
            // evaluation), with no mid-run recompute. A freshly resolved asset starts in a
            // pre-execution state, so this cannot spin.
            if matches!(
                asset.status().await,
                Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
            ) {
                match query.key() {
                    Some(key) => {
                        self.remove_expired_from_maps(asset.id(), None, Some(&key))
                            .await;
                    }
                    None => {
                        self.remove_expired_from_maps(asset.id(), Some(query), None)
                            .await;
                    }
                }
                continue;
            }
            // Already data-bearing or finished — nothing to schedule.
            if asset.poll_state().await.is_some() || asset.status().await.is_finished() {
                return Ok(asset);
            }
            // Fast-track from the store if the value is already persisted.
            let fast_tracked = {
                let mut lock = asset.data.write().await;
                lock.try_fast_track().await?
            };
            if fast_tracked {
                return Ok(asset);
            }
            // Schedule: start now if capacity allows, else enqueue on the parent's LOCAL queue
            // (Status::Submitted here means "queued on a local queue", not the global jobs list).
            if !self.job_queue.try_to_start_immediately(&asset).await? {
                asset.submitted().await?;
                self.job_queue
                    .push_local_dependency(parent.id(), &asset)
                    .await;
            }
            return Ok(asset);
        }
    }

    async fn get_dependency_asset_with_payload(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
        payload: Option<E::Payload>,
        payload_path: Vec<Query>,
    ) -> Result<AssetRef<E>, Error> {
        let _ = parent;
        // A payload-requiring query is volatile by construction, so `get_query_asset`
        // resolves it to a fresh, unshared asset that is in no map and cannot be reused.
        // There is nothing to fast-track and nothing to evict; run it inline in the
        // caller's future rather than consuming a job-queue slot.
        let asset = if let Some(key) = query.key() {
            self.get_resource_asset(&key).await?
        } else {
            self.get_query_asset(query).await?
        };
        asset.set_payload_path(payload_path).await;
        asset.run_immediately(payload).await?;
        Ok(asset)
    }

    async fn drain_dependencies(&self, parent: &AssetRef<E>) -> Result<(), Error> {
        let parent_id = parent.id();
        let mut entered = false;
        while let Some(dep) = self.job_queue.pop_local_dependency(parent_id).await {
            if dep.poll_state().await.is_some() || dep.status().await.is_finished() {
                continue;
            }
            match dep.try_claim_for_run(&self.job_queue).await? {
                Some(claim) => {
                    parent.enter_dependencies(&dep).await?;
                    entered = true;
                    // Inline, recursive: dep.run() may itself schedule and drain dep's own
                    // local queue in this same task. Box::pin bounds the future type; depth
                    // is bounded by the (cycle-free) dependency DAG.
                    let _ = Box::pin(dep.run()).await;
                    claim.complete();
                }
                None => {
                    // Running or finished elsewhere — skip.
                }
            }
        }
        if entered {
            parent.leave_dependencies_and_resume().await?;
        }
        Ok(())
    }

    async fn wait_for_dependency(
        &self,
        parent: &AssetRef<E>,
        dependency: &AssetRef<E>,
    ) -> Result<State<E::Value>, Error> {
        loop {
            // Authoritative status first: poll_state fabricates a none-valued state for
            // Error/Cancelled, so those MUST be handled before polling.
            let status = dependency.status().await;
            match status {
                Status::Error | Status::Cancelled => {
                    let e = Error::general_error(format!(
                        "Dependency asset {} did not produce a value (status {:?})",
                        dependency.id(),
                        status
                    ));
                    let _ = parent.fail_due_to_dependency(e.clone()).await;
                    return Err(e);
                }
                Status::Ready
                | Status::Source
                | Status::Override
                | Status::Volatile
                | Status::Directory => {
                    if let Some(state) = dependency.poll_state().await {
                        let _ = parent.leave_dependencies_and_resume().await;
                        return Ok(state);
                    }
                }
                Status::Expired => {
                    // Execution-time expiry: this dependent's evaluation has already started,
                    // so we do NOT recompute the dependency (that risks unbounded re-execution
                    // when its freshness window is shorter than this asset's evaluation time —
                    // scheduling-time refresh happens only in `get_dependency_asset`).
                    match dependency.poll_state().await {
                        // Stale value still present: use it and propagate staleness — this
                        // dependent is labeled expired at completion (see finish_run_with_result).
                        Some(state) => {
                            parent.note_expired_dependency(dependency).await?;
                            let _ = parent.leave_dependencies_and_resume().await;
                            return Ok(state);
                        }
                        // Expired AND evicted: the input is gone and cannot be reproduced
                        // here — fail the dependent's evaluation.
                        None => {
                            let e = Error::general_error(format!(
                                "Dependency asset {} expired and was evicted before its value \
                                 could be used",
                                dependency.id()
                            ));
                            let _ = parent.fail_due_to_dependency(e.clone()).await;
                            return Err(e);
                        }
                    }
                }
                Status::None
                | Status::Recipe
                | Status::Submitted
                | Status::Dependencies
                | Status::Processing
                | Status::Partial
                | Status::Storing => {}
            }

            // Guarantee progress: drain our own local queue (may inline-run the dependency).
            self.drain_dependencies(parent).await?;
            if dependency.poll_state().await.is_some() {
                // Resolved (data-bearing) or now Error/Cancelled — let the loop top decide.
                continue;
            }

            // Not on our queue and not yet resolved: claim it directly (recovers deps
            // re-parked by a cancelled holder or globally submitted), else subscribe & wait.
            match dependency.try_claim_for_run(&self.job_queue).await? {
                Some(claim) => {
                    parent.enter_dependencies(dependency).await?;
                    let _ = Box::pin(dependency.run()).await;
                    claim.complete();
                }
                None => {
                    parent.enter_dependencies(dependency).await?;
                    let mut rx = dependency.subscribe_to_notifications().await;
                    // Close the lost-wakeup race: the dependency may have resolved between
                    // the claim attempt above and this subscribe, in which case its
                    // ValueProduced/JobFinished notification already fired and `changed()`
                    // would block forever. The resolving write (`set_state`/`set_value`)
                    // stores the value under the same `data.write()` lock it then sends the
                    // notification from, so if we still observe no resolution here, a future
                    // notification is guaranteed to arrive. Re-poll before awaiting; the loop
                    // top re-evaluates once we return, so no extra notification is needed.
                    if dependency.poll_state().await.is_none()
                        && !dependency.status().await.is_finished()
                    {
                        let _ = rx.changed().await;
                    }
                }
            }
        }
    }

    /// Get asset infor for a resource asset
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> {
        if self.assets.contains_async(key).await {
            let assetref = self.get(key).await?;
            assetref.get_asset_info().await
        } else {
            let store = self.get_envref().get_async_store();
            eprintln!(
                "Checking if store contains key {} {:?}",
                key,
                store.contains(key).await
            );
            if store.contains(key).await? {
                eprintln!("Getting asset info from store for key {}", key);
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

    /// Create an ad-hoc asset applied to a state
    async fn apply(
        &self,
        recipe: Recipe,
        initial_state: State<E::Value>,
    ) -> Result<AssetRef<E>, Error> {
        let asset_ref =
            AssetData::new_ext(self.next_id(), recipe, initial_state, self.get_envref()).to_ref();
        // No fast track makes sense now, since apply can't be stored, however in the future
        // TODO: support fast-track once it makes sense
        self.job_queue.submit(asset_ref.clone()).await?;

        Ok(asset_ref)
    }

    /// Create an ad-hoc asset applied to a state
    async fn apply_immediately(
        &self,
        recipe: Recipe,
        initial_state: State<E::Value>,
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error> {
        let asset_ref =
            AssetData::new_ext(self.next_id(), recipe, initial_state, self.get_envref()).to_ref();
        // No fast track makes sense now, since apply can't be stored, however in the future
        // TODO: support fast-track once it makes sense
        asset_ref.run_immediately(payload).await?;

        Ok(asset_ref)
    }

    /// Get resource asset
    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        loop {
            eprintln!("Getting asset for key {}", key);
            let asset_ref = self.get_resource_asset(key).await?;
            let status = asset_ref.status().await;
            // Stale-terminal states are a cache miss: Expired, Error and Cancelled are all
            // re-evaluated on a fresh manager request (a failure may be transient). Volatile joins
            // them for a different reason — a volatile asset is used once and never reused, so it
            // must not be served from the map even though it is finished. Dropping the entry
            // rebuilds a fresh Recipe asset next iteration (no infinite loop, since a stored
            // Error/Cancelled is not fast-tracked, and a volatile asset is never inserted in the
            // first place).
            //
            // Note this is the check on *entry*: an asset that turns out volatile during this
            // call is still returned to the caller below. Used once, then evicted.
            if matches!(
                status,
                Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
            ) {
                let asset_id = asset_ref.id();
                if let Some(entry) = self.assets.get_async(key).await {
                    if entry.get().id() == asset_id {
                        drop(entry);
                        let _ = self.assets.remove_async(key).await;
                    }
                }
                continue;
            }
            if status.is_finished() {
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

            return Ok(asset_ref);
        }
    }

    /// Get recipe option for a resource asset
    async fn recipe_opt(&self, key: &Key) -> Result<Option<Recipe>, Error> {
        self.get_recipe_provider()
            .recipe_opt(key, self.get_envref())
            .await
    }

    /// Computes and returns a boolean volatility check for a resource asset
    /// Used by: `get_query_asset`, `get_resource_asset`, `resolve_volatility_before_evaluation` in this module.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    async fn is_volatile(&self, key: &Key) -> Result<bool, Error> {
        if let Some(recipe) = self.recipe_opt(key).await? {
            let env = self.get_envref();
            Ok(recipe.is_volatile(env).await?)
        } else {
            Ok(false)
        }
    }

    /// Remove resource asset
    /// If asset is being evaluated, it is canceled. Asset is removed from the asset manager.
    /// Persistent value is removed from the store.
    /// Note: This does not remove the recipe, so an asset with a recipe can be requested again. However, it would remove an override.
    /// Used by: `remove_asset`, `run_expiration_monitor`, `test_remove_asset` in this module.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        // 1. Check if asset exists in memory and cancel if processing
        if self.assets.contains_async(key).await {
            if let Some(asset_entry) = self.assets.get_async(key).await {
                let asset_ref = asset_entry.get().clone();
                drop(asset_entry);

                // Cancel if processing
                asset_ref.cancel().await?;
                // Cancel any pending expiration tracking for this asset
                self.untrack_expiration(asset_ref.id());
            }

            // Remove from assets map
            let _ = self.assets.remove_async(key).await;
        }

        // 1b. Remove from dependency manager
        self.dependency_manager
            .remove(&crate::metadata::DependencyKey::from(key))
            .await;

        // 2. Remove from store
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            store.remove(key).await?;
        }

        Ok(())
    }

    /// Set binary value ot the asset
    /// Equivalent of store set method.
    /// For assets with recipes, this can be considered an Override, and the status is set accordingly.
    /// For assers without a recipe this simply creates a Source value and persists the data in store.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    /// - `binary`: Serialized bytes to persist for key-based set operations.
    /// - `metadata`: Metadata payload to read/update/persist for this operation.
    async fn set_binary(
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

        /// Adds non-fatal metadata consistency warnings for externally supplied values.
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
                // Cancel any pending expiration tracking for this asset
                self.untrack_expiration(asset_ref.id());
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

        // 4. Compute version from binary content and store in metadata
        let dep_key = crate::metadata::DependencyKey::from(key);
        if final_status != Status::Volatile && final_status != Status::Error {
            let version = crate::metadata::Version::from_bytes(binary);
            metadata.version = Some(version);
        }

        // 5. Handle Error status specially - store empty binary with metadata
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

        // 6. Register version and cascade expire dependents (non-volatile only)
        if matches!(
            final_status,
            Status::Ready | Status::Source | Status::Override
        ) {
            if let Some(version) = metadata.version {
                let expired = self
                    .dependency_manager
                    .register_version(&dep_key, version)
                    .await;
                self.expire_dependencies_result(expired).await;
            }
        }

        Ok(())
    }

    /// Set the state of the asset
    /// For assets with recipes, this can be considered an Override, and the status is set accordingly.
    /// For assers without a recipe this simply creates a Source value and persists the data in store.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
    /// - `state`: State value (data + metadata) to persist for key-based set_state operations.
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

        /// Adds non-fatal metadata consistency warnings for externally supplied state.
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
                // Cancel any pending expiration tracking for this asset
                self.untrack_expiration(asset_ref.id());
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

        // 4. Compute version for non-volatile states
        let dep_key = crate::metadata::DependencyKey::from(key);
        if final_status != Status::Volatile && final_status != Status::Error {
            let version = match state.as_bytes() {
                Ok(binary) => crate::metadata::Version::from_bytes(&binary),
                Err(_) => crate::metadata::Version::from_time_now(),
            };
            metadata.set_version(Some(version))?;
        }

        // 5. Create new AssetRef with the state
        let recipe: Recipe = key.into();
        let mut asset_data = AssetData::new_ext(
            self.next_id(),
            recipe,
            State::new(), // Empty initial state
            self.get_envref(),
        );
        asset_data.data = Some(Arc::new(state.data_unchecked().as_ref().clone()));
        asset_data.metadata = metadata.clone();
        asset_data.status = final_status;
        asset_data.binary = None; // Clear binary, we have the data

        let asset_ref = asset_data.to_ref();

        // 6. Store in assets map
        let _ = self
            .assets
            .insert_async(key.clone(), asset_ref.clone())
            .await;

        // 7. Handle Error status specially - store empty binary with metadata
        let store = self.get_envref().get_async_store();
        if final_status == Status::Error {
            // Store empty binary with error metadata
            store.set(key, &[], &metadata.clone().into()).await?;
        } else {
            // 8. Try to serialize and store (handle non-serializable gracefully)
            match state.as_bytes() {
                Ok(binary) => {
                    store.set(key, &binary, &metadata.clone().into()).await?;
                }
                Err(_) => {
                    // Non-serializable data - store metadata only
                    store.set_metadata(key, &metadata.clone().into()).await?;
                }
            }
        }

        // 9. Register version and cascade expire dependents (non-volatile only)
        if matches!(
            final_status,
            Status::Ready | Status::Source | Status::Override
        ) {
            if let Some(version) = metadata.version() {
                let expired = self
                    .dependency_manager
                    .register_version(&dep_key, version)
                    .await;
                self.expire_dependencies_result(expired).await;
            }
            // Track the asset in the dependency manager
            let expired = self.dependency_manager.track_asset(&asset_ref).await;
            self.expire_dependencies_result(expired).await;
        }

        Ok(())
    }

    /// Check if the resource asset exists.
    /// If asset has a recipe, it is considered to exist even if it is not currently cached or persisted, because it can be created by evaluating the recipe.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the target asset/resource.
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

    /// List the directory
    /// Equivalent of store listdir method, but considering the both the store and recipes.
    /// Used by: `listdir_keys` in this module.
    ///
    /// Arguments:
    /// - `key`: Store key identifying the directory asset
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

    // --- manager-primitive methods (delegate to inherent bodies / provide new ones) ---

    fn set_envref(&self, envref: EnvRef<E>) {
        DefaultAssetManager::set_envref(self, envref);
    }

    fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E> {
        DefaultAssetManager::dependency_manager(self)
    }

    fn eval_mode(&self) -> EvalMode {
        EvalMode::Queued
    }

    fn lookup_key_asset(&self, key: &Key) -> Option<AssetRef<E>> {
        self.assets.read_sync(key, |_k, v| v.clone())
    }

    async fn remove_key_asset(&self, key: &Key) {
        let _ = self.assets.remove_async(key).await;
    }

    async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool {
        // `remove_if_async` evaluates the predicate and removes under one bucket lock. A
        // `get_async` / compare / `remove_async` sequence would release the entry guard in
        // between, and a replacement registered in that gap would be the thing removed —
        // exactly the race the id check exists to prevent.
        self.assets
            .remove_if_async(key, |asset| asset.id() == asset_id)
            .await
            .is_some()
    }

    async fn insert_key_asset(&self, key: &Key, asset: AssetRef<E>) {
        let _ = self.assets.insert_async(key.clone(), asset).await;
    }

    fn next_id_for_asset(&self) -> u64 {
        self.next_id()
    }

    fn get_envref(&self) -> EnvRef<E> {
        DefaultAssetManager::get_envref(self)
    }

    fn untrack_expiration(&self, asset_id: u64) {
        DefaultAssetManager::untrack_expiration(self, asset_id);
    }

    fn create_temporary_asset(&self) -> AssetRef<E> {
        AssetRef::new_temporary(self.get_envref())
    }

    async fn start(&self) {
        if let Some(envref) = self.envref.get() {
            let cmr = envref.get_command_metadata_registry();
            load_command_versions(&self.dependency_manager, cmr).await;
        }
    }

    fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime) {
        DefaultAssetManager::track_expiration(self, asset_ref, expiration_time);
    }

    async fn remove_expired_from_maps(
        &self,
        asset_id: u64,
        query: Option<&Query>,
        key: Option<&Key>,
    ) -> bool {
        DefaultAssetManager::remove_expired_from_maps(self, asset_id, query, key).await
    }
}

/// Native bounded-concurrency queue used by [`DefaultAssetManager`].
///
/// Most applications interact with it indirectly through [`AssetManager`].
/// [`Self::submit`] either starts an asset immediately when capacity is available
/// or leaves it in `Submitted` status for the worker loop.
/// Proof that the holder is the unique runner of an asset, obtained by an atomic status
/// transition (not-yet-running → `Processing`) under one `AssetData` write lock.
///
/// Exists to give two guarantees a plain flag cannot:
/// - **execute-once**: `run()`/`run_immediately()` are called only by a claim holder,
///   closing the double-run window left by `run_with_future`'s `is_finished()`-only guard;
/// - **cancellation liveness**: if the holder's future is dropped mid-run, `Drop` (while
///   armed) re-parks the asset as `Submitted` and re-submits it so another runner recovers
///   the otherwise-stranded `Processing` asset. `complete()` disarms once `run()` returns.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct RunClaim<E: Environment + 'static> {
    asset: AssetRef<E>,
    /// `Arc` so the Drop repair can reach the queue even after the borrow it came from.
    queue: Arc<JobQueue<E>>,
    armed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment + 'static> RunClaim<E> {
    /// Disarm the claim after `run()` returned (Ok or Err); the asset's own terminal or
    /// error status is authoritative from then on, so Drop must not repair.
    pub(crate) fn complete(mut self) {
        self.armed = false;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment + 'static> Drop for RunClaim<E> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // The owning future was dropped mid-run (e.g. parent cancellation dropped an
        // inline drain). Spawn a best-effort repair: if the asset is still owned by this
        // (now-gone) runner — `Processing`, or `Dependencies` when it was parked awaiting a
        // child — reset it to Submitted and re-park globally so another runner recovers it.
        let asset = self.asset.clone();
        let queue = self.queue.clone();
        tokio::spawn(async move {
            let mut lock = asset.data.write().await;
            match lock.status {
                Status::Processing | Status::Dependencies => {
                    let _ = lock.set_status(Status::Submitted);
                    let _ = lock
                        .notification_tx
                        .send(AssetNotificationMessage::StatusChanged(Status::Submitted));
                    drop(lock);
                    let _ = queue.submit(asset.clone()).await;
                }
                Status::None
                | Status::Directory
                | Status::Recipe
                | Status::Submitted
                | Status::Partial
                | Status::Error
                | Status::Storing
                | Status::Ready
                | Status::Expired
                | Status::Cancelled
                | Status::Source
                | Status::Override
                | Status::Volatile => {
                    // Finished or already handled by another path; nothing to repair.
                }
            }
        });
    }
}

impl<E: Environment + 'static> AssetRef<E> {
    /// Atomically claim the exclusive right to execute this asset's body.
    ///
    /// Under one `data.write()` lock, an explicit status match (no default arm) either
    /// transitions a not-yet-running asset to `Processing` and returns `Some(RunClaim)`,
    /// or returns `None` if the asset is already running/finished. Invariant: `run()` /
    /// `run_immediately()` are invoked only by claim holders.
    ///
    /// `Status::Dependencies` is an **active** state (set by the interpreter once evaluation
    /// has started and the live runner is parked awaiting a child), so it is treated like
    /// `Processing` — NOT claimable. Claiming it would let a second waiter re-run an asset
    /// whose runner is merely parked, breaking the execute-once guarantee. A runner dropped
    /// while parked in `Dependencies` is recovered by the `RunClaim` Drop repair, which
    /// re-parks it as `Submitted`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn try_claim_for_run(
        &self,
        queue: &Arc<JobQueue<E>>,
    ) -> Result<Option<RunClaim<E>>, Error> {
        let mut lock = self.data.write().await;
        match lock.status {
            Status::None | Status::Recipe | Status::Submitted => {
                lock.set_status(Status::Processing)?;
                drop(lock);
                Ok(Some(RunClaim {
                    asset: self.clone(),
                    queue: queue.clone(),
                    armed: true,
                }))
            }
            Status::Directory
            | Status::Processing
            | Status::Dependencies
            | Status::Partial
            | Status::Error
            | Status::Storing
            | Status::Ready
            | Status::Expired
            | Status::Cancelled
            | Status::Source
            | Status::Override
            | Status::Volatile => Ok(None),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct JobQueue<E: Environment> {
    jobs: Arc<Mutex<Vec<AssetRef<E>>>>,
    running_count: Arc<AtomicUsize>,
    notify: Arc<Notify>,
    shutdown: Arc<AtomicBool>,
    capacity: usize,
    /// Per-dependent local dependency queues, keyed by the DEPENDENT asset id.
    /// Created lazily on the no-capacity fallback and removed when drained or when the
    /// dependent finishes — zero per-asset cost on the common path.
    local_deps: Arc<Mutex<HashMap<u64, VecDeque<AssetRef<E>>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: Environment + 'static> JobQueue<E> {
    /// Create a new job queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        eprintln!("Creating job queue with capacity {}", capacity);
        JobQueue {
            jobs: Arc::new(Mutex::new(Vec::new())),
            running_count: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(Notify::new()),
            shutdown: Arc::new(AtomicBool::new(false)),
            capacity,
            local_deps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get current number of running jobs
    pub fn running_count(&self) -> usize {
        self.running_count.load(Ordering::SeqCst)
    }

    /// Submit an asset for processing.
    ///
    /// Public semantics unchanged: start immediately if capacity allows, otherwise park
    /// the asset globally as `Submitted` for the worker loop. Reimplemented over
    /// internal `try_to_start_immediately` method.
    pub async fn submit(self: &Arc<Self>, asset: AssetRef<E>) -> Result<(), Error> {
        if !self.try_to_start_immediately(&asset).await? {
            // No capacity — park globally as Submitted so the worker runs it later.
            asset.submitted().await?;
            self.notify.notify_one();
        }
        Ok(())
    }

    /// Try to start `asset` on a reserved capacity slot right now.
    ///
    /// Returns `Ok(true)` when the asset is being taken care of — either a slot was
    /// reserved and the asset was atomically claimed and spawned, or it was already
    /// claimed/finished elsewhere (any reserved slot released). Returns `Ok(false)` on
    /// no capacity, in which case the caller must guarantee progress (park globally in
    /// `submit`, or enqueue on the parent's local dependency queue).
    pub(crate) async fn try_to_start_immediately(
        self: &Arc<Self>,
        asset: &AssetRef<E>,
    ) -> Result<bool, Error> {
        let asset_id = asset.id();

        // Dedup-register in the global jobs list (as submit did before).
        {
            let mut jobs = self.jobs.lock().await;
            if !jobs.iter().any(|a| a.id() == asset_id) {
                jobs.push(asset.clone());
            }
        }
        self.notify.notify_one();

        // Reserve a capacity slot via CAS.
        let current = self.running_count.load(Ordering::SeqCst);
        if current >= self.capacity {
            return Ok(false);
        }
        if self
            .running_count
            .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // Lost the reservation race — let the caller guarantee progress.
            return Ok(false);
        }

        // Slot reserved: atomically claim the asset for running.
        match asset.try_claim_for_run(self).await? {
            Some(claim) => {
                let asset_clone = asset.clone();
                let running_count = self.running_count.clone();
                let notify = self.notify.clone();
                let queue = self.clone();
                eprintln!("Starting asset job {} immediately", asset_id);
                tokio::spawn(async move {
                    let _ = asset_clone.run().await;
                    claim.complete();
                    running_count.fetch_sub(1, Ordering::SeqCst);
                    notify.notify_one();
                    // Re-park any undrained local dependencies of the finished asset so
                    // waiters are never stranded (shared re-park; the manager refines this
                    // for non-shared entries at its cleanup point). Re-parked directly
                    // (not via submit) to avoid recursing into try_to_start_immediately.
                    for dep in queue.take_local_dependencies(asset_id).await {
                        let _ = queue.repark_as_submitted(&dep).await;
                    }
                    eprintln!("Asset job {} finished", asset_clone.id());
                });
                Ok(true)
            }
            None => {
                // Already claimed/finished elsewhere — release the reserved slot.
                self.running_count.fetch_sub(1, Ordering::SeqCst);
                Ok(true)
            }
        }
    }

    /// Re-park `asset` globally as `Submitted` for the worker loop, without attempting an
    /// immediate start. Non-recursive counterpart used by cleanup paths (avoids recursing
    /// through `try_to_start_immediately`).
    async fn repark_as_submitted(&self, asset: &AssetRef<E>) -> Result<(), Error> {
        {
            let mut jobs = self.jobs.lock().await;
            let id = asset.id();
            if !jobs.iter().any(|a| a.id() == id) {
                jobs.push(asset.clone());
            }
        }
        asset.submitted().await?;
        self.notify.notify_one();
        Ok(())
    }

    /// Push `dep` onto the local dependency queue of dependent `parent_id`
    /// (lazily creating the entry; dedup by asset id within that queue).
    pub(crate) async fn push_local_dependency(&self, parent_id: u64, dep: &AssetRef<E>) {
        let mut map = self.local_deps.lock().await;
        let queue = map.entry(parent_id).or_insert_with(VecDeque::new);
        let dep_id = dep.id();
        if !queue.iter().any(|a| a.id() == dep_id) {
            queue.push_back(dep.clone());
        }
    }

    /// Pop the next local dependency of `parent_id`, if any (removing the map entry when
    /// the queue becomes empty).
    pub(crate) async fn pop_local_dependency(&self, parent_id: u64) -> Option<AssetRef<E>> {
        let mut map = self.local_deps.lock().await;
        let result = map.get_mut(&parent_id).and_then(|q| q.pop_front());
        if let Some(q) = map.get(&parent_id) {
            if q.is_empty() {
                map.remove(&parent_id);
            }
        }
        result
    }

    /// Remove and return all remaining local dependencies of `parent_id` (terminal cleanup).
    pub(crate) async fn take_local_dependencies(&self, parent_id: u64) -> Vec<AssetRef<E>> {
        let mut map = self.local_deps.lock().await;
        map.remove(&parent_id)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
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
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                return;
            }
            // Check if we can start more jobs
            if self.running_count.load(Ordering::SeqCst) < self.capacity {
                let candidates = {
                    let jobs = self.jobs.lock().await;
                    jobs.iter().cloned().collect::<Vec<_>>()
                };

                for asset in candidates {
                    if self.running_count.load(Ordering::SeqCst) >= self.capacity {
                        break;
                    }
                    if asset.status().await == Status::Submitted {
                        // One primitive reserves the slot, atomically claims, and spawns.
                        // `false` = no capacity (stop this pass); an asset already claimed
                        // by an inline drain returns `true` without a double spawn and is
                        // removed from `jobs` on the next cleanup pass. This removes the
                        // former TOCTOU between the status read and set_status(Processing).
                        match self.try_to_start_immediately(&asset).await {
                            Ok(true) => {}
                            Ok(false) => break,
                            Err(e) => {
                                eprintln!("Failed to start asset {}: {}", asset.id(), e);
                            }
                        }
                    }
                }
            }

            let removed = self.cleanup_completed_internal().await;
            if removed > 0 {
                eprintln!("Cleaned up {} finished jobs", removed);
            }

            self.notify.notified().await;
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
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

/// Spawn-free, timer-free asset manager.
///
/// Every evaluation runs in the caller's task. There is no `JobQueue`,
/// expiration monitor, or background manager loop. Expiration is checked lazily
/// when an asset is requested. This implementation supports browser execution and
/// can also be used for deterministic inline evaluation on native targets.
pub struct ImmediateAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    // Plain maps behind std Mutex: locked only for brief SYNC get/insert/remove, NEVER across an
    // `.await` (the guard is `!Send`, which statically enforces this on native).
    assets: std::sync::Mutex<std::collections::HashMap<Key, AssetRef<E>>>,
    query_assets: std::sync::Mutex<std::collections::HashMap<Query, AssetRef<E>>>,
    dependency_manager: crate::dependencies::DependencyManager<E>,
    started: tokio::sync::OnceCell<()>,
}

impl<E: Environment> Default for ImmediateAssetManager<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Environment> ImmediateAssetManager<E> {
    /// Creates an uninitialized inline manager.
    ///
    /// Its environment back-reference is installed during environment
    /// initialization before evaluation methods can be used.
    pub fn new() -> Self {
        ImmediateAssetManager {
            id: std::sync::atomic::AtomicU64::new(1000),
            envref: std::sync::OnceLock::new(),
            assets: std::sync::Mutex::new(std::collections::HashMap::new()),
            query_assets: std::sync::Mutex::new(std::collections::HashMap::new()),
            dependency_manager: crate::dependencies::DependencyManager::new(),
            started: tokio::sync::OnceCell::new(),
        }
    }

    fn next_id(&self) -> u64 {
        self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    fn envref(&self) -> EnvRef<E> {
        self.envref
            .get()
            .expect("Environment not set in ImmediateAssetManager")
            .clone()
    }

    /// Ensure command versions are loaded exactly once (lazy startup — no init-time spawn).
    async fn ensure_started(&self) {
        self.started
            .get_or_init(|| async {
                if let Some(envref) = self.envref.get() {
                    let cmr = envref.get_command_metadata_registry();
                    load_command_versions(&self.dependency_manager, cmr).await;
                }
            })
            .await;
    }

    async fn make_volatile(&self, recipe_src: Recipe) -> AssetRef<E> {
        let asset_ref = AssetRef::new_from_recipe(self.next_id(), recipe_src, self.envref());
        {
            let mut data = asset_ref.data.write().await;
            data.is_volatile = true;
            if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                mr.is_volatile = true;
            }
        }
        asset_ref
    }

    async fn get_query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if query.is_volatile(self.envref()).await? {
            return Ok(self.make_volatile(query.into()).await);
        }
        {
            let map = self
                .query_assets
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = map.get(query) {
                return Ok(existing.clone());
            }
        }
        let asset_ref = AssetRef::new_from_recipe(self.next_id(), query.into(), self.envref());
        let mut map = self
            .query_assets
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = map.get(query) {
            return Ok(existing.clone());
        }
        map.insert(query.clone(), asset_ref.clone());
        Ok(asset_ref)
    }

    async fn get_resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        if self.is_volatile(key).await? {
            return Ok(self.make_volatile(key.into()).await);
        }
        {
            let map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = map.get(key) {
                return Ok(existing.clone());
            }
        }
        let asset_ref = AssetRef::new_from_recipe(self.next_id(), key.into(), self.envref());
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = map.get(key) {
            return Ok(existing.clone());
        }
        map.insert(key.clone(), asset_ref.clone());
        Ok(asset_ref)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AssetManager<E> for ImmediateAssetManager<E> {
    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if let Some(key) = query.key() {
            return self.get(&key).await;
        }
        self.ensure_started().await;
        loop {
            let assetref = self.get_query_asset(query).await?;
            let status = assetref.status().await;
            if matches!(
                status,
                Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
            ) {
                let asset_id = assetref.id();
                let mut map = self
                    .query_assets
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if let Some(existing) = map.get(query) {
                    if existing.id() == asset_id {
                        map.remove(query);
                    }
                }
                drop(map);
                continue;
            }
            if status.is_finished() {
                // Lazy expiration-on-access (replaces the monitor task).
                if status == Status::Ready && assetref.is_expired().await {
                    let _ = assetref.expire_without_cascade().await;
                    let mut map = self
                        .query_assets
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let asset_id = assetref.id();
                    if let Some(existing) = map.get(query) {
                        if existing.id() == asset_id {
                            map.remove(query);
                        }
                    }
                    drop(map);
                    continue;
                }
                return Ok(assetref);
            }
            // Backstop against re-entry: if this asset is already being run inline on this
            // manager, running it again is the recursion `owned_key_asset` exists to prevent.
            // Report it instead of exhausting the stack.
            assetref.run_inline().await?;
            return Ok(assetref);
        }
    }

    async fn apply(&self, recipe: Recipe, to: State<E::Value>) -> Result<AssetRef<E>, Error> {
        self.ensure_started().await;
        let asset_ref = AssetData::new_ext(self.next_id(), recipe, to, self.envref()).to_ref();
        asset_ref.run_inline().await?;
        Ok(asset_ref)
    }

    /// Inline counterpart of `DefaultAssetManager`'s override.
    ///
    /// This manager inherits the trait-default `get_dependency_asset` (which delegates to
    /// `get_asset`), but the payload variant must be implemented here: `get_asset` has no
    /// payload parameter, so delegating would silently drop it.
    async fn get_dependency_asset_with_payload(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
        payload: Option<E::Payload>,
        payload_path: Vec<Query>,
    ) -> Result<AssetRef<E>, Error> {
        let _ = parent;
        self.ensure_started().await;
        // Volatile by construction, so this is a fresh unshared asset in no map.
        let asset = if let Some(key) = query.key() {
            self.get_resource_asset(&key).await?
        } else {
            self.get_query_asset(query).await?
        };
        asset.set_payload_path(payload_path).await;
        asset.run_immediately_inline(payload).await?;
        Ok(asset)
    }

    async fn apply_immediately(
        &self,
        recipe: Recipe,
        to: State<E::Value>,
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error> {
        self.ensure_started().await;
        let asset_ref = AssetData::new_ext(self.next_id(), recipe, to, self.envref()).to_ref();
        asset_ref.run_immediately_inline(payload).await?;
        Ok(asset_ref)
    }

    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        self.ensure_started().await;
        loop {
            let asset_ref = self.get_resource_asset(key).await?;
            let status = asset_ref.status().await;
            if matches!(
                status,
                Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
            ) {
                let asset_id = asset_ref.id();
                let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(existing) = map.get(key) {
                    if existing.id() == asset_id {
                        map.remove(key);
                    }
                }
                drop(map);
                continue;
            }
            if status.is_finished() {
                if status == Status::Ready && asset_ref.is_expired().await {
                    let _ = asset_ref.expire_without_cascade().await;
                    let asset_id = asset_ref.id();
                    let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(existing) = map.get(key) {
                        if existing.id() == asset_id {
                            map.remove(key);
                        }
                    }
                    drop(map);
                    continue;
                }
                return Ok(asset_ref);
            }
            {
                let mut data = asset_ref.data.write().await;
                if data.try_fast_track().await? {
                    drop(data);
                    return Ok(asset_ref);
                }
            }
            // Backstop against re-entry — see the note in `get_asset`.
            asset_ref.run_inline().await?;
            return Ok(asset_ref);
        }
    }

    // --- primitives ---

    fn set_envref(&self, envref: EnvRef<E>) {
        if self.envref.set(envref).is_err() {
            panic!("Environment already set in ImmediateAssetManager");
        }
    }

    fn dependency_manager(&self) -> &crate::dependencies::DependencyManager<E> {
        &self.dependency_manager
    }

    fn eval_mode(&self) -> EvalMode {
        EvalMode::Inline
    }

    fn lookup_key_asset(&self, key: &Key) -> Option<AssetRef<E>> {
        let map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        map.get(key).cloned()
    }

    async fn remove_key_asset(&self, key: &Key) {
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(key);
    }

    async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool {
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some(existing) if existing.id() == asset_id => map.remove(key).is_some(),
            Some(_) | None => false,
        }
    }

    async fn insert_key_asset(&self, key: &Key, asset: AssetRef<E>) {
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(key.clone(), asset);
    }

    fn next_id_for_asset(&self) -> u64 {
        self.next_id()
    }

    fn get_envref(&self) -> EnvRef<E> {
        self.envref()
    }

    fn create_temporary_asset(&self) -> AssetRef<E> {
        AssetRef::new_temporary(self.envref())
    }

    async fn start(&self) {
        self.ensure_started().await;
    }

    /// No monitor task in immediate mode — expiration is checked lazily on access.
    fn track_expiration(&self, _asset_ref: &AssetRef<E>, _expiration_time: &ExpirationTime) {}

    async fn remove_expired_from_maps(
        &self,
        asset_id: u64,
        query: Option<&Query>,
        key: Option<&Key>,
    ) -> bool {
        let mut removed = false;
        if let Some(query) = query {
            let mut map = self
                .query_assets
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = map.get(query) {
                if existing.id() == asset_id {
                    map.remove(query);
                    removed = true;
                }
            }
        }
        if !removed {
            if let Some(key) = key {
                let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(existing) = map.get(key) {
                    if existing.id() == asset_id {
                        map.remove(key);
                        removed = true;
                    }
                }
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;
    use crate::command_metadata::CommandKey;
    use crate::context::{SimpleEnvironment, SimpleEnvironmentWithPayload};
    use crate::metadata::{DependencyRecord, Metadata, MetadataRecord, Version};
    use crate::parse::{parse_key, parse_query};
    use crate::query::Key;
    use crate::store::{AsyncMemoryStore, AsyncStore};
    use crate::value::{Value, ValueInterface};

    struct FailingSetStore;

    #[async_trait]
    impl AsyncStore for FailingSetStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            Err(Error::key_not_found(key))
        }

        async fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
            Err(Error::key_write_error(
                key,
                "FailingSetStore",
                "intentional store set failure",
            ))
        }

        async fn set_metadata(&self, _key: &Key, _metadata: &Metadata) -> Result<(), Error> {
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct CountingMetadataStore {
        metadata_writes: Arc<AtomicUsize>,
        last_metadata: Arc<StdMutex<Option<Metadata>>>,
    }

    #[async_trait]
    impl AsyncStore for CountingMetadataStore {
        async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
            Err(Error::key_not_found(key))
        }

        async fn set(&self, _key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
            Ok(())
        }

        async fn set_metadata(&self, _key: &Key, metadata: &Metadata) -> Result<(), Error> {
            self.metadata_writes.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut lock) = self.last_metadata.lock() {
                *lock = Some(metadata.clone());
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_metadata_save_is_throttled_and_coalesced() {
        let key = parse_key("metadata-throttle.txt").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let store = CountingMetadataStore::default();
        env.with_async_store(Box::new(store.clone()));
        let envref = env.to_ref();

        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(9999, key.clone().into(), envref.clone());

        for i in 0..8 {
            asset_data
                .metadata
                .add_log_entry(LogEntry::info(format!("log-entry-{i}")))
                .unwrap();
            asset_data.save_metadata_to_store().await.unwrap();
        }

        sleep(Duration::from_millis(300)).await;

        let writes = store.metadata_writes.load(Ordering::SeqCst);
        assert!(writes >= 1, "expected at least one metadata save");
        assert!(
            writes < 8,
            "expected coalesced writes, got {writes} for 8 save requests"
        );
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
    async fn test_weak_asset_ref_upgrade() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = parse_key("test/weak_upgrade.txt").unwrap();
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(43210, key.into(), env.to_ref()).to_ref();

        let weak = asset.downgrade();
        assert_eq!(weak.id(), asset.id());
        let upgraded = weak
            .upgrade()
            .expect("weak reference should upgrade while strong ref exists");
        assert_eq!(upgraded.id(), asset.id());
    }

    #[tokio::test]
    async fn test_weak_asset_ref_upgrade_after_drop_returns_none() {
        let weak = {
            let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
            let key = parse_key("test/weak_drop.txt").unwrap();
            let asset = AssetData::<SimpleEnvironment<Value>>::new(43211, key.into(), env.to_ref())
                .to_ref();
            let weak = asset.downgrade();
            assert!(weak.upgrade().is_some());
            weak
        };

        assert!(weak.upgrade().is_none());
    }

    #[tokio::test]
    async fn test_asset_loading() {
        let key = parse_key("test.txt").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
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
            state.unwrap().try_into_string().unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_asset_loading_skips_non_ready_source_override_statuses() {
        let key = parse_key("test_non_ready.txt").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        env.get_async_store()
            .set(
                &key,
                b"queued payload",
                &Metadata::MetadataRecord(
                    MetadataRecord::new()
                        .with_key(key.clone())
                        .with_type_identifier("text".to_owned())
                        .with_status(Status::Submitted)
                        .clone(),
                ),
            )
            .await
            .unwrap();

        let envref = env.to_ref();
        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1235, key.into(), envref.clone());

        assert!(!asset_data.try_fast_track().await.unwrap());
        assert!(asset_data.poll_state().is_none());
        assert!(asset_data.poll_binary().is_none());
        assert_eq!(asset_data.status, Status::None);
    }

    #[tokio::test]
    async fn test_asset_loading_binary_type_uses_raw_bytes() {
        let key = parse_key("test_binary_payload.bin").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let raw = vec![0, 159, 146, 150, 255];
        env.get_async_store()
            .set(&key, &raw, &{
                let mut metadata = MetadataRecord::new();
                metadata.with_key(key.clone());
                metadata.with_type_identifier("bytes".to_owned());
                // Intentionally inconsistent with bytes type to assert from_bytes path.
                metadata.data_format = Some("txt".to_owned());
                metadata.with_status(Status::Override);
                Metadata::MetadataRecord(metadata)
            })
            .await
            .unwrap();

        let envref = env.to_ref();
        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1236, key.into(), envref.clone());

        assert!(asset_data.try_fast_track().await.unwrap());
        let state = asset_data.poll_state().expect("state must be available");
        assert_eq!(state.data_unchecked().try_into_bytes().unwrap(), raw);
    }

    #[tokio::test]
    async fn test_asset_loading_corrupted_ready_payload_returns_false_and_clears() {
        let key = parse_key("test_corrupted_ready.json").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        env.get_async_store()
            .set(&key, b"not valid json", &{
                let mut metadata = MetadataRecord::new();
                metadata.with_key(key.clone());
                metadata.with_type_identifier("text".to_owned());
                metadata.data_format = Some("json".to_owned());
                metadata.with_status(Status::Ready);
                Metadata::MetadataRecord(metadata)
            })
            .await
            .unwrap();

        let envref = env.to_ref();
        let mut asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(1237, key.into(), envref.clone());

        assert!(!asset_data.try_fast_track().await.unwrap());
        assert!(asset_data.poll_state().is_none());
        assert!(asset_data.poll_binary().is_none());
        assert_eq!(asset_data.status, Status::None);
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
            state.unwrap().try_into_string().unwrap(),
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
            state.unwrap().try_into_string().unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_asset_run_handles_dependencies_status() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(2234, query.into(), envref.clone());
        let assetref = asset_data.to_ref();
        {
            let mut lock = assetref.data.write().await;
            lock.status = Status::Dependencies;
            lock.metadata.set_status(Status::Dependencies).unwrap();
        }

        assetref.run().await.unwrap();
        assert_eq!(assetref.status().await, Status::Ready);
    }

    #[tokio::test]
    async fn test_dependencies_status_has_no_data() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let asset = AssetData::<SimpleEnvironment<Value>>::new(
            2236,
            parse_query("test").unwrap().into(),
            env.to_ref(),
        )
        .to_ref();
        {
            let mut lock = asset.data.write().await;
            lock.data = Some(Arc::new(Value::from("hidden while waiting")));
            lock.set_status(Status::Dependencies).unwrap();
        }

        assert_eq!(asset.status().await, Status::Dependencies);
        assert!(asset.poll_state().await.is_none());
    }

    #[tokio::test]
    async fn test_context_add_dependency_does_not_downgrade_known_version_to_unknown() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let asset = AssetData::<SimpleEnvironment<Value>>::new(
            2237,
            parse_query("test").unwrap().into(),
            env.to_ref(),
        )
        .to_ref();
        let context = asset.create_context().await;
        let dep_key = DependencyKey::new("-R/dep.txt");

        context
            .add_dependency(DependencyRecord::new(dep_key.clone(), Version::new(42)))
            .await;
        context
            .add_dependency(DependencyRecord::new(dep_key.clone(), Version::unknown()))
            .await;

        let deps = context.take_pending_dependencies().await;
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].key, dep_key);
        assert_eq!(deps[0].version, Version::new(42));
    }

    #[tokio::test]
    async fn test_record_dependency_on_asset_does_not_downgrade_known_metadata_version_to_unknown()
    {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let parent_key = parse_key("parent.txt").unwrap();
        let dep_key = parse_key("dep.txt").unwrap();
        let parent =
            AssetData::<SimpleEnvironment<Value>>::new(2238, parent_key.into(), envref.clone())
                .to_ref();
        let dependency =
            AssetData::<SimpleEnvironment<Value>>::new(2239, dep_key.clone().into(), envref)
                .to_ref();
        let dependency_record_key = DependencyKey::from(&dep_key);

        {
            let mut lock = parent.data.write().await;
            lock.metadata
                .add_dependency(DependencyRecord::new(
                    dependency_record_key.clone(),
                    Version::new(42),
                ))
                .unwrap();
        }

        parent
            .record_dependency_on_asset(&dependency)
            .await
            .unwrap();

        let metadata = parent.get_metadata().await.unwrap();
        let deps = metadata.get_dependencies();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].key, dependency_record_key);
        assert_eq!(deps[0].version, Version::new(42));
    }

    #[tokio::test]
    async fn test_asset_run_immediately_handles_dependencies_status() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");

        let envref = env.to_ref();
        let asset_data =
            AssetData::<SimpleEnvironment<Value>>::new(2235, query.into(), envref.clone());
        let assetref = asset_data.to_ref();
        {
            let mut lock = assetref.data.write().await;
            lock.status = Status::Dependencies;
            lock.metadata.set_status(Status::Dependencies).unwrap();
        }

        assetref.run_immediately(None).await.unwrap();
        assert_eq!(assetref.status().await, Status::Ready);
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
        assert!(metadata_before.primary_progress().is_off());

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
        assert!(metadata_after.primary_progress().is_off());
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
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
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
        eprintln!("store_key: {}", store_key);
        eprintln!("Store keys: {:?}", store.keys().await.unwrap());
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
            .apply(query.into(), State::new().with_data("WORLD".into()))
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
            .apply_immediately(query.into(), State::new().with_data("WORLD".into()), None)
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
    async fn test_recipe_yaml_title_and_description_reach_asset_metadata() {
        use crate::command_metadata::CommandKey;
        use crate::context::{Environment, SimpleEnvironment};
        use crate::metadata::Metadata;
        use crate::parse::parse_key;
        use crate::recipes::DefaultRecipeProvider;
        use crate::store::{AsyncMemoryStore, AsyncStore};
        use crate::value::Value;

        // Specify the recipe exactly as a user would in recipes.yaml.
        let yaml_content = r#"
recipes:
  - query: hello/hello.txt
    title: Test Hello Recipe
    description: A hello recipe
"#;

        // 3. Set it into memory store under key test/recipes.yaml
        let recipes_key = parse_key("test/recipes.yaml").unwrap();
        let metadata = Metadata::new();
        let memory_store = AsyncMemoryStore::new(&parse_key("").unwrap());
        memory_store
            .set(&recipes_key, yaml_content.as_bytes(), &metadata)
            .await
            .unwrap();

        // 4. Set the memory store in environment wrapped as AsyncStore
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(memory_store));

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
        let asset_metadata = asset1.get_metadata().await.unwrap();
        assert_eq!(asset_metadata.title(), "Test Hello Recipe");
        assert_eq!(asset_metadata.description(), "A hello recipe");

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
            .apply_immediately(
                query.into(),
                State::new().with_data("WORLD".into()),
                Some("Hi".to_owned()),
            )
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
                eprintln!("HELLO from test");
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

    // ============ RunClaim / try_claim_for_run Tests ============

    #[tokio::test]
    async fn test_try_claim_for_run_unique_under_race() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(1, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Submitted).await.unwrap();

        let (a, b) = tokio::join!(
            asset.try_claim_for_run(&queue),
            asset.try_claim_for_run(&queue)
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert_ne!(a.is_some(), b.is_some(), "exactly one claimer should win");
        assert_eq!(asset.status().await, Status::Processing);
        // Disarm the winner so no Drop repair fires.
        if let Some(c) = a {
            c.complete();
        }
        if let Some(c) = b {
            c.complete();
        }
    }

    #[tokio::test]
    async fn test_try_claim_for_run_none_when_finished_or_processing() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(2, query.into(), envref.clone()).to_ref();

        asset.set_status(Status::Ready).await.unwrap();
        assert!(asset.try_claim_for_run(&queue).await.unwrap().is_none());
        asset.set_status(Status::Processing).await.unwrap();
        assert!(asset.try_claim_for_run(&queue).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_runclaim_complete_disarms() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(3, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Submitted).await.unwrap();

        let claim = asset
            .try_claim_for_run(&queue)
            .await
            .unwrap()
            .expect("should claim");
        assert_eq!(asset.status().await, Status::Processing);
        claim.complete();
        // No repair task: status stays Processing.
        tokio::task::yield_now().await;
        assert_eq!(asset.status().await, Status::Processing);
    }

    #[tokio::test]
    async fn test_runclaim_drop_reparks_when_armed() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        // Capacity 0 so the repair's submit re-parks as Submitted without spawning run().
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(0));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(4, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Submitted).await.unwrap();

        {
            let _claim = asset
                .try_claim_for_run(&queue)
                .await
                .unwrap()
                .expect("should claim");
            assert_eq!(asset.status().await, Status::Processing);
            // `_claim` drops here (still armed) → spawns the repair task.
        }

        let repaired = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if asset.status().await == Status::Submitted {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            repaired.is_ok(),
            "armed Drop should re-park the asset as Submitted"
        );
    }

    /// `Status::Dependencies` is an active state (a live runner parked awaiting a child),
    /// so it must NOT be claimable — otherwise a second waiter would re-run the asset,
    /// breaking execute-once. (PR #6 review, comment 1.)
    #[tokio::test]
    async fn test_try_claim_for_run_none_when_dependencies() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(30, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Dependencies).await.unwrap();
        assert!(
            asset.try_claim_for_run(&queue).await.unwrap().is_none(),
            "an asset parked in Dependencies must not be claimable"
        );
        assert_eq!(asset.status().await, Status::Dependencies);
    }

    /// A runner dropped while parked in `Dependencies` (its claim still armed) must be
    /// recovered: the Drop repair re-parks the asset as `Submitted`. (PR #6 review, comment 1.)
    #[tokio::test]
    async fn test_runclaim_drop_reparks_from_dependencies() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        // Capacity 0 so the repair's submit re-parks as Submitted without spawning run().
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(0));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(31, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Submitted).await.unwrap();

        {
            let _claim = asset
                .try_claim_for_run(&queue)
                .await
                .unwrap()
                .expect("should claim");
            // Runner parks on a child dependency.
            asset.set_status(Status::Dependencies).await.unwrap();
            // `_claim` drops here (still armed) while status is Dependencies.
        }

        let repaired = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if asset.status().await == Status::Submitted {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            repaired.is_ok(),
            "armed Drop should re-park a Dependencies-parked asset as Submitted"
        );
    }

    /// An asset that consumed a dependency which expired mid-execution must be labeled
    /// `Expired` at completion (staleness propagation): the produced value is kept but the
    /// asset is recomputed on next access. (PR #6 review, comment 3.)
    #[tokio::test]
    async fn test_stale_dependency_labels_asset_expired_on_completion() {
        let query = parse_query("test").unwrap();
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let key = CommandKey::new_name("test");
        env.command_registry
            .register_command(key.clone(), |_, _, _| Ok(Value::from("Hello, world!")))
            .expect("register_command failed");
        let envref = env.to_ref();

        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(32, query.into(), envref.clone()).to_ref();
        // A throwaway dependency, only used for its id in the diagnostic warning.
        let dep = AssetData::<SimpleEnvironment<Value>>::new(
            33,
            parse_query("dep").unwrap().into(),
            envref.clone(),
        )
        .to_ref();
        asset.note_expired_dependency(&dep).await.unwrap();

        asset.run().await.unwrap();

        assert_eq!(
            asset.status().await,
            Status::Expired,
            "an asset that used a stale dependency must be labeled Expired"
        );
        // Normal poll_state() now treats Expired as a cache miss (WP-3) — the value is not lost,
        // just no longer served through the normal read path.
        assert!(
            asset.poll_state().await.is_none(),
            "poll_state() must treat Expired as a cache miss"
        );
        // The produced value is still present (used, not discarded) — retrievable via the
        // explicit recovery read.
        let state = asset.poll_state_any_status().await;
        assert!(state.is_some(), "the produced value must be retained");
        assert_eq!(state.unwrap().try_into_string().unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_try_to_start_immediately_false_at_capacity() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(0));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(20, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Submitted).await.unwrap();
        assert!(!queue.try_to_start_immediately(&asset).await.unwrap());
        assert_eq!(queue.running_count(), 0);
    }

    #[tokio::test]
    async fn test_try_to_start_immediately_true_when_already_active() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let asset =
            AssetData::<SimpleEnvironment<Value>>::new(21, query.into(), envref.clone()).to_ref();
        asset.set_status(Status::Processing).await.unwrap();
        // Already Processing: no claim available -> true, and the reserved slot is released.
        assert!(queue.try_to_start_immediately(&asset).await.unwrap());
        assert_eq!(queue.running_count(), 0);
    }

    #[tokio::test]
    async fn test_local_dependency_fifo_dedup_and_removal() {
        let query = parse_query("test").unwrap();
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let queue = Arc::new(JobQueue::<SimpleEnvironment<Value>>::new(4));
        let mk = |id: u64| {
            AssetData::<SimpleEnvironment<Value>>::new(id, query.clone().into(), envref.clone())
                .to_ref()
        };
        let d1 = mk(11);
        let d2 = mk(12);
        let d3 = mk(13);

        queue.push_local_dependency(100, &d1).await;
        queue.push_local_dependency(100, &d2).await;
        queue.push_local_dependency(100, &d1).await; // dedup by id
        queue.push_local_dependency(100, &d3).await;

        assert_eq!(queue.pop_local_dependency(100).await.map(|a| a.id()), Some(11));
        assert_eq!(queue.pop_local_dependency(100).await.map(|a| a.id()), Some(12));
        assert_eq!(queue.pop_local_dependency(100).await.map(|a| a.id()), Some(13));
        assert!(queue.pop_local_dependency(100).await.is_none());

        queue.push_local_dependency(200, &d1).await;
        queue.push_local_dependency(200, &d2).await;
        let taken = queue.take_local_dependencies(200).await;
        assert_eq!(taken.len(), 2);
        assert!(queue.pop_local_dependency(200).await.is_none());
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
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();

        let key = parse_key("test/set_source").unwrap();
        let binary = b"test data".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());

        let manager = envref.get_asset_manager();
        manager.set_binary(&key, &binary, metadata).await.unwrap();

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
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();

        let key = parse_key("test/set_expired").unwrap();
        let binary = b"expired data".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());
        metadata.status = Status::Expired;

        let manager = envref.get_asset_manager();
        manager.set_binary(&key, &binary, metadata).await.unwrap();

        let store = envref.get_async_store();
        let (_, stored_metadata) = store.get(&key).await.unwrap();
        assert_eq!(stored_metadata.status(), Status::Expired);
    }

    #[tokio::test]
    async fn test_set_with_error_status() {
        // Set with Error status - should preserve Error and not store binary
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
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
        manager.set_binary(&key, &binary, metadata).await.unwrap();

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
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
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
    async fn test_untrack_cancels_duplicate_deadlines_for_same_asset() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/untrack_duplicate_deadlines.txt").unwrap();
        let asset = manager.create_asset(key.into());
        asset.set_value(Value::from("keep")).await.unwrap();

        let first = chrono::Utc::now() + chrono::Duration::milliseconds(200);
        let second = first + chrono::Duration::milliseconds(100);

        // This timing test intentionally uses delayed deadlines so both Track messages and the
        // Untrack message are processed before any timer can fire.
        manager.track_expiration(&asset, &ExpirationTime::At(first));
        manager.track_expiration(&asset, &ExpirationTime::At(second));
        manager.untrack_expiration(asset.id());

        sleep(Duration::from_millis(450)).await;
        assert_eq!(asset.status().await, Status::Ready);
    }

    /// WP-3 regression guard: the expiration monitor's heap entry (`TimedAsset`) holds a
    /// `WeakAssetRef`, not a strong `AssetRef`, so a pending timer never keeps a dropped asset's
    /// memory alive. `create_asset` deliberately does not register the asset in the manager's own
    /// map (see its doc comment), so once the local `asset` binding and the tracked-until-later
    /// heap entry are the only possible strong refs, dropping `asset` must let the captured
    /// `WeakAssetRef` fail to upgrade shortly after (a short bounded retry loop, since the
    /// monitor processes the `Track` message asynchronously relative to this test).
    #[tokio::test]
    async fn test_untrack_releases_strong_ref() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/untrack_releases_strong_ref.txt").unwrap();
        let asset = manager.create_asset(key.into());
        asset.set_value(Value::from("keep")).await.unwrap();

        let weak = asset.downgrade();

        // Track with a deadline far in the future: the monitor's heap holds this entry for the
        // rest of the test, so if it held a STRONG ref (pre-WP-3 behavior), the asset would stay
        // alive even after every external strong ref below is dropped.
        let far_future = chrono::Utc::now() + chrono::Duration::hours(1);
        manager.track_expiration(&asset, &ExpirationTime::At(far_future));
        drop(asset);

        for _ in 0..20 {
            if weak.upgrade().is_none() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("WeakAssetRef::upgrade() should be None: the monitor must hold no strong reference");
    }

    /// WP-3 regression guard: earliest-deadline-wins retracking. Tracking a LATER deadline then
    /// immediately retracking the SAME asset at an EARLIER deadline must fire at the earlier
    /// time, not the later (superseded) one, and must not re-fire once the later deadline also
    /// passes. Timing-control note: uses short real (wall-clock) deadlines rather than
    /// `tokio::time::pause()`/`advance()` because the monitor compares against
    /// `chrono::Utc::now()`, which paused tokio virtual time does not affect — this matches the
    /// convention already used by the other monitor timing tests in this module.
    #[tokio::test]
    async fn test_retrack_earlier_deadline_fires_once() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/retrack_earlier_deadline.txt").unwrap();
        let asset = manager.create_asset(key.into());
        asset.set_value(Value::from("keep")).await.unwrap();

        let later = chrono::Utc::now() + chrono::Duration::milliseconds(600);
        let sooner = chrono::Utc::now() + chrono::Duration::milliseconds(100);
        manager.track_expiration(&asset, &ExpirationTime::At(later));
        manager.track_expiration(&asset, &ExpirationTime::At(sooner));

        // Must not have fired yet (well before the earlier deadline).
        sleep(Duration::from_millis(40)).await;
        assert_eq!(
            asset.status().await,
            Status::Ready,
            "must not fire before the earlier deadline"
        );

        // Must have fired at/near the earlier deadline, not waited for the later one.
        sleep(Duration::from_millis(160)).await;
        assert_eq!(
            asset.status().await,
            Status::Expired,
            "must fire at the earlier deadline, not the later superseded one"
        );

        // Confirm it does not re-fire / change status once the (superseded) later deadline
        // also passes.
        sleep(Duration::from_millis(500)).await;
        assert_eq!(
            asset.status().await,
            Status::Expired,
            "must not re-fire at the superseded later deadline"
        );
    }

    // NOTE: WP-3's planned `test_expire_failure_preserves_processing_asset` (status-aware
    // eviction leaves an in-flight Processing asset in the manager's map after a failed
    // expire()) is already covered by the pre-existing `test_failed_expire_inflight_status_is_
    // not_evicted` test immediately below — no new test added here to avoid duplicating it.

    #[tokio::test]
    async fn test_failed_expire_nonrunning_status_evicts_keyed_asset() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/expire_source_evict.txt").unwrap();
        let asset = manager.create_asset(key.clone().into());
        asset.set_status(Status::Source).await.unwrap();
        let _ = manager
            .assets
            .insert_async(key.clone(), asset.clone())
            .await;

        let deadline = chrono::Utc::now() + chrono::Duration::milliseconds(80);
        // We wait a bounded interval to let the monitor process the scheduled timer deterministically.
        manager.track_expiration(&asset, &ExpirationTime::At(deadline));
        sleep(Duration::from_millis(220)).await;

        assert!(
            !manager.assets.contains_async(&key).await,
            "Source asset should be evicted when expire() fails"
        );
    }

    #[tokio::test]
    async fn test_failed_expire_inflight_status_is_not_evicted() {
        let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/expire_processing_keep.txt").unwrap();
        let asset = manager.create_asset(key.clone().into());
        asset.set_status(Status::Processing).await.unwrap();
        let _ = manager
            .assets
            .insert_async(key.clone(), asset.clone())
            .await;

        let deadline = chrono::Utc::now() + chrono::Duration::milliseconds(80);
        // This delay gives the monitor enough time to attempt expiration and apply fallback policy.
        manager.track_expiration(&asset, &ExpirationTime::At(deadline));
        sleep(Duration::from_millis(220)).await;

        let entry = manager.assets.get_async(&key).await;
        assert!(entry.is_some(), "Processing asset should remain cached");
        if let Some(entry) = entry {
            assert_eq!(entry.get().id(), asset.id());
        }
    }

    #[tokio::test]
    async fn test_set_state_replacement_untracks_old_timer() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/set_state_untracks_old_timer.txt").unwrap();
        let old_asset = manager.create_asset(key.clone().into());
        old_asset.set_value(Value::from("old")).await.unwrap();
        let _ = manager
            .assets
            .insert_async(key.clone(), old_asset.clone())
            .await;

        let old_deadline = chrono::Utc::now() + chrono::Duration::milliseconds(120);
        manager.track_expiration(&old_asset, &ExpirationTime::At(old_deadline));

        let value = Value::from("new");
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = value.identifier().to_string();
        metadata.type_name = value.type_name().to_string();
        metadata.data_format = Some("txt".to_string());
        let state = State::from_value_and_metadata(value, Arc::new(metadata.into()));
        manager.set_state(&key, state).await.unwrap();

        // The wait is required to allow the old deadline to pass and prove stale timer suppression.
        sleep(Duration::from_millis(260)).await;

        assert_eq!(old_asset.status().await, Status::Ready);
        let current = manager.assets.get_async(&key).await.unwrap();
        assert_ne!(current.get().id(), old_asset.id());
    }

    #[tokio::test]
    async fn test_get_key_skips_stale_expired_cached_asset() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let key = parse_key("test/stale_expired_key.txt").unwrap();
        let mut stored = MetadataRecord::new();
        stored.type_identifier = "text".to_string();
        stored.type_name = "text".to_string();
        stored.data_format = Some("txt".to_string());
        stored.status = Status::Source;
        envref
            .get_async_store()
            .set(&key, b"fresh", &Metadata::MetadataRecord(stored))
            .await
            .unwrap();

        let stale = manager.create_asset(key.clone().into());
        stale.set_status(Status::Expired).await.unwrap();
        let _ = manager
            .assets
            .insert_async(key.clone(), stale.clone())
            .await;

        let fresh = manager.get(&key).await.unwrap();
        assert_ne!(fresh.id(), stale.id());
        assert_ne!(fresh.status().await, Status::Expired);
    }

    #[tokio::test]
    async fn test_get_query_skips_stale_expired_cached_asset() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let command = CommandKey::new_name("stale_query_cmd");
        env.command_registry
            .register_command(command, |_, _, _| Ok(Value::from("ok")))
            .expect("register_command failed");
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let query = parse_query("stale_query_cmd").unwrap();
        let stale = manager.create_asset(query.clone().into());
        stale.set_status(Status::Expired).await.unwrap();
        let _ = manager
            .query_assets
            .insert_async(query.clone(), stale.clone())
            .await;

        let fresh = manager.get_asset(&query).await.unwrap();
        assert_ne!(fresh.id(), stale.id());
        assert_ne!(fresh.status().await, Status::Expired);
    }

    /// A dependency resolved at scheduling time that is a stale terminal Error is evicted and
    /// rebuilt — the same cache-miss policy as top-level `get`/`get_asset`, not left cached to
    /// fail every dependent. (Regression: `get_dependency_asset` previously evicted only Expired.)
    #[tokio::test]
    async fn test_get_dependency_skips_stale_error_cached_asset() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let command = CommandKey::new_name("dep_error_cmd");
        env.command_registry
            .register_command(command, |_, _, _| Ok(Value::from("ok")))
            .expect("register_command failed");
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let query = parse_query("dep_error_cmd").unwrap();
        let stale = manager.create_asset(query.clone().into());
        stale.set_status(Status::Error).await.unwrap();
        let _ = manager
            .query_assets
            .insert_async(query.clone(), stale.clone())
            .await;

        let parent = manager.create_asset(parse_query("dep_error_parent").unwrap().into());
        let fresh = manager
            .get_dependency_asset(&parent, &query)
            .await
            .unwrap();
        assert_ne!(fresh.id(), stale.id(), "stale Error dependency must be evicted");
        assert_ne!(fresh.status().await, Status::Error);
    }

    /// Same policy for a stale Cancelled dependency.
    #[tokio::test]
    async fn test_get_dependency_skips_stale_cancelled_cached_asset() {
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        let command = CommandKey::new_name("dep_cancelled_cmd");
        env.command_registry
            .register_command(command, |_, _, _| Ok(Value::from("ok")))
            .expect("register_command failed");
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let query = parse_query("dep_cancelled_cmd").unwrap();
        let stale = manager.create_asset(query.clone().into());
        stale.set_status(Status::Cancelled).await.unwrap();
        let _ = manager
            .query_assets
            .insert_async(query.clone(), stale.clone())
            .await;

        let parent = manager.create_asset(parse_query("dep_cancelled_parent").unwrap().into());
        let fresh = manager
            .get_dependency_asset(&parent, &query)
            .await
            .unwrap();
        assert_ne!(
            fresh.id(),
            stale.id(),
            "stale Cancelled dependency must be evicted"
        );
        assert_ne!(fresh.status().await, Status::Cancelled);
    }

    #[tokio::test]
    async fn test_remove_asset() {
        // First set an asset, then remove it
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();

        let key = parse_key("test/to_remove").unwrap();
        let binary = b"to be removed".to_vec();
        let mut metadata = MetadataRecord::new();
        metadata.type_identifier = "text".to_string();
        metadata.type_name = "text".to_string();
        metadata.data_format = Some("txt".to_string());

        let manager = envref.get_asset_manager();
        manager.set_binary(&key, &binary, metadata).await.unwrap();

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
        env.with_async_store(Box::new(FailingSetStore));
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
            dbg!(meta.log.clone());
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
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
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

    // ==================================================================================
    // expired-binary-read-safety: the read contract for binary data.
    // Examples 1-3 and U4-U11, I1 from specs/design/expired-binary-read-safety/.
    // ==================================================================================

    /// All fifteen statuses, for the tests that sweep them.
    const ALL_STATUSES: [Status; 15] = [
        Status::None,
        Status::Directory,
        Status::Recipe,
        Status::Submitted,
        Status::Dependencies,
        Status::Processing,
        Status::Partial,
        Status::Error,
        Status::Storing,
        Status::Expired,
        Status::Cancelled,
        Status::Ready,
        Status::Source,
        Status::Override,
        Status::Volatile,
    ];

    /// An AssetData parked in `status`, holding cached bytes (and optionally a value).
    ///
    /// Assigning the fields directly keeps these tests about the gate rather than about how each
    /// status is reached; the in-file test module can do this, integration tests cannot.
    fn asset_with_binary(
        id: u64,
        status: Status,
        with_value: bool,
        envref: EnvRef<SimpleEnvironment<Value>>,
    ) -> AssetData<SimpleEnvironment<Value>> {
        let key = parse_key("gate/subject.txt").expect("test key");
        let mut d = AssetData::<SimpleEnvironment<Value>>::new(id, key.into(), envref);
        d.binary = Some(Arc::new(b"bytes that must not escape".to_vec()));
        if with_value {
            d.data = Some(Arc::new(Value::from("v")));
        }
        d.status = status;
        d
    }

    fn test_envref() -> EnvRef<SimpleEnvironment<Value>> {
        SimpleEnvironment::<Value>::new().to_ref()
    }

    /// U4 — the claim that extracting the classifier changes no state-read behaviour.
    /// Asserts the behaviour *class* for every status, not merely that something is returned.
    #[tokio::test]
    async fn test_poll_state_agrees_with_read_exposure() {
        let envref = test_envref();
        for (i, status) in ALL_STATUSES.into_iter().enumerate() {
            let d = asset_with_binary(9200 + i as u64, status, true, envref.clone());
            match status.read_exposure() {
                ReadExposure::Value => {
                    let st = d.poll_state().expect("Value exposure must yield a state");
                    assert!(!st.is_none(), "{:?} must carry a value", status);
                }
                ReadExposure::MetadataOnly => {
                    let st = d.poll_state().expect("MetadataOnly must yield a state");
                    assert!(st.is_none(), "{:?} must carry no value", status);
                }
                ReadExposure::Expired | ReadExposure::Pending => {
                    assert!(d.poll_state().is_none(), "{:?} must be hidden", status);
                }
            }
        }
    }

    /// Directory's "dir" type identifier survives the MetadataOnly arm's inner match.
    #[tokio::test]
    async fn test_poll_state_directory_keeps_dir_type_identifier() {
        let envref = test_envref();
        let d = asset_with_binary(9210, Status::Directory, false, envref.clone());
        let st = d.poll_state().expect("Directory yields a metadata-only state");
        // The identifier is stamped on the METADATA; `State::type_identifier` reports the value's,
        // which is `none` for every metadata-only state.
        assert_eq!(
            st.metadata.type_identifier().unwrap_or_default(),
            "dir",
            "Directory must keep its type identifier through the MetadataOnly arm"
        );

        // Error and Cancelled deliberately do not stamp one.
        let e = asset_with_binary(9211, Status::Error, false, envref);
        let st = e.poll_state().expect("Error yields a metadata-only state");
        assert_ne!(st.metadata.type_identifier().unwrap_or_default(), "dir");
    }

    /// U5 — poll_binary exposes bytes only for Value exposure. This is the bug fix.
    #[tokio::test]
    async fn test_poll_binary_is_gated_by_status() {
        let envref = test_envref();
        for (i, status) in ALL_STATUSES.into_iter().enumerate() {
            let d = asset_with_binary(9220 + i as u64, status, true, envref.clone());
            match status.read_exposure() {
                ReadExposure::Value => assert!(
                    d.poll_binary().is_some(),
                    "{:?} must expose cached bytes",
                    status
                ),
                ReadExposure::MetadataOnly | ReadExposure::Expired | ReadExposure::Pending => {
                    assert!(
                        d.poll_binary().is_none(),
                        "{:?} must not expose cached bytes",
                        status
                    )
                }
            }
        }
    }

    /// U6 — poll_binary_any_status adds Expired and nothing else.
    #[tokio::test]
    async fn test_poll_binary_any_status_adds_only_expired() {
        let envref = test_envref();
        for (i, status) in ALL_STATUSES.into_iter().enumerate() {
            let d = asset_with_binary(9240 + i as u64, status, true, envref.clone());
            match status.read_exposure() {
                ReadExposure::Value | ReadExposure::Expired => assert!(
                    d.poll_binary_any_status().is_some(),
                    "{:?} must be recoverable",
                    status
                ),
                ReadExposure::MetadataOnly | ReadExposure::Pending => assert!(
                    d.poll_binary_any_status().is_none(),
                    "{:?} has nothing to recover",
                    status
                ),
            }
        }
    }

    /// U7 — binary_unchecked is status-blind. This is its whole contract, and what keeps
    /// persistence independent of the read gate.
    #[tokio::test]
    async fn test_binary_unchecked_is_status_blind() {
        let envref = test_envref();
        for (i, status) in ALL_STATUSES.into_iter().enumerate() {
            let d = asset_with_binary(9260 + i as u64, status, true, envref.clone());
            let (bytes, _) = d
                .binary_unchecked()
                .unwrap_or_else(|| panic!("binary_unchecked must yield bytes for {:?}", status));
            assert_eq!(bytes.as_ref().as_slice(), b"bytes that must not escape");
        }
    }

    /// Example 1 — an expired asset with cached bytes hides them from every normal read,
    /// while the bytes remain recoverable.
    #[tokio::test]
    async fn test_expired_asset_hides_cached_binary() -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();
        let assetref = asset_with_binary(9280, Status::Ready, true, envref).to_ref();
        assert!(
            assetref.poll_binary().await.is_some(),
            "precondition: bytes are cached and visible while Ready"
        );

        assetref.expire().await?;
        assert_eq!(assetref.status().await, Status::Expired);

        assert!(
            assetref.poll_binary().await.is_none(),
            "poll_binary must hide expired bytes"
        );
        assert!(
            assetref.try_poll_binary().is_none(),
            "try_poll_binary must hide expired bytes"
        );
        assert!(
            assetref.poll_state().await.is_none(),
            "state reads unchanged: still hidden"
        );

        let err = assetref
            .get_binary()
            .await
            .expect_err("get_binary must fail when expired");
        assert!(err.to_string().to_lowercase().contains("expired"));

        // Hidden, not dropped.
        let (recovered, _) = assetref
            .poll_binary_any_status()
            .await
            .ok_or("retained bytes must remain reachable")?;
        assert_eq!(recovered.as_ref().as_slice(), b"bytes that must not escape");
        Ok(())
    }

    /// U9 — `get` gets the same pre-wait check, so it errors instead of blocking forever.
    /// The timeout is the assertion: before this change the test hangs rather than fails.
    #[tokio::test]
    async fn test_get_on_expired_errors_instead_of_blocking()
    -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();
        let assetref = asset_with_binary(9290, Status::Ready, true, envref).to_ref();
        assetref.expire().await?;

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), assetref.get()).await;
        let result = outcome.map_err(|_| "get() blocked on an expired asset instead of erroring")?;
        assert!(result.is_err(), "get() on Expired must return Err");
        Ok(())
    }

    /// Example 2 / U11 — recovery works whether or not bytes were ever cached.
    /// Every other test here reaches Expired via a populated `binary`; this one does not,
    /// which is the case a strict alias implementation would have failed.
    #[tokio::test]
    async fn test_binary_recovery_serializes_when_nothing_cached()
    -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();
        let key = parse_key("gate/retained_no_bytes.txt")?;
        let mut d = AssetData::<SimpleEnvironment<Value>>::new(9300, key.into(), envref);
        d.data = Some(Arc::new(Value::from("recoverable")));
        d.binary = None; // never serialized — the common case
        d.status = Status::Expired;
        let assetref = d.to_ref();

        // The poll-level recovery honestly reports "nothing cached".
        assert!(assetref.poll_binary_any_status().await.is_none());

        // The get-level recovery materialises it.
        let (bytes, _) = assetref
            .get_binary_any_status()
            .await?
            .ok_or("retained expired data must be recoverable as binary")?;
        assert_eq!(bytes.as_ref().as_slice(), b"recoverable");

        // State and binary recovery must agree about whether anything is there.
        assert!(assetref.poll_state_any_status().await.is_some());
        Ok(())
    }

    /// U8 / Example 3 — the statuses with no valid binary, and where their error comes from.
    #[tokio::test]
    async fn test_get_binary_error_identity() -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();

        // Error: reuses the asset's own recorded failure.
        let failed = asset_with_binary(9310, Status::Ready, true, envref.clone()).to_ref();
        failed
            .fail_asset(Error::general_error("recipe blew up".to_owned()))
            .await?;
        assert_eq!(failed.status().await, Status::Error);
        let err = failed
            .get_binary()
            .await
            .expect_err("Error must not yield bytes");
        assert!(
            err.to_string().contains("recipe blew up"),
            "must reuse the recorded failure, got: {}",
            err
        );
        assert!(failed.poll_binary().await.is_none());
        assert!(
            failed.poll_state().await.is_some(),
            "poll_state unchanged: metadata-only state"
        );

        // Cancelled: records no error, so one is constructed.
        let cancelled = asset_with_binary(9311, Status::Cancelled, true, envref.clone()).to_ref();
        let err = cancelled
            .get_binary()
            .await
            .expect_err("Cancelled must not yield bytes");
        assert!(err.to_string().to_lowercase().contains("cancel"));
        assert!(cancelled.poll_binary().await.is_none());
        assert!(cancelled.poll_state().await.is_some());

        // Directory: likewise constructed.
        let dir = asset_with_binary(9312, Status::Directory, false, envref).to_ref();
        let err = dir
            .get_binary()
            .await
            .expect_err("Directory must not yield bytes");
        assert!(err.to_string().to_lowercase().contains("director"));
        assert!(dir.poll_binary().await.is_none());
        assert!(dir.poll_state().await.is_some());
        Ok(())
    }

    /// The `MetadataOnly` and `Pending` rows of `get_binary_any_status`, which the first pass
    /// through the suite left unasserted — every other test reaches recovery through a status
    /// that *has* a binary form.
    ///
    /// These statuses have no binary representation at all, so recovery must report absence
    /// rather than serialize the metadata-only sentinel `poll_state_any_status` returns for them.
    /// Serializing it fails two different ways: `Error` surfaces its recorded value error as if
    /// serialization had broken, and `Cancelled`/`Directory` have no value error, so they
    /// fabricate bytes out of a `none` value.
    #[tokio::test]
    async fn test_binary_recovery_declines_statuses_with_no_binary()
    -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();
        for (i, status) in ALL_STATUSES.into_iter().enumerate() {
            match status.read_exposure() {
                ReadExposure::MetadataOnly | ReadExposure::Pending => {}
                ReadExposure::Value | ReadExposure::Expired => continue,
            }
            // A retained value but no cached bytes: the shape that would tempt serialization.
            let key = parse_key("gate/no_binary_form.txt")?;
            let mut d =
                AssetData::<SimpleEnvironment<Value>>::new(9340 + i as u64, key.into(), envref.clone());
            d.data = Some(Arc::new(Value::from("not serializable via this path")));
            d.binary = None;
            d.status = status;
            let assetref = d.to_ref();

            assert!(
                assetref.get_binary_any_status().await?.is_none(),
                "{:?} has no binary representation; recovery must report absence, not serialize",
                status
            );
        }
        Ok(())
    }

    /// U10 — the gate is not "binary required": a Value-exposure asset with no cached bytes
    /// still yields bytes, by serializing on demand.
    #[tokio::test]
    async fn test_get_binary_serializes_when_nothing_cached()
    -> Result<(), Box<dyn std::error::Error>> {
        let envref = test_envref();
        let key = parse_key("gate/value_no_bytes.txt")?;
        let mut d = AssetData::<SimpleEnvironment<Value>>::new(9320, key.into(), envref);
        d.data = Some(Arc::new(Value::from("serialize me")));
        d.binary = None;
        d.status = Status::Ready;
        let assetref = d.to_ref();

        assert!(assetref.poll_binary().await.is_none());
        let (bytes, _) = assetref.get_binary().await?;
        assert_eq!(bytes.as_ref().as_slice(), b"serialize me");
        Ok(())
    }

    /// I1 — persistence must not regress. `save_to_store` runs at statuses the read gate hides
    /// (`AssetRef::set_state` persists with whatever status the caller supplied), so it must use
    /// the status-blind accessor. Before Part B of Step 3 this test fails with
    /// "Failed to obtain binary value for storing".
    #[tokio::test]
    async fn test_persistence_works_at_non_value_status()
    -> Result<(), Box<dyn std::error::Error>> {
        let key = parse_key("gate/persist_at_storing.txt")?;
        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        let envref = env.to_ref();
        let store = envref.get_async_store();

        let mut d =
            AssetData::<SimpleEnvironment<Value>>::new(9330, key.clone().into(), envref.clone());
        d.binary = Some(Arc::new(b"persist me anyway".to_vec()));
        d.status = Status::Storing; // Pending exposure: hidden from every normal read
        let assetref = d.to_ref();

        assert!(
            assetref.poll_binary().await.is_none(),
            "precondition: the read gate hides these bytes"
        );

        assetref.save_to_store().await?;

        assert!(store.contains(&key).await?);
        let (stored, _) = store.get(&key).await?;
        assert_eq!(stored, b"persist me anyway");
        Ok(())
    }

    // ==================================================================================
    // Keyed-recipe ownership — `owned_key_asset`, `remove_key_asset_if`
    //
    // See `specs/design/keyed-recipe-ownership/`. The property these protect is that
    // asking *who owns a key* never evaluates anything: `evaluate_recipe` asks it about
    // the key it is itself evaluating, so an evaluating answer recurses forever.
    // ==================================================================================

    /// Environment whose `counted.txt` recipe runs a command that counts its own calls.
    ///
    /// The counter is per-environment rather than a `static`, so these tests do not
    /// interfere when the harness runs them in parallel.
    async fn ownership_env() -> (EnvRef<SimpleEnvironment<Value>>, Arc<AtomicUsize>) {
        use crate::context::Environment;
        use crate::recipes::DefaultRecipeProvider;

        let calls = Arc::new(AtomicUsize::new(0));
        let store = AsyncMemoryStore::new(&parse_key("").expect("root key"));
        store
            .set(
                &parse_key("recipes.yaml").expect("recipes key"),
                b"recipes:\n  - query: counted/counted.txt\n",
                &Metadata::new(),
            )
            .await
            .expect("write recipes.yaml");

        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));

        let counter = calls.clone();
        env.command_registry
            .register_command(CommandKey::new_name("counted"), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("counted"))
            })
            .expect("register counted");

        (env.to_ref(), calls)
    }

    /// A volatile asset registered under `key`, which is a state the manager itself never
    /// creates — it is what a stale entry looks like after a result turns out volatile.
    async fn insert_volatile_asset(
        envref: &EnvRef<SimpleEnvironment<Value>>,
        key: &Key,
    ) -> AssetRef<SimpleEnvironment<Value>> {
        let manager = envref.get_asset_manager();
        let asset = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            key.clone().into(),
            envref.clone(),
        );
        {
            let mut data = asset.data.write().await;
            data.is_volatile = true;
        }
        manager.insert_key_asset(key, asset.clone()).await;
        asset
    }

    /// T10 — the property the whole design rests on: the ownership query must not evaluate.
    ///
    /// Asserted with a call counter, before and after the key is registered. A future
    /// "simplification" of `owned_key_asset` back into `get` fails here, and the message
    /// says why that matters.
    #[tokio::test]
    async fn owned_key_asset_does_not_evaluate() {
        let (envref, calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");

        assert!(
            manager.owned_key_asset(&key).await.is_none(),
            "nothing is registered before the first request"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "asking who owns a key must not evaluate its recipe"
        );

        manager.get(&key).await.expect("get").get().await.expect("value");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "precondition: evaluated once");

        assert!(manager.owned_key_asset(&key).await.is_some());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the ownership query must not re-run the recipe either"
        );
    }

    /// T7 — a registered key reports its own asset as the owner.
    #[tokio::test]
    async fn owned_key_asset_returns_registered_owner() {
        let (envref, _calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");

        let asset = manager.get(&key).await.expect("get");
        let owner = manager.owned_key_asset(&key).await.expect("an owner");
        assert_eq!(owner.id(), asset.id());
    }

    /// T8 — an unregistered key has no owner, and that is an answer rather than an error.
    #[tokio::test]
    async fn owned_key_asset_none_when_unregistered() {
        let (envref, _calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        assert!(manager
            .owned_key_asset(&parse_key("never/requested.txt").expect("key"))
            .await
            .is_none());
    }

    /// T9 — a volatile entry is not an owner, and is evicted on the way out.
    ///
    /// This is what makes `None` mean "evaluate it yourself" for a volatile keyed recipe,
    /// which is `VOLATILE-KEYED-RECIPE-SELF-DELEGATION`.
    #[tokio::test]
    async fn owned_key_asset_evicts_volatile_entry() {
        let (envref, _calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");
        let volatile = insert_volatile_asset(&envref, &key).await;

        assert!(
            manager.owned_key_asset(&key).await.is_none(),
            "a volatile asset is never an owner"
        );
        assert!(
            manager.lookup_key_asset(&key).is_none(),
            "and it is removed rather than left to be found again"
        );
        assert!(volatile.is_volatile().await, "sanity: it really was volatile");
    }

    /// T12 — an asset that turns out volatile is used once, then dropped from the key map.
    ///
    /// The recipe is not volatile, so the manager registers the asset as normal; volatility
    /// is discovered on the *result*. `try_to_set_ready` reaches that state from a metadata
    /// expiry a command sets during evaluation, which no registration-time check can predict
    /// — here it is set directly, so the test pins the eviction rule rather than the expiry
    /// machinery that leads to it.
    ///
    /// Without the eviction the second `get` returns the same asset forever:
    /// `Status::Volatile.is_finished()` is true, and the expiry re-check only fires for
    /// `Status::Ready`.
    ///
    /// The command counter is deliberately not asserted here. This key's value was persisted
    /// with `Status::Ready` before the flag was flipped, so the replacement asset fast-tracks
    /// it from the store rather than re-running the recipe — correct, and the store's gate
    /// doing its job. `volatile_keyed_recipe_recomputes_every_time` covers the genuine case,
    /// where the stored copy carries `Status::Volatile` and `try_fast_track` refuses it.
    #[tokio::test]
    async fn runtime_volatile_asset_is_evicted_from_key_map() {
        let (envref, calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");

        let first = manager.get(&key).await.expect("first get");
        assert_eq!(
            first.get().await.expect("value").try_into_string().expect("string"),
            "counted",
            "the caller that computed the value still receives it"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // The result turns out volatile.
        {
            let mut data = first.data.write().await;
            data.is_volatile = true;
            data.status = Status::Volatile;
        }

        let second = manager.get(&key).await.expect("second get");
        assert_ne!(
            second.id(),
            first.id(),
            "a volatile asset must not be served from the key map"
        );
        assert!(
            !second.is_volatile().await,
            "and the replacement is a fresh, non-volatile asset"
        );
        assert_eq!(
            second.get().await.expect("value").try_into_string().expect("string"),
            "counted"
        );
    }

    /// A genuinely volatile keyed recipe recomputes on every request.
    ///
    /// This is the user-visible half of "used once, never reused", and unlike T12 the command
    /// counter is meaningful: a volatile result persists with `Status::Volatile`, which
    /// `try_fast_track` refuses, so nothing short-circuits the second evaluation.
    #[tokio::test]
    async fn volatile_keyed_recipe_recomputes_every_time() {
        use crate::context::Environment;
        use crate::recipes::DefaultRecipeProvider;

        let calls = Arc::new(AtomicUsize::new(0));
        let store = AsyncMemoryStore::new(&parse_key("").expect("root key"));
        store
            .set(
                &parse_key("recipes.yaml").expect("recipes key"),
                b"recipes:\n  - query: vol_counted/vol.txt\n",
                &Metadata::new(),
            )
            .await
            .expect("write recipes.yaml");

        let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        let counter = calls.clone();
        env.command_registry
            .register_command(CommandKey::new_name("vol_counted"), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("vol"))
            })
            .expect("register vol_counted")
            .volatile = true;

        let envref = env.to_ref();
        let manager = envref.get_asset_manager();
        let key = parse_key("vol.txt").expect("key");

        for expected in 1..=2 {
            let asset = manager.get(&key).await.expect("get");
            assert_eq!(
                asset.get().await.expect("value").try_into_string().expect("string"),
                "vol"
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                expected,
                "a volatile recipe must run again on request {expected}"
            );
        }
    }

    /// T12 on the inline manager, whose key map is a plain `HashMap` behind a `std::Mutex`
    /// rather than `scc` — a separate eviction site with the same rule.
    #[tokio::test]
    async fn runtime_volatile_asset_is_evicted_from_key_map_immediate() {
        use crate::context::{Environment, ImmediateEnvironment};
        use crate::recipes::DefaultRecipeProvider;

        let calls = Arc::new(AtomicUsize::new(0));
        let store = AsyncMemoryStore::new(&parse_key("").expect("root key"));
        store
            .set(
                &parse_key("recipes.yaml").expect("recipes key"),
                b"recipes:\n  - query: counted/counted.txt\n",
                &Metadata::new(),
            )
            .await
            .expect("write recipes.yaml");

        let mut env: ImmediateEnvironment<Value> = ImmediateEnvironment::new();
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        let counter = calls.clone();
        env.command_registry
            .register_command(CommandKey::new_name("counted"), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("counted"))
            })
            .expect("register counted");

        let envref = env.to_ref();
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");

        let first = manager.get(&key).await.expect("first get");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        {
            let mut data = first.data.write().await;
            data.is_volatile = true;
            data.status = Status::Volatile;
        }

        let second = manager.get(&key).await.expect("second get");
        assert_ne!(second.id(), first.id());
        assert!(!second.is_volatile().await);
        assert_eq!(
            second.get().await.expect("value").try_into_string().expect("string"),
            "counted"
        );
    }

    /// T14 — the same rule on the query map, which `get_query_asset` bypasses for volatile
    /// queries exactly as the keyed path does.
    #[tokio::test]
    async fn runtime_volatile_query_asset_is_recomputed() {
        let (envref, calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let query = parse_query("counted").expect("query");

        let first = manager.get_asset(&query).await.expect("first get_asset");
        first.get().await.expect("value");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        {
            let mut data = first.data.write().await;
            data.is_volatile = true;
            data.status = Status::Volatile;
        }

        let second = manager.get_asset(&query).await.expect("second get_asset");
        assert_ne!(
            second.id(),
            first.id(),
            "a volatile asset must not be served from the query map either"
        );
        // Awaited, because the queued manager returns before evaluation finishes. A query
        // asset is not a resource, so `try_fast_track` declines it and the recipe genuinely
        // re-runs — which makes the counter meaningful here in a way it is not for T12.
        second.get().await.expect("value");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// A failed keyed evaluation can be retried: the second request runs the command again
    /// rather than being refused or serving the first failure from cache.
    ///
    /// Written for a re-entrancy guard that was reverted (see
    /// `INLINE-PATH-LACKS-EXECUTE-ONCE`), and kept because the property is worth holding on
    /// its own — `Status::Error` is a stale-terminal state precisely so a transient failure
    /// does not poison a key.
    #[tokio::test]
    async fn failed_keyed_evaluation_is_retried() {
        use crate::context::{Environment, ImmediateEnvironment};
        use crate::recipes::DefaultRecipeProvider;

        let attempts = Arc::new(AtomicUsize::new(0));
        let store = AsyncMemoryStore::new(&parse_key("").expect("root key"));
        store
            .set(
                &parse_key("recipes.yaml").expect("recipes key"),
                b"recipes:\n  - query: boom/boom.txt\n",
                &Metadata::new(),
            )
            .await
            .expect("write recipes.yaml");

        let mut env: ImmediateEnvironment<Value> = ImmediateEnvironment::new();
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        let counter = attempts.clone();
        env.command_registry
            .register_command(CommandKey::new_name("boom"), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(Error::general_error("boom".to_string()))
            })
            .expect("register boom");

        let envref = env.to_ref();
        let manager = envref.get_asset_manager();
        let key = parse_key("boom.txt").expect("key");

        // The inline manager runs during `get`, so a failing command surfaces there.
        for attempt in 1..=2 {
            let err = match manager.get(&key).await {
                Ok(asset) => asset
                    .get()
                    .await
                    .and_then(|state| state.value_state().map(|_| ()))
                    .expect_err("the command fails"),
                Err(e) => e,
            };
            assert_eq!(
                err.error_type,
                ErrorType::General,
                "attempt {attempt} must surface the command's own error: {err}"
            );
        }
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "a failed evaluation must be retried, not cached"
        );
    }

    /// T11 — `remove_key_asset_if` will not evict a replacement.
    ///
    /// The race it closes: a caller looks the key up, releases the lock to await the
    /// volatility check, and by the time it removes, a different asset is registered.
    ///
    /// This asserts the *decision*, sequentially. That the decision and the removal are also
    /// **atomic** is structural rather than testable here — each manager performs both under
    /// one lock (`remove_if_async` on `scc`, one `Mutex` acquisition on the inline map). A
    /// test cannot pin that without racing threads and being flaky; the thing to preserve is
    /// the single lock, not this assertion.
    #[tokio::test]
    async fn remove_key_asset_if_respects_id() {
        let (envref, _calls) = ownership_env().await;
        let manager = envref.get_asset_manager();
        let key = parse_key("counted.txt").expect("key");

        let first = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            key.clone().into(),
            envref.clone(),
        );
        manager.insert_key_asset(&key, first.clone()).await;

        let second = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            key.clone().into(),
            envref.clone(),
        );
        // Removed first, deliberately: the two managers disagree on whether
        // `insert_key_asset` overwrites an existing entry — `scc::insert_async` reports a
        // duplicate and the result is discarded, while `HashMap::insert` replaces. See
        // `specs/issues/ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE.md`. This test is about
        // `remove_key_asset_if`, so it does not depend on that unsettled behaviour.
        manager.remove_key_asset(&key).await;
        manager.insert_key_asset(&key, second.clone()).await;

        assert!(
            !manager.remove_key_asset_if(&key, first.id()).await,
            "the stale id must not match"
        );
        assert_eq!(
            manager.lookup_key_asset(&key).map(|a| a.id()),
            Some(second.id()),
            "the replacement survives"
        );

        assert!(manager.remove_key_asset_if(&key, second.id()).await);
        assert!(manager.lookup_key_asset(&key).is_none());
    }
}
