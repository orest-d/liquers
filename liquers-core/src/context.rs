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
//! configured environment, refreshes command metadata versions, creates the shared
//! reference, and invokes [`Environment::init_with_envref`] so the asset manager
//! receives its environment back-reference. `EnvRef::new` only wraps the value in an
//! `Arc`; evaluation can panic if manager initialization is skipped, and command
//! metadata versions are not refreshed.
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
//! parameters.
//!
//! Nested asset evaluation **inherits** it, but only where it is needed and only where it
//! can be honoured. A command declares `payload: required` in `register_command!`, which
//! propagates to `Plan::payload_required`; when a nested query's plan requires a payload,
//! [`Context::evaluate`], [`Context::get_dependency_state`] and [`Context::apply`] forward
//! this context's payload and the nested asset is evaluated inline. Requiring a payload
//! implies volatility, so such an asset is fresh, unshared and never persisted.
//!
//! Two boundaries limit this. **Keys are a payload boundary**: keys are global while a
//! payload is per-evaluation, so a keyed recipe may not require one, and a requirement never
//! propagates through a keyed step. And a payload-evaluated asset may *have* dependencies but
//! may never *be* one, because a payload is not part of the dependency key — so it is not
//! registered in the dependency graph, and cycles among such assets are detected along the
//! evaluation path instead.
//!
//! Evaluating a payload-requiring plan without a payload is an error.

use core::panic;
use std::sync::{Arc, Mutex};

use crate::maybe_send::MaybeBoxed;

use crate::{
    assets::{AssetManager, AssetRef, AssetServiceMessage},
    command_metadata::CommandMetadataRegistry,
    commands::{CommandExecutor, CommandRegistry},
    dependencies::ScheduleNode,
    error::Error,
    expiration::Expires,
    metadata::{DependencyKey, DependencyRecord, LogEntry, MetadataRecord, ProgressEntry, Version},
    query::{CwdCursor, Key, Query, TryToQuery, RELATIVE_WITHOUT_CWD_WARNING},
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

    /// Returns the mutable registry used while the environment is still owned.
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;

    /// Returns the registry of value types this build knows.
    ///
    /// Mirrors [`Environment::get_command_metadata_registry`]: built once at construction from
    /// [`ValueInterface::type_descriptions`], read-only thereafter. The deserialization path needs
    /// it because it has bytes and a type identifier but no value yet; every other check has a
    /// value in hand and uses the instance methods on [`ValueInterface`].
    fn get_type_registry(&self) -> &crate::type_system::TypeRegistry;
    /// Returns the command executor used by the interpreter.
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    /// Returns the asynchronous persistence store.
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

    /// Constructs, installs and **starts** this environment's asset manager.
    ///
    /// Called once by [`Self::try_to_ref`], with an [`EnvRef`] that nothing else can observe yet.
    /// On return the manager must be fully usable: constructed with this reference, installed in
    /// the environment, and started. That obligation is the entire readiness guarantee, and it is
    /// the one thing a hand-written [`Environment`] must get right.
    ///
    /// The expected shape is the deferred-slot pattern the built-in environments use: hold the
    /// manager in a `OnceLock`, construct it here with the reference in hand, install it, start it.
    ///
    /// ```ignore
    /// fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
    ///     let manager = Arc::new(ImmediateAssetManager::new(envref));
    ///     let _ = self.asset_store.set(manager.clone());
    ///     manager.start()
    /// }
    /// ```
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error>;

    /// Consumes, shares and initializes this environment, reporting a startup failure.
    ///
    /// Refreshes command metadata versions — `register_command!` mutates metadata after the
    /// registry first computes `metadata_version`, so the versions are stale until refreshed, and
    /// startup snapshots them into the dependency manager — then creates the [`EnvRef`] and hands
    /// it to [`Self::init_with_envref`] before returning it. No reference escapes this function
    /// before the manager is started, so the value it returns is ready to evaluate.
    ///
    /// This is the single readiness sequence: [`Self::to_ref`] and
    /// `EnvironmentBuilder::build` both run it rather than reimplementing it.
    fn try_to_ref(mut self) -> Result<EnvRef<Self>, Error> {
        self.get_mut_command_metadata_registry()
            .refresh_metadata_versions();
        #[allow(deprecated)]
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone())?;
        Ok(envref)
    }

    /// Consumes, shares, and initializes this environment.
    ///
    /// The recommended construction path is `EnvironmentBuilder::build`, which configures an
    /// environment and reports errors; this remains supported for an ad-hoc or hand-written
    /// [`Environment`], where replicating the builder is not worth it.
    ///
    /// # Panics
    ///
    /// If manager startup fails. Neither built-in manager can fail — startup writes an in-memory
    /// map — but a custom [`Self::init_with_envref`] can. Use [`Self::try_to_ref`] where that
    /// matters.
    fn to_ref(self) -> EnvRef<Self> {
        match self.try_to_ref() {
            Ok(envref) => envref,
            Err(e) => panic!("environment initialization failed: {e}"),
        }
    }
}

/// Reads an environment's installed asset manager.
///
/// The slot is written by [`Environment::init_with_envref`], which runs inside
/// [`Environment::to_ref`] before any [`EnvRef`] is observable, so by the time any caller holds an
/// environment to ask this of, the manager is installed. The unset branch is therefore
/// unreachable rather than merely unlikely, and it panics rather than fabricating a detached
/// manager: a manager with no environment behind it is exactly the state
/// `QUEUED-MANAGER-STARTUP-READINESS` is about, and silently producing one would hide the defect
/// this function's caller is relying on being absent.
fn installed_manager<M>(slot: &std::sync::OnceLock<Arc<M>>) -> Arc<M> {
    match slot.get() {
        Some(manager) => manager.clone(),
        None => panic!(
            "asset manager read before Environment::init_with_envref installed it; \
             construct environments with Environment::to_ref or EnvironmentBuilder::build"
        ),
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
    /// # Deprecated
    ///
    /// This produces a reference whose asset manager was never constructed or started — the state
    /// `QUEUED-MANAGER-STARTUP-READINESS` is about. Every evaluation path assumes an installed,
    /// started manager, so a reference from here is not safe to evaluate through. Use
    /// [`Environment::to_ref`], [`Environment::try_to_ref`] or `EnvironmentBuilder::build`, each
    /// of which runs the readiness sequence.
    ///
    /// Prefer [`Environment::to_ref`] for an environment that will evaluate
    /// queries. This constructor is a low-level building block used by `to_ref`.
    #[deprecated(
        note = "produces an EnvRef with no asset manager installed or started; use \
                Environment::to_ref, Environment::try_to_ref, or EnvironmentBuilder::build"
    )]
    pub fn new(env: E) -> Self {
        EnvRef(Arc::new(env))
    }
    /// Returns the configured asynchronous store.
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
    /// Returns the registry of value types this build knows.
    pub fn get_type_registry(&self) -> &crate::type_system::TypeRegistry {
        self.0.get_type_registry()
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
                .apply(query.into(), State::new(), Some(payload))
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

    /// Queries currently being evaluated with an inherited payload, along this evaluation path.
    ///
    /// Payload-evaluated assets are not registered in the dependency graph — a payload is not
    /// part of the dependency key, so nothing may hold an edge to one. That removes the site
    /// where `register_scheduled_dependency` would detect a cycle, and neither end of a
    /// payload-to-payload chain is a graph node in the first place. This set restores
    /// detection along the evaluation path, in the same way `find_dependencies` uses a visited
    /// stack while walking recipe chains.
    ///
    /// Shared across context clones so it tracks the path rather than one action.
    active_payload_queries: Arc<tokio::sync::Mutex<Vec<Query>>>,
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
            active_payload_queries: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Seeds the payload-evaluation path inherited from the asset this context belongs to.
    ///
    /// Called by `AssetRef::create_context`. Each nested asset builds a fresh context, so the
    /// path cannot live on `Context` alone — it is carried on the asset and re-seeded here.
    pub(crate) async fn seed_payload_path(&self, path: Vec<Query>) {
        if !path.is_empty() {
            *self.active_payload_queries.lock().await = path;
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

    /// Returns whether a payload is present, without cloning it.
    pub fn has_payload(&self) -> bool {
        self.payload.is_some()
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
    ///
    /// If the dependency's plan requires an evaluation payload, this instead takes the
    /// payload path: the parent's payload is forwarded and the asset is evaluated inline.
    /// Such an asset is **not** registered in the dependency graph, because a payload is not
    /// part of the dependency key and two evaluations with different payloads would
    /// otherwise share one identity. It still records its own dependencies, and cycles are
    /// detected along the evaluation path via `active_payload_queries`.
    /// Rejects a query that cannot name an asset on its own.
    ///
    /// A plan is frozen before it executes, so every operand a command receives *through the plan*
    /// is already absolute. A query a command **builds** is not. A relative one would mean
    /// different things in different directories while looking identical, so it could not be
    /// identified, cached or shared — and nothing marks which commands read the directory, so the
    /// alternative is to carry a CWD in every query, multiplying cache entries per folder for the
    /// majority that need none.
    ///
    /// The supported way to reach the current directory is a `-R-key/.` link argument: explicit in
    /// the query, overridable per call, and visible to the planner.
    fn reject_relative_query(query: &Query) -> Result<(), Error> {
        if !query.has_relative_operand() {
            return Ok(());
        }
        let error = Error::not_supported(format!(
            "Query '{}' is relative and cannot be evaluated from a command. Take the current \
             directory as a link argument (`-R-key/.`) and build an absolute query from it.",
            query.encode()
        ))
        .with_query(query);
        Err(match query.first_relative_operand_position() {
            Some(position) => error.with_position(&position),
            None => error,
        })
    }

    pub(crate) async fn schedule_dependency_asset(
        &self,
        query: &Query,
    ) -> Result<AssetRef<E>, Error> {
        Self::reject_relative_query(query)?;
        let query = self.resolve_query_from_cwd(query)?;
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        let query_dep_key = DependencyKey::from(&query);

        // Does this dependency need a payload to run? Known from the plan, so the path is
        // chosen without speculatively evaluating anything.
        let requirement = {
            use crate::interpreter::RequiresPayload;
            query.requires_payload(envref.clone()).await?
        };

        if requirement.is_required() {
            return self.schedule_payload_dependency_asset(&query).await;
        }

        // Only an asset that still owns its immutable construction-time key may act as a
        // keyed dependent. Provider resolution can replace the mutable recipe, so deriving
        // this identity from `AssetData::recipe` would register edges under the wrong key.
        let owner_key = self.owner_key().await?;

        let version = manager
            .dependency_manager()
            .get_version(&query_dep_key)
            .await
            .unwrap_or_else(Version::unknown);

        // Classify the dependent: keyed asset -> graph node; non-keyed query -> expression;
        // ad-hoc (no key, no query) -> skip registration (not a graph participant).
        let dependent_opt = if let Some(ref k) = owner_key {
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
        let asset = manager.get_dependency_asset(&self.assetref, &query).await?;

        // Record the runtime dependency (path-independent capture) as evaluate did.
        if owner_key.is_some() {
            manager
                .dependency_manager()
                .add_dependent_asset(&query_dep_key, self.assetref.downgrade())
                .await;
        }
        self.add_dependency(DependencyRecord::new(query_dep_key, version))
            .await;

        Ok(asset)
    }

    /// Schedule a dependency whose plan requires an evaluation payload.
    ///
    /// Differs from the normal path in exactly three ways, all following from the fact that
    /// a payload is not part of the dependency key:
    ///
    /// 1. No graph edge is registered, and the parent is not recorded as a dependent asset
    ///    of this query — nothing may hold a reference *to* a payload-evaluated asset.
    /// 2. Cycles are detected along the evaluation path instead of through the graph.
    /// 3. The payload is forwarded and the asset is evaluated inline.
    ///
    /// The asset's own dependency record is still written to the parent's metadata: a payload
    /// asset may *have* dependencies, it just may not *be* one.
    async fn schedule_payload_dependency_asset(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        let manager = envref.get_asset_manager();
        let query_dep_key = DependencyKey::from(query);

        let payload = self.payload.clone();
        if payload.is_none() {
            return Err(Error::general_error(format!(
                "Query '{}' requires an evaluation payload, but the evaluation was started \
                 without one. Use EnvRef::evaluate_immediately to supply a payload, or remove \
                 the 'payload: required' declaration from the commands involved.",
                query.encode()
            ))
            .with_query(query));
        }

        // Path-based cycle detection. The dependency graph cannot see these cycles: neither
        // end of a payload-to-payload chain is a graph node. The extended path travels with
        // the child asset, since the child builds its own context.
        let child_path = {
            let active = self.active_payload_queries.lock().await;
            if active.iter().any(|q| q == query) {
                return Err(Error::dependency_cycle(&query_dep_key));
            }
            let mut path = active.clone();
            path.push(query.clone());
            path
        };

        let version = manager
            .dependency_manager()
            .get_version(&query_dep_key)
            .await
            .unwrap_or_else(Version::unknown);

        let asset = manager
            .get_dependency_asset_with_payload(&self.assetref, query, payload, child_path)
            .await?;

        // A payload asset may have dependencies even though it may not be one.
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
    /// If the nested query requires a payload, this context's payload is inherited; see
    /// [`Self::schedule_dependency_asset`].
    pub async fn get_dependency_state(&self, query: &Query) -> Result<State<E::Value>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.wait_for_dependency(&asset).await
    }

    /// Schedules and records a dependency, then returns its asset handle.
    ///
    /// The local dependency queue is drained before return so callers can safely
    /// wait through [`AssetRef::get`](crate::assets::AssetRef::get). If the nested query
    /// requires a payload, this context's payload is inherited and the nested asset is
    /// evaluated inline instead of being queued.
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let asset = self.schedule_dependency_asset(query).await?;
        self.evaluate_local_queue().await?;
        Ok(asset)
    }

    /// Applies a query to a supplied state as an ad-hoc asset.
    ///
    /// The result is not recorded as a dependency, and cannot be: an ad-hoc asset is not
    /// reproducible from its identity — its value depends on the supplied state — so nothing may
    /// hold a reference *to* it. This is the same rule that excludes payload-evaluated assets,
    /// not a special case for `apply`.
    ///
    /// This context's payload is inherited unconditionally. Whether a payload is *required* is
    /// settled by the authoritative gate in [`apply_plan`](crate::interpreter::apply_plan), which
    /// every execution path passes through; the pre-check that used to live here duplicated that
    /// gate and its error message.
    pub async fn apply(&self, query: &Query, to: State<E::Value>) -> Result<AssetRef<E>, Error> {
        Self::reject_relative_query(query)?;
        let query = self.resolve_query_from_cwd(query)?;
        let envref = self.assetref.get_envref().await;
        envref
            .get_asset_manager()
            .apply((&query).into(), to, self.payload.clone())
            .await
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
            active_payload_queries: self.active_payload_queries.clone(),
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
            active_payload_queries: self.active_payload_queries.clone(),
        }
    }
    /// Returns the current working key used for relative query resolution.
    pub(crate) fn get_cwd_key(&self) -> Option<Key> {
        self.cwd_key
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// Replaces the current working key shared by all context clones.
    pub(crate) fn set_cwd_key(&self, key: Option<Key>) {
        let mut guard = self
            .cwd_key
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *guard = key;
    }

    /// Resolves a key against the live Context CWD and installs logical root on fallback.
    pub(crate) fn resolve_key_from_cwd(&self, key: &Key) -> Result<Key, Error> {
        let (resolved, installed_root) = {
            let mut guard = self
                .cwd_key
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut cursor = CwdCursor::new(guard.clone());
            let resolved = cursor.resolve_key(key);
            let installed_root = cursor.take_root_fallback() && guard.is_none();
            if installed_root {
                *guard = Some(Key::new());
            }
            (resolved, installed_root)
        };

        if installed_root {
            self.warning(RELATIVE_WITHOUT_CWD_WARNING)?;
        }
        Ok(resolved)
    }

    /// Resolves a query copy against the live Context CWD without rewriting the stored plan.
    pub(crate) fn resolve_query_from_cwd(&self, query: &Query) -> Result<Query, Error> {
        let (resolved, installed_root) = {
            let mut guard = self
                .cwd_key
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut cursor = CwdCursor::new(guard.clone());
            let resolved = cursor.resolve_query_scoped(query);
            let installed_root = cursor.take_root_fallback() && guard.is_none();
            if installed_root {
                *guard = Some(Key::new());
            }
            (resolved, installed_root)
        };

        if installed_root {
            self.warning(RELATIVE_WITHOUT_CWD_WARNING)?;
        }
        Ok(resolved)
    }

    /// Resolves and then installs a new CWD for subsequent plan steps.
    pub(crate) fn set_cwd_from_key(&self, key: &Key) -> Result<(), Error> {
        let resolved = self.resolve_key_from_cwd(key)?;
        let mut guard = self
            .cwd_key
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *guard = Some(resolved);
        Ok(())
    }

    /// Installs logical root when a pre-pass observed fallback before runtime execution.
    pub(crate) fn install_logical_root_if_unset(&self) -> bool {
        let mut guard = self
            .cwd_key
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if guard.is_some() {
            false
        } else {
            *guard = Some(Key::new());
            true
        }
    }

    /// Returns the immutable keyed identity owned by this Context's asset, if any.
    pub(crate) async fn owner_key(&self) -> Result<Option<Key>, Error> {
        self.assetref.bound_owner_key().await
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

    /// Records that this evaluation's plan required an evaluation payload.
    ///
    /// Called by [`apply_plan`](crate::interpreter::apply_plan), beside the gate that already
    /// reads `Plan::payload_required`, so every execution path records it once rather than each
    /// entry point re-deriving the requirement. The fact reaches
    /// [`MetadataRecord::payload_required`](crate::metadata::MetadataRecord::payload_required)
    /// and from there `AssetInfo`.
    ///
    /// Note this records the plan's *requirement*, not whether a payload happened to be supplied:
    /// a plan that needs no payload stays `None` even when one was in scope.
    pub async fn set_payload_required(&self) -> Result<(), Error> {
        let mut lock = self.assetref.data.write().await;
        lock.metadata.set_payload_required()?;
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
            active_payload_queries: self.active_payload_queries.clone(),
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

/// The environment every built-in name resolves to.
///
/// One type parameterized by value type, payload type and an **asset-manager kind**, replacing the
/// four near-duplicate structs that preceded it. [`SimpleEnvironment`],
/// [`SimpleEnvironmentWithPayload`], [`ImmediateEnvironment`] and
/// [`ImmediateEnvironmentWithPayload`] are aliases of it, so every existing signature still names
/// a real type and nothing had to move.
///
/// Construct one with [`crate::environment_builder::EnvironmentBuilder`], which is the recommended
/// path; [`Environment::to_ref`] remains available for an environment assembled by hand.
///
/// # The manager slot
///
/// `asset_store` is written exactly once, by [`Environment::init_with_envref`], before any
/// [`EnvRef`] to this environment is observable. The manager cannot be built earlier because it
/// needs that reference — this is the construction cycle, and the slot is where it is broken.
pub struct GenericEnvironment<
    V: ValueInterface,
    P: crate::commands::PayloadType = (),
    K: crate::environment_builder::AssetManagerKind = crate::environment_builder::DefaultKind,
> {
    type_registry: crate::type_system::TypeRegistry,
    async_store: Arc<dyn crate::store::AsyncStore>,
    /// Commands, and the metadata registry planning reads.
    pub command_registry: CommandRegistry<Self>,
    asset_store: std::sync::OnceLock<Arc<K::Manager<Self>>>,
    /// Never `Option`: a default is resolved at construction, so there is no unconfigured state to
    /// report or to panic on.
    recipe_provider: Arc<dyn AsyncRecipeProvider<Self>>,
    manager_options: crate::environment_builder::AssetManagerOptions,
    _payload: std::marker::PhantomData<P>,
}

impl<
        V: ValueInterface,
        P: crate::commands::PayloadType,
        K: crate::environment_builder::AssetManagerKind,
    > GenericEnvironment<V, P, K>
{
    /// Creates an environment with a trivial recipe provider and no store.
    ///
    /// Prefer [`crate::environment_builder::EnvironmentBuilder`]; this exists for an ad-hoc
    /// environment and for the many call sites that predate the builder.
    pub fn new() -> Self {
        Self::new_with_type_registry(crate::type_system::TypeRegistry::from_value_type::<V>())
    }

    /// Creates an environment with a caller-supplied type registry.
    ///
    /// For an integration that adds a type `V` cannot describe statically — a foreign language
    /// handle whose identifier belongs to the integration crate rather than to the value type.
    /// **Extend** [`TypeRegistry::from_value_type`](crate::type_system::TypeRegistry::from_value_type):
    /// starting from `TypeRegistry::new()` loses every type the build already had, including the
    /// `error` pseudo-type that even a failed asset needs.
    ///
    /// The registry is never written after this point, which is what lets
    /// [`Environment::get_type_registry`] hand out a shared reference with no lock.
    pub fn new_with_type_registry(type_registry: crate::type_system::TypeRegistry) -> Self {
        use crate::environment_builder::AssetManagerKind as _;
        Self::assemble(
            type_registry,
            Arc::new(crate::store::NoAsyncStore),
            CommandRegistry::new(),
            // The kind carries the default, because it is the only thing distinguishing one
            // built-in environment from another now that they share this type — and the core and
            // library environments have always disagreed about it.
            K::default_recipe_provider(),
            crate::environment_builder::AssetManagerOptions::default(),
        )
    }

    /// Assembles an environment from already-resolved services.
    pub(crate) fn assemble(
        type_registry: crate::type_system::TypeRegistry,
        async_store: Arc<dyn crate::store::AsyncStore>,
        command_registry: CommandRegistry<Self>,
        recipe_provider: Arc<dyn AsyncRecipeProvider<Self>>,
        manager_options: crate::environment_builder::AssetManagerOptions,
    ) -> Self {
        GenericEnvironment {
            type_registry,
            async_store,
            command_registry,
            asset_store: std::sync::OnceLock::new(),
            recipe_provider,
            manager_options,
            _payload: std::marker::PhantomData,
        }
    }

    /// Sets the asynchronous store used by assets.
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }

    /// Sets the keyed recipe provider.
    pub fn with_recipe_provider(
        &mut self,
        provider: Box<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Arc::from(provider);
        self
    }

    /// Selects one of the built-in recipe providers by name.
    ///
    /// The same vocabulary [`crate::environment_builder::EnvironmentBuilder`] and a configuration
    /// document use, so the choice is spelled one way everywhere.
    pub fn with_recipe_provider_choice(
        &mut self,
        choice: crate::recipes::RecipeProviderChoice,
    ) -> &mut Self {
        self.recipe_provider = choice.provider();
        self
    }

    /// Reads recipes through the environment's store
    /// ([`RecipeProviderChoice::Default`](crate::recipes::RecipeProviderChoice::Default)).
    pub fn with_default_recipe_provider(&mut self) -> &mut Self {
        self.with_recipe_provider_choice(crate::recipes::RecipeProviderChoice::Default)
    }

    /// Resolves no recipes at all
    /// ([`RecipeProviderChoice::Trivial`](crate::recipes::RecipeProviderChoice::Trivial)).
    pub fn with_trivial_recipe_provider(&mut self) -> &mut Self {
        self.with_recipe_provider_choice(crate::recipes::RecipeProviderChoice::Trivial)
    }
}

impl<
        V: ValueInterface,
        P: crate::commands::PayloadType,
        K: crate::environment_builder::AssetManagerKind,
    > Default for GenericEnvironment<V, P, K>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<
        V: ValueInterface,
        P: crate::commands::PayloadType,
        K: crate::environment_builder::AssetManagerKind,
    > Environment for GenericEnvironment<V, P, K>
{
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = P;
    type AssetManager = K::Manager<Self>;

    fn get_type_registry(&self) -> &crate::type_system::TypeRegistry {
        &self.type_registry
    }

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
        &mut self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    fn get_async_store(&self) -> Arc<dyn crate::store::AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<Self::AssetManager> {
        installed_manager(&self.asset_store)
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        self.recipe_provider.clone()
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

            finalize_plan(envref.clone(), &mut plan, &context, &input_state).await?;
            let combined_expires = plan.expires.clone() | recipe_expires;
            context.set_expires(combined_expires).await?;

            apply_plan(plan, input_state, context, envref).await
        }
        .maybe_boxed()
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
        let manager = K::build(envref, &self.manager_options)?;
        let _ = self.asset_store.set(manager.clone());
        manager.start()
    }
}

/// Native environment with unit payload and queued asset evaluation.
///
/// Construction requires an active Tokio runtime, because the queued manager spawns its job queue
/// and expiration monitor. An alias of [`GenericEnvironment`] since the environment-builder work.
#[cfg(not(target_arch = "wasm32"))]
pub type SimpleEnvironment<V> = GenericEnvironment<V, (), crate::environment_builder::Queued>;

/// Native environment with a custom payload and queued asset evaluation.
#[cfg(not(target_arch = "wasm32"))]
pub type SimpleEnvironmentWithPayload<V, P> =
    GenericEnvironment<V, P, crate::environment_builder::Queued>;

/// Spawn-free environment with unit payload and inline asset evaluation.
///
/// No job queue and no expiration-monitor task, so it can be constructed without a Tokio runtime
/// and runs in a browser. Also useful natively for deterministic inline evaluation.
pub type ImmediateEnvironment<V> = GenericEnvironment<V, (), crate::environment_builder::Inline>;

/// Spawn-free environment with a custom payload and inline asset evaluation.
///
/// The pairing that makes the wasm-compatible payload path exercisable natively.
pub type ImmediateEnvironmentWithPayload<V, P> =
    GenericEnvironment<V, P, crate::environment_builder::Inline>;


#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetData;
    use crate::command_metadata::{CommandKey, CommandMetadata};
    use crate::metadata::LogEntryKind;
    use crate::parse::{parse_key, parse_query};
    use crate::query::{ActionParameter, QuerySegment};
    use crate::type_system::{TypeInfo, TypeRegistry};
    use crate::value::Value;

    type TestEnvironment = ImmediateEnvironment<Value>;

    fn add_stale_command_version(
        registry: &mut CommandMetadataRegistry,
        name: &str,
    ) -> crate::metadata::Version {
        registry.add_command(&CommandMetadata::new(name));
        let key = CommandKey::new("", "root", name);
        let stale = registry.get(key.clone()).unwrap().metadata_version;
        registry
            .get_mut(key)
            .unwrap()
            .with_doc("changed after the initial metadata version was calculated");
        stale
    }

    fn expected_refreshed_command_version(
        registry: &CommandMetadataRegistry,
        name: &str,
    ) -> crate::metadata::Version {
        let mut refreshed = registry.clone();
        refreshed.refresh_metadata_versions();
        refreshed
            .get(CommandKey::new("", "root", name))
            .unwrap()
            .metadata_version
    }

    #[test]
    fn immediate_environment_to_ref_refreshes_metadata_versions() {
        let mut env = ImmediateEnvironment::<Value>::new();
        let stale =
            add_stale_command_version(&mut env.command_registry.command_metadata_registry, "a");
        let expected = expected_refreshed_command_version(
            &env.command_registry.command_metadata_registry,
            "a",
        );

        let envref = env.to_ref();
        let actual = envref
            .get_command_metadata_registry()
            .get(CommandKey::new("", "root", "a"))
            .unwrap()
            .metadata_version;

        assert_ne!(actual, stale);
        assert_eq!(actual, expected);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn simple_environment_to_ref_refreshes_metadata_versions() {
        let mut env = SimpleEnvironment::<Value>::new();
        let stale =
            add_stale_command_version(&mut env.command_registry.command_metadata_registry, "a");
        let expected = expected_refreshed_command_version(
            &env.command_registry.command_metadata_registry,
            "a",
        );

        let envref = env.to_ref();
        let actual = envref
            .get_command_metadata_registry()
            .get(CommandKey::new("", "root", "a"))
            .unwrap()
            .metadata_version;

        assert_ne!(actual, stale);
        assert_eq!(actual, expected);
    }

    #[test]
    fn immediate_environment_with_payload_to_ref_refreshes_metadata_versions() {
        let mut env = ImmediateEnvironmentWithPayload::<Value, ()>::new();
        let stale =
            add_stale_command_version(&mut env.command_registry.command_metadata_registry, "a");
        let expected = expected_refreshed_command_version(
            &env.command_registry.command_metadata_registry,
            "a",
        );

        let envref = env.to_ref();
        let actual = envref
            .get_command_metadata_registry()
            .get(CommandKey::new("", "root", "a"))
            .unwrap()
            .metadata_version;

        assert_ne!(actual, stale);
        assert_eq!(actual, expected);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn simple_environment_with_payload_to_ref_refreshes_metadata_versions() {
        let mut env = SimpleEnvironmentWithPayload::<Value, ()>::new();
        let stale =
            add_stale_command_version(&mut env.command_registry.command_metadata_registry, "a");
        let expected = expected_refreshed_command_version(
            &env.command_registry.command_metadata_registry,
            "a",
        );

        let envref = env.to_ref();
        let actual = envref
            .get_command_metadata_registry()
            .get(CommandKey::new("", "root", "a"))
            .unwrap()
            .metadata_version;

        assert_ne!(actual, stale);
        assert_eq!(actual, expected);
    }

    /// A type identifier no value type describes — the shape an integration supplies at
    /// construction. `provider.LocalName`, so it satisfies the naming rule.
    fn foreign_info() -> TypeInfo {
        TypeInfo::new("test.Foreign")
            .with_type_name("test_foreign")
            .with_defaults("json", "json", "application/json", "value.json")
    }

    /// `fvt2.1` — `new()` delegates, so it still describes exactly what the value type describes.
    ///
    /// The delegation is what keeps the field initialisation in one place; if it were copied
    /// instead, the two constructors could drift apart silently.
    #[test]
    fn new_matches_new_with_the_default_registry() {
        let delegated = ImmediateEnvironment::<Value>::new();
        let explicit =
            ImmediateEnvironment::<Value>::new_with_type_registry(TypeRegistry::from_value_type::<
                Value,
            >());

        let described: Vec<&str> = delegated
            .get_type_registry()
            .iter()
            .map(|(key, _)| key.type_identifier.as_str())
            .collect();
        let explicit_described: Vec<&str> = explicit
            .get_type_registry()
            .iter()
            .map(|(key, _)| key.type_identifier.as_str())
            .collect();

        assert_eq!(described, explicit_described);
        assert!(
            described.contains(&"Text") && described.contains(&"None"),
            "and both describe the value type's own types: {described:?}"
        );
    }

    /// `fvt2.2` — a supplied registry is what the environment reports, extra type included.
    ///
    /// This is the whole registration mechanism: an integration extends the base registry and
    /// hands it over, and nothing writes to it afterwards.
    #[test]
    fn a_supplied_registry_is_what_the_environment_reports(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut types = TypeRegistry::from_value_type::<Value>();
        types.register(foreign_info())?;

        let env = ImmediateEnvironment::<Value>::new_with_type_registry(types);
        let registry = env.get_type_registry();

        assert!(
            registry.contains("test.Foreign"),
            "the supplied type is visible"
        );
        assert!(registry.contains("Text"), "and the base types survived");
        assert!(
            !registry.contains("error"),
            "there is no error type: an errored state is typed by the value it holds, which is none"
        );
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn assert_unconfigured_provider_is_trivial<E: Environment>(
        envref: EnvRef<E>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = parse_key("missing/recipe.txt")?;
        let provider = envref.get_recipe_provider();

        assert!(!provider.has_recipes(&key, envref.clone()).await?);
        assert!(provider.recipe(&key, envref).await.is_err());
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn unconfigured_core_environments_return_trivial_recipe_provider(
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_unconfigured_provider_is_trivial(SimpleEnvironment::<Value>::new().to_ref()).await?;
        assert_unconfigured_provider_is_trivial(ImmediateEnvironment::<Value>::new().to_ref())
            .await?;
        assert_unconfigured_provider_is_trivial(
            SimpleEnvironmentWithPayload::<Value, ()>::new().to_ref(),
        )
        .await?;
        assert_unconfigured_provider_is_trivial(
            ImmediateEnvironmentWithPayload::<Value, ()>::new().to_ref(),
        )
        .await?;
        Ok(())
    }

    fn test_context() -> (
        Context<TestEnvironment>,
        tokio::sync::mpsc::UnboundedReceiver<AssetServiceMessage>,
    ) {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let assetref = AssetData::<TestEnvironment>::new_temporary(envref.clone()).to_ref();
        let (service_tx, service_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Context {
                assetref,
                envref,
                cwd_key: Arc::new(Mutex::new(None)),
                service_tx,
                payload: None,
                is_volatile: false,
                pending_dependencies: Arc::new(tokio::sync::Mutex::new(Vec::new())),
                active_payload_queries: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            },
            service_rx,
        )
    }

    fn first_resource_key(query: &Query) -> &Key {
        match &query.segments[0] {
            QuerySegment::Resource(resource) => &resource.key,
            QuerySegment::Transform(_) => std::panic!("expected resource segment"),
        }
    }

    #[tokio::test]
    async fn owner_key_matches_non_evaluating_registered_owner() {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let manager = envref.get_asset_manager();
        let key = parse_key("a/b/result.txt").expect("bound key");
        let asset = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            key.clone().into(),
            Some(key.clone()),
            envref.clone(),
        );
        assert!(manager.try_insert_key_asset(&key, asset.clone()).await);

        let mut provider_recipe: Recipe = parse_key("source/result.txt")
            .expect("provider query")
            .into();
        provider_recipe.cwd = Some("a/b".to_owned());
        {
            let mut data = asset.data.write().await;
            data.recipe = provider_recipe;
        }
        let context = Context::new(asset.clone(), false).await;

        assert_eq!(context.owner_key().await.expect("owner lookup"), Some(key));
        assert_eq!(
            manager
                .owned_key_asset(&parse_key("a/b/result.txt").expect("bound key"))
                .await
                .expect("registered owner")
                .id(),
            asset.id()
        );
    }

    #[tokio::test]
    async fn owner_key_rejects_temporary_ad_hoc_volatile_and_provider_mismatch() {
        let envref = ImmediateEnvironment::<Value>::new().to_ref();
        let temporary = AssetData::<TestEnvironment>::new_temporary(envref.clone()).to_ref();
        assert_eq!(
            Context::new(temporary, false)
                .await
                .owner_key()
                .await
                .expect("temporary owner"),
            None
        );

        let ad_hoc_key = parse_key("ad-hoc/value.txt").expect("ad-hoc key");
        let mut ad_hoc_recipe: Recipe = ad_hoc_key.into();
        ad_hoc_recipe
            .arguments
            .insert("argument".to_owned(), serde_json::json!(1));
        let ad_hoc = AssetRef::new_from_recipe(
            envref.get_asset_manager().next_id_for_asset(),
            ad_hoc_recipe,
            None,
            envref.clone(),
        );
        assert_eq!(
            Context::new(ad_hoc, false)
                .await
                .owner_key()
                .await
                .expect("ad-hoc owner"),
            None
        );

        let manager = envref.get_asset_manager();
        let volatile_key = parse_key("bound/volatile.txt").expect("volatile key");
        let volatile = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            volatile_key.clone().into(),
            Some(volatile_key.clone()),
            envref.clone(),
        );
        volatile
            .set_status(crate::metadata::Status::Volatile)
            .await
            .expect("mark volatile");
        assert!(
            manager
                .try_insert_key_asset(&volatile_key, volatile.clone())
                .await
        );
        assert_eq!(
            Context::new(volatile, true)
                .await
                .owner_key()
                .await
                .expect("volatile owner"),
            None
        );
        assert!(manager.lookup_key_asset(&volatile_key).is_none());

        let key = parse_key("a/b/mismatch.txt").expect("mismatch key");
        let asset = AssetRef::new_from_recipe(
            manager.next_id_for_asset(),
            key.clone().into(),
            Some(key.clone()),
            envref.clone(),
        );
        assert!(manager.try_insert_key_asset(&key, asset.clone()).await);
        let mut provider_recipe: Recipe = parse_key("source/mismatch.txt")
            .expect("provider query")
            .into();
        provider_recipe.cwd = Some("a/c".to_owned());
        {
            let mut data = asset.data.write().await;
            data.recipe = provider_recipe;
        }

        assert_eq!(
            Context::new(asset, false)
                .await
                .owner_key()
                .await
                .expect("mismatch owner"),
            None
        );
    }

    #[test]
    fn resolver_installs_root_once_across_context_clones() {
        let (context, mut receiver) = test_context();
        let cloned = context.clone();

        let first = context
            .resolve_key_from_cwd(&parse_key("./one").expect("first relative key"))
            .expect("first resolution");
        let second = cloned
            .resolve_key_from_cwd(&parse_key("../two").expect("second relative key"))
            .expect("second resolution");

        assert_eq!(first.encode(), "one");
        assert_eq!(second.encode(), "two");
        assert_eq!(context.get_cwd_key(), Some(Key::new()));
        assert_eq!(cloned.get_cwd_key(), Some(Key::new()));
        match receiver.try_recv().expect("one warning") {
            AssetServiceMessage::LogMessage(entry) => {
                assert_eq!(entry.kind, LogEntryKind::Warning);
                assert_eq!(entry.message, RELATIVE_WITHOUT_CWD_WARNING);
            }
            message => std::panic!("expected warning log, got {message:?}"),
        }
        assert!(receiver.try_recv().is_err(), "warning must be emitted once");
    }

    #[test]
    fn root_fallback_warning_delivery_error_propagates() {
        let (context, receiver) = test_context();
        drop(receiver);

        let error = context
            .resolve_key_from_cwd(&parse_key("./missing-base").expect("relative key"))
            .expect_err("closed warning channel must fail");

        assert!(error.message.contains("Failed to send log message"));
        assert_eq!(context.get_cwd_key(), Some(Key::new()));
    }

    #[test]
    fn absolute_operands_ignore_missing_cwd() {
        let (context, mut receiver) = test_context();
        let ordinary = parse_key("ordinary/value.txt").expect("ordinary key");

        assert_eq!(
            context
                .resolve_key_from_cwd(&ordinary)
                .expect("ordinary resolution"),
            ordinary
        );
        let absolute = parse_query("/-R/./absolute.txt").expect("absolute query");
        let resolved = context
            .resolve_query_from_cwd(&absolute)
            .expect("absolute resolution");

        assert_eq!(first_resource_key(&resolved).encode(), "absolute.txt");
        assert_eq!(context.get_cwd_key(), None);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn absolute_query_does_not_absolutize_relative_link() {
        let (context, mut receiver) = test_context();
        context.set_cwd_key(Some(parse_key("a/b").expect("context cwd")));
        let query = parse_query("/-R/./outer.txt/-/action-~X~-R/./linked.txt~E")
            .expect("absolute query with relative link");

        let resolved = context
            .resolve_query_from_cwd(&query)
            .expect("scoped resolution");
        assert_eq!(first_resource_key(&resolved).encode(), "outer.txt");
        let linked = match &resolved.segments[1] {
            QuerySegment::Transform(transform) => match &transform.query[0].parameters[0] {
                ActionParameter::Link(link, _) => link,
                ActionParameter::String(_, _) => std::panic!("expected linked query"),
            },
            QuerySegment::Resource(_) => std::panic!("expected transform segment"),
        };
        assert_eq!(first_resource_key(linked).encode(), "a/b/linked.txt");
        assert_eq!(context.get_cwd_key(), Some(parse_key("a/b").expect("cwd")));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn absolute_outer_query_keeps_relative_link_independent() {
        let (context, mut receiver) = test_context();
        let query =
            parse_query("/-R/./outer.txt/-/action-~X~-R/./relative.txt~E-~X~/-R/./absolute.txt~E")
                .expect("absolute query with relative and absolute links");

        let resolved = context
            .resolve_query_from_cwd(&query)
            .expect("scoped resolution without an entry cwd");
        assert!(resolved.absolute);
        assert_eq!(first_resource_key(&resolved).encode(), "outer.txt");
        let QuerySegment::Transform(transform) = &resolved.segments[1] else {
            std::panic!("expected transform segment");
        };
        let ActionParameter::Link(relative_link, _) = &transform.query[0].parameters[0] else {
            std::panic!("expected relative linked query");
        };
        let ActionParameter::Link(absolute_link, _) = &transform.query[0].parameters[1] else {
            std::panic!("expected absolute linked query");
        };
        assert!(!relative_link.absolute);
        assert_eq!(first_resource_key(relative_link).encode(), "relative.txt");
        assert!(absolute_link.absolute);
        assert_eq!(first_resource_key(absolute_link).encode(), "absolute.txt");
        assert_eq!(context.get_cwd_key(), Some(Key::new()));

        let warning = receiver.try_recv().expect("root fallback warning");
        let AssetServiceMessage::LogMessage(warning) = warning else {
            std::panic!("expected a log message");
        };
        assert_eq!(warning.kind, LogEntryKind::Warning);
        assert_eq!(warning.message, RELATIVE_WITHOUT_CWD_WARNING);
        assert!(receiver.try_recv().is_err());
    }

    /// All three command-facing entry points refuse a query that cannot name an asset.
    ///
    /// A relative query would mean different things in different directories while looking
    /// identical, so it could not be identified or cached. The directory reaches a command as a
    /// `-R-key/.` link argument instead, and the command builds an absolute query from it — which
    /// is what the positive half of this test exercises.
    #[tokio::test]
    async fn context_entry_points_reject_relative_queries() {
        let (context, _receiver) = test_context();
        context.set_cwd_key(Some(parse_key("a/b").expect("context cwd")));

        let relative = parse_query("-R-key/./from-state").expect("relative query");

        let state_error = context
            .get_dependency_state(&relative)
            .await
            .expect_err("get_dependency_state must refuse a relative query");
        let Err(evaluate_error) = context.evaluate(&relative).await else {
            std::panic!("evaluate must refuse a relative query");
        };
        let Err(apply_error) = context.apply(&relative, State::new()).await else {
            std::panic!("apply must refuse a relative query");
        };

        for error in [state_error, evaluate_error, apply_error] {
            assert_eq!(error.error_type, crate::error::ErrorType::NotSupported);
            assert!(
                error.message.contains("-R-key/."),
                "the message must name the supported replacement: {error}"
            );
        }

        // The absolute form a command is expected to build still works at all three entry points.
        let absolute = parse_query("-R-key/a/b/from-state").expect("absolute query");
        let expected = Value::Key(parse_key("a/b/from-state").expect("expected key"));

        let state = context
            .get_dependency_state(&absolute)
            .await
            .expect("dependency state");
        assert_eq!(state.value().expect("state value").as_ref(), &expected);

        let evaluated = context.evaluate(&absolute).await.expect("evaluate");
        assert_eq!(
            evaluated
                .get()
                .await
                .expect("evaluated state")
                .value()
                .expect("evaluated value")
                .as_ref(),
            &expected
        );

        let applied = context.apply(&absolute, State::new()).await.expect("apply");
        assert_eq!(
            applied
                .get()
                .await
                .expect("applied state")
                .value()
                .expect("applied value")
                .as_ref(),
            &expected
        );
    }
}
