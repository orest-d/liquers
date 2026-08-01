//! Runtime environments, shared environment references, and command contexts.
//!
//! # Roles
//!
//! [`Environment`] is a collection of global shared services and configurations.
//! It binds the concrete value, command executor, payload, asset
//! manager, recipe provider, store, and session types used by one Liquers runtime.
//! It is usually configured while owned and then consumed by
//! [`Environment::to_ref`].
//!
//! [`EnvRef`] is the cloneable, shared application handle to that environment.
//! Application code evaluates queries through [`EnvRef::evaluate`] or
//! [`EnvRef::evaluate_immediately`] and obtains an
//! [`AssetRef`].
//!
//! [`Context`] is the command-facing context for one asset evaluation.
//! Note that each action (application of a command) defines a separate asset,
//! thus context is not necessarily shared across actions in the same evaluation.
//! The interpreter clones it across the actions in that evaluation. Its clones share
//! the current asset, working-directory cell, service channel, and pending
//! dependency records. The payload value and volatility flag are cloned by value.
//!
//! Payload is a mechanism for injecting custom data into the evaluation context.
//! It may be e.g. graphics context, user preferences, or other variable application state.
//!
//! [`Session`] and [`User`] are currently minimal identity abstractions.
//! `Environment::create_session` constructs a session, but `Context` does not
//! currently contain or expose a session or user.
//! This is by design: Assets are shared for all the users. Asset evaluation should not depend on the user.
//! Only the access rights will (in the future) depend on the user.
//!
//! # Initialization
//!
//! Prefer [`Environment::to_ref`] over [`EnvRef::new`]. `to_ref` consumes the
//! configured environment, creates the shared reference, and invokes
//! [`Environment::init_with_envref`] so the asset manager receives its environment
//! back-reference. `EnvRef::new` only wraps the value in an `Arc`; evaluation can
//! panic if manager initialization is skipped.
//!
//! Native [`SimpleEnvironment`] and [`SimpleEnvironmentWithPayload`] use
//! `DefaultAssetManager`, whose construction and initialization spawn Tokio tasks
//! and therefore require an active Tokio runtime. [`ImmediateEnvironment`] uses
//! `ImmediateAssetManager`, does not spawn, and starts lazily on first evaluation.
//!
//! # Evaluation flow
//!
//! ```text
//! Environment::to_ref
//!     -> EnvRef::evaluate or evaluate_immediately
//!     -> AssetManager
//!     -> AssetRef creates Context
//!     -> Environment::apply_recipe
//!     -> plan/interpreter/CommandExecutor
//!     -> AssetRef containing the resulting State
//! ```
//!
//! `evaluate` uses the environment's normal manager mode: queued managers may
//! return before evaluation finishes, while inline managers finish before return.
//! `evaluate_immediately` always evaluates before returning, supplies a payload,
//! and uses an empty input state. Assets with payload or arguments are not uniquely identifiable by a query,
//! so they are not cached by the asset manager. Hence they can't be reused and requested again.
//! This is the reason why assets with payload or arguments must be evaluated immediately.
//!
//!
//! # Context, dependencies, and payload
//!
//! Context is an entry point for commands to interact with the environment services and asset mechanism.
//! Commands use `Context` to report logs and progress, inspect metadata, change
//! the working key or filename, and evaluate dependencies. [`Context::evaluate`]
//! records the nested query as a dependency and returns its asset handle;
//! [`Context::get_dependency_state`] schedules and waits for its state.
//! [`Context::apply`] is an ad-hoc application and does not record a dependency.
//! For apply operation, it may be difficult to safely define dependencies, nevertheless, use of apply bypasses
//! the dependency tracking mechanism and should be used with caution.
//!
//! A payload is optional on `Context` even though its type is fixed by
//! [`Environment::Payload`]. [`EnvRef::evaluate_immediately`] installs one.
//! It remains available to actions in that evaluation, including injected command
//! parameters. Nested asset evaluation through [`Context::evaluate`] or
//! [`Context::get_dependency_state`] does **not** currently inherit it.

use core::panic;
use std::sync::{Arc, Mutex};

use crate::maybe_send::MaybeBoxed;
use futures::FutureExt;

#[cfg(not(target_arch = "wasm32"))]
use crate::assets::DefaultAssetManager;
#[cfg(not(target_arch = "wasm32"))]
use crate::cache::Cache;
#[cfg(not(target_arch = "wasm32"))]
use crate::store::{NoStore, Store};
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

/// Identity associated with a [`Session`].
///
/// Sessions are not attached to [`Context`] or enforced by evaluation.
pub enum User {
    /// Internal or automated system identity.
    System,
    /// Unauthenticated identity.
    Anonymous,
    /// Named identity without an authorization model.
    Named(String),
}

/// Minimal user-session abstraction.
///
/// The environment can create sessions, but the current evaluation and context
/// APIs do not carry them.
pub trait Session {
    /// Returns the user represented by this session.
    fn get_user(&self) -> &User;
}

/// Defines the concrete runtime services and types used during evaluation.
///
/// This trait is a static integration boundary, not a trait-object interface.
/// Implementations must keep the returned service objects associated with the same
/// environment instance. See [`Self::apply_recipe`] and
/// [`Self::init_with_envref`] for the two lifecycle hooks.
pub trait Environment:
    Sized + crate::maybe_send::MaybeSync + crate::maybe_send::MaybeSend + 'static
{
    /// Value representation carried by [`State`].
    type Value: ValueInterface;
    /// Executor used by interpreter action steps.
    type CommandExecutor: CommandExecutor<Self>;
    /// Session representation created by [`Self::create_session`].
    type SessionType: Session;
    /// Per-evaluation payload type.
    ///
    /// A context stores this as `Option<Payload>`; selecting a type does not make a
    /// payload present on every evaluation.
    type Payload: crate::commands::PayloadType;
    /// Asset manager implementation and its queued or inline execution model.
    type AssetManager: AssetManager<Self>;

    /// Returns the registry used for planning and command metadata lookup.
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    /// Returns the command executor used by the interpreter.
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    /// Returns the asynchronous persistence store.
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore>;

    /// Returns the shared asset manager.
    fn get_asset_manager(&self) -> Arc<Self::AssetManager>;

    /// Returns the recipe provider used to resolve keyed assets.
    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>>;

    /// Creates a session for a user.
    ///
    /// The resulting session is not currently connected to query evaluation.
    fn create_session(&self, user: User) -> Self::SessionType;

    /// Applies a recipe to an input state inside an asset context.
    ///
    /// This is the environment's interpreter hook. It allows the environment to
    /// customize the planner and/or interpreter's behavior.
    /// Among other things, this would be useful to implement multi-realm evaluation.
    /// Realm-specifics can thus be implemented using the Environment and this hook.
    ///
    /// The built-in implementations
    /// build and finalize a plan, combine recipe and plan expiration, update the
    /// context expiration, and call
    /// [`apply_plan`](crate::interpreter::apply_plan). A custom implementation is
    /// responsible for equivalent metadata, dependency, expiration, and command
    /// execution semantics when those facilities are desired.
    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>>;

    /// Installs the environment back-reference and starts manager services.
    ///
    /// Called once by [`Self::to_ref`]. Implementations normally call
    /// [`AssetManager::set_envref`] and arrange for [`AssetManager::start`].
    fn init_with_envref(&self, envref: EnvRef<Self>);

    /// Consumes, shares, and initializes this environment.
    fn to_ref(self) -> EnvRef<Self> {
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone());
        envref
    }
}

/// Cloneable shared reference to an initialized [`Environment`].
///
/// The inner `Arc` is public for direct access to environment-specific services.
/// Construct application references with [`Environment::to_ref`] so initialization
/// is not skipped.
pub struct EnvRef<E: Environment>(pub Arc<E>);

impl<E: Environment> EnvRef<E> {
    /// Wraps an environment without invoking [`Environment::init_with_envref`].
    ///
    /// Prefer [`Environment::to_ref`] for an environment that will evaluate
    /// queries. This constructor is a low-level building block used by `to_ref`.
    pub fn new(env: E) -> Self {
        EnvRef(Arc::new(env))
    }
    /// Returns the configured asynchronous store.
    #[cfg(feature = "async_store")]
    pub fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.0.get_async_store()
    }
    /// Returns the command metadata registry.
    pub fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        self.0.get_command_metadata_registry()
    }
    /// Returns the command executor.
    pub fn get_command_executor(&self) -> &E::CommandExecutor {
        self.0.get_command_executor()
    }

    /// Returns the shared asset manager.
    pub fn get_asset_manager(&self) -> Arc<E::AssetManager> {
        self.0.get_asset_manager()
    }

    /// Returns the configured recipe provider.
    pub fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<E>> {
        self.0.get_recipe_provider()
    }

    /// Delegates recipe application to [`Environment::apply_recipe`].
    ///
    /// This is framework infrastructure used by assets; ordinary callers normally
    /// use [`Self::evaluate`] or [`Self::evaluate_immediately`].
    pub fn apply_recipe(
        &self,
        input_state: State<E::Value>,
        recipe: Recipe,
        context: Context<E>,
    ) -> crate::maybe_send::BoxFuture<'static, Result<Arc<E::Value>, Error>> {
        Box::pin(E::apply_recipe(self.clone(), input_state, recipe, context))
    }

    /// Resolves a query through the environment's asset manager.
    ///
    /// The returned future waits for parsing and manager submission, not
    /// necessarily for the asset value. With a queued manager, call
    /// [`AssetRef::get`](crate::assets::AssetRef::get) to wait for state. With an
    /// inline manager, evaluation completes before this method returns.
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

    /// Evaluates a query immediately with an empty input state and a payload.
    ///
    /// This delegates to [`AssetManager::apply_immediately`], so it completes the
    /// evaluation before returning and does not persist the produced value.
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

/// Command-facing context for one asset evaluation.
///
/// The interpreter clones a context across plan steps. Clones share asset-bound
/// mutation and dependency state, while the payload itself is cloned. Normal
/// contexts are created by [`AssetRef::create_context`](crate::assets::AssetRef::create_context).
pub struct Context<E: Environment> {
    assetref: AssetRef<E>,
    envref: EnvRef<E>,
    cwd_key: Arc<Mutex<Option<Key>>>, // TODO: CWD should be owned by the context or maybe it should be in the Metadata
    service_tx: tokio::sync::mpsc::UnboundedSender<AssetServiceMessage>,
    /// Optional evaluation payload.
    ///
    /// Prefer [`Self::get_payload_clone`] for read access. Direct replacement
    /// affects only that context clone.
    pub payload: Option<E::Payload>,

    /// If true, this context is evaluating a volatile asset.
    /// Propagates to nested evaluations via context.evaluate()
    is_volatile: bool,

    /// Dependencies discovered during evaluation (via Context::evaluate calls).
    /// Collected here and written to the asset's metadata after evaluation completes.
    pending_dependencies: Arc<tokio::sync::Mutex<Vec<DependencyRecord>>>,
}

impl<E: Environment> Context<E> {
    /// Creates a context bound to an asset.
    ///
    /// The environment, service sender, and volatility flag are derived or supplied
    /// by asset execution. Application code rarely constructs a context directly.
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

    /// Installs a payload on this context clone.
    pub fn set_payload(&mut self, payload: E::Payload) {
        self.payload = Some(payload);
    }

    /// Returns a clone of the optional payload.
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
        let asset = manager.get_dependency_asset(&self.assetref, query).await?;

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

    /// Drains the current asset's scheduled local dependency queue.
    ///
    /// This delegates to [`AssetManager::drain_dependencies`] and is primarily an
    /// interpreter/dependency-scheduler operation.
    pub async fn evaluate_local_queue(&self) -> Result<(), Error> {
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        manager.drain_dependencies(&self.assetref).await
    }

    /// Schedules a dependency, records it, and waits for its state.
    ///
    /// The nested evaluation does not inherit this context's payload.
    pub async fn get_dependency_state(&self, query: &Query) -> Result<State<E::Value>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.wait_for_dependency(&asset).await
    }

    /// Schedules and records a dependency, then returns its asset handle.
    ///
    /// The local dependency queue is drained before return so callers can safely
    /// wait through [`AssetRef::get`](crate::assets::AssetRef::get). The nested
    /// asset receives no copy of this context's payload.
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.evaluate_local_queue().await?;
        Ok(asset)
    }

    /// Applies a query to a supplied state as an ad-hoc asset.
    ///
    /// This delegates to [`AssetManager::apply`]. It does not record the result as
    /// a dependency and does not pass this context's payload.
    pub async fn apply(&self, query: &Query, to: State<E::Value>) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        envref.get_asset_manager().apply(query.into(), to).await
    }

    /// Returns the current asset's structured metadata record.
    ///
    /// Legacy JSON metadata cannot be represented as `MetadataRecord` and returns
    /// an error.
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
    /// Sends a primary-progress update to the current asset.
    pub fn progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdatePrimaryProgress(progress))
            .map_err(|e| Error::general_error(format!("Failed to send progress message: {}", e)))
    }

    /// Returns whether this context is evaluating a volatile asset.
    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }

    /// Clones this context and adds a volatility requirement.
    ///
    /// Volatility is contagious: the returned context is volatile if either the
    /// existing context or the argument is volatile. This does not create a
    /// separate child asset.
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

    /// Sends a secondary-progress update to the current asset.
    pub fn secondary_progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdateSecondaryProgress(progress))
            .map_err(|e| {
                Error::general_error(format!("Failed to send secondary progress message: {}", e))
            })
    }
    /// Sets the filename in the current asset's metadata.
    pub async fn set_filename(&self, filename: &str) -> Result<(), Error> {
        self.assetref
            .data
            .write()
            .await
            .metadata
            .set_filename(filename)
            .map(|_| ())
    }
    /// Sends a structured log entry to the current asset.
    pub fn add_log_entry(&self, entry: LogEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::LogMessage(entry))
            .map_err(|e| Error::general_error(format!("Failed to send log message: {}", e)))
    }
    /// Logs a debug message and writes it to stderr.
    pub fn debug(&self, message: &str) -> Result<(), Error> {
        eprintln!("DEBUG:   {}", message);
        self.add_log_entry(LogEntry::debug(message.to_string()))
    }
    /// Logs an informational message and writes it to stderr.
    pub fn info(&self, message: &str) -> Result<(), Error> {
        eprintln!("INFO:    {}", message);
        self.add_log_entry(LogEntry::info(message.to_string()))
    }
    /// Logs a warning and writes it to stderr.
    pub fn warning(&self, message: &str) -> Result<(), Error> {
        eprintln!("WARNING: {}", message);
        self.add_log_entry(LogEntry::warning(message.to_string()))
    }
    /// Logs an error-level message and writes it to stderr.
    ///
    /// This does not fail the asset; use [`Self::set_error`] for that operation.
    pub fn error(&self, message: &str) -> Result<(), Error> {
        eprintln!("ERROR:   {}", message);
        self.add_log_entry(LogEntry::error(message.to_string()))
    }
    /// Clones the context.
    ///
    /// This is equivalent to [`Clone::clone`] despite being async.
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
    /// Returns the current working key used for relative query resolution.
    pub fn get_cwd_key(&self) -> Option<Key> {
        self.cwd_key.lock().unwrap().clone()
    }

    /// Replaces the current working key shared by all context clones.
    pub fn set_cwd_key(&self, key: Option<Key>) {
        let mut guard = self.cwd_key.lock().unwrap();
        *guard = key;
    }

    /// Returns the current asset handle.
    pub fn get_asset_ref(&self) -> AssetRef<E> {
        self.assetref.clone()
    }

    /// Returns the shared environment reference.
    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    /// Takes and clears the dependencies collected during evaluation.
    ///
    /// This is an interpreter/asset finalization primitive. Calling it early
    /// removes records that would otherwise be written to result metadata.
    pub async fn take_pending_dependencies(&self) -> Vec<DependencyRecord> {
        std::mem::take(&mut *self.pending_dependencies.lock().await)
    }

    /// Adds or updates a dependency awaiting metadata finalization.
    ///
    /// A known version is not replaced by the `Version(0)` unknown sentinel.
    /// This is normally called by dependency scheduling and the interpreter.
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

    /// Applies an expiration policy to the current asset metadata and deadline.
    ///
    /// This is a framework hook used by `Environment::apply_recipe`
    /// implementations after plan finalization.
    pub async fn set_expires(&self, expires: Expires) -> Result<(), Error> {
        let expiration_time = {
            let mut lock = self.assetref.data.write().await;
            lock.metadata.set_expiration_time_from(&expires)?;
            lock.metadata.expiration_time()
        };
        self.assetref.set_expiration_time(expiration_time).await;
        Ok(())
    }

    /// Fails the current asset with an error.
    ///
    /// Unlike [`Self::error`], this changes the asset's terminal state.
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

/// Minimal session containing only a [`User`].
pub struct SimpleSession {
    /// User represented by this session.
    pub user: User,
}
impl Session for SimpleSession {
    fn get_user(&self) -> &User {
        &self.user
    }
}

/// Native environment with unit payload and queued asset evaluation.
///
/// This uses [`CommandRegistry`] for execution and metadata,
/// `DefaultAssetManager` for queued evaluation, and an optional asynchronous store
/// and recipe provider. Construction requires an active Tokio runtime because the
/// manager spawns its queue and expiration-monitor tasks.
///
/// If no recipe provider is configured, this environment returns
/// [`TrivialRecipeProvider`](crate::recipes::TrivialRecipeProvider).
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
    /// Creates a native queued environment with no asynchronous persistence.
    ///
    /// The environment still needs [`Environment::to_ref`] before evaluation.
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
    /// Sets the legacy synchronous store field.
    ///
    /// Current asset evaluation uses [`Self::with_async_store`]; this synchronous
    /// store is not exposed through [`Environment`] and is not used by the asset
    /// manager.
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::from(store);
        self
    }
    /// Sets the keyed recipe provider.
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
    /// Sets the asynchronous store used by assets.
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    /// Unsupported legacy cache setter.
    ///
    /// This method always panics.
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

/// Spawn-free environment with unit payload and inline asset evaluation.
///
/// This uses [`crate::assets::ImmediateAssetManager`], has no job queue or
/// expiration-monitor task, and starts lazily on first evaluation. It can be
/// constructed without a Tokio runtime and is suitable for Wasm and deterministic
/// inline execution. It supports only the asynchronous store API.
///
/// If no recipe provider is configured, this environment returns
/// [`TrivialRecipeProvider`](crate::recipes::TrivialRecipeProvider).
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
    /// Creates an inline environment with no asynchronous persistence.
    pub fn new() -> Self {
        ImmediateEnvironment {
            command_registry: CommandRegistry::new(),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(crate::store::NoAsyncStore),
            asset_store: Arc::new(crate::assets::ImmediateAssetManager::new()),
            recipe_provider: None,
        }
    }
    /// Sets the asynchronous store used by assets.
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    /// Sets the keyed recipe provider.
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

/// Spawn-free environment with a custom payload and inline asset evaluation.
///
/// This is the payload-bearing counterpart to [`ImmediateEnvironment`], and the inline
/// counterpart to [`SimpleEnvironmentWithPayload`]. It uses
/// [`crate::assets::ImmediateAssetManager`], has no job queue or expiration-monitor task,
/// and starts lazily on first evaluation, so it can be constructed without a Tokio runtime.
///
/// This is the only environment pairing a payload with the inline asset manager, and it is
/// therefore what makes the Wasm-compatible payload path exercisable natively.
///
/// If no recipe provider is configured, this environment returns
/// [`TrivialRecipeProvider`](crate::recipes::TrivialRecipeProvider).
pub struct ImmediateEnvironmentWithPayload<V: ValueInterface, P: crate::commands::PayloadType> {
    #[cfg(feature = "async_store")]
    async_store: Arc<dyn crate::store::AsyncStore>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<crate::assets::ImmediateAssetManager<Self>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,
    _payload: std::marker::PhantomData<P>,
}

impl<V: ValueInterface, P: crate::commands::PayloadType> Default
    for ImmediateEnvironmentWithPayload<V, P>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface, P: crate::commands::PayloadType> ImmediateEnvironmentWithPayload<V, P> {
    /// Creates an inline payload-bearing environment with no asynchronous persistence.
    pub fn new() -> Self {
        ImmediateEnvironmentWithPayload {
            command_registry: CommandRegistry::new(),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(crate::store::NoAsyncStore),
            asset_store: Arc::new(crate::assets::ImmediateAssetManager::new()),
            recipe_provider: None,
            _payload: std::marker::PhantomData::<P>,
        }
    }
    /// Sets the asynchronous store used by assets.
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    /// Sets the keyed recipe provider.
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
}

impl<V: ValueInterface, P: crate::commands::PayloadType> Environment
    for ImmediateEnvironmentWithPayload<V, P>
{
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = P;
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

/// Native environment with a custom payload and queued asset evaluation.
///
/// This is the payload-bearing counterpart to [`SimpleEnvironment`]. Construction
/// requires an active Tokio runtime. A recipe provider must be configured before a
/// keyed recipe lookup; otherwise [`Environment::get_recipe_provider`] panics.
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
    /// Creates a native queued environment with no asynchronous persistence.
    ///
    /// The environment still needs [`Environment::to_ref`] before evaluation.
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
    /// Sets the legacy synchronous store field.
    ///
    /// Current asset evaluation uses [`Self::with_async_store`]; this synchronous
    /// store is not exposed through [`Environment`] and is not used by the asset
    /// manager.
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::from(store);
        self
    }
    /// Sets the keyed recipe provider.
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(Arc::from(provider));
        self
    }
    /// Sets the asynchronous store used by assets.
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }
    /// Unsupported legacy cache setter.
    ///
    /// This method always panics.
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
