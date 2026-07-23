//! This defines Environment and Context.
//!
//! * [Environment] is a global object that holds configuration and services like command executor, asset store, etc.
//! * [Session] connects multiple actions of a single user.
//! * [User] represents an individual user interacting with the system.
//! * [Context] is a per-action object that holds e.g. the environment reference, metadata, current working directory.
//!
//! This builds a natural hierarchy. The most specific structure is the [Context],
//! which provides access to thhe [Session] and [Environment].
//! [ActionContext] is a public interface to the Context.

use core::panic;
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use crate::maybe_send::MaybeBoxed;

use crate::{
    assets::{AssetManager, AssetRef, AssetServiceMessage},

    command_metadata::CommandMetadataRegistry,
    commands::{CommandExecutor, CommandRegistry},
    dependencies::ScheduleNode,
    error::Error,
    expiration::Expires,
    metadata::{DependencyKey, DependencyRecord, LogEntry, MetadataRecord, ProgressEntry, Version},
    query::{Key, Query, TryToQuery},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    value::ValueInterface,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::assets::DefaultAssetManager;
#[cfg(not(target_arch = "wasm32"))]
use crate::store::{NoStore, Store};
#[cfg(not(target_arch = "wasm32"))]
use crate::cache::Cache;

pub enum User {
    System,
    Anonymous,
    Named(String),
}

pub trait Session {
    fn get_user(&self) -> &User;
}

pub trait Environment:
    Sized + crate::maybe_send::MaybeSync + crate::maybe_send::MaybeSend + 'static
{
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<Self>;
    type SessionType: Session;
    type Payload: crate::commands::PayloadType;
    type AssetManager: AssetManager<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore>;

    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>>;

    fn create_session(&self, user: User) -> Self::SessionType;

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>>;

    fn init_with_envref(&self, envref: EnvRef<Self>);

    fn to_ref(self) -> EnvRef<Self> {
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone());
        envref
    }
}

// TODO: Define Session and User; Session connects multiple actions of a single user.
// TODO: Session could be "SystemSession" for automated tasks or recipes.
pub struct EnvRef<E: Environment>(pub Arc<E>);

impl<E: Environment> EnvRef<E> {
    pub fn new(env: E) -> Self {
        EnvRef(Arc::new(env))
    }
    #[cfg(feature = "async_store")]
    pub fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.0.get_async_store()
    }
    pub fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        self.0.get_command_metadata_registry()
    }
    pub fn get_command_executor(&self) -> &E::CommandExecutor {
        self.0.get_command_executor()
    }

    pub fn get_asset_manager(&self) -> Arc<E::AssetManager> {
        self.0.get_asset_manager()
    }

    pub fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<E>> {
        self.0.get_recipe_provider()
    }

    pub fn apply_recipe(
        &self,
        input_state: State<E::Value>,
        recipe: Recipe,
        context: Context<E>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<E::Value>, Error>> {
        Box::pin(E::apply_recipe(self.clone(), input_state, recipe, context))
    }

    pub fn evaluate<Q: TryToQuery>(
        &self,
        query: Q,
    ) -> crate::maybe_send::BoxFuture<'static, Result<AssetRef<E>, Error>> {
        let envref = self.clone();
        let rquery = query.try_to_query();

        async move {
            let asset_manager = envref.get_asset_manager();
            asset_manager.get_asset(&rquery?).await
        }
        .maybe_boxed()
    }

    pub fn evaluate_immediately<Q: TryToQuery>(
        &self,
        query: Q,
        payload: E::Payload,
    ) -> crate::maybe_send::BoxFuture<'static, Result<AssetRef<E>, Error>> {
        let envref = self.clone();
        let rquery = query.try_to_query();

        async move {
            let asset_manager = envref.get_asset_manager();
            let query = rquery?;
            asset_manager
                .apply_immediately(query.into(), State::new(), Some(payload))
                .await
        }
        .maybe_boxed()
    }
}

impl<E: Environment> Clone for EnvRef<E> {
    fn clone(&self) -> Self {
        EnvRef(self.0.clone())
    }
}

// TODO: There should be an asset reference
pub struct Context<E: Environment> {
    assetref: AssetRef<E>,
    envref: EnvRef<E>,
    cwd_key: Arc<Mutex<Option<Key>>>, // TODO: CWD should be owned by the context or maybe it should be in the Metadata
    service_tx: tokio::sync::mpsc::UnboundedSender<AssetServiceMessage>,
    pub payload: Option<E::Payload>,

    /// If true, this context is evaluating a volatile asset.
    /// Propagates to nested evaluations via context.evaluate()
    is_volatile: bool,

    /// Dependencies discovered during evaluation (via Context::evaluate calls).
    /// Collected here and written to the asset's metadata after evaluation completes.
    pending_dependencies: Arc<tokio::sync::Mutex<Vec<DependencyRecord>>>,
}

impl<E: Environment> Context<E> {
    pub async fn new(assetref: AssetRef<E>, is_volatile: bool) -> Self {
        let service_tx = assetref.service_sender().await;
        let envref = assetref.get_envref().await;
        Context {
            assetref,
            envref,
            cwd_key: Arc::new(Mutex::new(None)),
            service_tx,
            payload: None,
            is_volatile, // Initialize from parameter
            pending_dependencies: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn set_payload(&mut self, payload: E::Payload) {
        self.payload = Some(payload);
    }

    pub fn get_payload_clone(&self) -> Option<E::Payload> {
        self.payload.clone()
    }

    /// Schedule a dependency of the current asset without waiting for it, returning the
    /// captured child `AssetRef`. Internal helper (not a command-facing schedule/wait API):
    /// the only callers are `evaluate`, `get_dependency_state`, and the interpreter pre-pass.
    ///
    /// Classifies dependent/dependency as `ScheduleNode`s, cycle-checks and registers the
    /// edge at schedule time via `register_scheduled_dependency` (keyed-expansion model),
    /// captures the AssetRef exactly once (volatile-safe) via `get_dependency_asset`, and
    /// records the runtime dependency (metadata + untracked dependent). Does NOT enter
    /// `Status::Dependencies` — that happens at drain/wait time.
    pub(crate) async fn schedule_dependency_asset(
        &self,
        query: &Query,
    ) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        let query_dep_key = DependencyKey::from(query);

        // Current asset's key (if keyed) for dependent classification.
        let current_key_opt = {
            let lock = self.assetref.data.read().await;
            lock.recipe.key().ok().flatten()
        };

        let version = manager
            .dependency_manager()
            .get_version(&query_dep_key)
            .await
            .unwrap_or_else(Version::unknown);

        // Classify the dependent: keyed asset -> graph node; non-keyed query -> expression;
        // ad-hoc (no key, no query) -> skip registration (not a graph participant).
        let dependent_opt = if let Some(ref k) = current_key_opt {
            Some(ScheduleNode::Keyed(DependencyKey::from(k)))
        } else if let Some(q) = self.assetref.query().await {
            Some(ScheduleNode::Expression(DependencyKey::from(&q)))
        } else {
            None
        };
        if let Some(dependent) = &dependent_opt {
            let dependency = if query.key().is_some() {
                ScheduleNode::Keyed(query_dep_key.clone())
            } else {
                ScheduleNode::Expression(query_dep_key.clone())
            };
            // Cycle check + edge registration at schedule time (may return dependency_cycle).
            manager
                .dependency_manager()
                .register_scheduled_dependency(dependent, &dependency, version)
                .await?;
        }

        // Capture the AssetRef exactly once (volatile-safe) and schedule it.
        let asset = manager
            .get_dependency_asset(&self.assetref, query)
            .await?;

        // Record the runtime dependency (path-independent capture) as evaluate did.
        if current_key_opt.is_some() {
            manager
                .dependency_manager()
                .add_dependent_asset(&query_dep_key, self.assetref.downgrade())
                .await;
        }
        self.add_dependency(DependencyRecord::new(query_dep_key, version))
            .await;

        Ok(asset)
    }

    /// Wait on a previously-scheduled dependency AssetRef on behalf of the current asset.
    /// Thin wrapper over `AssetManager::wait_for_dependency`; idempotent.
    pub(crate) async fn wait_for_dependency(
        &self,
        asset: &AssetRef<E>,
    ) -> Result<State<E::Value>, Error> {
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        manager.wait_for_dependency(&self.assetref, asset).await
    }

    /// Drain the current asset's local dependency queue
    /// (= `AssetManager::drain_dependencies(current asset)`).
    pub async fn evaluate_local_queue(&self) -> Result<(), Error> {
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        manager.drain_dependencies(&self.assetref).await
    }

    /// Convenience: schedule a dependency and wait for its state.
    pub async fn get_dependency_state(&self, query: &Query) -> Result<State<E::Value>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.wait_for_dependency(&asset).await
    }

    /// Backwards-compatible: schedule the dependency, eagerly drain the local queue (so a
    /// handle-unaware caller may still `.get().await` the returned AssetRef safely), and
    /// return the captured AssetRef. Public signature unchanged.
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.evaluate_local_queue().await?;
        Ok(asset)
    }

    pub async fn apply(&self, query: &Query, to: State<E::Value>) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        envref.get_asset_manager().apply(query.into(), to).await
    }

    pub async fn get_metadata(&self) -> Result<MetadataRecord, Error> {
        let metadata = {
            let lock = self.assetref.data.read().await;
            lock.metadata.metadata_record()
        };

        if let Some(metadata) = metadata {
            Ok(metadata)
        } else {
            Err(Error::unexpected_error(format!(
                "{} has legacy metadata",
                self.assetref.asset_reference().await
            )))
        }
    }
    pub fn progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdatePrimaryProgress(progress))
            .map_err(|e| Error::general_error(format!("Failed to send progress message: {}", e)))
    }

    /// Returns true if this context is evaluating a volatile asset
    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }

    /// Create child context for nested evaluation, inheriting volatility
    /// NOTE: Context does NOT implement Clone trait (AssetRef prevents it).
    /// This method manually constructs a new Context with cloned Arc references.
    pub fn with_volatile(&self, volatile: bool) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
            is_volatile: volatile || self.is_volatile, // Propagate if parent is volatile
            pending_dependencies: self.pending_dependencies.clone(),
        }
    }

    pub fn secondary_progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdateSecondaryProgress(progress))
            .map_err(|e| {
                Error::general_error(format!("Failed to send secondary progress message: {}", e))
            })
    }
    pub async fn set_filename(&self, filename: &str) -> Result<(), Error> {
        self.assetref
            .data
            .write()
            .await
            .metadata
            .set_filename(filename)
            .map(|_| ())
    }
    pub fn add_log_entry(&self, entry: LogEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::LogMessage(entry))
            .map_err(|e| Error::general_error(format!("Failed to send log message: {}", e)))
    }
    pub fn debug(&self, message: &str) -> Result<(), Error> {
        eprintln!("DEBUG:   {}", message);
        self.add_log_entry(LogEntry::debug(message.to_string()))
    }
    pub fn info(&self, message: &str) -> Result<(), Error> {
        eprintln!("INFO:    {}", message);
        self.add_log_entry(LogEntry::info(message.to_string()))
    }
    pub fn warning(&self, message: &str) -> Result<(), Error> {
        eprintln!("WARNING: {}", message);
        self.add_log_entry(LogEntry::warning(message.to_string()))
    }
    pub fn error(&self, message: &str) -> Result<(), Error> {
        eprintln!("ERROR:   {}", message);
        self.add_log_entry(LogEntry::error(message.to_string()))
    }
    pub async fn clone_context(&self) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
            is_volatile: self.is_volatile,
            pending_dependencies: self.pending_dependencies.clone(),
        }
    }
    pub fn get_cwd_key(&self) -> Option<Key> {
        self.cwd_key.lock().unwrap().clone()
    }

    pub fn set_cwd_key(&self, key: Option<Key>) {
        let mut guard = self.cwd_key.lock().unwrap();
        *guard = key;
    }

    pub fn get_asset_ref(&self) -> AssetRef<E> {
        self.assetref.clone()
    }

    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    /// Get the pending dependencies collected during evaluation.
    pub async fn take_pending_dependencies(&self) -> Vec<DependencyRecord> {
        std::mem::take(&mut *self.pending_dependencies.lock().await)
    }

    /// Upsert a dependency into the pending list.
    /// If a record with the same key already exists, its version is replaced.
    pub async fn add_dependency(&self, record: DependencyRecord) {
        let mut deps = self.pending_dependencies.lock().await;
        if let Some(existing) = deps.iter_mut().find(|d| d.key == record.key) {
            // Version(0) is the dependency-manager sentinel for "unknown".
            // Do not let an unknown later observation erase a previously known
            // version for the same dependency.
            if existing.version.is_unknown() || !record.version.is_unknown() {
                existing.version = record.version;
            }
        } else {
            deps.push(record);
        }
    }

    pub(crate) async fn set_value(&self, value: E::Value) -> Result<(), Error> {
        self.assetref.set_value(value).await
    }

    pub(crate) async fn set_metadata_value(&self, metadata: MetadataRecord) -> Result<(), Error> {
        self.assetref
            .set_value(E::Value::from_metadata(metadata))
            .await
    }

    pub(crate) async fn set_state(&self, state: State<E::Value>) -> Result<(), Error> {
        self.assetref.set_state(state).await
    }

    // FIXME: Should not be public (only pub(crate)) - but needs to be used now in every environment in apply_recipe
    pub async fn set_expires(&self, expires: Expires) -> Result<(), Error> {
        let expiration_time = {
            let mut lock = self.assetref.data.write().await;
            lock.metadata.set_expiration_time_from(&expires)?;
            lock.metadata.expiration_time()
        };
        self.assetref.set_expiration_time(expiration_time).await;
        Ok(())
    }

    pub async fn set_error(&self, error: Error) -> Result<(), Error> {
        self.assetref.set_error(error).await
    }
}

impl<E: Environment> Clone for Context<E> {
    fn clone(&self) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
            is_volatile: self.is_volatile,
            pending_dependencies: self.pending_dependencies.clone(),
        }
    }
}
// TODO: There should be a reference to input_state_query
// TODO: There should be a reference to query including the current action

pub struct SimpleSession {
    pub user: User,
}
impl Session for SimpleSession {
    fn get_user(&self) -> &User {
        &self.user
    }
}

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
#[cfg(not(target_arch = "wasm32"))]
pub struct SimpleEnvironment<V: ValueInterface> {
    store: Arc<dyn Store>,
    #[cfg(feature = "async_store")]
    async_store: Arc<dyn crate::store::AsyncStore>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<DefaultAssetManager<Self>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface> Default for SimpleEnvironment<V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface> SimpleEnvironment<V> {
    pub fn new() -> Self {
        SimpleEnvironment {
            store: Arc::new(NoStore),
            command_registry: CommandRegistry::new(),
            //            cache: Arc::new(tokio::sync::RwLock::new(Box::new(NoCache::<V>::new()))),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(crate::store::NoAsyncStore),
            asset_store: Arc::new(crate::assets::DefaultAssetManager::new()),
            recipe_provider: None,
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::from(store);
        self
    }
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self {
        panic!("SimpleEnvironment does not support cache for now");
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface> Environment for SimpleEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = ();
    type AssetManager = DefaultAssetManager<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<DefaultAssetManager<Self>> {
        self.asset_store.clone()
    }
    fn create_session(&self, user: User) -> Self::SessionType {
        SimpleSession { user }
    }

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>> {
        use crate::interpreter::{apply_plan, finalize_plan};

        async move {
            let recipe_expires = recipe.expires.clone();
            let mut plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };

            finalize_plan(envref.clone(), &mut plan, &context).await?;
            let combined_expires = plan.expires.clone() | recipe_expires;
            context.set_expires(combined_expires).await?;

            let res = apply_plan(plan, input_state, context, envref).await?;

            Ok(res)
        }
        .maybe_boxed()
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        eprintln!("No recipe provider configured in SimpleEnvironment");
        Arc::new(crate::recipes::TrivialRecipeProvider)
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref.clone());
        let am = self.get_asset_manager();
        tokio::spawn(async move {
            am.start().await;
        });
    }
}

/// Environment backed by the spawn-free [`ImmediateAssetManager`] (inline evaluation).
///
/// Primary use: the manager-parametric test suite, so `ImmediateAssetManager` is exercised on
/// native alongside `SimpleEnvironment` (→ `DefaultAssetManager`). Also usable for embedded /
/// no-runtime contexts. Async-store only (no sync `Store`/`Cache`); `init_with_envref` does NOT
/// spawn — `start()` runs lazily on first evaluation.
pub struct ImmediateEnvironment<V: ValueInterface> {
    #[cfg(feature = "async_store")]
    async_store: Arc<dyn crate::store::AsyncStore>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<crate::assets::ImmediateAssetManager<Self>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,
}

impl<V: ValueInterface> Default for ImmediateEnvironment<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface> ImmediateEnvironment<V> {
    pub fn new() -> Self {
        ImmediateEnvironment {
            command_registry: CommandRegistry::new(),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(crate::store::NoAsyncStore),
            asset_store: Arc::new(crate::assets::ImmediateAssetManager::new()),
            recipe_provider: None,
        }
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
}

impl<V: ValueInterface> Environment for ImmediateEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = ();
    type AssetManager = crate::assets::ImmediateAssetManager<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<crate::assets::ImmediateAssetManager<Self>> {
        self.asset_store.clone()
    }

    fn create_session(&self, user: User) -> Self::SessionType {
        SimpleSession { user }
    }

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>> {
        use crate::interpreter::{apply_plan, finalize_plan};
        async move {
            let recipe_expires = recipe.expires.clone();
            let mut plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };
            finalize_plan(envref.clone(), &mut plan, &context).await?;
            let combined_expires = plan.expires.clone() | recipe_expires;
            context.set_expires(combined_expires).await?;
            let res = apply_plan(plan, input_state, context, envref).await?;
            Ok(res)
        }
        .maybe_boxed()
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        Arc::new(crate::recipes::TrivialRecipeProvider)
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) {
        // No spawn: ImmediateAssetManager::start() runs lazily on first evaluation.
        self.get_asset_manager().set_envref(envref);
    }
}

/// Simple environment with payload and configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
#[cfg(not(target_arch = "wasm32"))]
pub struct SimpleEnvironmentWithPayload<V: ValueInterface, P: crate::commands::PayloadType> {
    store: Arc<dyn Store>,
    #[cfg(feature = "async_store")]
    async_store: Arc<dyn crate::store::AsyncStore>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<DefaultAssetManager<Self>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,
    _payload: std::marker::PhantomData<P>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface, P: crate::commands::PayloadType> Default
    for SimpleEnvironmentWithPayload<V, P>
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface, P: crate::commands::PayloadType> SimpleEnvironmentWithPayload<V, P> {
    pub fn new() -> Self {
        SimpleEnvironmentWithPayload {
            store: Arc::new(NoStore),
            command_registry: CommandRegistry::new(),
            //            cache: Arc::new(tokio::sync::RwLock::new(Box::new(NoCache::<V>::new()))),
            _payload: std::marker::PhantomData::<P>::default(),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(crate::store::NoAsyncStore),
            asset_store: Arc::new(crate::assets::DefaultAssetManager::new()),
            recipe_provider: None,
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::from(store);
        self
    }
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self {
        panic!("SimpleEnvironment does not support cache for now");
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<V: ValueInterface, P: crate::commands::PayloadType> Environment
    for SimpleEnvironmentWithPayload<V, P>
{
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = P;
    type AssetManager = DefaultAssetManager<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<DefaultAssetManager<Self>> {
        self.asset_store.clone()
    }
    fn create_session(&self, user: User) -> Self::SessionType {
        SimpleSession { user }
    }

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>> {
        use crate::interpreter::{apply_plan, finalize_plan};

        async move {
            let recipe_expires = recipe.expires.clone();
            let mut plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };

            finalize_plan(envref.clone(), &mut plan, &context).await?;
            let combined_expires = plan.expires.clone() | recipe_expires;
            context.set_expires(combined_expires).await?;

            let res = apply_plan(plan, input_state, context, envref).await?;

            Ok(res)
        }
        .maybe_boxed()
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        panic!("No recipe provider configured in SimpleEnvironmentWithPayload");
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref.clone());
        let am = self.get_asset_manager();
        tokio::spawn(async move {
            am.start().await;
        });
    }
}
