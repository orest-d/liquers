//! `MinimalInlineAssetManager`: an `AssetManager` written outside `liquers-core`.
//!
//! Written from scratch against the public API only (this file is part of an integration-test
//! crate, so it cannot see `pub(crate)` items). It is **not** a wrapper of `ImmediateAssetManager`:
//! a wrapper would prove nothing about whether the trait can be implemented from outside.
//!
//! What it demonstrates, in the order a manager author needs it:
//! - hold one `DependencyManager` and one key-mutation lock, and hand them out
//!   (`DependencyManagerAccess`, `KeyMutationAccess`);
//! - honour the registration invariants: at most one registered asset per key, `lookup_key_asset`
//!   returns exactly that asset, a volatile asset is never registered;
//! - create assets with `AssetData::new(..).to_ref()` and evaluate with `AssetRef::run_inline`;
//! - expire lazily with `AssetRef::expire_without_cascade`;
//! - publish new versions of written keys with `AssetManager::publish_version`;
//! - override `record_expiry` and record every call.
//!
//! Limits, deliberately: `set_state` stores a state that has bytes (a value that cannot be
//! serialized is an error), and the asset replaced by a write is cancelled but not told it was
//! removed (`AssetRef::notify_removed` is crate-private).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use liquers_core::{
    assets::{
        AssetData, AssetManager, AssetRef, DependencyManagerAccess, EvalMode, ExternalChangePolicy,
        KeyMutationAccess,
    },
    context::{EnvRef, Environment, GenericEnvironment},
    dependencies::DependencyManager,
    environment_builder::{
        AssetManagerKind, AssetManagerOptions, DependencyAuditPolicy, VersionVerification,
    },
    error::Error,
    expiration::ExpirationTime,
    metadata::{
        DependencyKey, ExpiryCause, ExpiryReason, LogEntry, Metadata, MetadataRecord, Status,
        Version,
    },
    query::{Key, Query},
    recipes::Recipe,
    state::State,
    value::Value,
};

/// An environment whose manager is [`MinimalInlineAssetManager`].
pub type MinimalEnv = GenericEnvironment<Value, (), MinimalKind>;

/// Selects [`MinimalInlineAssetManager`] as the builder's third type parameter.
pub struct MinimalKind;

impl AssetManagerKind for MinimalKind {
    type Manager<E: Environment> = MinimalInlineAssetManager<E>;

    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error> {
        if options.job_capacity.is_some() {
            return Err(Error::general_error(
                "job_capacity is set, but the minimal inline asset manager has no job queue"
                    .to_string(),
            ));
        }
        Ok(Arc::new(MinimalInlineAssetManager::new(envref, options)))
    }
}

pub struct MinimalInlineAssetManager<E: Environment> {
    envref: EnvRef<E>,
    /// Created once and handed out; never called into directly.
    graph: DependencyManager<E>,
    /// Serialises keyed mutations (`KeyMutationAccess`).
    mutation_lock: tokio::sync::Mutex<()>,
    /// At most one asset per key.
    assets: Mutex<HashMap<Key, AssetRef<E>>>,
    queries: Mutex<HashMap<Query, AssetRef<E>>>,
    next_id: AtomicU64,
    started: AtomicBool,
    audit: DependencyAuditPolicy,
    verification: VersionVerification,
    external_change: ExternalChangePolicy,
    /// Every `record_expiry` call, to prove the method is the single writer.
    expiries: Mutex<Vec<(String, ExpiryReason)>>,
    /// When set, `record_expiry` also appends a distinguishing log line.
    audit_trail: AtomicBool,
}

impl<E: Environment> MinimalInlineAssetManager<E> {
    pub fn new(envref: EnvRef<E>, options: &AssetManagerOptions) -> Self {
        MinimalInlineAssetManager {
            envref,
            graph: DependencyManager::new(),
            mutation_lock: tokio::sync::Mutex::new(()),
            assets: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(9000),
            started: AtomicBool::new(false),
            audit: options.dependency_audit,
            verification: options.verify_versions,
            external_change: options.external_change,
            expiries: Mutex::new(Vec::new()),
            audit_trail: AtomicBool::new(false),
        }
    }

    /// Every `(subject, reason)` passed to `record_expiry` so far.
    pub fn recorded_expiries(&self) -> Vec<(String, ExpiryReason)> {
        self.expiries.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Make `record_expiry` append [`AUDIT_TRAIL_PREFIX`] log lines as well.
    pub fn enable_audit_trail(&self) {
        self.audit_trail.store(true, Ordering::SeqCst);
    }

    /// The number of registered keyed assets.
    pub fn registered_key_count(&self) -> usize {
        self.assets.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// The recipe a fresh resource asset is constructed with, carrying the `stored` / `cached`
    /// flags of the key's real recipe.
    fn resource_recipe(key: &Key, stored: Option<bool>, cached: Option<bool>) -> Recipe {
        let mut recipe: Recipe = key.into();
        recipe.stored = stored;
        recipe.cached = cached;
        recipe
    }

    /// The asset for `key`: the registered one, or a fresh one registered atomically.
    ///
    /// A volatile key, and a key whose recipe says `cached: false`, get a fresh asset that is
    /// registered nowhere.
    async fn resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        let recipe = self.recipe_opt(key).await?;
        let (stored, cached) = match &recipe {
            Some(recipe) => (recipe.stored, recipe.cached),
            None => (None, None),
        };
        let fresh = || {
            AssetData::new(
                self.new_id(),
                Self::resource_recipe(key, stored, cached),
                Some(key.clone()),
                self.envref.clone(),
            )
            .to_ref()
        };
        if self.is_volatile(key).await? || !cached.unwrap_or(true) {
            return Ok(fresh());
        }
        if let Some(existing) = self.lookup_key_asset(key) {
            return Ok(existing);
        }
        let candidate = fresh();
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        Ok(map.entry(key.clone()).or_insert(candidate).clone())
    }

    async fn query_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let fresh = || {
            AssetData::new(self.new_id(), query.into(), None, self.envref.clone()).to_ref()
        };
        if self.is_volatile_query(query).await? {
            return Ok(fresh());
        }
        let candidate = fresh();
        let mut map = self.queries.lock().unwrap_or_else(|e| e.into_inner());
        Ok(map.entry(query.clone()).or_insert(candidate).clone())
    }

    fn forget_query_asset(&self, query: &Query, asset_id: u64) {
        let mut map = self.queries.lock().unwrap_or_else(|e| e.into_inner());
        if map.get(query).map(|existing| existing.id()) == Some(asset_id) {
            map.remove(query);
        }
    }

    /// Whether a finished, `Ready` asset has passed its deadline; if so expires it (lazily, with
    /// no cascade) and says so.
    async fn expire_if_past_deadline(&self, asset: &AssetRef<E>) -> Result<bool, Error> {
        if asset.status().await != Status::Ready {
            return Ok(false);
        }
        let expiration_time: ExpirationTime = asset.expiration_time().await;
        if !expiration_time.is_expired() {
            return Ok(false);
        }
        asset
            .expire_without_cascade(ExpiryReason::Direct {
                cause: ExpiryCause::Deadline { expiration_time },
            })
            .await?;
        Ok(true)
    }
}

/// Prefix of the log line [`MinimalInlineAssetManager::enable_audit_trail`] adds.
pub const AUDIT_TRAIL_PREFIX: &str = "audit-trail: ";

impl<E: Environment> DependencyManagerAccess<E> for MinimalInlineAssetManager<E> {
    fn dependency_manager(&self) -> &DependencyManager<E> {
        &self.graph
    }
}

impl<E: Environment> KeyMutationAccess for MinimalInlineAssetManager<E> {
    fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.mutation_lock
    }
}

fn is_unusable(status: Status) -> bool {
    matches!(
        status,
        Status::Expired | Status::Error | Status::Cancelled | Status::Volatile
    )
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AssetManager<E> for MinimalInlineAssetManager<E> {
    fn dependency_audit_policy(&self) -> DependencyAuditPolicy {
        self.audit
    }
    fn version_verification(&self) -> VersionVerification {
        self.verification
    }
    fn external_change_policy(&self) -> ExternalChangePolicy {
        self.external_change
    }

    // Called under the asset's `data` lock: synchronous, no awaiting, no asset access.
    fn record_expiry(&self, metadata: &mut Metadata, subject: &str, reason: &ExpiryReason) {
        self.expiries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((subject.to_string(), reason.clone()));
        match metadata {
            Metadata::MetadataRecord(_) => {
                let _ = metadata.set_expiry_reason(reason.clone());
                let _ = metadata.add_log_entry(reason.log_entry(subject));
                if self.audit_trail.load(Ordering::SeqCst) {
                    let _ = metadata.add_log_entry(LogEntry::info(format!(
                        "{AUDIT_TRAIL_PREFIX}{subject}"
                    )));
                }
            }
            Metadata::LegacyMetadata(_) => {}
        }
    }

    async fn get_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        if let Some(key) = query.key() {
            return self.get(&key).await;
        }
        loop {
            let asset = self.query_asset(query).await?;
            let status = asset.status().await;
            if is_unusable(status) {
                self.forget_query_asset(query, asset.id());
                continue;
            }
            if status.is_finished() {
                if self.expire_if_past_deadline(&asset).await? {
                    self.forget_query_asset(query, asset.id());
                    continue;
                }
                return Ok(asset);
            }
            asset.run_inline(None).await?;
            return Ok(asset);
        }
    }

    async fn apply(
        &self,
        recipe: Recipe,
        to: State<E::Value>,
        payload: Option<E::Payload>,
    ) -> Result<AssetRef<E>, Error> {
        let asset =
            AssetData::new_ext(self.new_id(), recipe, to, None, self.envref.clone()).to_ref();
        asset.run_inline(payload).await?;
        Ok(asset)
    }

    async fn get_dependency_asset_with_payload(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
        payload: Option<E::Payload>,
        payload_path: Vec<Query>,
    ) -> Result<AssetRef<E>, Error> {
        let _ = parent;
        if query.key().is_some() {
            return Err(Error::general_error(format!(
                "Query '{}' is a key and cannot be evaluated with a payload",
                query.encode()
            ))
            .with_query(query));
        }
        let asset = self.query_asset(query).await?;
        asset.set_payload_path(payload_path).await;
        asset.run_inline(payload).await?;
        Ok(asset)
    }

    async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        loop {
            let asset = self.resource_asset(key).await?;
            let status = asset.status().await;
            if is_unusable(status) {
                let _mutation = self.mutation_lock.lock().await;
                self.remove_key_asset_if(key, asset.id()).await;
                continue;
            }
            if status.is_finished() {
                if self.expire_if_past_deadline(&asset).await? {
                    let _mutation = self.mutation_lock.lock().await;
                    self.remove_key_asset_if(key, asset.id()).await;
                    continue;
                }
                return Ok(asset);
            }
            if asset.fast_track().await? {
                return Ok(asset);
            }
            asset.run_inline(None).await?;
            return Ok(asset);
        }
    }

    async fn set_binary(
        &self,
        key: &Key,
        binary: &[u8],
        mut metadata: MetadataRecord,
    ) -> Result<(), Error> {
        let mutation = self.mutation_lock.lock().await;
        if let Some(old) = self.lookup_key_asset(key) {
            old.cancel().await?;
            self.remove_key_asset(key).await;
        }
        let final_status = if metadata.status == Status::Expired {
            Status::Expired
        } else if metadata.status == Status::Error {
            Status::Error
        } else if self.recipe_opt(key).await?.is_some() {
            Status::Override
        } else {
            Status::Source
        };
        metadata.status = final_status;
        metadata.set_updated_now();
        metadata.add_log_entry(LogEntry::info("Data set externally".to_string()));
        if final_status != Status::Error {
            metadata.version = Some(Version::from_content(binary));
        }
        if metadata.stored() {
            let stored_bytes: &[u8] = if final_status == Status::Error { &[] } else { binary };
            self.get_envref()
                .get_async_store()
                .set(key, stored_bytes, &Metadata::MetadataRecord(metadata.clone()))
                .await?;
        }
        if matches!(final_status, Status::Ready | Status::Source | Status::Override) {
            if let Some(version) = metadata.version {
                self.publish_version(&DependencyKey::from(key), version).await;
            }
        }
        drop(mutation);
        self.refresh_listing_version(&key.parent()).await;
        Ok(())
    }

    async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error> {
        let binary = state.as_bytes().map_err(|e| {
            Error::general_error(format!(
                "the minimal manager can only store a state that has bytes: {e}"
            ))
        })?;
        let mut metadata = match state.metadata.as_ref().clone() {
            Metadata::MetadataRecord(record) => record,
            Metadata::LegacyMetadata(_) => MetadataRecord::new(),
        };
        metadata.add_log_entry(LogEntry::info("State set externally".to_string()));
        self.set_binary(key, &binary, metadata).await
    }

    // --- primitives ---

    fn eval_mode(&self) -> EvalMode {
        EvalMode::Inline
    }

    fn lookup_key_asset(&self, key: &Key) -> Option<AssetRef<E>> {
        self.assets.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()
    }

    fn lookup_query_asset(&self, query: &Query) -> Option<AssetRef<E>> {
        match query.key() {
            Some(key) => self.lookup_key_asset(&key),
            None => self.queries.lock().unwrap_or_else(|e| e.into_inner()).get(query).cloned(),
        }
    }

    async fn remove_key_asset(&self, key: &Key) {
        self.assets.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }

    async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool {
        let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
        if map.get(key).map(|existing| existing.id()) == Some(asset_id) {
            map.remove(key);
            true
        } else {
            false
        }
    }

    fn next_id_for_asset(&self) -> u64 {
        self.new_id()
    }

    fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    fn create_temporary_asset(&self) -> AssetRef<E> {
        AssetRef::new_temporary(self.envref.clone())
    }

    fn start(&self) -> Result<(), Error> {
        self.refresh_command_versions()?;
        self.started.store(true, Ordering::Release);
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    /// No monitor task: deadlines are checked lazily on access.
    fn track_expiration(&self, _asset_ref: &AssetRef<E>, _expiration_time: &ExpirationTime) {}

    async fn remove_expired_from_maps(
        &self,
        asset_id: u64,
        query: Option<&Query>,
        key: Option<&Key>,
    ) -> bool {
        if let Some(query) = query {
            let mut map = self.queries.lock().unwrap_or_else(|e| e.into_inner());
            if map.get(query).map(|existing| existing.id()) == Some(asset_id) {
                map.remove(query);
                return true;
            }
        }
        if let Some(key) = key {
            let _mutation = self.mutation_lock.lock().await;
            return self.remove_key_asset_if(key, asset_id).await;
        }
        false
    }
}
