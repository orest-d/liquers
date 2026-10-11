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
//! - expire lazily with `AssetRef::expire_without_cascade`, then cascade to the dependents with
//!   `AssetManager::cascade_expire_dependents`, as a monitor does;
//! - publish new versions of written keys with `AssetManager::publish_version`;
//! - override `record_expiry` and record every call.
//!
//! - replace an asset on a write with `AssetRef::cancel_for_replacement`, unmap it, and tell its
//!   subscribers with `AssetRef::notify_removed` once the key holds the replacement;
//! - install a supplied state, serializable or not, with `AssetRef::new_installed`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use liquers_core::{
    cache_strategy::CacheStrategy,
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

    /// Cancels and unmaps the key's current asset. The caller sends it `notify_removed` once the
    /// key holds its replacement, so a woken subscriber does not find the old asset again.
    async fn replace_key_asset(&self, key: &Key) -> Result<Option<AssetRef<E>>, Error> {
        let old = self.lookup_key_asset(key);
        if let Some(old) = &old {
            old.cancel_for_replacement().await?;
            self.remove_key_asset(key).await;
        }
        Ok(old)
    }

    /// `set_binary`'s write, under the caller's key-mutation lock.
    async fn write_binary(
        &self,
        key: &Key,
        binary: &[u8],
        mut metadata: MetadataRecord,
    ) -> Result<(), Error> {
        let final_status = self.written_status(key, metadata.status).await?;
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
        Ok(())
    }

    /// `set_state`'s write, under the caller's key-mutation lock.
    async fn install_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error> {
        let mut metadata = match state.metadata.as_ref().clone() {
            Metadata::MetadataRecord(record) => record,
            Metadata::LegacyMetadata(_) => MetadataRecord::new(),
        };
        let final_status = self.written_status(key, metadata.status).await?;
        metadata.status = final_status;
        metadata.set_updated_now();
        metadata.add_log_entry(LogEntry::info("State set externally".to_string()));
        // A value with no byte form is still installed and served; it gets a unique version.
        let binary = state.as_bytes().ok();
        if final_status != Status::Error {
            metadata.version = Some(match &binary {
                Some(bytes) => Version::from_content(bytes),
                None => Version::new_unique(),
            });
        }
        let asset = AssetRef::new_installed(
            self.new_id(),
            key.clone(),
            state.with_metadata(Metadata::MetadataRecord(metadata.clone())),
            final_status,
            self.envref.clone(),
        )?;
        if metadata.stored() {
            let store = self.get_envref().get_async_store();
            let stored = Metadata::MetadataRecord(metadata.clone());
            match (&binary, final_status) {
                (_, Status::Error) => store.set(key, &[], &stored).await?,
                (Some(bytes), _) => store.set(key, bytes, &stored).await?,
                (None, _) => {
                    // A metadata-only entry: bytes a previous value left must not outlive it.
                    if store.contains(key).await? {
                        store.remove(key).await?;
                    }
                    store.set_metadata(key, &stored).await?
                }
            }
        }
        // Mapped only once persisted: a failed write leaves the key without the rejected value.
        self.assets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.clone(), asset.clone());
        if matches!(final_status, Status::Source | Status::Override) {
            if let Some(version) = metadata.version {
                self.publish_version(&DependencyKey::from(key), version).await;
                // The state's dependency records, so a later change to one expires it.
                self.track_installed(&asset, version).await;
            }
        }
        Ok(())
    }

    /// The status a supplied value is written with: `Expired` and `Error` are kept, anything else
    /// is `Override` when the key has a recipe and `Source` otherwise.
    async fn written_status(&self, key: &Key, supplied: Status) -> Result<Status, Error> {
        Ok(match supplied {
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
        })
    }

    fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// The recipe a fresh resource asset is constructed with, carrying the `stored` / `cached`
    /// flags of the key's real recipe.
    fn resource_recipe(key: &Key, stored: Option<bool>, cached: Option<CacheStrategy>) -> Recipe {
        let mut recipe: Recipe = key.into();
        recipe.stored = stored;
        recipe.cached = cached;
        recipe
    }

    /// The asset for `key`: the registered one, or a fresh one registered atomically.
    ///
    /// A volatile key gets a fresh asset registered nowhere; so does a key whose recipe's caching
    /// strategy keeps no result, unless an asset for it already exists.
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
        if self.is_volatile(key).await? {
            return Ok(fresh());
        }
        // An existing asset is reused under every caching strategy.
        if let Some(existing) = self.lookup_key_asset(key) {
            return Ok(existing);
        }
        // The recipe strategy governs a recipe evaluation; a key with no recipe is kept.
        let keeps_result = match &recipe {
            Some(recipe) => recipe
                .effective_cache_strategy(self.recipe_cache_strategy())
                .keeps_result(),
            None => true,
        };
        if !keeps_result {
            return Ok(fresh());
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

    /// Whether a finished, `Ready` asset has passed its deadline; if so expires it and its
    /// dependents and says so. Laziness is only how the expiry is discovered: once it is known,
    /// the cascade follows, as from a monitor.
    async fn expire_if_past_deadline(&self, asset: &AssetRef<E>) -> Result<bool, Error> {
        if asset.status().await != Status::Ready {
            return Ok(false);
        }
        let expiration_time: ExpirationTime = asset.expiration_time().await;
        if !expiration_time.is_expired() {
            return Ok(false);
        }
        let cause = ExpiryCause::Deadline { expiration_time };
        asset
            .expire_without_cascade(ExpiryReason::Direct {
                cause: cause.clone(),
            })
            .await?;
        if let Some(key) = asset.key().await {
            self.cascade_expire_dependents(&DependencyKey::from(&key), cause)
                .await;
        }
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
        let replaced = self.replace_key_asset(key).await?;
        let result = self.write_binary(key, binary, metadata).await;
        if let Some(old) = &replaced {
            old.notify_removed().await;
        }
        drop(mutation);
        result?;
        self.refresh_listing_version(&key.parent()).await;
        Ok(())
    }

    async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error> {
        let mutation = self.mutation_lock.lock().await;
        let replaced = self.replace_key_asset(key).await?;
        let result = self.install_state(key, state).await;
        if let Some(old) = &replaced {
            old.notify_removed().await;
        }
        drop(mutation);
        result?;
        self.refresh_listing_version(&key.parent()).await;
        Ok(())
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
